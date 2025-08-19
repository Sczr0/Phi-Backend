use crate::models::song::{SongDifficulty, SongInfo};
use crate::utils::data_loader::{DIFFICULTY_MAP, SONG_INFO, SONG_NICKNAMES};
use crate::utils::error::{AppError, AppResult};

// 歌曲服务，提供歌曲信息查询
#[derive(Clone)]
pub struct SongService {
    // ID到歌曲信息的映射
    id_to_song: std::collections::HashMap<String, SongInfo>,
    // 歌曲名到歌曲信息的映射（小写）
    name_to_song: std::collections::HashMap<String, SongInfo>,
    // 别名到歌曲名的映射（小写）
    nickname_to_song: std::collections::HashMap<String, String>,
}

impl SongService {
    // 创建新的歌曲服务
    pub fn new() -> Self {
        let mut id_to_song = std::collections::HashMap::new();
        let mut name_to_song = std::collections::HashMap::new();
        let mut nickname_to_song = std::collections::HashMap::new();
        
        // 预处理数据，构建查找映射
        for song_info in SONG_INFO.iter() {
            id_to_song.insert(song_info.id.clone(), song_info.clone());
            name_to_song.insert(song_info.song.to_lowercase(), song_info.clone());
        }
        
        // 构建别名映射
        for (song_name, nicknames) in SONG_NICKNAMES.iter() {
            for nickname in nicknames {
                nickname_to_song.insert(nickname.to_lowercase(), song_name.clone());
            }
        }
        
        Self {
            id_to_song,
            name_to_song,
            nickname_to_song,
        }
    }

    // 统一搜索函数：自动判断输入是ID、歌曲名还是别名
    pub fn search_song(&self, initial_query: &str) -> AppResult<SongInfo> {
        if initial_query.is_empty() {
            return Err(AppError::SongNotFound("输入为空".to_string()));
        }

        let query = initial_query.trim();
        let query_lower = query.to_lowercase();
        log::info!("统一搜索歌曲: '{query}'");

        // 1. 尝试作为歌曲ID直接查找 (O(1) 复杂度)
        if let Some(info) = self.id_to_song.get(query) {
            log::info!("通过ID精确匹配找到歌曲: {}", info.song);
            return Ok(info.clone());
        }

        // 2. 尝试作为歌曲名称精确查找 (O(1) 复杂度)
        if let Some(info) = self.name_to_song.get(&query_lower) {
            log::info!("通过歌曲名精确匹配找到歌曲: {}", info.song);
            return Ok(info.clone());
        }

        // 3. 尝试作为别名精确查找 (O(1) 复杂度)
        if let Some(song_name) = self.nickname_to_song.get(&query_lower) {
            log::info!("通过别名精确匹配找到歌曲: {song_name} (别名: {query})");
            // 通过歌曲名查找歌曲信息
            if let Some(info) = self.name_to_song.get(&song_name.to_lowercase()) {
                return Ok(info.clone());
            }
        }

        // 4. 尝试歌曲名模糊匹配 (O(N) 复杂度，但只在必要时执行)
        let name_matches: Vec<_> = self.name_to_song
            .iter()
            .filter(|(name, _)| name.contains(&query_lower))
            .map(|(_, info)| info)
            .collect();
            
        if name_matches.len() == 1 {
            log::info!("通过歌曲名模糊匹配找到歌曲: {}", name_matches[0].song);
            return Ok(name_matches[0].clone());
        }

        // 5. 尝试别名模糊匹配 (O(N) 复杂度，但只在必要时执行)
        let nickname_matches: Vec<_> = self.nickname_to_song
            .iter()
            .filter_map(|(nickname, song_name)| {
                if nickname.contains(&query_lower) {
                    // 通过歌曲名查找歌曲信息
                    self.name_to_song.get(&song_name.to_lowercase()).map(|info| (info, nickname))
                } else {
                    None
                }
            })
            .collect();

        if nickname_matches.len() == 1 {
            let (info, nickname) = &nickname_matches[0];
            log::info!("通过别名模糊匹配找到歌曲: {} (别名: {})", info.song, nickname);
            return Ok((*info).clone());
        }

        // 6. 处理多个匹配的情况
        if !name_matches.is_empty() {
            let matches_str = name_matches
                .iter()
                .map(|info| info.song.clone())
                .collect::<Vec<_>>()
                .join(", ");
            log::info!("歌曲名模糊匹配找到多个歌曲: {matches_str}");
            return Err(AppError::AmbiguousSongName(matches_str));
        }

        if !nickname_matches.is_empty() {
            let matches_str = nickname_matches
                .iter()
                .map(|(info, nickname)| format!("{} (别名: {})", info.song, nickname))
                .collect::<Vec<_>>()
                .join(", ");
            log::info!("别名模糊匹配找到多个歌曲: {matches_str}");
            return Err(AppError::AmbiguousSongName(matches_str));
        }

        // 7. 如果未找到，则返回错误
        log::info!("找不到匹配查询 '{query}' 的歌曲");
        return Err(AppError::SongNotFound(query.to_string()));
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
