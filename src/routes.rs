use actix_web::web;
use crate::controllers;

pub fn configure(cfg: &mut web::ServiceConfig) {
    // API 路由
    cfg
        // Auth
        .service(web::resource("/auth/qrcode")
            .route(web::get().to(controllers::generate_qr_code))
            .route(web::post().to(controllers::generate_qr_code)))
        .service(web::resource("/auth/qrcode/{qrId}/status")
            .route(web::get().to(controllers::check_qr_status)))
        // Binding
        .service(controllers::bind_user)       // POST /bind
        .service(controllers::unbind_user)     // POST /unbind
        .service(controllers::list_tokens)     // POST /token/list
        // Saves
        .service(controllers::get_cloud_saves) // POST /get/cloud/saves
        .service(controllers::get_cloud_saves_with_difficulty) // POST /get/cloud/saves/with_difficulty
        // RKS / BN
        .service(controllers::calculate_rks)   // POST /rks
        .service(controllers::get_b30)         // POST /b30
        .service(controllers::get_bn)          // POST /bn/{n}
        // Song Search (Recommended)
        .service(controllers::search_song)     // GET /song/search
        .service(controllers::search_song_record) // POST /song/search/record
        .service(controllers::search_song_predictions) // GET /song/search/predictions
        // Song Search (Old/Compatible)
        .service(controllers::get_song_info)   // GET /song/info
        .service(controllers::get_song_record); // POST /song/record

    // 图片路由
    cfg.service(
        web::scope("/image")
            .service(controllers::generate_bn_image)
            .service(controllers::generate_song_image)
            .service(controllers::get_rks_leaderboard)
    );
}
