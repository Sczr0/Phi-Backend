use actix_web::web;
use crate::controllers::rks_controller;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg
        .route("/rks", web::post().to(rks_controller::calculate_rks))
        .route("/b30", web::post().to(rks_controller::get_b30))
        .route("/bn/{n}", web::post().to(rks_controller::get_bn));
} 