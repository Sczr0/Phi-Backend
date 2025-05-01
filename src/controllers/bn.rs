use actix_web::{post, web, HttpResponse};
use serde::{Serialize, Deserialize};
use std::cmp;
use std::collections::HashMap;

use crate::models::{ApiResponse, RksRecord, IdentifierRequest};
use crate::services::phigros::PhigrosService;
use crate::services::user::UserService;
use crate::services::player_archive_service::PlayerArchiveService;
use crate::utils::error::{AppError, AppResult};
use crate::utils::save_parser::{check_session_token, calculate_b30};
use crate::utils::token_helper::resolve_token;
use tokio;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BnResult {
    pub rks: Option<f64>,
    pub ap_best3: Vec<RksRecord>,
    pub best_n: Vec<RksRecord>,
}

// POST /bn/{n}
#[post("/bn/{n}")]
pub async fn get_bn(
    req: web::Json<IdentifierRequest>,
    path: web::Path<u32>,
    phigros_service: web::Data<PhigrosService>,
    user_service: web::Data<UserService>,
    player_archive_service: web::Data<PlayerArchiveService>,
) -> AppResult<HttpResponse> {
    let n_param = path.into_inner();
    let n = cmp::max(n_param, 27) as usize; // 确保 N 至少为 27
    
    // 1. 解析获取有效的 SessionToken
    let token = resolve_token(&req, &user_service).await?;

    // 2. 检查会话令牌
    check_session_token(&token)?;

    // (优化后) 并行获取 RKS列表+存档 和 Profile
    let (rks_save_res, profile_res) = tokio::join!(
        phigros_service.get_rks(&token), // get_rks 现在返回 (RksResult, GameSave)
        phigros_service.get_profile(&token)
    );

    // 解包结果
    let (rks_result, save) = rks_save_res?;

    // 获取玩家ID和昵称 (使用从 get_rks 返回的 save)
    let player_id = save.user.as_ref()
        .and_then(|u| u.get("objectId"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let player_name = match profile_res {
        Ok(profile) => profile.nickname,
        Err(e) => {
            log::warn!("获取用户 Profile 失败 (get_bn): {}, 将使用 Player ID 作为名称", e);
            player_id.clone()
        }
    };

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

    // 更新数据库中的玩家存档和 RKS (异步执行)
    let archive_service_clone = player_archive_service.clone();
    let player_id_clone = player_id.clone();
    let player_name_clone = player_name.clone();
    let records_clone = rks_result.records.clone(); // Use the full rks_result for update
    let fc_map_clone = fc_map.clone();

    tokio::spawn(async move {
        log::info!("[后台任务] (get_bn) 开始为玩家 {} ({}) 更新数据库存档...", player_name_clone, player_id_clone);
        match archive_service_clone.update_player_scores_from_rks_records(
            &player_id_clone,
            &player_name_clone,
            &records_clone,
            &fc_map_clone
        ).await {
            Ok(_) => log::info!("[后台任务] (get_bn) 玩家 {} ({}) 数据库存档更新完成。", player_name_clone, player_id_clone),
            Err(e) => log::error!("[后台任务] (get_bn) 更新玩家 {} ({}) 数据库存档失败: {}", player_name_clone, player_id_clone, e),
        }
    });

    // 4. 计算真实的 B30 RKS (使用从 get_rks 返回的 save)
    let b30_result = calculate_b30(&save)?;
    let actual_rks = b30_result.overall_rks;

    // 5. 获取用于 BestN 的 RKS 记录列表 (使用从 get_rks 返回的 rks_result)
    let rks_result_for_best_n = rks_result;
    
    // 6. 计算 BestN
    let best_n = if rks_result_for_best_n.records.len() > n {
        rks_result_for_best_n.records[0..n].to_vec()
    } else {
        rks_result_for_best_n.records.clone()
    };
    
    // 7. 从 GameSave 中提取所有 AP 记录 (使用从 get_rks 返回的 save)
    let game_record = save.game_record.as_ref()
        .ok_or_else(|| AppError::Other("没有游戏记录数据".to_string()))?;
    
    let mut ap_records = Vec::new();
    
    // 遍历所有歌曲记录，找出 AP 记录
    for (song_id, difficulties) in game_record {
        let song_name = crate::utils::data_loader::get_song_name_by_id(song_id)
            .unwrap_or_else(|| song_id.clone());
        
        // 遍历所有难度记录
        for (diff_name, record) in difficulties {
            // 检查是否为 AP（分数为 1,000,000）
            if let Some(score) = record.score {
                if score == 1_000_000.0 && record.difficulty.is_some() && record.acc.is_some() {
                    let rks_record = RksRecord::new(
                        song_id.clone(),
                        song_name.clone(),
                        diff_name.clone(),
                        record.difficulty.unwrap(),
                        record,
                    );
                    ap_records.push(rks_record);
                }
            }
        }
    }
    
    // 按 RKS 降序排序
    ap_records.sort_by(|a, b| b.rks.partial_cmp(&a.rks).unwrap_or(std::cmp::Ordering::Equal));
    
    // 取前 3 个 AP 记录
    let ap_best3 = if ap_records.len() > 3 {
        ap_records[0..3].to_vec()
    } else {
        ap_records
    };
    
    // 8. 构建响应 (使用 actual_rks)
    let result = BnResult {
        rks: Some(actual_rks),
        ap_best3,
        best_n,
    };

    Ok(HttpResponse::Ok().json(ApiResponse {
        code: 200,
        status: "ok".to_string(),
        message: None,
        data: Some(result),
    }))
} 