//! rks_utils.rs
//!
//! This module provides utility functions for calculating RKS and "Push ACC".
//! It centralizes the logic to ensure consistency across the application.

use crate::models::rks::RksRecord;
use std::cmp::Ordering;

// --- RKS 计算辅助函数 ---

/// 根据已排序的 RKS 记录列表计算玩家的精确 RKS 和四舍五入后的 RKS。
/// records: 必须是按 RKS 降序排序的 RksRecord 列表。
pub fn calculate_player_rks_details(records: &[RksRecord]) -> (f64, f64) {
    log::debug!("[B30 RKS] 开始计算玩家RKS详情，总成绩数: {}", records.len());

    if records.is_empty() {
        log::debug!("[B30 RKS] 无成绩记录，RKS = 0");
        return (0.0, 0.0);
    }

    // 确保 records 是按 RKS 降序排列的
    let mut sorted_records = records.to_vec();
    sorted_records.sort_by(|a, b| b.rks.partial_cmp(&a.rks).unwrap_or(std::cmp::Ordering::Equal));

    // 计算 Best 27 RKS 总和
    let best_27_sum: f64 = sorted_records.iter()
        .take(27) // 取前 27 个 (如果不足 27 个，则取所有)
        .map(|r| r.rks)
        .sum();
    let b27_count = sorted_records.len().min(27);
    log::debug!("[B30 RKS] Best 27 计算: 使用了 {} 个成绩，总和 = {:.4}", b27_count, best_27_sum);

    // 筛选出所有 Phi (ACC = 100%) 的成绩，并按 RKS 降序排列
    let ap_records: Vec<&RksRecord> = sorted_records.iter()
        .filter(|r| r.acc >= 100.0)
        .collect(); // 这里已经是按 RKS 降序的，因为是从 sorted_records 筛选的
    
    // 计算 AP Top 3 RKS 总和
    let ap_top_3_sum: f64 = ap_records.iter()
        .take(3) // 取 AP 中的前 3 个 (如果不足 3 个，则取所有 AP)
        .map(|r| r.rks)
        .sum();
    let ap3_count = ap_records.len().min(3);
    log::debug!("[B30 RKS] AP Top 3 计算: AP成绩数={}, 使用了 {} 个，总和 = {:.4}", ap_records.len(), ap3_count, ap_top_3_sum);

    // 计算最终 B30 RKS (B27 总和 + AP3 总和) / 30
    let final_exact_rks = (best_27_sum + ap_top_3_sum) / 30.0;
    let final_rounded_rks = (final_exact_rks * 100.0).round() / 100.0;

    log::debug!("[B30 RKS] 最终 RKS 计算: (B27 Sum {:.4} + AP3 Sum {:.4}) / 30 = {:.6} -> Rounded {:.2}",
               best_27_sum, ap_top_3_sum, final_exact_rks, final_rounded_rks);

    (final_exact_rks, final_rounded_rks)
}

/// 计算指定谱面的 RKS 值。
pub fn calculate_chart_rks(acc_percent: f64, constant: f64) -> f64 {
    // 确保ACC在合理范围内
    let acc = acc_percent / 100.0;
    let acc_factor = if acc < 0.7 {
        0.0
    } else {
        ((acc * 100.0 - 55.0) / 45.0).powf(2.0)
    };
    
    let rks = acc_factor * constant;
    log::debug!("谱面RKS计算: ACC={:.2}%, 定数={:.1}, ACC={:.4}, RKS={:.4}", 
                acc_percent, constant, acc_factor, rks);
    
    rks
}

/// (内部辅助结构) 存储RKS计算的预计算值
#[derive(Debug, Clone)]
struct PrecalculatedRksDetails {
    exact_rks: f64,
    b27_sum: f64,
    ap3_sum: f64,
    b27_records: Vec<RksRecord>, // 排序好的前27个记录
    ap_records_sorted_by_rks: Vec<RksRecord>, // 所有AP记录，按RKS排序
}

impl PrecalculatedRksDetails {
    fn from_sorted_records(records: &[RksRecord]) -> Self {
        let b27_records: Vec<RksRecord> = records.iter().take(27).cloned().collect();
        let b27_sum: f64 = b27_records.iter().map(|r| r.rks).sum();

        let ap_records_sorted_by_rks: Vec<RksRecord> = records.iter()
            .filter(|r| r.acc >= 100.0)
            .cloned()
            .collect(); // 已经是按RKS排序的
        let ap3_sum: f64 = ap_records_sorted_by_rks.iter().take(3).map(|r| r.rks).sum();

        let exact_rks = (b27_sum + ap3_sum) / 30.0;

        PrecalculatedRksDetails {
            exact_rks,
            b27_sum,
            ap3_sum,
            b27_records,
            ap_records_sorted_by_rks,
        }
    }
}

/// (优化后) 模拟计算将指定谱面提升到某个 ACC 后的精确 RKS 值，使用增量更新。
fn simulate_rks_increase(
    target_chart_id_full: &str, 
    target_chart_constant: f64, 
    test_acc: f64, 
    all_sorted_records: &[RksRecord], 
    precalculated: &PrecalculatedRksDetails,
) -> f64 {
    log::debug!("模拟RKS增长: 目标谱面={}, 测试ACC={:.4}%, 定数={:.1}",
                target_chart_id_full, test_acc, target_chart_constant);

    // 分离song_id和difficulty
    let parts: Vec<&str> = target_chart_id_full.rsplitn(2, '-').collect();
    if parts.len() != 2 { return precalculated.exact_rks; } // 格式错误则返回原值
    let song_id = parts[1];
    let difficulty = parts[0];
    
    // 计算模拟后的谱面 RKS
    let simulated_chart_rks = calculate_chart_rks(test_acc, target_chart_constant);
    let simulated_is_ap = test_acc >= 100.0;

    let mut current_b27_sum = precalculated.b27_sum;
    let mut current_ap3_sum = precalculated.ap3_sum;

    // 查找原记录信息
    let original_record_opt = all_sorted_records.iter()
        .find(|r| r.song_id == song_id && r.difficulty == difficulty);

    let original_is_ap = original_record_opt.map_or(false, |r| r.acc >= 100.0);

    // --- 模拟 B27 更新 ---
    let mut b27_candidates = precalculated.b27_records.clone();

    // 移除旧记录（如果存在于B27中）
    let mut removed_from_b27 = false;
    if let Some(original_record) = original_record_opt {
        if let Some(pos) = b27_candidates.iter().position(|r| r.song_id == original_record.song_id && r.difficulty == original_record.difficulty) {
            current_b27_sum -= original_record.rks;
            b27_candidates.remove(pos);
            removed_from_b27 = true;
            log::trace!(" - 从B27移除旧记录: {}", target_chart_id_full);
        }
    }

    // 判断新记录是否能进入B27
    // 获取B27的最低RKS（如果B27不满27个，则最低为0）
    let _b27_min_rks = if b27_candidates.len() < 27 && !removed_from_b27 {
        0.0 // B27未满且旧记录不在B27中（或者不存在旧记录），新记录肯定能进
    } else if b27_candidates.is_empty() {
        0.0 // B27为空，新记录肯定能进
    } else {
        // 如果B27满了，需要和第27个（移除旧记录后可能是第26个）或B27之外的最高分比较
        let min_rks_in_current_b27 = b27_candidates.last().map_or(0.0, |r| r.rks);
        if b27_candidates.len() >= 27 { // B27已满
             min_rks_in_current_b27
        } else { // B27未满 (因为移除了一个)，需要考虑第28个记录
             all_sorted_records.get(27).map_or(min_rks_in_current_b27, |next_r| next_r.rks.min(min_rks_in_current_b27))
        }
    };

    // 判断新记录是否能进入B27
    let b27_threshold = precalculated.b27_records.last().map_or(0.0, |r| r.rks);

    if simulated_chart_rks > b27_threshold || removed_from_b27 {
        // 如果新RKS高于B27门槛，或者被更新的歌曲原本就在B27中
        
        // 1. 加上新RKS
        current_b27_sum += simulated_chart_rks;
        log::trace!(" + 添加到B27 Sum: {} (RKS {:.4})", target_chart_id_full, simulated_chart_rks);

        // 2. 如果是从外部挤入B27，则需要减去被挤掉的成绩
        if !removed_from_b27 && precalculated.b27_records.len() >= 27 {
            if let Some(record_to_remove) = precalculated.b27_records.last() {
                current_b27_sum -= record_to_remove.rks;
                log::trace!(" - B27中被挤掉的记录: {} (RKS {:.4})", record_to_remove.song_id, record_to_remove.rks);
            }
        }
        // 如果removed_from_b27为true, 说明只是更新B27内部的歌曲, 在之前已经减去了旧值, 此处无需操作
    }

    // --- 模拟 AP Top 3 更新 ---
    let mut ap_candidates = precalculated.ap_records_sorted_by_rks.clone();

    // 移除旧AP记录（如果存在）
    if original_is_ap {
       if let Some(pos) = ap_candidates.iter().position(|r| r.song_id == song_id && r.difficulty == difficulty) {
            ap_candidates.remove(pos);
            log::trace!(" - 从AP列表移除旧记录: {}", target_chart_id_full);
       }
    }

    // 添加新AP记录（如果模拟结果是AP）
    if simulated_is_ap {
        let new_ap_record = RksRecord { // 构造一个临时的 RksRecord 用于排序
             song_id: song_id.to_string(),
             difficulty: difficulty.to_string(),
             rks: simulated_chart_rks,
             // 其他字段不重要
             song_name: "".to_string(), difficulty_value: 0.0, score: None, acc: 100.0, is_fc: false,
        };
        ap_candidates.push(new_ap_record);
        ap_candidates.sort_by(|a, b| b.rks.partial_cmp(&a.rks).unwrap_or(Ordering::Equal)); // 按RKS重新排序AP记录
        log::trace!(" + 添加到AP列表: {} (RKS {:.4})", target_chart_id_full, simulated_chart_rks);
    }

    // 重新计算AP3 Sum
    let new_ap3_sum = ap_candidates.iter().take(3).map(|r| r.rks).sum();
    log::trace!(" - 旧AP3 Sum: {:.4}, 新AP3 Sum: {:.4}", current_ap3_sum, new_ap3_sum);
    current_ap3_sum = new_ap3_sum;


    // 计算最终模拟RKS
    let simulated_exact_rks = (current_b27_sum + current_ap3_sum) / 30.0;

    log::debug!("模拟RKS增长结果: 原RKS={:.6}, 模拟RKS={:.6}",
                precalculated.exact_rks, simulated_exact_rks);

    simulated_exact_rks
}

/// (重构后) 计算指定谱面需要达到多少 ACC 才能使玩家总 RKS (四舍五入后) 增加 0.01
/// 返回 Option<f64>，None 表示谱面格式错误，Some(100.0) 表示无法推分或已满
pub fn calculate_target_chart_push_acc(
    target_chart_id_full: &str, 
    target_chart_constant: f64, 
    all_sorted_records: &Vec<RksRecord> // 确保传入的是已按RKS排序的Vec
) -> Option<f64> 
{
    log::debug!("开始计算推分ACC (优化版): 目标谱面={}, 定数={:.1}",
                target_chart_id_full, target_chart_constant);
    
    // 1. 预计算当前 RKS 详情
    let precalculated = PrecalculatedRksDetails::from_sorted_records(all_sorted_records);
    let current_exact_rks = precalculated.exact_rks;
    let current_rounded_rks = (current_exact_rks * 100.0).round() / 100.0;

    log::debug!("当前玩家RKS: 精确值={:.6}, 四舍五入={:.2}",
                current_exact_rks, current_rounded_rks);
    log::trace!("预计算详情: {:?}", precalculated);
    
    // 2. 计算目标精确 RKS 阈值 (逻辑不变)
    let target_rks_threshold = {
        let third_decimal = ((current_exact_rks * 1000.0) % 10.0) as i32;
        let threshold = if third_decimal < 5 {
            (current_exact_rks * 100.0).floor() / 100.0 + 0.005
        } else {
            (current_exact_rks * 100.0).floor() / 100.0 + 0.015
        };
        log::debug!("目标RKS阈值计算: 当前RKS={:.6}, 第三位小数={}, 目标阈值={:.6}",
                   current_exact_rks, third_decimal, threshold);
        threshold
    };

    // --- 新增：基于 B27 和 AP3 的预过滤 ---
    let should_skip_calculation = {
        let b27_full = precalculated.b27_records.len() >= 27;
        let ap3_full = precalculated.ap_records_sorted_by_rks.len() >= 3;

        // 获取 B27 和 AP3 的最低 RKS (如果列表已满)
        let min_rks_in_b27 = if b27_full { precalculated.b27_records.last().map_or(0.0, |r| r.rks) } else { 0.0 };
        let min_rks_in_ap3 = if ap3_full { precalculated.ap_records_sorted_by_rks.get(2).map_or(0.0, |r| r.rks) } else { 0.0 }; // AP3的第3个

        // 只有当 B27 和 AP3 都满了，且目标谱面定数低于两者的最低 RKS 时，才跳过计算
        // （注意：这里的 target_chart_constant 是谱面定数，不是计算出的RKS）
        // 考虑特殊情况：如果一个谱面定数很高，但acc很低导致rks低，AP后仍可能进入B27或AP3
        // 所以更安全的检查是用AP后的RKS (即定数本身) 来比较
        let potential_ap_rks = target_chart_constant; // AP RKS = 定数

        if b27_full && ap3_full && potential_ap_rks < min_rks_in_b27 && potential_ap_rks < min_rks_in_ap3 {
            log::debug!("预过滤: 目标谱面 {} (定数 {:.1}) 低于 B27 最低 RKS ({:.4}) 和 AP3 最低 RKS ({:.4})，跳过推分计算",
                       target_chart_id_full, target_chart_constant, min_rks_in_b27, min_rks_in_ap3);
            true // 跳过计算
        } else {
            false // 不跳过
        }
    };

    if should_skip_calculation {
        return Some(100.0); // 跳过，视为无法推分
    }
    // --- 预过滤结束 ---

    // 3. 边界检查 1: 当前 RKS 是否已达标?
    if current_exact_rks >= target_rks_threshold {
        log::debug!("推分计算: 玩家 RKS ({:.6}) 已达到或超过目标阈值 ({:.6})，无需推分 {}",
                   current_exact_rks, target_rks_threshold, target_chart_id_full);
        return Some(100.0); // 返回 100.0 代表无需推分或已满
    }

    // 4. 分离 song_id 和 difficulty
    let parts: Vec<&str> = target_chart_id_full.rsplitn(2, '-').collect();
    if parts.len() != 2 { 
        log::debug!("谱面ID格式错误: {}", target_chart_id_full);
        return None; // 格式错误返回 None
    }
    let song_id = parts[1];
    let difficulty = parts[0];
    
    // 5. 获取当前谱面的 ACC (如果存在)
    let current_acc = all_sorted_records.iter()
        .find(|r| r.song_id == song_id && r.difficulty == difficulty)
        .map_or(70.0, |r| r.acc.max(70.0)); // 如果没打过，从 70 开始；否则从当前 ACC 开始
    log::debug!("当前谱面ACC: {}", current_acc);

    // 6. 边界检查 2: 检查是否可以通过提高当前成绩的ACC到100%来达成推分 (使用模拟函数)
    let simulated_rks_at_100 = simulate_rks_increase(
        target_chart_id_full, target_chart_constant, 100.0, all_sorted_records, &precalculated
    );

    if simulated_rks_at_100 < target_rks_threshold {
         log::debug!("推分计算: {} ACC 100% (模拟RKS {:.6}) 仍无法达到目标RKS阈值 ({:.6})",
                    target_chart_id_full, simulated_rks_at_100, target_rks_threshold);
         return Some(100.0); // 无法推分，返回 100.0
    }

    // 7. 二分查找最小达标ACC
    let mut low = current_acc;
    let mut high = 100.0;
    log::debug!("开始二分查找推分ACC for {}, 区间: [{:.4}, {:.4}]", 
              target_chart_id_full, low, high);

    for i in 0..100 { // 迭代次数
        if high - low < 1e-5 { break; } // 精度控制
        let mid = low + (high - low) / 2.0;

        // 使用模拟函数检查 RKS
        let simulated_rks = simulate_rks_increase(
            target_chart_id_full, target_chart_constant, mid, all_sorted_records, &precalculated
        );

        if simulated_rks >= target_rks_threshold {
            high = mid; // mid 满足条件，尝试更低的 acc
            log::trace!("迭代#{}: mid={:.4} (模拟RKS {:.6}) 满足条件，新区间[{:.4}, {:.4}]",
                      i, mid, simulated_rks, low, high);
        } else {
            low = mid; // mid 不满足条件，需要更高的 acc
            log::trace!("迭代#{}: mid={:.4} (模拟RKS {:.6}) 不满足条件，新区间[{:.4}, {:.4}]",
                      i, mid, simulated_rks, low, high);
        }
    }
    
    log::debug!("二分查找结束 for {}, 结果 high = {:.6}", target_chart_id_full, high);

    // 8. 处理结果 (逻辑不变)
    let result_acc = high.max(70.0);
    let rounded_to_2_decimals = (result_acc * 100.0).ceil() / 100.0;
    let final_acc = if (rounded_to_2_decimals - current_acc).abs() < 1e-5 {
        (result_acc * 1000.0).ceil() / 1000.0
    } else {
        rounded_to_2_decimals
    };
    let constrained_acc = final_acc.min(100.0);

    log::debug!("最终推分ACC结果: {:.4}%", constrained_acc);
    Some(constrained_acc)
}
/// --- 计算最小推分 ACC (旧版算法) ---
#[deprecated(note = "This is a legacy push ACC calculation method. Use calculate_target_chart_push_acc for more accurate results.")]
pub fn calculate_min_push_acc(current_rks: f64, difficulty_value: f64) -> f64 {
    // 无法有效计算的情况
    if difficulty_value <= 0.0 { return 100.0; } // 定数为0无法计算

    // 计算当前RKS四舍五入到两位小数
    let current_rks_rounded = (current_rks * 100.0).round() / 100.0;
    // 计算要达到下一个0.01 RKS所需的精确RKS阈值 (当前舍入值 + 0.005)
    let target_rks_threshold = current_rks_rounded + 0.005;

    // 如果当前RKS已经达到或超过了下一个阈值，说明无法通过"提升ACC"来达成"+0.01"的目标
    if current_rks >= target_rks_threshold {
        return 100.0; // 返回100表示已满或无法提升
    }

    // Phigros RKS正确计算公式: RKS = ((100×Acc - 55)/45)² × 定数
    // 反解得到 ACC 小数 = (55 + 45 * sqrt(目标RKS / 定数)) / 100

    // 防止除零或负数开方
    let rks_ratio = target_rks_threshold / difficulty_value;
    if rks_ratio < 0.0 {
        return 100.0; // 不应该发生，但做保护
    }

    let sqrt_term = rks_ratio.sqrt();
    let acc_decimal = (55.0 + 45.0 * sqrt_term) / 100.0;

    // 转换为百分数形式并检查合理性
    let acc_percent = acc_decimal * 100.0;

    // 应用约束 [70.0, 100.0]
    let constrained_acc = acc_percent.max(70.0).min(100.0);

    // 向上取整到小数点后两位，确保稳定跨过阈值
    (constrained_acc * 100.0).ceil() / 100.0
}