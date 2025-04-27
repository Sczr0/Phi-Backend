use actix_web::web;
use crate::controllers::image_controller;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/image")
            .service(image_controller::generate_bn_image)
    );
} 