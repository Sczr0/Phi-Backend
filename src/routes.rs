use crate::controllers;
use actix_web::web;

pub fn configure(cfg: &mut web::ServiceConfig) {
    // API 路由
    cfg
        // Auth
        .service(
            web::resource("/auth/qrcode")
                .route(web::get().to(controllers::auth::generate_qr_code))
                .route(web::post().to(controllers::auth::generate_qr_code)),
        )
        .service(
            web::resource("/auth/qrcode/{qrId}/status")
                .route(web::get().to(controllers::auth::check_qr_status)),
        )
        // Binding
        .service(controllers::binding::bind_user) // POST /bind
        .service(controllers::binding::unbind_user) // POST /unbind
        .service(controllers::binding::list_tokens) // POST /token/list
        // Saves
        .service(controllers::save::get_cloud_saves) // POST /get/cloud/saves
        .service(controllers::save::get_cloud_saves_with_difficulty) // POST /get/cloud/saves/with_difficulty
        .service(controllers::save::get_cloud_save_info) // GET /get/cloud/saveInfo
        // RKS / BN
        .service(controllers::rks::get_rks) // POST /rks
        .service(controllers::b30::get_b30) // POST /b30
        .service(controllers::rks::get_bn) // POST /bn/{n}
        // Song Search (Recommended)
        .service(controllers::song::search_song) // GET /song/search
        .service(controllers::song::search_song_record) // POST /song/search/record
        .service(controllers::song::search_song_predictions) // GET /song/search/predictions
        // Song Search (Old/Compatible)
        .service(controllers::song::get_song_info) // GET /song/info
        .service(controllers::song::get_song_record) // POST /song/record
        .service(controllers::status::get_status) // GET /status
        .service(controllers::health::health_check); // GET /health

    // 图片路由
    cfg.service(
        web::scope("/image")
            .service(controllers::image::generate_bn_image)
            .service(controllers::image::generate_song_image)
            .service(controllers::image::get_rks_leaderboard)
            .service(controllers::image::get_cache_stats)
            .service(controllers::image::get_image_stats)
            .service(controllers::image::get_image_stats_by_type),
    );
}
