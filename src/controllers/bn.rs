use actix_web::{post, web, HttpResponse};
use serde::{Serialize, Deserialize};
use std::cmp;

use crate::models::{ApiResponse, RksRecord, IdentifierRequest};
use crate::services::phigros::PhigrosService;
use crate::services::user::UserService;
use crate::utils::error::{AppError, AppResult};
use crate::utils::save_parser::{check_session_token, calculate_b30};
use crate::utils::token_helper::resolve_token;

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
) -> AppResult<HttpResponse> {
    let n_param = path.into_inner();
    let n = cmp::max(n_param, 27) as usize; // 确保 N 至少为 27
    
    // 1. 解析获取有效的 SessionToken
    let token = resolve_token(&req, &user_service).await?;

    // 2. 检查会话令牌
    check_session_token(&token)?;

    // 3. 获取用户存档 (这个需要带难度信息)
    let save = phigros_service.get_save_with_difficulty(&token).await?;

    // 4. 计算真实的 B30 RKS
    let b30_result = calculate_b30(&save)?;
    let actual_rks = b30_result.overall_rks;

    // 5. 获取用于 BestN 的 RKS 记录列表 (注意：PhigrosService::get_rks 返回排序好的列表)
    let rks_result_for_best_n = phigros_service.get_rks(&token).await?;
    
    // 6. 计算 BestN (从排序好的 RKS 记录中取前 N 个)
    let best_n = if rks_result_for_best_n.records.len() > n {
        rks_result_for_best_n.records[0..n].to_vec()
    } else {
        rks_result_for_best_n.records.clone()
    };
    
    // 7. 从 GameSave 中提取所有 AP 记录
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