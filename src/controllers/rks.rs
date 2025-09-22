use actix_web::{post, web, HttpResponse};
use log::debug;
use std::collections::HashMap;
use utoipa;

use crate::models::rks::{RksRecord, RksResult};
use crate::models::user::{ApiResponse, IdentifierRequest};
use crate::services::phigros::PhigrosService;
use crate::services::player_archive_service::PlayerArchiveService;
use crate::services::user::UserService;
use crate::utils::error::AppResult;
use crate::utils::save_parser::check_session_token;
use crate::utils::token_helper::resolve_token;
use tokio;

/// 计算并返回玩家的RKS及b19和r10成绩
///
/// 此接口会计算用户的RKS，并可选择性地将玩家的最新成绩存档到数据库中。
#[utoipa::path(
    post,
    path = "/rks",
    request_body = IdentifierRequest,
    responses(
        (status = 200, description = "成功计算RKS", body = ApiResponse<RksResult>)
    )
)]
#[post("/rks")]
pub async fn get_rks(
    req: web::Json<IdentifierRequest>,
    phigros_service: web::Data<PhigrosService>,
    user_service: web::Data<UserService>,
    player_archive_service: web::Data<PlayerArchiveService>,
) -> AppResult<HttpResponse> {
    let (rks_result, save, player_id, player_name) = if req.data_source.as_deref() == Some("external") {
        // 外部数据源：直接调用服务方法，不需要token验证
        phigros_service.get_rks_with_source(&req).await?
    } else {
        // 内部数据源：需要token验证
        let _token = resolve_token(&req, &user_service).await?;
        check_session_token(&_token)?;
        phigros_service.get_rks_with_source(&req).await?
    };

    let mut fc_map = HashMap::new();
    if let Some(game_record_map) = &save.game_record {
        for (song_id, difficulties) in game_record_map {
            for (diff_name, record) in difficulties {
                if let Some(true) = record.fc {
                    let key = format!("{song_id}-{diff_name}");
                    fc_map.insert(key, true);
                }
            }
        }
    }

    let archive_service_clone = player_archive_service.clone();
    let player_id_clone = player_id.clone();
    let player_name_clone = player_name.clone();
    let records_clone = rks_result.records.clone();
    let fc_map_clone = fc_map.clone();

    tokio::spawn(async move {
        log::info!("[后台任务] (get_rks) 开始为玩家 {player_name_clone} ({player_id_clone}) 更新数据库存档...");
        let is_external = req.data_source.as_deref() == Some("external");
        match archive_service_clone
            .update_player_scores_from_rks_records(
                &player_id_clone,
                &player_name_clone,
                &records_clone,
                &fc_map_clone,
                is_external,
            )
            .await
        {
            Ok(_) => log::info!("[后台任务] (get_rks) 玩家 {player_name_clone} ({player_id_clone}) 数据库存档更新完成。"),
            Err(e) => log::error!("[后台任务] (get_rks) 更新玩家 {player_name_clone} ({player_id_clone}) 数据库存档失败: {e}"),
        }
    });

    Ok(HttpResponse::Ok().json(ApiResponse {
        code: 200,
        status: "ok".to_string(),
        message: None,
        data: Some(rks_result),
    }))
}

/// 获取玩家最好的N项成绩
///
/// 根据计算出的RKS，返回玩家分数最高的N条记录。
#[utoipa::path(
    post,
    path = "/bn/{n}",
    params(
        ("n" = u32, Path, description = "要获取的最高成绩数量")
    ),
    request_body = IdentifierRequest,
    responses(
        (status = 200, description = "成功获取B<n>成绩", body = ApiResponse<Vec<RksRecord>>),
        (status = 400, description = "无效的n值")
    )
)]
#[post("/bn/{n}")]
pub async fn get_bn(
    n: web::Path<u32>,
    req: web::Json<IdentifierRequest>,
    phigros_service: web::Data<PhigrosService>,
    user_service: web::Data<UserService>,
) -> AppResult<HttpResponse> {
    let n = n.into_inner();
    debug!("接收到B{n}查询请求");

    if n == 0 {
        return Ok(HttpResponse::Ok().json(ApiResponse {
            code: 400,
            status: "ERROR".to_string(),
            message: Some("参数n必须大于0".to_string()),
            data: None::<Vec<()>>,
        }));
    }

    let (rks_result, _, _, _) = if req.data_source.as_deref() == Some("external") {
        // 外部数据源：直接调用服务方法，不需要token验证
        phigros_service.get_rks_with_source(&req).await?
    } else {
        // 内部数据源：需要token验证
        let _token = resolve_token(&req, &user_service).await?;
        phigros_service.get_rks_with_source(&req).await?
    };

    let bn = rks_result
        .records
        .into_iter()
        .take(n as usize)
        .collect::<Vec<_>>();

    Ok(HttpResponse::Ok().json(ApiResponse {
        code: 200,
        status: "OK".to_string(),
        message: None,
        data: Some(bn),
    }))
}
