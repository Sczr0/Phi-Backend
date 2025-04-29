use crate::models::user::IdentifierRequest;
use crate::models::rks::RksRecord;
use crate::services::phigros::PhigrosService;
use crate::services::user::UserService;
use crate::services::song::SongService;
use crate::utils::error::AppError;
use crate::utils::image_renderer::{self, PlayerStats, SongRenderData, SongDifficultyScore};
use crate::utils::token_helper::resolve_token;
use crate::utils::cover_loader;
use chrono::Utc;
use actix_web::web;
use std::cmp::Ordering;
use std::collections::HashMap;
use itertools::Itertools;
use tokio;
use crate::models::user::UserProfile;
use std::path::PathBuf;

// --- RKS 计算辅助函数 ---

/// 根据已排序的 RKS 记录列表计算玩家的精确 RKS 和四舍五入后的 RKS。
/// records: 必须是按 RKS 降序排序的 RksRecord 列表。
pub fn calculate_player_rks_details(records: &[RksRecord]) -> (f64, f64) {
    log::debug!("开始计算玩家RKS详情，总成绩数: {}", records.len());

    // 如果记录为空，返回0
    if records.is_empty() {
        log::debug!("无成绩记录，RKS = 0");
        return (0.0, 0.0);
    }

    // 确保records已经是按RKS降序排列的
    let mut sorted_records = records.to_vec();
    sorted_records.sort_by(|a, b| b.rks.partial_cmp(&a.rks).unwrap_or(std::cmp::Ordering::Equal));

    // 计算Best 27
    let best_27_count = sorted_records.len().min(27);
    let best_27_sum: f64 = sorted_records.iter().take(best_27_count).map(|r| r.rks).sum();
    let best_27_avg = if best_27_count > 0 {
        best_27_sum / best_27_count as f64
    } else {
        0.0
    };
    log::debug!("Best 27 计算: 使用了前{}个成绩，总和={:.4}，平均={:.4}", 
                best_27_count, best_27_sum, best_27_avg);

    // 计算AP Top 3
    let ap_records: Vec<&RksRecord> = sorted_records.iter()
        .filter(|r| r.acc >= 100.0)
        .collect();
    let ap_count = ap_records.len().min(3);
    let ap_sum: f64 = ap_records.iter().take(ap_count).map(|r| r.rks).sum();
    let ap_avg = if ap_count > 0 {
        ap_sum / ap_count as f64
    } else {
        0.0
    };
    log::debug!("AP Top 3 计算: AP成绩数={}, 使用前{}个，总和={:.4}，平均={:.4}", 
                ap_records.len(), ap_count, ap_sum, ap_avg);

    // 根据AP数量计算最终RKS
    let final_rks = match ap_count {
        0 => best_27_avg, // 无AP成绩
        1 => best_27_avg * 5.0 / 6.0 + ap_avg / 6.0, // 1个AP成绩
        2 => best_27_avg * 2.0 / 3.0 + ap_avg / 3.0, // 2个AP成绩
        _ => best_27_avg * 0.5 + ap_avg * 0.5, // 3个或更多AP成绩
    };
    
    log::debug!("最终RKS计算: AP数量={}, 最终精确RKS={:.4}, 四舍五入后={:.2}", 
                ap_count, final_rks, (final_rks * 100.0).round() / 100.0);

    (final_rks, (final_rks * 100.0).round() / 100.0)
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

/// 计算将指定谱面提升到某个 ACC 后，玩家总 RKS (四舍五入后) 能否增加 0.01。
/// target_chart_id_full: 要计算的谱面 ID (例如 "song_id-IN")
/// target_chart_constant: 该谱面的定数
/// all_sorted_records: 玩家所有谱面的记录，按 RKS 降序排序
/// target_rks_threshold: 玩家总 RKS 需要达到的精确 RKS 阈值 (current_rounded_rks + 0.005)
pub fn check_rks_increase(
    target_chart_id_full: &str, 
    target_chart_constant: f64, 
    test_acc: f64, 
    all_sorted_records: &[RksRecord], 
    target_rks_threshold: f64
) -> bool 
{
    log::debug!("检查RKS增长: 目标谱面={}, 测试ACC={:.4}%, 定数={:.1}, 目标RKS阈值={:.4}", 
                target_chart_id_full, test_acc, target_chart_constant, target_rks_threshold);

    // 分离song_id和difficulty
    let parts: Vec<&str> = target_chart_id_full.rsplitn(2, '-').collect();
    if parts.len() != 2 {
        log::debug!("谱面ID格式错误: {}", target_chart_id_full);
        return false;
    }
    let song_id = parts[1];
    let difficulty = parts[0];
    
    // 克隆记录以便模拟修改
    let mut test_records = all_sorted_records.to_vec();
    
    // 查找并更新或插入测试记录
    let test_rks = calculate_chart_rks(test_acc, target_chart_constant);
    
    // 记录修改前的信息
    let (old_exact_rks, _) = calculate_player_rks_details(&test_records);
    log::debug!("修改前: 玩家RKS={:.4}", old_exact_rks);
    
    // 查找目标谱面是否存在记录
    let mut found = false;
    let mut original_index = None;
    let mut original_record = None;
    
    for (i, record) in test_records.iter_mut().enumerate() {
        if record.song_id == song_id && record.difficulty == difficulty {
            original_record = Some(record.clone());
            original_index = Some(i);
            // 更新现有记录的ACC和RKS
            record.acc = test_acc;
            record.rks = test_rks;
            found = true;
            log::debug!("更新现有记录: 谱面={}, 原ACC={:.2}%, 新ACC={:.2}%, 新RKS={:.4}", 
                      target_chart_id_full, original_record.as_ref().unwrap().acc, test_acc, test_rks);
            break;
        }
    }
    
    if !found {
        // 插入新记录
        let new_record = RksRecord {
            song_id: song_id.to_string(),
            song_name: format!("Song {}", song_id), // 临时名称
            difficulty: difficulty.to_string(),
            difficulty_value: target_chart_constant,
            score: Some(test_acc * 10000.0), // 近似
            acc: test_acc,
            rks: test_rks,
        };
        test_records.push(new_record);
        log::debug!("插入新记录: 谱面={}, ACC={:.2}%, RKS={:.4}", 
                  target_chart_id_full, test_acc, test_rks);
    }
    
    // 按RKS重新排序
    test_records.sort_by(|a, b| b.rks.partial_cmp(&a.rks).unwrap_or(std::cmp::Ordering::Equal));
    
    // 记录排序后的前30条记录
    if log::log_enabled!(log::Level::Debug) {
        log::debug!("排序后Top记录:");
        for (i, r) in test_records.iter().take(30).enumerate() {
            log::debug!("  #{}: {}={}-{} ACC={:.2}% RKS={:.4}", 
                      i+1, r.song_name, r.song_id, r.difficulty, r.acc, r.rks);
        }
    }
    
    // 检查原记录是否在B27中
    if let Some(idx) = original_index {
        let original_in_b27 = idx < test_records.len().min(27);
        log::debug!("原记录在B27中: {}", original_in_b27);
    }
    
    // 检查修改后记录是否在B27中
    let mut modified_in_b27 = false;
    let mut modified_in_ap3 = false;
    for (i, r) in test_records.iter().enumerate() {
        if r.song_id == song_id && r.difficulty == difficulty {
            modified_in_b27 = i < test_records.len().min(27);
            modified_in_ap3 = r.acc >= 100.0 && i < 3;
            log::debug!("修改后记录位置: #{}, 在B27中: {}, AP且在前3: {}", 
                      i+1, modified_in_b27, modified_in_ap3);
            break;
        }
    }
    
    // 计算新的精确RKS
    let (new_exact_rks, _) = calculate_player_rks_details(&test_records);
    log::debug!("修改后: 玩家RKS={:.4}, 是否达到目标={}, 增长量={:.4}", 
              new_exact_rks, new_exact_rks >= target_rks_threshold, new_exact_rks - old_exact_rks);
    
    // 判断是否达到目标RKS阈值
    new_exact_rks >= target_rks_threshold
}

/// 计算指定谱面需要达到多少 ACC 才能使玩家总 RKS (四舍五入后) 增加 0.01
/// 返回 Option<f64>，None 表示无法推分或已达上限
pub fn calculate_target_chart_push_acc(
    target_chart_id_full: &str, 
    target_chart_constant: f64, 
    all_sorted_records: &Vec<RksRecord>
) -> Option<f64> 
{
    log::debug!("开始计算推分ACC: 目标谱面={}, 定数={:.1}", 
                target_chart_id_full, target_chart_constant);
    
    // 获取当前玩家 RKS 状态
    let (current_exact_rks, current_rounded_rks) = calculate_player_rks_details(all_sorted_records);
    log::debug!("当前玩家RKS: 精确值={:.4}, 四舍五入={:.2}", 
                current_exact_rks, current_rounded_rks);
    
    // 计算目标精确 RKS 阈值
    // 1.考虑计算的rks后的四位或者更多位小数
    // 2.如果当前rks小数点后第3位小数小于5，则计算目标rks为xx.xx5及以上
    // 3.如果大于等于5，则计算目标rks为xx.xx+0.015
    let target_rks_threshold = {
        // 获取小数点后第三位的值
        let third_decimal = ((current_exact_rks * 1000.0) % 10.0) as i32;
        
        let threshold = if third_decimal < 5 {
            // 例如：15.352 -> 目标为 15.355
            (current_exact_rks * 100.0).floor() / 100.0 + 0.005
        } else {
            // 例如：15.357 -> 目标为 15.365
            (current_exact_rks * 100.0).floor() / 100.0 + 0.015
        };
        log::debug!("目标RKS阈值计算: 当前RKS={:.4}, 第三位小数={}, 目标阈值={:.4}", 
                   current_exact_rks, third_decimal, threshold);
        threshold
    };

    // 边界检查 1: 当前 RKS 是否已达标?
    if current_exact_rks >= target_rks_threshold {
        log::debug!("推分计算: 玩家 RKS ({:.4}) 已达到或超过目标阈值 ({:.4})，无需推分 {}", 
                   current_exact_rks, target_rks_threshold, target_chart_id_full);
        return Some(100.0); // 返回 100.0 代表无需推分或已满
    }

    // 分离 song_id 和 difficulty
    let parts: Vec<&str> = target_chart_id_full.rsplitn(2, '-').collect();
    if parts.len() != 2 { 
        log::debug!("谱面ID格式错误: {}", target_chart_id_full);
        return None; 
    } // 格式错误
    
    let song_id = parts[1];
    let difficulty = parts[0];
    
    // 获取当前谱面的 ACC (如果存在)
    let current_acc = all_sorted_records.iter()
        .find(|r| r.song_id == song_id && r.difficulty == difficulty)
        .map_or(70.0, |r| r.acc.max(70.0)); // 如果没打过，从 70 开始；否则从当前 ACC 开始
    log::debug!("当前谱面ACC: {}", current_acc);

    // 边界检查 2: 检查是否可以通过提高当前成绩的ACC到100%来达成推分
    if !check_rks_increase(target_chart_id_full, target_chart_constant, 100.0, all_sorted_records, target_rks_threshold) {
        log::debug!("通过提高ACC到100%无法达到目标RKS，检查是否通过AP Best 3影响总RKS");
        
        // 检查是否可以通过新增或替换 AP Best 3 来推分
        let mut ap_test_records = all_sorted_records.clone();
        
        // 查找并更新或插入 AP 记录
        let mut found = false;
        for record in ap_test_records.iter_mut() {
            if record.song_id == song_id && record.difficulty == difficulty {
                // 更新现有记录为 AP
                let old_acc = record.acc;
                let old_rks = record.rks;
                record.acc = 100.0;
                record.rks = calculate_chart_rks(100.0, target_chart_constant);
                found = true;
                log::debug!("更新记录为AP: 谱面={}, 原ACC={:.2}%, 原RKS={:.4}, 新RKS={:.4}", 
                           target_chart_id_full, old_acc, old_rks, record.rks);
                break;
            }
        }
        
        if !found {
            // 插入新的 AP 记录
            let new_record = RksRecord {
                song_id: song_id.to_string(),
                song_name: song_id.to_string(), // 临时替代
                difficulty: difficulty.to_string(),
                difficulty_value: target_chart_constant,
                score: Some(1_000_000.0),
                acc: 100.0,
                rks: calculate_chart_rks(100.0, target_chart_constant),
            };
            log::debug!("插入新AP记录: 谱面={}, RKS={:.4}", 
                       target_chart_id_full, new_record.rks);
            ap_test_records.push(new_record);
        }
        
        // 输出AP Top 3情况
        if log::log_enabled!(log::Level::Debug) {
            let ap_records: Vec<&RksRecord> = all_sorted_records.iter()
                .filter(|r| r.acc >= 100.0)
                .collect();
            log::debug!("原AP记录数量: {}", ap_records.len());
            if !ap_records.is_empty() {
                for (i, r) in ap_records.iter().enumerate().take(3) {
                    log::debug!("  原AP Top #{}: {}={}-{} RKS={:.4}", 
                              i+1, r.song_name, r.song_id, r.difficulty, r.rks);
                }
            }
        }
        
        // 重新排序并计算新的 RKS
        ap_test_records.sort_by(|a, b| b.rks.partial_cmp(&a.rks).unwrap_or(std::cmp::Ordering::Equal));
        
        // 输出新AP Top 3情况
        if log::log_enabled!(log::Level::Debug) {
            let ap_records: Vec<&RksRecord> = ap_test_records.iter()
                .filter(|r| r.acc >= 100.0)
                .collect();
            log::debug!("新AP记录数量: {}", ap_records.len());
            if !ap_records.is_empty() {
                for (i, r) in ap_records.iter().enumerate().take(3) {
                    log::debug!("  新AP Top #{}: {}={}-{} RKS={:.4}", 
                              i+1, r.song_name, r.song_id, r.difficulty, r.rks);
                }
            }
        }
        
        let (new_exact_rks, _) = calculate_player_rks_details(&ap_test_records);
        
        // 如果 AP 也无法达到目标 RKS，则无法通过此谱面推分
        if new_exact_rks < target_rks_threshold {
            log::debug!("推分计算: {} AP也无法达到目标RKS阈值 ({:.4}), AP后RKS={:.4}", 
                       target_chart_id_full, target_rks_threshold, new_exact_rks);
            return Some(100.0); // 返回 100.0 代表无法通过此谱面推分
        }
        log::debug!("AP能达到目标RKS，AP后RKS={:.4} >= 目标阈值={:.4}", 
                   new_exact_rks, target_rks_threshold);
    }

    // 二分查找最小达标ACC
    let mut low = current_acc;
    let mut high = 100.0;
    log::debug!("开始二分查找推分ACC for {}, 区间: [{:.4}, {:.4}]", 
              target_chart_id_full, low, high);

    for i in 0..100 { // 迭代次数可以调整，100 次精度足够高
        if high - low < 1e-5 { // 精度足够时退出
            log::debug!("二分查找达到精度要求，迭代{}次后退出", i);
            break;
        }
        let mid = low + (high - low) / 2.0;
        let check_result = check_rks_increase(target_chart_id_full, target_chart_constant, mid, all_sorted_records, target_rks_threshold);
        if check_result {
            high = mid; // mid 满足条件，尝试更低的 acc
            log::debug!("迭代#{}: mid={:.4}满足条件，新区间[{:.4}, {:.4}]", 
                      i, mid, low, high);
        } else {
            low = mid; // mid 不满足条件，需要更高的 acc
            log::debug!("迭代#{}: mid={:.4}不满足条件，新区间[{:.4}, {:.4}]", 
                      i, mid, low, high);
        }
    }
    
    log::debug!("二分查找结束 for {}, 结果 high = {:.6}", target_chart_id_full, high);

    // 处理结果：
    // 1. 先确保不低于 70%
    let result_acc = high.max(70.0);
    
    // 2. 向上取整到两位小数
    let rounded_to_2_decimals = (result_acc * 100.0).ceil() / 100.0;
    
    // 3. 如果取整后与当前 ACC 相同，则向上取整到三位小数
    let final_acc = if (rounded_to_2_decimals - current_acc).abs() < 1e-5 {
        let acc_3_decimals = (result_acc * 1000.0).ceil() / 1000.0;
        log::debug!("取整后与当前ACC相同，改为三位小数: {:.2} -> {:.3}", 
                   rounded_to_2_decimals, acc_3_decimals);
        acc_3_decimals
    } else {
        log::debug!("取整后ACC: {:.2}", rounded_to_2_decimals);
        rounded_to_2_decimals
    };

    // 最终约束，避免超过 100
    let constrained_acc = final_acc.min(100.0);
    log::debug!("最终推分ACC结果: {:.4}%", constrained_acc);
    
    Some(constrained_acc)
}

// --- 服务层函数 ---

pub async fn generate_bn_image(
    n: u32, 
    identifier: IdentifierRequest, 
    phigros_service: web::Data<PhigrosService>,
    user_service: web::Data<UserService>
) -> Result<Vec<u8>, AppError> {
    // 获取 token (从QQ或直接使用token)
    let token = match (identifier.token, identifier.qq) {
        (Some(token), _) => token,
        (_, Some(qq)) => user_service.get_user_by_qq(&qq).await?.session_token,
        _ => return Err(AppError::BadRequest("必须提供token或QQ".to_string())),
    };
    
    // 并行获取 RKS 和 Profile
    let (rks_result, profile_result) = tokio::join!(
        phigros_service.get_rks(&token),
        phigros_service.get_profile(&token)
    );

    // 处理 RKS 结果
    let all_rks_result = rks_result?;
    if all_rks_result.records.is_empty() {
        return Err(AppError::Other("No scores found for this user".to_string()));
    }
    let all_scores = all_rks_result.records;

    // 确保记录已排序 (get_rks 应该返回已排序的，但再次确认)
    let mut sorted_scores = all_scores.clone();
    sorted_scores.sort_by(|a, b| b.rks.partial_cmp(&a.rks).unwrap_or(Ordering::Equal));

    // 处理 Profile 结果 (即使获取失败也继续，只是昵称为 None)
    let player_nickname = match profile_result {
        Ok(profile) => Some(profile.nickname),
        Err(e) => {
            log::warn!("获取用户 Profile 失败: {}, 将不显示昵称", e);
            None
        }
    };
    
    // 使用新的函数计算玩家 RKS
    let (exact_rks, rounded_rks) = calculate_player_rks_details(&sorted_scores);
    
    // 获取 Top N 成绩
    let top_n_scores = sorted_scores.iter()
        .take(n as usize)
        .cloned()
        .collect::<Vec<_>>();
    
    // 计算需要显示的统计信息
    
    // 1. AP Top 3
    let ap_scores_ranked = sorted_scores.iter()
        .filter(|s| s.acc == 100.0)
        .sorted_by(|a, b| b.rks.partial_cmp(&a.rks).unwrap_or(Ordering::Equal))
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
        player_name: player_nickname,
        update_time: Utc::now(),
        n,
        ap_top_3_scores, // AP Top 3 具体成绩列表
    };
    
    // 1. 生成 SVG 字符串
    let svg_string = image_renderer::generate_svg_string(&top_n_scores, &stats, Some(&push_acc_map))?;

    // 2. 将 SVG 渲染为 PNG
    let png_data = web::block(move || image_renderer::render_svg_to_png(svg_string))
        .await
        .map_err(|e| AppError::InternalError(format!("Blocking task error: {}", e)))??;

    Ok(png_data)
}

// 新增：生成单曲成绩图片的服务逻辑
pub async fn generate_song_image_service(
    song_query: String,
    identifier: IdentifierRequest,
    phigros_service: web::Data<PhigrosService>,
    user_service: web::Data<UserService>,
    song_service: web::Data<SongService>,
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

    // 3. 并行获取存档和 RKS 列表
    let (save_result, rks_result, profile_result) = tokio::join!(
        phigros_service.get_save_with_difficulty(&token),
        phigros_service.get_rks(&token),
        phigros_service.get_profile(&token)
    );

    // 处理存档结果
    let save = match save_result {
        Ok(s) => s,
        Err(e) => {
            // 简化错误处理
            return Err(e);
        }
    };

    // 处理 RKS 结果 (必须获取成功才能计算推分)
    let mut all_records = match rks_result {
        Ok(res) => res.records,
        Err(e) => {
            log::error!("获取用户 RKS 失败，无法计算推分: {}", e);
            // 如果无法获取 RKS，则不能计算推分，返回错误或一个不含推分信息的图片?
            // 这里选择返回错误
            return Err(AppError::Other("无法获取 RKS 记录，无法计算推分".to_string()));
        }
    };
    // 确保记录已排序 (get_rks 应该返回已排序的，但再次确认)
    all_records.sort_by(|a, b| b.rks.partial_cmp(&a.rks).unwrap_or(Ordering::Equal));

    // 处理 Profile 结果
    let player_name = match profile_result {
        Ok(profile) => Some(profile.nickname),
        Err(e) => {
            log::warn!("获取用户 Profile 失败: {}, 将不显示昵称", e);
            None
        }
    };
    
    // 提取存档中的游戏记录
    let game_record_map = match save.game_record {
        Some(gr) => gr,
        None => {
            log::warn!("用户存档中没有 game_record 数据");
            HashMap::new() // 返回空的 map
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
            // 创建一个空的或者默认的难度常量结构体?
            crate::models::SongDifficulty { id: song_id.clone(), ez: None, hd: None, inl: None, at: None }
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

        // 计算玩家总 RKS 推分 ACC
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
            player_push_acc, // 使用新计算的值
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
        player_name,
        update_time: Utc::now(),
        difficulty_scores: difficulty_scores_map,
        illustration_path,
    };

    // 8. 生成 SVG 字符串
    let svg_string = image_renderer::generate_song_svg_string(&render_data)?;

    // 9. 将 SVG 渲染为 PNG
    let png_data = web::block(move || image_renderer::render_svg_to_png(svg_string))
        .await
        .map_err(|e| AppError::InternalError(format!("Blocking task error: {}", e)))??;

    Ok(png_data)
} 