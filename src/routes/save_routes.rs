use actix_web::web;
use crate::controllers::save_controller;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/get")
            .service(
                web::scope("/cloud")
                    .route("/saves", web::post().to(save_controller::get_cloud_saves))
                    .route("/saves/with_difficulty", web::post().to(save_controller::get_cloud_saves_with_difficulty))
            )
    );
} 