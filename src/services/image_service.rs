use crate::models::user::IdentifierRequest;
use crate::models::rks::RksRecord;
use crate::services::phigros::PhigrosService;
use crate::services::user::UserService;
use crate::services::song::SongService;
use crate::utils::error::AppError;
use crate::utils::image_renderer::{self, PlayerStats, SongRenderData, SongDifficultyScore};
use crate::utils::cover_loader;
use chrono::Utc;
use actix_web::web;
use std::cmp::Ordering;
use std::collections::HashMap;
use itertools::Itertools;
use tokio;
use std::path::PathBuf;
use crate::services::player_archive_service::PlayerArchiveService;
use crate::utils::image_renderer::LeaderboardRenderData;

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
    log::debug!("谱面RKS计算: ACC={:.2}%, 定数={:.1}, ACC因子={:.4}, RKS={:.4}", 
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
///
/// Args:
/// * `target_chart_id_full`: 目标谱面ID (song_id-difficulty)
/// * `target_chart_constant`: 目标谱面定数
/// * `test_acc`: 模拟的ACC百分比
/// * `all_sorted_records`: 玩家所有成绩，按RKS降序排列
/// * `precalculated`: 预计算的RKS详情
///
/// Returns:
/// * 模拟计算后的精确 RKS 值
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
    let mut simulated_ap_records = precalculated.ap_records_sorted_by_rks.clone();

    // 查找原记录信息
    let original_record_opt = all_sorted_records.iter()
        .find(|r| r.song_id == song_id && r.difficulty == difficulty);

    let original_rks = original_record_opt.map_or(0.0, |r| r.rks);
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
    let b27_min_rks = if b27_candidates.len() < 27 && !removed_from_b27 {
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


    if simulated_chart_rks > b27_min_rks {
        current_b27_sum += simulated_chart_rks;
        // 如果B27原本已满，需要减去被挤掉的那个（即更新前的第27个，如果旧记录被移除，则是第27个）
        if precalculated.b27_records.len() >= 27 {
            if removed_from_b27 {
                 // 旧记录在B27被移除，新纪录加入，第27个被挤出
                 if let Some(record_to_remove) = precalculated.b27_records.get(26) { // 原来的第27个
                     current_b27_sum -= record_to_remove.rks;
                     log::trace!(" - B27移除记录: {} (RKS {:.4})", record_to_remove.song_id, record_to_remove.rks);
                 }
            } else {
                // 旧记录不在B27，新纪录加入，第27个被挤出
                 if let Some(record_to_remove) = precalculated.b27_records.last() { // 原来的第27个
                     current_b27_sum -= record_to_remove.rks;
                     log::trace!(" - B27移除记录: {} (RKS {:.4})", record_to_remove.song_id, record_to_remove.rks);
                 }
            }
        }
        log::trace!(" + 添加到B27: {} (RKS {:.4})", target_chart_id_full, simulated_chart_rks);
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
             song_name: "".to_string(), difficulty_value: 0.0, score: None, acc: 100.0,
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

// --- 服务层函数 ---

pub async fn generate_bn_image(
    n: u32, 
    identifier: IdentifierRequest, 
    phigros_service: web::Data<PhigrosService>,
    user_service: web::Data<UserService>,
    player_archive_service: web::Data<PlayerArchiveService>,
) -> Result<Vec<u8>, AppError> {
    // 获取 token (从QQ或直接使用token)
    let token = match (identifier.token, identifier.qq) {
        (Some(token), _) => token,
        (_, Some(qq)) => user_service.get_user_by_qq(&qq).await?.session_token,
        _ => return Err(AppError::BadRequest("必须提供token或QQ".to_string())),
    };
    
    // (优化后) 并行获取 RKS列表+存档 和 Profile
    let (rks_save_res, profile_res) = tokio::join!(
        phigros_service.get_rks(&token), // get_rks 现在返回 (RksResult, GameSave)
        phigros_service.get_profile(&token)
    );

    // 解包结果
    let (all_rks_result, save) = rks_save_res?;
    if all_rks_result.records.is_empty() {
        return Err(AppError::Other("用户无成绩记录，无法生成 B{} 图片".to_string()));
    }
    let all_scores = all_rks_result.records;

    // 获取 player_id (使用从 get_rks 返回的 save)
    let player_id = save.user.as_ref()
        .and_then(|u| u.get("objectId"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    // 处理 Profile 结果 (即使获取失败也继续，只是昵称为 None)
    let player_nickname = match profile_res {
        Ok(profile) => Some(profile.nickname),
        Err(e) => {
            log::warn!("(generate_bn_image) 获取用户 Profile 失败: {}, 将不显示昵称", e);
            None
        }
    };
    let player_name_for_archive = player_nickname.clone().unwrap_or_else(|| player_id.clone());

    // 从存档构建 FC Map
    let mut fc_map = HashMap::new();
    if let Some(game_record_map) = &save.game_record {
        for (song_id, difficulties) in game_record_map {
            for (diff_name, record) in difficulties {
                if let Some(true) = record.fc {
                    let key = format!("{}-{}", song_id, diff_name);
                    fc_map.insert(key, true);
                }
            }
        }
    }

    // 更新玩家存档 (异步执行)
    let archive_service_clone = player_archive_service.clone();
    let player_id_clone = player_id.clone();
    let player_name_clone = player_name_for_archive.clone();
    let scores_clone = all_scores.clone(); // all_scores is Vec<RksRecord>
    let fc_map_clone = fc_map.clone();

    tokio::spawn(async move {
        log::info!("[后台任务] (generate_bn_image) 开始为玩家 {} ({}) 更新数据库存档...", player_name_clone, player_id_clone);
        match archive_service_clone.update_player_scores_from_rks_records(
            &player_id_clone,
            &player_name_clone,
            &scores_clone,
            &fc_map_clone
        ).await {
            Ok(_) => log::info!("[后台任务] (generate_bn_image) 玩家 {} ({}) 数据库存档更新完成。", player_name_clone, player_id_clone),
            Err(e) => log::error!("[后台任务] (generate_bn_image) 更新玩家 {} ({}) 数据库存档失败: {}", player_name_clone, player_id_clone, e),
        }
    });

    // --- 以下为原有的图片生成逻辑 --- 

    // 确保记录已排序 (get_rks 应该返回已排序的，但再次确认)
    let mut sorted_scores = all_scores.clone();
    sorted_scores.sort_by(|a, b| b.rks.partial_cmp(&a.rks).unwrap_or(Ordering::Equal));
    
    // 使用新的函数计算玩家 RKS
    let (_exact_rks, rounded_rks) = calculate_player_rks_details(&sorted_scores); // _exact_rks is unused
    
    // 获取 Top N 成绩
    let top_n_scores = sorted_scores.iter()
        .take(n as usize)
        .cloned()
        .collect::<Vec<_>>();
    
    // 计算需要显示的统计信息
    
    // 1. AP Top 3
    let ap_scores_ranked = sorted_scores.iter()
        .filter(|s| s.acc == 100.0)
        .collect::<Vec<_>>();
    let ap_top_3_scores = ap_scores_ranked.iter().take(3).map(|&s| s.clone()).collect::<Vec<RksRecord>>();
    let ap_top_3_avg = if ap_top_3_scores.len() >= 3 {
        Some(ap_top_3_scores.iter().map(|s| s.rks).sum::<f64>() / 3.0)
    } else {
        None
    };
    
    // 2. Best 27 平均值 (这个计算仅用于 PlayerStats 的 best_27_avg 字段)
    let count_for_b27_display_avg = sorted_scores.len().min(27);
    let best_27_avg = if count_for_b27_display_avg > 0 {
        Some(sorted_scores.iter().take(count_for_b27_display_avg).map(|s| s.rks).sum::<f64>() / count_for_b27_display_avg as f64)
    } else {
        None
    };
    
    // 3. 为每个Top N谱面预计算推分ACC（使用新算法）
    let mut push_acc_map = HashMap::new();
    for score in top_n_scores.iter() {
        if score.acc < 100.0 && score.difficulty_value > 0.0 { // 只有未Phi且定数>0的谱面才需要计算推分
            let target_chart_id_full = format!("{}-{}", score.song_id, score.difficulty);
            if let Some(push_acc) = calculate_target_chart_push_acc(&target_chart_id_full, score.difficulty_value, &sorted_scores) {
                push_acc_map.insert(target_chart_id_full, push_acc);
            }
        }
    }
    
    // 构建统计数据结构
    let stats = PlayerStats {
        ap_top_3_avg, // AP Top 3 分数的平均值，供显示
        best_27_avg,  // B27 分数的平均值，供显示
        real_rks: Some(rounded_rks), // 使用新计算的四舍五入后的 RKS
        player_name: player_nickname, // Use optional nickname for display
        update_time: Utc::now(),
        n,
        ap_top_3_scores, // AP Top 3 具体成绩列表
    };
    
    // 将 SVG 生成和渲染都移到 blocking task 中
    let png_data = web::block(move || {
        // SVG 生成，返回 Result<String, AppError>
        let svg_string = image_renderer::generate_svg_string(&top_n_scores, &stats, Some(&push_acc_map))?;

        // 渲染 SVG 到 PNG，返回 Result<Vec<u8>, AppError>
        image_renderer::render_svg_to_png(svg_string)
    })
    .await
    // 处理 web::block 的 JoinError
    .map_err(|e| AppError::InternalError(format!("Blocking task join error: {}", e)))
    // 解开 Result<Result<Vec<u8>, AppError>, AppError> 为 Result<Vec<u8>, AppError>
    .and_then(|inner_result| inner_result)?;

    Ok(png_data)
}

// 新增：生成单曲成绩图片的服务逻辑
pub async fn generate_song_image_service(
    song_query: String,
    identifier: IdentifierRequest,
    phigros_service: web::Data<PhigrosService>,
    user_service: web::Data<UserService>,
    song_service: web::Data<SongService>,
    player_archive_service: web::Data<PlayerArchiveService>,
) -> Result<Vec<u8>, AppError> {
    // 1. 解析获取有效的 SessionToken
    let token = match (identifier.token, identifier.qq) {
        (Some(token), _) => token,
        (_, Some(qq)) => user_service.get_user_by_qq(&qq).await?.session_token,
        _ => return Err(AppError::BadRequest("必须提供token或QQ".to_string())),
    };

    // 2. 查询歌曲信息 (获取 ID, 名称)
    let song_info = song_service.search_song(&song_query)?;
    let song_id = song_info.id.clone();
    let song_name = song_info.song.clone();

    // (优化后) 并行获取 RKS列表+存档 和 Profile
    let (rks_save_res, profile_res) = tokio::join!(
        phigros_service.get_rks(&token), // get_rks 现在返回 (RksResult, GameSave)
        phigros_service.get_profile(&token)
    );

    // 解包结果
    let (all_rks_result, save) = rks_save_res?;
    let mut all_records = all_rks_result.records; // Use the fetched RKS records

    // 获取 player_id (使用从 get_rks 返回的 save)
    let player_id = save.user.as_ref()
        .and_then(|u| u.get("objectId"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    // 处理 Profile 结果
    let player_nickname = match profile_res {
        Ok(profile) => Some(profile.nickname),
        Err(e) => {
            log::warn!("(generate_song_image) 获取用户 Profile 失败: {}, 将不显示昵称", e);
            None
        }
    };
    let player_name_for_archive = player_nickname.clone().unwrap_or_else(|| player_id.clone());

    // 从存档构建 FC Map
    let mut fc_map = HashMap::new();
    if let Some(game_record_map) = &save.game_record {
        for (record_song_id, difficulties) in game_record_map {
            for (diff_name, record) in difficulties {
                if let Some(true) = record.fc {
                    let key = format!("{}-{}", record_song_id, diff_name);
                    fc_map.insert(key, true);
                }
            }
        }
    }

    // 更新玩家存档 (异步执行)
    let archive_service_clone = player_archive_service.clone();
    let player_id_clone = player_id.clone();
    let player_name_clone = player_name_for_archive.clone();
    let records_clone = all_records.clone(); // all_records is Vec<RksRecord>
    let fc_map_clone = fc_map.clone();

    tokio::spawn(async move {
        log::info!("[后台任务] (generate_song_image) 开始为玩家 {} ({}) 更新数据库存档...", player_name_clone, player_id_clone);
        match archive_service_clone.update_player_scores_from_rks_records(
            &player_id_clone,
            &player_name_clone,
            &records_clone, 
            &fc_map_clone
        ).await {
            Ok(_) => log::info!("[后台任务] (generate_song_image) 玩家 {} ({}) 数据库存档更新完成。", player_name_clone, player_id_clone),
            Err(e) => log::error!("[后台任务] (generate_song_image) 更新玩家 {} ({}) 数据库存档失败: {}", player_name_clone, player_id_clone, e),
        }
    });

    // --- 以下为原有的图片生成逻辑 --- 

    // 确保记录已排序 (推分计算需要)
    all_records.sort_by(|a, b| b.rks.partial_cmp(&a.rks).unwrap_or(Ordering::Equal));

    // 提取存档中的游戏记录 (用于显示单曲成绩)
    let game_record_map = match save.game_record {
        Some(gr) => gr,
        None => {
            log::warn!("用户存档中没有 game_record 数据，无法生成单曲图片");
            // Return an error or a placeholder image?
            return Err(AppError::Other("存档中无成绩记录，无法生成单曲图片".to_string()));
        }
    };
    let song_difficulties_from_save = game_record_map.get(&song_id).cloned().unwrap_or_default();

    // 准备存储各难度最终数据的 Map
    let mut difficulty_scores_map: HashMap<String, Option<SongDifficultyScore>> = HashMap::new();
    let difficulties = ["EZ", "HD", "IN", "AT"];

    // 获取歌曲的所有难度定数
    let difficulty_constants = match song_service.get_song_difficulty(&song_id) {
        Ok(diff_map) => diff_map,
        Err(_) => {
            log::warn!("无法获取歌曲 {} 的难度定数信息", song_id);
            // Return default or error?
            return Err(AppError::Other(format!("无法获取歌曲 {} 的难度定数信息", song_id)));
        }
    };

    for diff_key in difficulties {
        let difficulty_value_opt = match diff_key {
            "EZ" => difficulty_constants.ez,
            "HD" => difficulty_constants.hd,
            "IN" => difficulty_constants.inl, // 注意字段名是 inl
            "AT" => difficulty_constants.at,
            _ => None,
        };

        // 从存档中查找当前难度的记录
        let record_opt = song_difficulties_from_save.get(diff_key);

        let current_rks = record_opt.and_then(|r| r.rks);
        let current_acc = record_opt.and_then(|r| r.acc);
        let is_phi = current_acc.map_or(false, |acc| acc == 100.0);
        let score = record_opt.and_then(|r| r.score);
        let is_fc = record_opt.and_then(|r| r.fc); // 使用 and_then 解开一层 Option

        // 计算玩家总 RKS 推分 ACC (使用已排序的 all_records)
        let player_push_acc = match difficulty_value_opt {
            Some(dv) if dv > 0.0 && !is_phi => { // 只有定数>0且未Phi才计算
                let target_chart_id_full = format!("{}-{}", song_id, diff_key);
                calculate_target_chart_push_acc(&target_chart_id_full, dv, &all_records)
            }
            _ => Some(100.0), // 定数<=0 或已 Phi，视为无法推分或已满
        };

        let score_entry = SongDifficultyScore {
            score,
            acc: current_acc,
            rks: current_rks,
            difficulty_value: difficulty_value_opt,
            is_fc,
            is_phi: Some(is_phi),
            player_push_acc,
        };
        difficulty_scores_map.insert(diff_key.to_string(), Some(score_entry));
    }

    // 7. 准备渲染数据
    let illustration_path_png = PathBuf::from(cover_loader::COVERS_DIR).join("ill").join(format!("{}.png", song_id));
    let illustration_path_jpg = PathBuf::from(cover_loader::COVERS_DIR).join("ill").join(format!("{}.jpg", song_id));
    let illustration_path = if illustration_path_png.exists() {
        Some(illustration_path_png)
    } else if illustration_path_jpg.exists() {
        Some(illustration_path_jpg)
    } else {
        None
    };

    let render_data = SongRenderData {
        song_name,
        song_id,
        player_name: player_nickname,
        update_time: Utc::now(),
        difficulty_scores: difficulty_scores_map,
        illustration_path,
    };

    // 将 SVG 生成和渲染都移到 blocking task 中
    let png_data = web::block(move || {
        // SVG 生成，返回 Result<String, AppError>
        let svg_string = image_renderer::generate_song_svg_string(&render_data)?;
        // 渲染 SVG 到 PNG，返回 Result<Vec<u8>, AppError>
        image_renderer::render_svg_to_png(svg_string)
    })
    .await
    // 处理 web::block 的 JoinError
    .map_err(|e| AppError::InternalError(format!("Blocking task join error: {}", e)))
    // 解开 Result<Result<Vec<u8>, AppError>, AppError> 为 Result<Vec<u8>, AppError>
    .and_then(|inner_result| inner_result)?;

    Ok(png_data)
}

// --- 排行榜相关函数 ---

pub async fn generate_rks_leaderboard_image(
    limit: Option<usize>, // 显示多少名玩家，默认 20
    player_archive_service: web::Data<PlayerArchiveService>,
) -> Result<Vec<u8>, AppError> {
    // 确定要显示的玩家数量，默认 20，可以根据需要设置上限，例如 100
    let actual_limit = limit.unwrap_or(20).min(100); 
    log::info!("生成RKS排行榜图片，显示前{}名玩家", actual_limit);
    
    let top_players = player_archive_service.get_rks_ranking(actual_limit).await?;
    
    let render_data = LeaderboardRenderData {
        title: "RKS 排行榜".to_string(), // Add title field
        entries: top_players, // Use entries field
        display_count: actual_limit, // Add display_count field
        update_time: Utc::now(),
    };
    
    // 生成 SVG
    let svg_string = image_renderer::generate_leaderboard_svg_string(&render_data)?;
    
    // 渲染 PNG
    let png_data = web::block(move || image_renderer::render_svg_to_png(svg_string))
        .await
        .map_err(|e| AppError::InternalError(format!("Blocking task error for leaderboard: {}", e)))??;

    Ok(png_data)
} 