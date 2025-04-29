use actix_web::web;
use crate::controllers::song_controller;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/song")
            .route("/search", web::get().to(song_controller::search_song))
            .route("/search/record", web::post().to(song_controller::search_song_record))
    );
} 