use actix_web::{get, HttpResponse, Responder};
use chrono::{DateTime, Utc};
use cron::Schedule;
use serde::Serialize;
use std::str::FromStr;
use utoipa::ToSchema;

use crate::config::CONFIG;

#[derive(Serialize, ToSchema)]
pub struct StatusResponse {
    pub status: String,
}

#[derive(Serialize, ToSchema)]
pub struct MaintenanceResponse {
    pub message: String,
}

#[utoipa::path(
    get,
    path = "/status",
    tag = "Status",
    responses(
        (status = 200, description = "服务正常运行", body = StatusResponse),
        (status = 503, description = "服务处于维护状态", body = MaintenanceResponse)
    )
)]
#[get("/status")]
pub async fn get_status() -> impl Responder {
    // 1. 检查手动维护模式
    if CONFIG.maintenance_mode {
        return HttpResponse::ServiceUnavailable().json(MaintenanceResponse {
            message: CONFIG.maintenance_message.clone(),
        });
    }

    // 2. 检查时间窗口维护模式
    if let (Some(start_str), Some(end_str)) =
        (&CONFIG.maintenance_start_time, &CONFIG.maintenance_end_time)
    {
        if let (Ok(start_time), Ok(end_time)) = (
            DateTime::parse_from_rfc3339(start_str).map(|dt| dt.with_timezone(&Utc)),
            DateTime::parse_from_rfc3339(end_str).map(|dt| dt.with_timezone(&Utc)),
        ) {
            let now = Utc::now();
            if now >= start_time && now <= end_time {
                return HttpResponse::ServiceUnavailable().json(MaintenanceResponse {
                    message: CONFIG.maintenance_message.clone(),
                });
            }
        }
    }

    // 3. 检查 Cron 表达式维护模式
    // 如果 cron 表达式设置了，则认为从上一个触发时间开始，到下一个触发时间结束，服务处于维护状态。
    if let Some(cron_str) = &CONFIG.maintenance_cron {
        if let Ok(schedule) = Schedule::from_str(cron_str) {
            let now = Utc::now();
            let mut upcoming_iter = schedule.upcoming(Utc);
            if let Some(next_event_time) = upcoming_iter.next() {
                // 如果当前时间已经超过了上一个计划事件时间，则进入维护。
                // 这意味着维护期是从上一个 cron 时间点开始，一直持续到下一个 cron 时间点。
                if now >= next_event_time - chrono::Duration::minutes(1) {
                    return HttpResponse::ServiceUnavailable().json(MaintenanceResponse {
                        message: CONFIG.maintenance_message.clone(),
                    });
                }
            }
        }
    }

    // 如果所有检查都通过，则服务正常
    HttpResponse::Ok().json(StatusResponse {
        status: "ok".to_string(),
    })
}
