pub mod phigros;
pub mod song;
pub mod user;
pub mod image_service;
pub mod player_archive_service;

// 重新导出主要的服务结构体，以便可以直接从 services 模块导入
pub use phigros::PhigrosService;
pub use song::SongService;
pub use user::UserService;
pub use player_archive_service::PlayerArchiveService; 