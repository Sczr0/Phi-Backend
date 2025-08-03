use crate::models::song::{SongInfo, SongDifficulty};
use crate::utils::data_loader::{SONG_INFO, DIFFICULTY_MAP, SONG_NICKNAMES};
use crate::utils::error::{AppError, AppResult};

// 歌曲服务，提供歌曲信息查询
#[derive(Clone)]
pub struct SongService {}

impl SongService {
    // 创建新的歌曲服务
    pub fn new() -> Self {
        Self {}
    }

    // 统一搜索函数：自动判断输入是ID、歌曲名还是别名
    pub fn search_song(&self, query: &str) -> AppResult<SongInfo> {
        if query.is_empty() {
            return Err(AppError::SongNotFound("输入为空".to_string()));
        }

        let query = query.trim();
        log::info!("统一搜索歌曲: '{}'", query);
        
        log::debug!("SONG_INFO 包含 {} 条记录", SONG_INFO.len());
        log::debug!("SONG_NICKNAMES 包含 {} 条记录", SONG_NICKNAMES.len());

        // 1. 尝试作为歌曲ID直接查找
        if let Some(info) = SONG_INFO.iter().find(|info| info.id == query) {
            log::info!("通过ID精确匹配找到歌曲: {}", info.song);
            return Ok(info.clone());
        }

        // 2. 尝试作为歌曲名称查找
        let query_lower = query.to_lowercase();
        if let Some(info) = SONG_INFO.iter().find(|info| info.song.to_lowercase() == query_lower) {
            log::info!("通过歌曲名精确匹配找到歌曲: {}", info.song);
            return Ok(info.clone());
        }

        // 3. 尝试作为别名查找
        if let Some(song_name) = SONG_NICKNAMES.iter().find_map(|(name, nicknames)| {
            nicknames.iter().any(|nick| nick.to_lowercase() == query_lower).then_some(name)
        }) {
            log::info!("通过别名精确匹配找到歌曲: {} (别名: {})", song_name, query);
            // 再次使用 search_song 查找，避免代码重复
            return self.search_song(song_name);
        }

        // 4. 尝试歌曲名模糊匹配
        let name_matches: Vec<_> = SONG_INFO.iter().filter(|info| info.song.to_lowercase().contains(&query_lower)).collect();
        if name_matches.len() == 1 {
            log::info!("通过歌曲名模糊匹配找到歌曲: {}", name_matches[0].song);
            return Ok(name_matches[0].clone());
        }

        // 5. 尝试别名模糊匹配
        let nickname_matches: Vec<_> = SONG_NICKNAMES.iter().filter_map(|(song_name, nicknames)| {
            nicknames.iter().find(|nick| nick.to_lowercase().contains(&query_lower)).map(|nick| (song_name, nick))
        }).collect();

        if nickname_matches.len() == 1 {
            let (song_name, nickname) = nickname_matches[0];
            log::info!("通过别名模糊匹配找到歌曲: {} (别名: {})", song_name, nickname);
            return self.search_song(song_name);
        }

        // 处理多个匹配的情况
        if !name_matches.is_empty() {
            let matches_str = name_matches.iter().map(|info| info.song.clone()).collect::<Vec<_>>().join(", ");
            log::info!("歌曲名模糊匹配找到多个歌曲: {}", matches_str);
            return Err(AppError::AmbiguousSongName(matches_str));
        }

        if !nickname_matches.is_empty() {
            let matches_str = nickname_matches.iter().map(|(name, nick)| format!("{} (别名: {})", name, nick)).collect::<Vec<_>>().join(", ");
            log::info!("别名模糊匹配找到多个歌曲: {}", matches_str);
            return Err(AppError::AmbiguousSongName(matches_str));
        }

        log::info!("找不到匹配查询 '{}' 的歌曲", query);
        Err(AppError::SongNotFound(query.to_string()))
    }
    
    // 根据统一查询找ID
    pub fn get_song_id(&self, query: &str) -> AppResult<String> {
        if query.is_empty() {
            return Err(AppError::SongNotFound("输入为空".to_string()));
        }
        
        let song_info = self.search_song(query)?;
        Ok(song_info.id.clone())
    }

    // 获取歌曲难度信息
    pub fn get_song_difficulty(&self, id: &str) -> AppResult<SongDifficulty> {
        DIFFICULTY_MAP
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::SongNotFound(id.to_string()))
    }

    // 获取所有歌曲信息
    #[allow(dead_code)]
    pub fn get_all_songs(&self) -> Vec<SongInfo> {
        SONG_INFO.to_vec()
    }
    
    // ===================== 以下为兼容性函数，使用新的统一搜索实现 =====================
    
    pub fn get_song_id_by_name(&self, name_or_alias: &str) -> AppResult<String> {
        self.get_song_id(name_or_alias)
    }

    pub fn get_song_id_by_nickname(&self, nickname: &str) -> AppResult<String> {
        self.get_song_id(nickname)
    }

    #[allow(dead_code)]
    pub fn get_song_info(&self, id: &str) -> AppResult<SongInfo> {
        self.search_song(id)
    }
    
    pub fn get_song_by_id(&self, id: &str) -> AppResult<SongInfo> {
        self.search_song(id)
    }
    
    pub fn search_song_by_name(&self, name: &str) -> AppResult<SongInfo> {
        self.search_song(name)
    }
    
    pub fn search_song_by_nickname(&self, nickname: &str) -> AppResult<SongInfo> {
        self.search_song(nickname)
    }
}