use actix_web::web;
use crate::controllers::image_controller::{generate_bn_image, generate_song_image}; // 导入新的控制器函数

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/image") // 路由组 /image
            .service(generate_bn_image) // POST /image/bn/{n}
            .service(generate_song_image) // POST /image/song?q=...
    );
} 