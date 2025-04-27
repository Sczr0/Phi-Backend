use crate::models::user::IdentifierRequest;
use crate::models::rks::RksRecord;
use crate::services::phigros::PhigrosService;
use crate::services::user::UserService;
use crate::utils::error::AppError;
use crate::utils::image_renderer::{self, PlayerStats};
use chrono::Utc;
use actix_web::web;
use std::cmp::Ordering;
use itertools::Itertools;
use tokio;
use crate::models::user::UserProfile;

pub async fn generate_bn_image(
    n: u32, 
    identifier: IdentifierRequest, 
    phigros_service: web::Data<PhigrosService>,
    user_service: web::Data<UserService>
) -> Result<Vec<u8>, AppError> {
    // 获取足够多的成绩数据 (至少 n，可能需要更多来计算 AP Top3)
    // let required_count = n.max(30); 
    
    // 获取 token (从QQ或直接使用token)
    let token = match (identifier.token, identifier.qq) {
        (Some(token), _) => token,
        (_, Some(qq)) => {
            // 使用 UserService 获取 token
            user_service.get_user_by_qq(&qq).await?.session_token
        },
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
    let all_scores = &all_rks_result.records;

    // 处理 Profile 结果 (即使获取失败也继续，只是昵称为 None)
    let player_nickname = match profile_result {
        Ok(profile) => Some(profile.nickname),
        Err(e) => {
            log::warn!("获取用户 Profile 失败: {}, 将不显示昵称", e);
            None
        }
    };
    
    // 获取 Top N 成绩
    let top_n_scores = all_scores.iter()
        .take(n as usize)
        .cloned()
        .collect::<Vec<_>>();
    
    // 计算需要显示的统计信息
    
    // 1. AP Top 3 (恢复平均值计算)
    let ap_scores_ranked = all_scores.iter()
        .filter(|s| s.acc == 100.0)
        .sorted_by(|a, b| b.rks.partial_cmp(&a.rks).unwrap_or(Ordering::Equal))
        .collect::<Vec<_>>();
    let ap_top_3_scores = ap_scores_ranked.iter().take(3).map(|&s| s.clone()).collect::<Vec<RksRecord>>();
    let ap_top_3_avg = if ap_top_3_scores.len() >= 3 { // 恢复计算
        Some(ap_top_3_scores.iter().map(|s| s.rks).sum::<f64>() / 3.0)
    } else {
        None
    };
    
    // 2. Best 27 平均值 (这个计算仅用于 PlayerStats 的 best_27_avg 字段，与 real_rks 计算分开)
    let count_for_b27_display_avg = all_scores.len().min(27);
    let best_27_avg = if count_for_b27_display_avg > 0 {
        Some(all_scores.iter().take(count_for_b27_display_avg).map(|s| s.rks).sum::<f64>() / count_for_b27_display_avg as f64)
    } else {
        None
    };
    
    // ---- 修改 Real RKS 计算逻辑 ----
    // 3. Real RKS (B27 + AT3, 允许重复)
    
    // Calculate B27 sum
    let b27_rks_sum: f64 = all_scores.iter()
        .take(27) // 取最高的 27 个
        .map(|s| s.rks)
        .sum();

    // Calculate AT3 sum (ap_scores_ranked 已经按 RKS 排序)
    let at3_rks_sum: f64 = ap_scores_ranked.iter() // 使用之前计算好的 ap_scores_ranked
        .take(3) // 取最高的 3 个 AP 成绩
        .map(|s| s.rks)
        .sum();

    // Calculate final Real RKS
    let total_rks_sum = b27_rks_sum + at3_rks_sum;
    let real_rks = if total_rks_sum > 0.0 { 
        // 总是除以 30，根据描述 "共30个谱面rks值的平均值"
        Some(total_rks_sum / 30.0) 
    } else {
        None // 如果没有任何分数，则为 None
    };
    // ---- 修改结束 ----
    
    // 构建统计数据结构
    let stats = PlayerStats {
        ap_top_3_avg, // 这个字段现在代表的是 AP Top 3 分数本身的平均值，供显示
        best_27_avg, // 这个字段现在代表的是 B27 分数本身的平均值，供显示
        real_rks,    // 这个是最终计算出的 B27+AT3 平均值
        player_name: player_nickname,
        update_time: Utc::now(),
        n,
        ap_top_3_scores, // AP Top 3 具体成绩列表保持不变
    };
    
    // 1. 生成 SVG 字符串 (同步任务，可以在当前线程执行)
    let svg_string = image_renderer::generate_svg_string(&top_n_scores, &stats)?;

    // 2. 将 SVG 渲染为 PNG (CPU密集型，移到阻塞线程)
    let png_data = web::block(move || image_renderer::render_svg_to_png(svg_string))
        .await
        .map_err(|e| AppError::InternalError(format!("Blocking task error: {}", e)))??; // 双 ?? 处理 BlockingError 和内部的 AppError

    Ok(png_data)
} 