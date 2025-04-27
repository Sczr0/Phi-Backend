use actix_web::web;
// 不需要导入 SqlitePool 了

use crate::controllers::*;
// 不需要导入服务了，因为它们在 main.rs 中创建和添加
// use crate::services::phigros::PhigrosService;
// use crate::services::song::SongService;
// use crate::services::user::UserService;

mod image_routes;

// 配置所有路由
pub fn configure(cfg: &mut web::ServiceConfig) {
    // 移除错误获取 pool 和创建服务的代码
    // let pool = ...
    // let phigros_service = ...
    // let song_service = ...
    // let user_service = ...
    
    // 只需要注册控制器处理函数
    // Actix 会自动注入在 main.rs 中添加的 web::Data<ServiceType>
    cfg
       // 绑定相关路由
       .service(bind_user)
       .service(unbind_user)
       // .service(get_token_by_qq) // 移除此路由
       // 存档相关路由
       .service(get_cloud_saves)
       .service(get_cloud_saves_with_difficulty)
       // RKS相关路由
       .service(get_rks)
       .service(get_b30)
       .service(get_bn)
       // 歌曲相关路由 - 新增统一搜索接口
       .service(search_song)
       .service(search_song_record)
       // 歌曲相关路由 - 兼容旧接口
       .service(get_song_record)
       .service(get_song_info);
       
    // 配置图像生成路由
    image_routes::configure(cfg);
} 