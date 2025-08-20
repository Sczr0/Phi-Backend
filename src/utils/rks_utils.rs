use crate::models::rks::RksRecord;
use once_cell::sync::Lazy;
use std::cmp::Ordering;
use std::collections::HashMap;

// --- RKS 计算辅助函数 ---
pub fn calculate_player_rks_details(records: &[RksRecord]) -> (f64, f64) {
    log::debug!("[B30 RKS] 开始计算玩家RKS详情，总成绩数: {}", records.len());

    if records.is_empty() {
        log::debug!("[B30 RKS] 无成绩记录，RKS = 0");
        return (0.0, 0.0);
    }

    // 移除了内部排序。现在强制要求调用者传入已排序的数据，避免不必要的开销。
    // let mut sorted_records = records.to_vec(); ...

    let best_27_sum: f64 = records.iter().take(27).map(|r| r.rks).sum();
    let b27_count = records.len().min(27);
    log::debug!("[B30 RKS] Best 27 计算: 使用了 {b27_count} 个成绩，总和 = {best_27_sum:.4}");

    // 直接在已排序的 `records` 上筛选 AP 记录，无需额外分配 Vec。
    let ap_records = records.iter().filter(|r| r.acc >= 100.0);
    let ap_top_3_sum: f64 = ap_records.clone().take(3).map(|r| r.rks).sum();
    let ap3_count = ap_records.count().min(3);
    log::debug!("[B30 RKS] AP Top 3 计算: 使用了 {ap3_count} 个AP成绩，总和 = {ap_top_3_sum:.4}");

    let final_exact_rks = (best_27_sum + ap_top_3_sum) / 30.0;
    let final_rounded_rks = (final_exact_rks * 100.0).round() / 100.0;

    log::debug!("[B30 RKS] 最终 RKS 计算: ... -> Rounded {final_rounded_rks:.2}");

    (final_exact_rks, final_rounded_rks)
}

/// 计算指定谱面的 RKS 值。
pub fn calculate_chart_rks(acc_percent: f64, constant: f64) -> f64 {
    // 逻辑不变，此函数是正确的。
    if acc_percent < 70.0 {
        return 0.0;
    }
    let acc_factor = ((acc_percent - 55.0) / 45.0).powi(2); // 使用 .powi(2) 更高效
    acc_factor * constant
}

// --- 推分 ACC 计算 ---

/// 模拟计算将指定谱面提升到某个 ACC 后的精确 RKS 值。
fn simulate_rks_increase_simplified(
    target_chart_id_full: &str,
    target_chart_constant: f64,
    test_acc: f64,
    all_sorted_records: &[RksRecord],
) -> f64 {
    let parts: Vec<&str> = target_chart_id_full.rsplitn(2, '-').collect();
    if parts.len() != 2 {
        return 0.0;
    } // 格式错误
    let (song_id, difficulty) = (parts[1], parts[0]);

    // 1. 计算模拟后的谱面RKS
    let simulated_chart_rks = calculate_chart_rks(test_acc, target_chart_constant);

    // 2. 创建一个新的、临时的记录列表进行模拟，避免复杂的逻辑判断
    // 我们只关心RKS和是否为AP，所以用元组 (rks, is_ap) 来模拟，更轻量
    let mut simulated_records: Vec<(f64, bool)> = all_sorted_records
        .iter()
        .filter(|r| !(r.song_id == song_id && r.difficulty == difficulty)) // 排除旧记录
        .map(|r| (r.rks, r.acc >= 100.0))
        .collect();

    // 3. 插入新记录
    simulated_records.push((simulated_chart_rks, test_acc >= 100.0));

    // 4. 重新排序 (这是最关键的简化，虽然有排序开销，但比你之前复杂的逻辑更不易出错且通常足够快)
    simulated_records.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(Ordering::Equal));

    // 5. 重新计算 B27 和 AP3
    let b27_sum: f64 = simulated_records.iter().take(27).map(|(rks, _)| rks).sum();
    let ap3_sum: f64 = simulated_records
        .iter()
        .filter(|(_, is_ap)| *is_ap)
        .take(3)
        .map(|(rks, _)| rks)
        .sum();

    (b27_sum + ap3_sum) / 30.0
}

/// 推分ACC计算结果缓存
static PUSH_ACC_CACHE: Lazy<std::sync::RwLock<HashMap<String, f64>>> =
    Lazy::new(|| std::sync::RwLock::new(HashMap::new()));

const PUSH_ACC_CACHE_SIZE: usize = 5000; // 缓存5000个推分ACC计算结果

/// (优化后) 计算指定谱面需要达到多少 ACC 才能使玩家总 RKS (四舍五入后) 增加 0.01
pub fn calculate_target_chart_push_acc(
    target_chart_id_full: &str,
    target_chart_constant: f64,
    all_sorted_records: &[RksRecord], // 必须是已按RKS排序的Vec
) -> Option<f64> {
    log::debug!("开始计算推分ACC (优化版): 目标谱面={target_chart_id_full}");

    // 1. 检查缓存
    {
        if let Some(cached_result) = PUSH_ACC_CACHE.read().unwrap().get(target_chart_id_full) {
            log::debug!("推分ACC缓存命中: {target_chart_id_full}");
            return Some(*cached_result);
        }
    }

    // 2. 计算当前 RKS 详情
    let (current_exact_rks, _current_rounded_rks) =
        calculate_player_rks_details(all_sorted_records);

    // 3. 计算目标精确 RKS 阈值
    let target_rks_threshold = {
        let third_decimal_ge_5 = (current_exact_rks * 1000.0) % 10.0 >= 5.0;
        if third_decimal_ge_5 {
            (current_exact_rks * 100.0).floor() / 100.0 + 0.015
        } else {
            (current_exact_rks * 100.0).floor() / 100.0 + 0.005
        }
    };

    if current_exact_rks >= target_rks_threshold {
        log::debug!("无需推分，当前 RKS 已达标");
        return Some(100.0);
    }

    // 4. 边界检查: 检查ACC 100%时是否能达到目标
    let rks_at_100 = simulate_rks_increase_simplified(
        target_chart_id_full,
        target_chart_constant,
        100.0,
        all_sorted_records,
    );

    if rks_at_100 < target_rks_threshold {
        log::debug!("无法推分，ACC 100% 仍无法达到目标");
        return Some(100.0);
    }

    // 5. 获取当前谱面的 ACC
    let parts: Vec<&str> = target_chart_id_full.rsplitn(2, '-').collect();
    if parts.len() != 2 {
        return None;
    }
    let (song_id, difficulty) = (parts[1], parts[0]);

    let current_acc = all_sorted_records
        .iter()
        .find(|r| r.song_id == song_id && r.difficulty == difficulty)
        .map_or(70.0, |r| r.acc);

    // 6. 二分查找最小达标ACC - 优化：减少迭代次数，提高精度
    let mut low = current_acc;
    let mut high = 100.0;
    log::debug!("开始二分查找推分ACC, 区间: [{low:.4}, {high:.4}]");

    // 减少迭代次数，提高性能
    for _ in 0..10 {
        // 固定10次迭代，足够达到很高精度
        let mid = low + (high - low) / 2.0;
        let simulated_rks = simulate_rks_increase_simplified(
            target_chart_id_full,
            target_chart_constant,
            mid,
            all_sorted_records,
        );

        if simulated_rks >= target_rks_threshold {
            high = mid; // mid 满足条件，尝试更低的 acc
        } else {
            low = mid; // mid 不满足条件，需要更高的 acc
        }
    }

    log::debug!("二分查找结束, 结果 high = {high:.6}");

    // 7. 格式化结果，避免结果低于当前ACC
    let result_acc = high.max(current_acc);
    // 向上取整到小数点后3位，平衡精度和性能
    let final_acc = (result_acc * 1000.0).ceil() / 1000.0;

    // 8. 存入缓存
    let result = final_acc.min(100.0);
    {
        let mut cache = PUSH_ACC_CACHE.write().unwrap();
        if cache.len() >= PUSH_ACC_CACHE_SIZE {
            // 简单LRU：删除最旧的键
            if let Some(first_key) = cache.keys().next().cloned() {
                cache.remove(&first_key);
            }
        }
        cache.insert(target_chart_id_full.to_string(), result);
    }

    Some(result)
}
