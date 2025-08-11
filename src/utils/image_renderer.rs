use crate::models::rks::RksRecord;
use crate::utils::error::AppError;
use crate::utils::rks_utils;
use crate::utils::cover_loader;
use resvg::usvg::{self, Options as UsvgOptions, fontdb};
use resvg::{render, tiny_skia::{Pixmap, Transform}};
use std::path::{PathBuf, Path};
use std::num::NonZeroUsize;
use chrono::{DateTime, Utc, FixedOffset};
use std::fmt::Write;
use std::collections::HashMap;
use std::fs;
use std::sync::{Arc, OnceLock};
use base64::{engine::general_purpose::STANDARD as base64_engine, Engine as _}; // Added
use rand::seq::SliceRandom;
use rand::thread_rng;
use crate::models::player_archive::RKSRankingEntry;
use lru::LruCache;


#[allow(dead_code)]
pub struct PlayerStats {
    pub ap_top_3_avg: Option<f64>,
    pub best_27_avg: Option<f64>,
    pub real_rks: Option<f64>,
    pub player_name: Option<String>,
    pub update_time: DateTime<Utc>,
    pub n: u32,  // 请求的 Best N 数量
    pub ap_top_3_scores: Vec<RksRecord>, // 添加 AP Top 3 的具体成绩
    pub challenge_rank: Option<(String, String)>, // 新增：课题等级 (颜色, 等级)
    pub data_string: Option<String>, // 新增：格式化后的Data字符串
}

// 新增：单曲成绩渲染所需数据结构
#[derive(Debug, Clone)]
pub struct SongDifficultyScore {
    pub score: Option<f64>,
    pub acc: Option<f64>,
    pub rks: Option<f64>,
    pub difficulty_value: Option<f64>,
    pub is_fc: Option<bool>, // 可选：是否 Full Combo
    pub is_phi: Option<bool>, // 可选：是否 Phi (ACC 100%)
    pub player_push_acc: Option<f64>, // 新增：玩家总RKS推分ACC
}

#[derive(Debug)]
pub struct SongRenderData {
    pub song_name: String,
    pub song_id: String, // 用于加载封面
    pub player_name: Option<String>,
    pub update_time: DateTime<Utc>,
    // 使用 HashMap 存储不同难度的成绩，Key 为 "EZ", "HD", "IN", "AT"
    pub difficulty_scores: HashMap<String, Option<SongDifficultyScore>>,
    // 歌曲插画路径 (用于渲染)
    pub illustration_path: Option<PathBuf>,
}

/// 排行榜渲染数据
#[allow(dead_code)]
pub struct LeaderboardRenderData {
    pub title: String,
    pub update_time: DateTime<Utc>,
    pub entries: Vec<RKSRankingEntry>,
    pub display_count: usize,
}


// 常量定义
const FONTS_DIR: &str = "resources/fonts";
const MAIN_FONT_NAME: &str = "思源黑体 CN";
const COVER_ASPECT_RATIO: f64 = 512.0 / 270.0;
#[allow(dead_code)]
const SONG_ILLUST_ASPECT_RATIO: f64 = 1.0; // 假设单曲图的插画是方形的

// 全局字体数据库单例
static GLOBAL_FONT_DB: OnceLock<Arc<fontdb::Database>> = OnceLock::new();

// 背景图片 LRU 缓存
static BACKGROUND_IMAGE_CACHE: OnceLock<std::sync::Mutex<LruCache<PathBuf, String>>> = OnceLock::new();
const BACKGROUND_CACHE_SIZE: usize = 10; // 缓存10张背景图片

// 封面图片路径列表
static COVER_FILES: OnceLock<Vec<PathBuf>> = OnceLock::new();

/// 初始化全局字体数据库
fn init_global_font_db() -> Arc<fontdb::Database> {
    let mut font_db = fontdb::Database::new();
    font_db.load_system_fonts();
    
    // 加载自定义字体
    let fonts_dir = PathBuf::from(FONTS_DIR);
    if fonts_dir.exists() {
        if let Ok(entries) = fs::read_dir(&fonts_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() &&
                   (path.extension() == Some("ttf".as_ref()) ||
                    path.extension() == Some("otf".as_ref())) {
                    if let Err(e) = font_db.load_font_file(&path) {
                        log::error!("加载字体文件失败 '{}': {}", path.display(), e);
                    }
                }
            }
        }
    }
    
    Arc::new(font_db)
}

/// 获取全局字体数据库
pub fn get_global_font_db() -> Arc<fontdb::Database> {
    GLOBAL_FONT_DB.get_or_init(init_global_font_db).clone()
}

/// 初始化背景图片缓存和封面文件列表
fn init_background_cache() -> (std::sync::Mutex<LruCache<PathBuf, String>>, Vec<PathBuf>) {
    // 初始化 LRU 缓存
    let cache = std::sync::Mutex::new(LruCache::new(NonZeroUsize::new(BACKGROUND_CACHE_SIZE).unwrap()));
    
    // 读取封面目录下的所有图片文件
    let cover_base_path = PathBuf::from(cover_loader::COVERS_DIR).join("ill");
    let cover_files = match fs::read_dir(&cover_base_path) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.is_file() &&
                   (path.extension() == Some("png".as_ref()) ||
                    path.extension() == Some("jpg".as_ref())))
            .collect(),
        Err(e) => {
            log::error!("读取封面目录失败 '{}': {}", cover_base_path.display(), e);
            Vec::new()
        }
    };
    
    (cache, cover_files)
}

/// 获取背景图片缓存
pub fn get_background_cache() -> &'static std::sync::Mutex<LruCache<PathBuf, String>> {
    BACKGROUND_IMAGE_CACHE.get_or_init(|| {
        let (cache, _) = init_background_cache();
        cache
    })
}

/// 获取封面文件列表
pub fn get_cover_files() -> &'static Vec<PathBuf> {
    COVER_FILES.get_or_init(|| {
        let (_, files) = init_background_cache();
        files
    })
}

/// 从缓存或磁盘加载背景图片
fn get_background_image(path: &PathBuf) -> Option<String> {
    let mut cache = get_background_cache().lock().unwrap();
    
    // 尝试从缓存中获取
    if let Some(cached_image) = cache.get(path) {
        return Some(cached_image.clone());
    }
    
    // 缓存未命中，从磁盘加载
    if let Ok(data) = fs::read(path) {
        let mime_type = if path.extension().map_or(false, |ext| ext == "png") {
            "image/png"
        } else {
            "image/jpeg"
        };
        let base64_encoded = base64_engine.encode(&data);
        let image_data = format!("data:{};base64,{}", mime_type, base64_encoded);
        
        // 放入缓存
        cache.put(path.clone(), image_data.clone());
        
        return Some(image_data);
    }
    
    None
}

// Helper function to generate a single score card SVG group
fn generate_card_svg(
    svg: &mut String,
    score: &RksRecord,
    index: usize,
    card_x: u32,
    card_y: u32,
    card_width: u32,
    is_ap_card: bool, // Flag to indicate if this is for the AP section
    is_ap_score: bool, // Flag to indicate if the score itself is AP
    pre_calculated_push_acc: Option<f64>, // 新增：预先计算的推分ACC
    all_sorted_records: &[RksRecord], // 新增：所有排序好的成绩，用于新版推分计算
    theme: &crate::controllers::image::Theme, // 新增：主题参数
) -> Result<(), AppError> {
    let fmt_err = |e| AppError::InternalError(format!("SVG formatting error: {}", e));

    // --- Card Dimensions & Layout ---
    let card_padding = 10.0; // Inner padding
    let text_line_height_song = 22.0;
    let text_line_height_score = 30.0;
    let text_line_height_acc = 18.0;
    let text_line_height_level = 18.0;
    let text_block_spacing = 4.0; // Spacing between text lines

    // Calculate text block height (approximate)
    let text_block_height = text_line_height_song
                            + text_line_height_score
                            + text_line_height_acc
                            + text_line_height_level
                            + text_block_spacing * 3.0;

    let cover_size_h = text_block_height;
    let cover_size_w = cover_size_h * COVER_ASPECT_RATIO;
    let card_height = (cover_size_h + card_padding * 2.0) as u32;
    let card_radius = 8;

    let cover_x = card_padding;
    let cover_y = card_padding;

    let card_class = if is_ap_score {
        "card card-ap"
    } else if score.is_fc {
        "card card-fc"
    } else {
        "card"
    };

    writeln!(svg, r#"<g transform="translate({}, {})">"#, card_x, card_y).map_err(fmt_err)?;

    // Card background rectangle
    writeln!(svg, r#"<rect width="{}" height="{}" rx="{}" ry="{}" class="{}" />"#,
             card_width, card_height, card_radius, card_radius, card_class).map_err(fmt_err)?;

    // --- Card Content ---
    // Define clip path for rounded cover
    let clip_path_id = format!("cover-clip-{}-{}", if is_ap_card {"ap"} else {"main"}, index);
    writeln!(svg, "<defs><clipPath id=\"{}\"><rect x=\"{}\" y=\"{}\" width=\"{:.1}\" height=\"{:.1}\" rx=\"4\" ry=\"4\" /></clipPath></defs>",
             clip_path_id, cover_x, cover_y, cover_size_w, cover_size_h).map_err(fmt_err)?;

    // Cover Image or Placeholder
    let cover_path_png = PathBuf::from(cover_loader::COVERS_DIR).join("illLow").join(format!("{}.png", score.song_id));
    let cover_path_jpg = PathBuf::from(cover_loader::COVERS_DIR).join("illLow").join(format!("{}.jpg", score.song_id));
    let cover_href = if cover_path_png.exists() {
        cover_path_png.canonicalize().ok().map(|p| p.to_string_lossy().into_owned())
    } else if cover_path_jpg.exists() {
        cover_path_jpg.canonicalize().ok().map(|p| p.to_string_lossy().into_owned())
    } else {
        None
    };
    if let Some(href) = cover_href {
        let escaped_href = escape_xml(&href);
         writeln!(svg, r#"<image href="{}" x="{}" y="{}" width="{:.1}" height="{:.1}" clip-path="url(#{})" />"#,
                  escaped_href, cover_x, cover_y, cover_size_w, cover_size_h, clip_path_id).map_err(fmt_err)?;
    } else {
        let placeholder_color = match theme {
            crate::controllers::image::Theme::White => "#DDD",
            crate::controllers::image::Theme::Black => "#333",
        };
        writeln!(svg, "<rect x='{}' y='{}' width='{:.1}' height='{:.1}' fill='{}' rx='4' ry='4'/>",
                 cover_x, cover_y, cover_size_w, cover_size_h, placeholder_color).map_err(fmt_err)?;
    }

    // Text content positioning
    let text_x = cover_x + cover_size_w + 15.0; // Padding between cover and text
    let text_width = (card_width as f64) - text_x - card_padding; // Available width for text

    // 新增一个垂直偏移量，用于微调文本块的整体位置
    // 可以调整这个值，直到视觉效果满意为止。数值越大，文本越往下。
    let vertical_text_offset = 5.0; 

    // Calculate Y positions for text lines to align with cover
    let song_name_y = cover_y + text_line_height_song * 0.75 + vertical_text_offset;
    let score_y = song_name_y + text_line_height_score * 0.8 + text_block_spacing + 2.0; // 分数部分向下移动2像素
    let acc_y = score_y + text_line_height_acc + text_block_spacing;
    let level_y = acc_y + text_line_height_level + text_block_spacing;

    // --- Song Name (智能判断是否需要压缩) ---

// 1. 定义一个简单的函数来判断字符是否为全角（主要针对中日韩字符）
fn is_full_width(ch: char) -> bool {
// 这个范围覆盖了常见的中日韩统一表意文字、平假名、片假名和全角符号
    (ch >= '\u{4E00}' && ch <= '\u{9FFF}') || // CJK Unified Ideographs
    (ch >= '\u{3040}' && ch <= '\u{30FF}') || // Hiragana and Katakana
    (ch >= '\u{FF00}' && ch <= '\u{FFEF}')    // Full-width forms
}

// 2. 估算文本渲染后的大致宽度
let mut estimated_width = 0.0;
// 根据CSS样式，.text-songname 的 font-size 是 19px。
// 全角字符宽度约等于字号，半角字符宽度约为一半。这里我们用稍大的值做估算。
let full_width_char_px = 19.0;
let half_width_char_px = 10.5; // 英文、数字等半角字符的平均宽度估值

for ch in score.song_name.chars() {
    if is_full_width(ch) {
        estimated_width += full_width_char_px;
    } else {
        estimated_width += half_width_char_px;
    }
}

// 3. 根据估算结果，决定是否启用SVG压缩
let song_name_escaped = escape_xml(&score.song_name);

if estimated_width > text_width {
    // 估算宽度超过了可用空间，启用 textLength 进行压缩
    writeln!(
        svg,
        r#"<text x="{}" y="{:.1}" class="text-songname" textLength="{:.1}" lengthAdjust="spacingAndGlyphs">{}</text>"#,
        text_x, song_name_y, text_width, song_name_escaped
    ).map_err(fmt_err)?;
} else {
    // 估算宽度足够，正常渲染，不压缩也不拉伸
    writeln!(
        svg,
        r#"<text x="{}" y="{:.1}" class="text-songname">{}</text>"#,
        text_x, song_name_y, song_name_escaped
    ).map_err(fmt_err)?;
}

    // Score
    let score_text = score.score.map_or("N/A".to_string(), |s| format!("{:.0}", s));
    writeln!(svg, r#"<text x="{}" y="{:.1}" class="text-score">{}</text>"#, text_x, score_y, score_text).map_err(fmt_err)?;

    // Accuracy (带推分acc)
    let acc_text = if !is_ap_score && score.acc < 100.0 && score.difficulty_value > 0.0 { // 只有定数>0时才显示推分
        // 如果有预计算的推分ACC，优先使用
        let push_acc = if let Some(pa) = pre_calculated_push_acc {
            pa
        } else {
            // 否则使用新算法计算
            let target_chart_id = format!("{}-{}", score.song_id, score.difficulty);
            let all_records_vec = all_sorted_records.to_vec(); // 需要 Vec<RksRecord>
            rks_utils::calculate_target_chart_push_acc(
                &target_chart_id,
                score.difficulty_value,
                &all_records_vec
            ).unwrap_or(100.0) // 如果计算失败（比如格式错误），则默认为100
        };

        // 如果推分acc非常接近100，直接显示 -> 100.00%
        if push_acc > 99.995 {
             format!("Acc: {:.2}% <tspan class='push-acc'>-> 100.00%</tspan>", score.acc)
        }
        // 如果两者差值非常小(小于0.005，对应四舍五入后两位不变)，则展示三位小数
        else if (push_acc - score.acc).abs() < 0.005 {
            format!("Acc: {:.2}% <tspan class='push-acc'>-> {:.3}%</tspan>", score.acc, push_acc)
        } else {
            format!("Acc: {:.2}% <tspan class='push-acc'>-> {:.2}%</tspan>", score.acc, push_acc)
        }
    } else {
        // AP或者已满分或者定数为0，只显示当前acc
        format!("Acc: {:.2}%", score.acc)
    };
    writeln!(svg, r#"<text x="{}" y="{:.1}" class="text-acc">{}</text>"#, text_x, acc_y, acc_text).map_err(fmt_err)?;

    // Level & RKS
    // 获取难度标签文本和颜色
    let (difficulty_text, difficulty_color) = match &score.difficulty {
        diff if diff.eq_ignore_ascii_case("EZ") => ("EZ", "#51AF44"), // 绿色
        diff if diff.eq_ignore_ascii_case("HD") => ("HD", "#3173B3"), // 蓝色
        diff if diff.eq_ignore_ascii_case("IN") => ("IN", "#BE2D23"), // 红色
        diff if diff.eq_ignore_ascii_case("AT") => ("AT", "#383838"), // 深灰色
        _ => ("??", "#888888") // 默认灰色
    };

    // 难度标签尺寸
    let badge_width = 36.0;
    let badge_height = 20.0;
    let badge_radius = 4.0;
    // 将标签放置在曲绘左下角
    let badge_x = cover_x + 5.0; // 曲绘左侧留出5px边距
    let badge_y = cover_y + cover_size_h - badge_height - 5.0; // 曲绘底部留出5px边距

    // 绘制难度标签背景
    writeln!(svg, r#"<rect x="{}" y="{:.1}" width="{:.1}" height="{:.1}" rx="{:.1}" ry="{:.1}" fill="{}" />"#,
             badge_x, badge_y, badge_width, badge_height, badge_radius, badge_radius, difficulty_color).map_err(fmt_err)?;

    // 绘制难度标签文本
    let badge_text_x = badge_x + badge_width / 2.0;
    let badge_text_y = badge_y + badge_height / 2.0 + 5.0; // 垂直居中
    writeln!(svg, r#"<text x="{:.1}" y="{:.1}" class="text-difficulty-badge" text-anchor="middle" fill="white">{}</text>"#,
             badge_text_x, badge_text_y, difficulty_text).map_err(fmt_err)?;

    // FC/AP标签尺寸
    let fc_ap_badge_width = 30.0;
    let fc_ap_badge_height = 20.0;
    let fc_ap_badge_radius = 4.0;
    let fc_ap_badge_spacing = 5.0; // 标签之间的间距

    // FC标签位置（在难度标签右侧）
    if score.is_fc {
        let fc_badge_x = badge_x + badge_width + fc_ap_badge_spacing;
        let fc_badge_y = badge_y;
        
        // 绘制FC标签背景
        let fc_badge_color = "#4682B4";
        writeln!(svg, r#"<rect x="{}" y="{:.1}" width="{:.1}" height="{:.1}" rx="{:.1}" ry="{:.1}" fill="{}" />"#,
                 fc_badge_x, fc_badge_y, fc_ap_badge_width, fc_ap_badge_height, fc_ap_badge_radius, fc_ap_badge_radius, fc_badge_color).map_err(fmt_err)?;
        
        // 绘制FC标签文本
        let fc_badge_text_x = fc_badge_x + fc_ap_badge_width / 2.0;
        let fc_badge_text_y = fc_badge_y + fc_ap_badge_height / 2.0 + 5.0; // 垂直居中
        writeln!(svg, r#"<text x="{:.1}" y="{:.1}" class="text-fc-ap-badge" text-anchor="middle" fill="white">FC</text>"#,
                 fc_badge_text_x, fc_badge_text_y).map_err(fmt_err)?;
    }

    // AP标签位置（在FC标签右侧或难度标签右侧）
    if score.acc == 100.0 {
        let ap_badge_x = if score.is_fc {
            badge_x + badge_width + fc_ap_badge_spacing * 2.0 + fc_ap_badge_width
        } else {
            badge_x + badge_width + fc_ap_badge_spacing
        };
        let ap_badge_y = badge_y;
        
        // 绘制AP标签背景
        let ap_badge_color = "gold";
        writeln!(svg, r#"<rect x="{}" y="{:.1}" width="{:.1}" height="{:.1}" rx="{:.1}" ry="{:.1}" fill="{}" />"#,
                 ap_badge_x, ap_badge_y, fc_ap_badge_width, fc_ap_badge_height, fc_ap_badge_radius, fc_ap_badge_radius, ap_badge_color).map_err(fmt_err)?;
        
        // 绘制AP标签文本
        let ap_badge_text_x = ap_badge_x + fc_ap_badge_width / 2.0;
        let ap_badge_text_y = ap_badge_y + fc_ap_badge_height / 2.0 + 5.0; // 垂直居中
        writeln!(svg, r#"<text x="{:.1}" y="{:.1}" class="text-fc-ap-badge" text-anchor="middle" fill="white" filter="url(#ap-text-shadow)">AP</text>"#,
                 ap_badge_text_x, ap_badge_text_y).map_err(fmt_err)?;
    }

    // 恢复等级和RKS的简单字符串拼接
    let level_text = format!("Lv.{} -> {:.2}", score.difficulty_value, score.rks);
    writeln!(svg, r#"<text x="{}" y="{:.1}" class="text-level">{}</text>"#, text_x, level_y, level_text).map_err(fmt_err)?;

    // Rank (Only for main scores, not AP)
    if !is_ap_card {
        let rank_text = format!("#{}", index + 1);
        // 将坐标改回右下角
        writeln!(svg, r#"<text x="{}" y="{:.1}" class="text-rank">{}</text>"#,
                 (card_width as f64) - card_padding, level_y, rank_text).map_err(fmt_err)?;
    }

    writeln!(svg, "</g>").map_err(fmt_err)?; // End card group
    Ok(())
}


// --- SVG 生成函数 ---

pub fn generate_svg_string(
    scores: &[RksRecord],
    stats: &PlayerStats,
    push_acc_map: Option<&HashMap<String, f64>>, // 新增：预先计算的推分ACC映射，键为"曲目ID-难度"
    theme: &crate::controllers::image::Theme, // 新增：主题参数
) -> Result<String, AppError> {
    // ... (width, height calculations etc. - keep these as they were) ...
    let width = 1200;
    let header_height = 120;
    let _ap_title_height = 50; // Prefix unused variable
    let footer_height = 50;
    let main_card_padding_outer = 12;
    let ap_card_padding_outer = 12;
    let columns = 3;

    let main_card_width = (width - main_card_padding_outer * (columns + 1)) / columns;
    let card_padding_inner = 10.0;
    let text_line_height_song = 22.0;
    let text_line_height_score = 30.0;
    let text_line_height_acc = 18.0;
    let text_line_height_level = 18.0;
    let text_block_spacing = 4.0;
    let text_block_height = text_line_height_song
                            + text_line_height_score
                            + text_line_height_acc
                            + text_line_height_level
                            + text_block_spacing * 3.0;
    let calculated_card_height = (text_block_height + card_padding_inner * 2.0) as u32;
    let ap_card_start_y = ap_card_padding_outer;
    let ap_section_height = if !stats.ap_top_3_scores.is_empty() {
        ap_card_start_y + calculated_card_height + ap_card_padding_outer
    } else { 0 };
    let rows = (scores.len() as u32 + columns - 1) / columns;
    let content_height = (calculated_card_height + main_card_padding_outer) * rows.max(1);
    let total_height = header_height + ap_section_height + content_height + footer_height + 10;


    // 根据主题定义颜色变量
    let (bg_color, text_color, card_bg_color, card_stroke_color, text_secondary_color, fc_stroke_color, ap_stroke_color) = match theme {
        crate::controllers::image::Theme::White => ("#FFFFFF", "#000000", "#F0F0F0", "#DDDDDD", "#666666", "#4682B4", "url(#ap-gradient)"),
        crate::controllers::image::Theme::Black => ("#141826", "#FFFFFF", "#1A1E2A", "#333848", "#BBBBBB", "#87CEEB", "url(#ap-gradient)"),
    };
    let (ap_card_fill, fc_card_fill) = match theme {
        crate::controllers::image::Theme::White => ("url(#ap-card-bg-gradient)".to_string(), "url(#fc-card-bg-gradient)".to_string()),
        crate::controllers::image::Theme::Black => (card_bg_color.to_string(), card_bg_color.to_string()),
    };
    
    let mut normal_card_stroke_color = match theme {
        crate::controllers::image::Theme::White => "url(#normal-card-stroke-gradient)".to_string(),
        crate::controllers::image::Theme::Black => "#252A38".to_string(), // Weaker border for black theme
    };
    let mut svg = String::new();
    let fmt_err = |e| AppError::InternalError(format!("SVG formatting error: {}", e));

    // --- 获取随机背景图 ---
    let mut background_image_href = None;
    let _background_fill = "url(#bg-gradient)".to_string(); // Prefix unused variable

    // 专门为背景图获取 illBlur 目录的文件列表
    let background_base_path = PathBuf::from(cover_loader::COVERS_DIR).join("illBlur");
    let background_files = match fs::read_dir(&background_base_path) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.is_file() &&
                   (path.extension() == Some("png".as_ref()) ||
                    path.extension() == Some("jpg".as_ref())))
            .collect::<Vec<PathBuf>>(),
        Err(e) => {
            log::error!("读取背景目录失败 '{}': {}", background_base_path.display(), e);
            Vec::new()
        }
    };
    
    if !background_files.is_empty() {
        let mut rng = thread_rng();
        if let Some(random_path) = background_files.choose(&mut rng) { // 随机选择一个路径
            // --- 新增：计算背景主色的反色 ---
            if let crate::controllers::image::Theme::White = theme {
                if let Some(inverse_color) = calculate_inverse_color_from_path(random_path) {
                    normal_card_stroke_color = inverse_color;
                    log::info!("使用背景反色作为卡片边框: {}", normal_card_stroke_color);
                }
            }
            // --- 结束新增 ---

            // 使用缓存函数获取背景图片
            if let Some(image_data) = get_background_image(random_path) {
                background_image_href = Some(image_data);
                log::info!("使用随机背景图: {}", random_path.display());
            } else {
                log::error!("获取背景图片失败: {}", random_path.display());
                // 获取失败则回退到渐变
            }
        } else {
            log::warn!("无法从背景文件列表中随机选择一个");
            // Fallback to gradient if choose fails (shouldn't happen with non-empty list)
        }
    } else {
        log::warn!("找不到任何背景文件用于随机背景");
        // Fallback to gradient if directory is empty or read failed
    }
    // --- 背景图获取结束 ---


    writeln!(
        svg,
        r#"<svg width="{}" height="{}" viewBox="0 0 {} {}" xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink">"#,
        width, total_height, width, total_height
    ).map_err(fmt_err)?;

    // --- Definitions (Styles, Gradients, Filters, Font) ---
    writeln!(svg, "<defs>").map_err(fmt_err)?;

    // Background Gradient (Fallback)
    match theme {
        crate::controllers::image::Theme::White => {
            writeln!(svg, r#"<linearGradient id="bg-gradient" x1="0%" y1="0%" x2="100%" y2="100%"><stop offset="0%" style="stop-color:#FFFFFF" /><stop offset="100%" style="stop-color:#F0F0F0" /></linearGradient>"#).map_err(fmt_err)?;
        }
        crate::controllers::image::Theme::Black => {
            writeln!(svg, r#"<linearGradient id="bg-gradient" x1="0%" y1="0%" x2="100%" y2="100%"><stop offset="0%" style="stop-color:#141826" /><stop offset="100%" style="stop-color:#252E48" /></linearGradient>"#).map_err(fmt_err)?;
        }
    }

    // Shadow Filter Definition
    writeln!(svg, r#"<filter id="card-shadow" x="-10%" y="-10%" width="120%" height="130%"><feDropShadow dx="0" dy="3" stdDeviation="3" flood-color="rgba(0,0,0,0.25)" flood-opacity="0.25" /></filter>"#).map_err(fmt_err)?;
    
    // FC Glow Filter Definition
    writeln!(svg, r#"<filter id="fc-glow" x="-50%" y="-50%" width="200%" height="200%"><feDropShadow dx="0" dy="0" stdDeviation="4" flood-color="{}" flood-opacity="0.8" /></filter>"#, fc_stroke_color).map_err(fmt_err)?;

    writeln!(svg, r#"<filter id="ap-glow" x="-50%" y="-50%" width="200%" height="200%"><feDropShadow dx="0" dy="0" stdDeviation="4" flood-color="{}" flood-opacity="0.8" /></filter>"#, fc_stroke_color).map_err(fmt_err)?;

    // AP Text Shadow Filter Definition
    writeln!(svg, r#"<filter id="ap-text-shadow" x="-20%" y="-20%" width="140%" height="140%"><feDropShadow dx="1" dy="1" stdDeviation="1" flood-color="rgba(0,0,0,0.5)"/></filter>"#).map_err(fmt_err)?;

    // Gaussian Blur Filter Definition
    writeln!(svg, r#"<filter id="bg-blur">"#).map_err(fmt_err)?;
    // 调整 stdDeviation 控制模糊程度, 10 是一个比较强的模糊效果
    writeln!(svg, r#"<feGaussianBlur stdDeviation="10" />"#).map_err(fmt_err)?;
    writeln!(svg, r#"</filter>"#).map_err(fmt_err)?;

    // Font style ... (保持不变) ...
    writeln!(svg, "<style>").map_err(fmt_err)?;
    write!(
        svg,
        r#"
        /* <![CDATA[ */
        svg {{ background-color: {bg_color}; /* Fallback background color */ }}
        .card {{
            fill: {card_bg_color};
            stroke: {normal_card_stroke_color};
            stroke-width: 1.5;
            filter: url(#card-shadow);
            transition: all 0.3s ease;
        }}
        .card-ap {{
          fill: {ap_card_fill};
          stroke: {ap_stroke_color};
          stroke-width: 2.5;
          filter: url(#ap-glow);
        }}
        .card-fc {{
          fill: {fc_card_fill};
          stroke: {fc_stroke_color}; /* Light Sky Blue */
          stroke-width: 2.5;
          filter: url(#fc-glow);
        }}
        /* ... (其他样式保持不变) ... */
        .text-title {{ font-size: 34px; fill: {text_color}; /* font-weight: bold; */ text-shadow: 0px 2px 4px rgba(0, 0, 0, 0.4); }}
        .text-stat {{ font-size: 21px; fill: {text_color}; }}
        .text-info {{ font-size: 16px; fill: {text_secondary_color}; text-anchor: end; }} /* For new info */
        .text-time {{ font-size: 14px; fill: {text_secondary_color}; text-anchor: end; }}
        .text-footer {{ font-size: 13px; fill: {text_secondary_color}; }}
        .text-songname {{ font-size: 20px; fill: {text_color}; font-weight: 600; }}
        .text-score {{ font-size: 30px; fill: {text_color}; font-weight: 700; }}
        .text-acc {{ font-size: 14px; fill: #999999; font-weight: 400; }}
        .text-level {{ font-size: 14px; fill: #999999; font-weight: 400; }}
        .text-rank {{ font-size: 14px; fill: #AAAAAA; font-weight: 400; text-anchor: end; }}
        .text-difficulty-badge {{ font-size: 12px; font-weight: 700; }} /* 难度标签文本样式 */
        .text-fc-ap-badge {{ font-size: 11px; font-weight: 700; }} /* FC/AP标签文本样式 */
        .push-acc {{ fill: #4CAF50; font-weight: 600; }}
        .text-rank-tag {{ font-size: 13px; fill: {text_secondary_color}; text-anchor: end; font-weight: 700; }}
        .text-section-title {{ font-size: 21px; fill: {text_color}; /* font-weight: bold; */ }}
        * {{ font-family: "{font_name}", "Microsoft YaHei", "SimHei", "DengXian", Arial, sans-serif; }}
        /* ]]> */
        "#,
        bg_color = bg_color,
        text_color = text_color,
        card_bg_color = card_bg_color,
        text_secondary_color = text_secondary_color,
        normal_card_stroke_color = normal_card_stroke_color,
        fc_stroke_color = fc_stroke_color,
        ap_stroke_color = ap_stroke_color,
        ap_card_fill = ap_card_fill,
        fc_card_fill = fc_card_fill,
        font_name = MAIN_FONT_NAME
    ).map_err(fmt_err)?;
    writeln!(svg, "</style>").map_err(fmt_err)?;


    // Define normal card stroke gradient
    writeln!(svg, r#"<linearGradient id="normal-card-stroke-gradient" x1="0%" y1="0%" x2="100%" y2="100%">"#).map_err(fmt_err)?;
    writeln!(svg, "<stop offset=\"0%\" style=\"stop-color:#555868\" />").map_err(fmt_err)?; // 深灰色
    writeln!(svg, "<stop offset=\"100%\" style=\"stop-color:#333848\" />").map_err(fmt_err)?; // 更深的灰色
    writeln!(svg, r#"</linearGradient>"#).map_err(fmt_err)?;
    
    // Define AP card stroke gradient
    writeln!(svg, r#"<linearGradient id="ap-gradient" x1="0%" y1="0%" x2="100%" y2="100%">"#).map_err(fmt_err)?;
    writeln!(svg, "<stop offset=\"0%\" style=\"stop-color:#FFDA63\" />").map_err(fmt_err)?;
    writeln!(svg, "<stop offset=\"100%\" style=\"stop-color:#D1913C\" />").map_err(fmt_err)?;
    writeln!(svg, r#"</linearGradient>"#).map_err(fmt_err)?;
    
    // 暂时不为白色主题定义更暗的AP渐变
    writeln!(svg, r#"<linearGradient id="ap-gradient-white" x1="0%" y1="0%" x2="100%" y2="100%">"#).map_err(fmt_err)?;
    writeln!(svg, "<stop offset=\"0%\" style=\"stop-color:#D4A017\" />").map_err(fmt_err)?; // 更暗的金色
    writeln!(svg, "<stop offset=\"100%\" style=\"stop-color:#B8860B\" />").map_err(fmt_err)?; // 更暗的金色
    writeln!(svg, r#"</linearGradient>"#).map_err(fmt_err)?;

    // Define AP card background gradient for white theme
    writeln!(svg, r#"<linearGradient id="ap-card-bg-gradient" x1="0%" y1="0%" x2="100%" y2="100%"><stop offset="0%" style="stop-color:#FFF9E6;stop-opacity:0.8" /><stop offset="100%" style="stop-color:#FFEB99;stop-opacity:0.8" /></linearGradient>"#).map_err(fmt_err)?;
    // Define FC card background gradient for white theme
    writeln!(svg, r#"<linearGradient id="fc-card-bg-gradient" x1="0%" y1="0%" x2="100%" y2="100%"><stop offset="0%" style="stop-color:#E6F2FF;stop-opacity:0.8" /><stop offset="100%" style="stop-color:#B3D9FF;stop-opacity:0.8" /></linearGradient>"#).map_err(fmt_err)?;

    writeln!(svg, "</defs>").map_err(fmt_err)?;

    // --- Background ---
    // 如果找到了背景图，则使用<image>并应用模糊，否则使用原来的<rect>和渐变
    if let Some(href) = background_image_href {
        writeln!(svg,
            // 使用 href (Base64 data URI), preserveAspectRatio 保证图片覆盖并居中裁剪, filter 应用模糊
            r#"<image href="{}" x="0" y="0" width="100%" height="100%" preserveAspectRatio="xMidYMid slice" filter="url(#bg-blur)" />"#,
            href
        ).map_err(fmt_err)?;
        // 可选：在模糊背景上加一层半透明叠加层，使前景文字更清晰
        // 调整 rgba 最后一个值 (alpha) 控制透明度, 0.7 = 70% 不透明
        match theme {
            crate::controllers::image::Theme::White => {
                writeln!(svg, r#"<rect width="100%" height="100%" fill="rgba(255, 255, 255, 0.7)" />"#).map_err(fmt_err)?;
            }
            crate::controllers::image::Theme::Black => {
                writeln!(svg, r#"<rect width="100%" height="100%" fill="rgba(20, 24, 38, 0.7)" />"#).map_err(fmt_err)?;
            }
        }
    } else {
        // 回退到渐变背景
        writeln!(svg, r#"<rect width="100%" height="100%" fill="url(#bg-gradient)"/>"#).map_err(fmt_err)?;
    }
    // --- 背景结束 ---


    // --- Header ---
    let player_name = stats.player_name.as_deref().unwrap_or("Phigros Player");
    let real_rks = stats.real_rks.unwrap_or(0.0);
    writeln!(svg, r#"<text x="40" y="55" class="text-title">{}({:.6})</text>"#, escape_xml(player_name), real_rks).map_err(fmt_err)?;
    let ap_text = match stats.ap_top_3_avg {
        Some(avg) => format!("AP Top 3 Avg: {:.4}", avg),
        None => "AP Top 3 Avg: N/A".to_string(),
    };
    writeln!(svg, r#"<text x="40" y="85" class="text-stat">{}</text>"#, ap_text).map_err(fmt_err)?;
    let b27_avg_str = stats.best_27_avg.map_or("N/A".to_string(), |avg| format!("{:.4}", avg));
    let bn_text = format!("Best 27 Avg: {}", b27_avg_str);
    writeln!(svg, r#"<text x="40" y="110" class="text-stat">{}</text>"#, bn_text).map_err(fmt_err)?;

    // --- Right-aligned info (Data, Challenge, Time) ---
    let mut info_y = 65.0; // Starting Y position for the top-right info block

    // Data String
    if let Some(data_str) = &stats.data_string {
        writeln!(svg, r#"<text x="{}" y="{}" class="text-info">{}</text>"#, width - 30, info_y, escape_xml(data_str)).map_err(fmt_err)?;
        info_y += 20.0; // Increment Y for the next line
    }

    // Challenge Rank
    if let Some((color, level)) = &stats.challenge_rank {
        let color_hex = match color.as_str() {
            "Green" => "#51AF44",
            "Blue" => "#3173B3",
            "Red" => "#BE2D23",
            "Gold" => "#D1913C",
            "Rainbow" => "url(#ap-gradient)", // Use existing gold gradient for rainbow for now
            _ => text_secondary_color,
        };
        writeln!(svg, r#"<text x="{}" y="{}" class="text-info">Challenge: <tspan fill="{}">{}</tspan> {}</text>"#,
                 width - 30, info_y, color_hex, color, level).map_err(fmt_err)?;
        info_y += 20.0; // Increment Y for the next line
    }

    // Update Time (always displayed)
    let update_time = format!("Updated at {} UTC", stats.update_time.format("%Y/%m/%d %H:%M:%S"));
    writeln!(svg, r#"<text x="{}" y="{}" class="text-time">{}</text>"#, width - 30, info_y, update_time).map_err(fmt_err)?;

    writeln!(svg, "<line x1='40' y1='{}' x2='{}' y2='{}' stroke='{}' stroke-width='1' stroke-opacity='0.7'/>",
             header_height, width - 40, header_height, card_stroke_color).map_err(fmt_err)?;


    // --- AP Top 3 Section --- (保持不变) ...
    let ap_section_start_y = header_height + 15;
    if !stats.ap_top_3_scores.is_empty() {
        writeln!(svg, r#"<g id="ap-top-3-section" transform="translate(0, {})">"#, ap_section_start_y).map_err(fmt_err)?;
        for (idx, score) in stats.ap_top_3_scores.iter().take(3).enumerate() {
            let x_pos = ap_card_padding_outer + idx as u32 * (main_card_width + ap_card_padding_outer);
            
            // AP Top 3 卡片可能不需要推分ACC（因为已经是100%），但为了统一处理，也获取一下
            let push_acc = push_acc_map.and_then(|map| {
                let key = format!("{}-{}", score.song_id, score.difficulty);
                map.get(&key).copied()
            });
            
            generate_card_svg(&mut svg, score, idx, x_pos, ap_card_start_y, main_card_width, true, true, push_acc, scores, theme)?;
        }
        writeln!(svg, r#"</g>"#).map_err(fmt_err)?;
    }


    // --- Main Score Cards Section --- (保持不变) ...
    let main_content_start_y = header_height + ap_section_height + 15;
    for (index, score) in scores.iter().enumerate() {
        let row = index as u32 / columns;
        let col = index as u32 % columns;
        let x = main_card_padding_outer + col * (main_card_width + main_card_padding_outer);
        let y = main_content_start_y + main_card_padding_outer + row * (calculated_card_height + main_card_padding_outer);
        let is_ap_score = score.acc >= 100.0;
        
        // 获取预计算的推分ACC（如果有）
        let push_acc = push_acc_map.and_then(|map| {
            let key = format!("{}-{}", score.song_id, score.difficulty);
            map.get(&key).copied()
        });
        
        generate_card_svg(&mut svg, score, index, x, y, main_card_width, false, is_ap_score, push_acc, scores, theme)?;
    }


    // --- Footer ---
    let footer_y = (total_height - footer_height / 2 + 10) as f64;
    // 获取当前 UTC 时间并转换为 UTC+8
    let now_utc = Utc::now();
    // 东八区偏移量 (8 * 3600 seconds)
    // unwrap() 在这里是安全的，因为东八区偏移量是有效的
    let offset = FixedOffset::east_opt(8 * 3600).unwrap();
    let now_utc8 = now_utc.with_timezone(&offset);
    // 格式化时间并添加 UTC+8 标识
    let generated_text = format!("Generated by Phi-Backend at {} UTC+8", now_utc8.format("%Y/%m/%d %H:%M:%S"));
    writeln!(svg, r#"<text x="{}" y="{:.1}" class="text-footer" text-anchor="middle">{}</text>"#, width / 2, footer_y, generated_text).map_err(fmt_err)?;


    writeln!(svg, "</svg>").map_err(fmt_err)?;

    Ok(svg)
}

// ... (render_svg_to_png function - unchanged) ...
pub fn render_svg_to_png(svg_data: String) -> Result<Vec<u8>, AppError> {
    // 使用全局字体数据库
    let font_db = get_global_font_db();

    let opts = UsvgOptions {
        resources_dir: Some(std::env::current_dir().map_err(|e|
            AppError::InternalError(format!("Failed to get current dir: {}", e)))?),
        font_family: MAIN_FONT_NAME.to_string(),
        font_size: 16.0, // Default font size, can be overridden by CSS
        languages: vec!["zh-CN".to_string(), "en".to_string()],
        shape_rendering: usvg::ShapeRendering::GeometricPrecision,
        text_rendering: usvg::TextRendering::OptimizeLegibility,
        image_rendering: usvg::ImageRendering::OptimizeQuality,
        ..Default::default()
    };

    let font_db_rc = Arc::clone(&font_db);
    let tree = usvg::Tree::from_data(&svg_data.as_bytes(), &opts, &font_db_rc)
        .map_err(|e| AppError::InternalError(format!("Failed to parse SVG: {}", e)))?;

    let pixmap_size = tree.size().to_int_size();
    let mut pixmap = Pixmap::new(pixmap_size.width(), pixmap_size.height())
        .ok_or_else(|| AppError::InternalError("Failed to create pixmap".to_string()))?;

    render(&tree, Transform::default(), &mut pixmap.as_mut());

    pixmap.encode_png()
        .map_err(|e| AppError::InternalError(format!("Failed to encode PNG: {}", e)))
}


// ... (escape_xml function - unchanged) ...
fn escape_xml(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// 从图片路径计算主色的反色
fn calculate_inverse_color_from_path(path: &Path) -> Option<String> {
    // 使用 image crate 打开图片
    let img = image::open(path).ok()?;
    let pixels = img.to_rgba8().into_raw();

    if pixels.is_empty() {
        return None;
    }

    let mut total_r: u64 = 0;
    let mut total_g: u64 = 0;
    let mut total_b: u64 = 0;

    // 像素数据是扁平的 [R, G, B, A, R, G, B, A, ...] 数组
    for chunk in pixels.chunks_exact(4) {
        total_r += u64::from(chunk[0]);
        total_g += u64::from(chunk[1]);
        total_b += u64::from(chunk[2]);
    }

    let num_pixels = (pixels.len() / 4) as u64;
    if num_pixels == 0 { return None; }

    let avg_r = (total_r / num_pixels) as u8;
    let avg_g = (total_g / num_pixels) as u8;
    let avg_b = (total_b / num_pixels) as u8;

    // 计算反色
    let inv_r = 255 - avg_r;
    let inv_g = 255 - avg_g;
    let inv_b = 255 - avg_b;

    Some(format!("#{:02X}{:02X}{:02X}", inv_r, inv_g, inv_b))
}

/// 从图片路径计算主色的反色
// ... (load_custom_fonts function - unchanged) ...
fn load_custom_fonts(font_db: &mut fontdb::Database) -> Result<(), AppError> {
    let fonts_dir = PathBuf::from(FONTS_DIR);
    if !fonts_dir.exists() {
        log::warn!("Fonts directory does not exist: {}", fonts_dir.display());
        return Ok(());
    }
    let entries = match fs::read_dir(&fonts_dir) {
         Ok(entries) => entries,
         Err(e) => {
             log::error!("Failed to read fonts directory '{}': {}", fonts_dir.display(), e);
             return Err(AppError::InternalError(format!("Failed to read fonts directory: {}", e)));
         }
     };
    let mut fonts_loaded = false;
    for entry in entries {
         let entry = match entry {
             Ok(entry) => entry,
             Err(e) => {
                 log::warn!("Failed to read directory entry in {}: {}", fonts_dir.display(), e);
                 continue;
             }
         };
        let path = entry.path();
        if path.is_file() && path.extension().map_or(false, |ext| ext == "ttf" || ext == "otf") {
             log::debug!("Loading custom font: {}", path.display());
             match font_db.load_font_file(&path) {
                 Ok(_) => fonts_loaded = true,
                 Err(e) => {
                     log::error!("CRITICAL: Failed to load custom font file '{}': {}. Text rendering might be incorrect.", path.display(), e);
                 }
             }
        }
    }
    if !fonts_loaded {
        log::warn!("No custom fonts were successfully loaded from {}", fonts_dir.display());
    }
    Ok(())
}


// ... (find_first_font_file function - unchanged, can likely be removed too) ...
#[allow(dead_code)]
fn find_first_font_file(dir: &Path) -> Result<Option<PathBuf>, AppError> {
    if !dir.exists() { return Ok(None); }
    let entries = fs::read_dir(dir)
        .map_err(|e| AppError::InternalError(format!("Failed to read directory {}: {}", dir.display(), e)))?;
    for entry in entries {
        let entry = entry.map_err(|e| AppError::InternalError(format!("Failed to read directory entry: {}", e)))?;
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == "ttf" || ext == "otf" { return Ok(Some(path)); }
            }
        }
    }
    Ok(None)
}


// --- 新增：生成单曲成绩 SVG ---
pub fn generate_song_svg_string(
    data: &SongRenderData,
) -> Result<String, AppError> {
    let fmt_err = |e| AppError::InternalError(format!("SVG formatting error: {}", e));

    // --- 整体布局与尺寸（横版）---
    let width = 1200; // 图片宽度
    let height = 720; // 图片高度增加，解决底部重叠问题
    let padding = 30.0;
    
    // 玩家信息区域高度
    let player_info_height = 78.0; // 原来是70.0，增加8px (上下各4px)
    
    // 曲绘尺寸 - 保持2048x1080的比例，但整体缩小
    let illust_height = height as f64 - padding * 3.0 - player_info_height - 80.0; // 给标题、页脚和曲目名称留出空间
    let illust_width = illust_height * (2048.0 / 1080.0); // 保持2048x1080的比例
    
    // 确保曲绘不会超过整体宽度的55%（拉宽曲绘，原来是50%）
    let illust_width = (illust_width).min(width as f64 * 0.60);
    
    // 曲目名称区域高度
    let song_name_height = 50.0;
    
    let _difficulty_info_height = 40.0; // Prefix unused variable
    
    // 成绩卡尺寸 - 调整为与曲绘总高度一致
    let card_area_width = width as f64 - illust_width - padding * 3.0;
    let difficulty_card_width = card_area_width;
    // 总共4张卡片，高度加上3个间距等于曲绘高度
    let difficulty_spacing_total = padding * 0.6 * 3.0; // 3个间距
    let difficulty_card_height = (illust_height - difficulty_spacing_total) / 4.0; // 每张卡片高度
    let difficulty_card_spacing = padding * 0.6; // 卡片间距略微调整

    let mut svg = String::new();

    // --- 获取随机背景图 ---
    let mut background_image_href = None;
    // 优先尝试使用当前曲目的曲绘作为背景
    let current_song_ill_path_png = PathBuf::from(cover_loader::COVERS_DIR).join("ill").join(format!("{}.png", data.song_id));
    let current_song_ill_path_jpg = PathBuf::from(cover_loader::COVERS_DIR).join("ill").join(format!("{}.jpg", data.song_id));

    // 优先尝试使用当前曲目的曲绘作为背景
    if current_song_ill_path_png.exists() {
        // 使用缓存函数获取背景图片
        if let Some(image_data) = get_background_image(&current_song_ill_path_png) {
            background_image_href = Some(image_data);
            log::info!("使用当前曲目曲绘作为背景: {}", current_song_ill_path_png.display());
        }
    } else if current_song_ill_path_jpg.exists() {
        // 使用缓存函数获取背景图片
        if let Some(image_data) = get_background_image(&current_song_ill_path_jpg) {
            background_image_href = Some(image_data);
            log::info!("使用当前曲目曲绘作为背景: {}", current_song_ill_path_jpg.display());
        }
    } else {
        // 如果找不到当前曲目的曲绘，则随机选一个
        let cover_files = get_cover_files();
        if !cover_files.is_empty() {
            let mut rng = thread_rng();
            if let Some(random_path) = cover_files.choose(&mut rng) {
                // 使用缓存函数获取背景图片
                if let Some(image_data) = get_background_image(random_path) {
                    background_image_href = Some(image_data);
                    log::info!("使用随机背景图: {}", random_path.display());
                } else {
                    log::error!("获取背景图片失败: {}", random_path.display());
                    // 获取失败则回退到渐变
                }
            } else {
                log::warn!("无法从封面文件列表中随机选择一个");
            }
        } else {
            log::warn!("找不到任何封面文件用于随机背景");
        }
    }

    // --- SVG 头部和 Defs ---
    writeln!(svg, r#"<svg width="{}" height="{}" viewBox="0 0 {} {}" xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink">"#,
             width, height, width, height).map_err(fmt_err)?;
    writeln!(svg, "<defs>").map_err(fmt_err)?;
    // Style
    writeln!(svg, "<style>").map_err(fmt_err)?;
    writeln!(svg, r#"
        /* 基本文本样式 */
        .text {{ font-family: '{}', sans-serif; fill: #E0E0E0; }}
        .text-title {{ font-size: 32px; font-weight: bold; fill: #FFFFFF; }}
        .text-subtitle {{ font-size: 18px; fill: #B0B0B0; }}
        .text-label {{ font-size: 28px; font-weight: bold; }} /* 增大难度标签字体 */
        .text-value {{ font-size: 18px; fill: #E0E0E0; }}
        .text-score {{ font-size: 32px; font-weight: bold; }}
        .text-acc {{ font-size: 20px; fill: #A0A0A0; }} /* 增大ACC字体 */
        .text-rks {{ font-size: 20px; fill: #E0E0E0; }} /* 新增单独的RKS样式 */
        .text-push-acc {{ font-size: 18px; }}
        .text-songname {{ font-size: 24px; font-weight: bold; fill: #FFFFFF; text-anchor: middle; }}
        .text-player-info {{ font-size: 22px; font-weight: bold; fill: #FFFFFF; }}
        .text-player-rks {{ font-size: 20px; fill: #E0E0E0; }}
        .text-difficulty-ez {{ fill: #77DD77; }}
        .text-difficulty-hd {{ fill: #87CEEB; }}
        .text-difficulty-in {{ fill: #FFB347; }}
        .text-difficulty-at {{ fill: #FF6961; }}
        .text-footer {{ font-size: 14px; fill: #888888; text-anchor: end; }}
        .text-constants {{ font-size: 16px; fill: #CCCCCC; }}
        .player-info-card {{ fill: rgba(40, 45, 60, 0.8); stroke: rgba(100, 100, 100, 0.4); stroke-width: 1; }}
        .difficulty-card {{ fill: rgba(40, 45, 60, 0.8); stroke: rgba(100, 100, 100, 0.4); stroke-width: 1; }}
        .difficulty-card-inactive {{ fill: rgba(40, 45, 60, 0.5); stroke: rgba(70, 70, 70, 0.3); stroke-width: 1; }}
        .difficulty-card-fc {{ fill: rgba(40, 45, 60, 0.8); stroke: #ADD8E6; stroke-width: 3; }} /* FC蓝色边框 */
        .difficulty-card-phi {{ fill: rgba(40, 45, 60, 0.8); stroke: gold; stroke-width: 3; }} /* AP/Phi金色边框 */
        .song-name-card {{ fill: rgba(40, 45, 60, 0.8); stroke: rgba(100, 100, 100, 0.4); stroke-width: 1; }}
        .constants-card {{ fill: rgba(40, 45, 60, 0.8); stroke: rgba(100, 100, 100, 0.4); stroke-width: 1; }}
        .rank-phi {{ fill: gold; }}
        .rank-v {{ fill: silver; }}
        .rank-s {{ fill: #FF6B6B; }}
    "#, MAIN_FONT_NAME).map_err(fmt_err)?;
    writeln!(svg, "</style>").map_err(fmt_err)?;
    
    // ... existing gradient and filter definitions ...
    writeln!(svg, r#"<linearGradient id="bg-gradient" x1="0%" y1="0%" x2="100%" y2="100%"><stop offset="0%" style="stop-color:#141826" /><stop offset="100%" style="stop-color:#252E48" /></linearGradient>"#).map_err(fmt_err)?;
    writeln!(svg, r#"<filter id="card-shadow" x="-10%" y="-10%" width="120%" height="130%"><feDropShadow dx="0" dy="3" stdDeviation="3" flood-color="rgba(0,0,0,0.25)" flood-opacity="0.25" /></filter>"#).map_err(fmt_err)?;
    writeln!(svg, r#"<filter id="bg-blur"><feGaussianBlur stdDeviation="10" /></filter>"#).map_err(fmt_err)?;
    writeln!(svg, r#"<linearGradient id="rks-gradient" x1="0%" y1="0%" x2="100%" y2="0%"><stop offset="0%" style="stop-color:#FDC830" /><stop offset="100%" style="stop-color:#F37335" /></linearGradient>"#).map_err(fmt_err)?;
    writeln!(svg, r#"<linearGradient id="rks-gradient-ap" x1="0%" y1="0%" x2="100%" y2="0%"><stop offset="0%" style="stop-color:#f6d365" /><stop offset="100%" style="stop-color:#fda085" /></linearGradient>"#).map_err(fmt_err)?;
    writeln!(svg, r#"<linearGradient id="rks-gradient-push" x1="0%" y1="0%" x2="100%" y2="0%"><stop offset="0%" style="stop-color:#a8e063" /><stop offset="100%" style="stop-color:#56ab2f" /></linearGradient>"#).map_err(fmt_err)?;
    writeln!(svg, "</defs>").map_err(fmt_err)?;

    // --- Background ---
    if let Some(href) = background_image_href {
        writeln!(svg, r#"<image href="{}" x="0" y="0" width="100%" height="100%" preserveAspectRatio="xMidYMid slice" filter="url(#bg-blur)" />"#, href).map_err(fmt_err)?;
        writeln!(svg, r#"<rect width="100%" height="100%" fill="rgba(20, 24, 38, 0.7)" />"#).map_err(fmt_err)?;
    } else {
        writeln!(svg, r#"<rect width="100%" height="100%" fill="url(#bg-gradient)"/>"#).map_err(fmt_err)?;
    }

    // --- 玩家信息区域（顶部） ---
    let player_info_x = padding;
    let player_info_y = padding;
    let player_info_width = width as f64 - padding * 2.0;
    
    // 玩家信息卡片
    writeln!(svg, r#"<rect x="{}" y="{}" width="{}" height="{}" rx="8" ry="8" class="player-info-card" filter="url(#card-shadow)" />"#,
             player_info_x, player_info_y, player_info_width, player_info_height).map_err(fmt_err)?;
    
    // 玩家名称 - 加前缀"Player："并移除歌曲名
    let player_name_display = data.player_name.as_deref().unwrap_or("Player");
    writeln!(svg, r#"<text x="{}" y="{}" class="text text-player-info">Player: {}</text>"#, 
             player_info_x + 20.0, player_info_y + 49.0, player_name_display).map_err(fmt_err)?;
    
    // 时间戳放在右侧
    let shanghai_offset = FixedOffset::east_opt(8 * 3600).unwrap();
    let local_time = data.update_time.with_timezone(&shanghai_offset);
    let time_str = local_time.format("%Y-%m-%d %H:%M:%S").to_string();
    writeln!(svg, r#"<text x="{}" y="{}" class="text text-subtitle" text-anchor="end">{}</text>"#,
             width as f64 - padding - 20.0, player_info_y + 49.0, time_str).map_err(fmt_err)?;

    // --- 曲绘（左侧）---
    let illust_x = padding;
    let illust_y = player_info_y + player_info_height + padding; // 在玩家信息区域下方
    let illust_href = data.illustration_path.as_ref().and_then(|p| {
        p.canonicalize().ok().map(|canon_p| canon_p.to_string_lossy().into_owned())
    });
    
    // 曲绘裁剪路径（圆角矩形）
    let illust_clip_id = "illust-clip";
    writeln!(svg, "<defs><clipPath id=\"{}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"10\" ry=\"10\" /></clipPath></defs>",
             illust_clip_id, illust_x, illust_y, illust_width, illust_height).map_err(fmt_err)?;
    
    // 曲绘图片或占位矩形
    if let Some(href) = illust_href {
        writeln!(svg, r#"<image href="{}" x="{}" y="{}" width="{}" height="{}" clip-path="url(#{})" preserveAspectRatio="xMidYMid slice" />"#,
                 escape_xml(&href), illust_x, illust_y, illust_width, illust_height, illust_clip_id).map_err(fmt_err)?;
    } else {
        writeln!(svg, "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#333\" rx=\"10\" ry=\"10\"/>",
                 illust_x, illust_y, illust_width, illust_height).map_err(fmt_err)?;
    }
    
    // --- 曲目名称（曲绘下方） ---
    let song_name_x = illust_x;
    let song_name_y = illust_y + illust_height + padding / 2.0;
    let song_name_width = illust_width;
    
    // 曲目名称背景卡片
    writeln!(svg, r#"<rect x="{}" y="{}" width="{}" height="{}" rx="8" ry="8" class="song-name-card" filter="url(#card-shadow)" />"#,
             song_name_x, song_name_y, song_name_width, song_name_height).map_err(fmt_err)?;
    
    // 曲目名称文字（居中）
    writeln!(svg, r#"<text x="{}" y="{}" class="text text-songname">{}</text>"#,
             song_name_x + song_name_width / 2.0, song_name_y + song_name_height / 2.0 + 8.0, escape_xml(&data.song_name)).map_err(fmt_err)?;

    // --- 难度卡片（右侧垂直排列）---
    let difficulties = ["EZ", "HD", "IN", "AT"]; // 难度顺序
    
    // 计算右侧卡片区域的起始位置
    let cards_start_x = illust_x + illust_width + padding;
    let cards_start_y = illust_y; // 与曲绘顶部对齐
    
    // 收集所有定数值用于底部展示
    let mut constants = Vec::new();
    
    // 渲染四个难度卡片
    for (i, &diff_key) in difficulties.iter().enumerate() {
        let pos_x = cards_start_x;
        let pos_y = cards_start_y + (difficulty_card_height + difficulty_card_spacing) * i as f64;
        
        // 收集定数值
        if let Some(Some(score_data)) = data.difficulty_scores.get(diff_key) {
            if let Some(dv) = score_data.difficulty_value {
                constants.push((diff_key, dv));
            }
        }
        
        // 检查是否有该难度的数据，决定卡片样式
        let has_difficulty_data = data.difficulty_scores.get(diff_key)
            .map_or(false, |opt| opt.as_ref().map_or(false, |score| score.acc.is_some()));
        
        // 判断是否是FC或Phi，选择相应的卡片样式
        let card_class = if has_difficulty_data {
            if let Some(Some(score_data)) = data.difficulty_scores.get(diff_key) {
                if score_data.is_phi == Some(true) {
                    "difficulty-card-phi" // Phi/AP成绩使用金色边框
                } else if score_data.is_fc == Some(true) {
                    "difficulty-card-fc" // FC成绩使用蓝色边框
                } else {
                    "difficulty-card" // 普通成绩使用默认边框
                }
            } else {
                "difficulty-card"
            }
        } else {
            "difficulty-card-inactive" // 无数据使用灰色卡片
        };

        // 绘制卡片背景 (添加圆角)
        writeln!(svg, r#"<rect x="{}" y="{}" width="{}" height="{}" rx="8" ry="8" class="{}" filter="url(#card-shadow)" />"#,
                 pos_x, pos_y, difficulty_card_width, difficulty_card_height, card_class).map_err(fmt_err)?;

        // 卡片内容边距
        let content_padding = 20.0;
        
        // 计算卡片中央分隔线 - 将卡片分为左右两部分
        let card_middle = pos_x + content_padding + 70.0; // 难度标签占用左侧区域，宽度为70px
        
        // 难度标签 - 垂直居中位置，仅显示在左侧
        let diff_label_class = format!("text text-label text-difficulty-{}", diff_key.to_lowercase());
        let label_x = pos_x + content_padding + 35.0;  // 左侧居中
        let label_y = pos_y + difficulty_card_height / 2.0 + 10.0; // 垂直居中位置，+10是为了视觉上的微调
        
        writeln!(svg, r#"<text x="{}" y="{}" class="{}" text-anchor="middle">{}</text>"#,
                label_x, label_y, diff_label_class, diff_key).map_err(fmt_err)?;

        // 判断是否有该难度的谱面数据
        let has_difficulty_chart = data.difficulty_scores.get(diff_key)
            .map_or(false, |opt| opt.as_ref().map_or(false, |score| score.difficulty_value.is_some()));
        
        // 该难度的成绩信息 - 放在右侧区域
        let right_area_start = card_middle;
        let right_area_width = difficulty_card_width - (card_middle - pos_x);
        let right_area_center = right_area_start + right_area_width / 2.0;
        
        if let Some(Some(score_data)) = data.difficulty_scores.get(diff_key) {
            // 有成绩数据
            if score_data.acc.is_some() {
                // 有ACC记录，显示完整成绩信息
                
                // 分数显示 - 在卡片上部左侧
                let score_text = score_data.score.map_or("N/A".to_string(), |s| format!("{:.0}", s));
                let score_x = right_area_start + 20.0;  // 靠左对齐
                let score_y = pos_y + difficulty_card_height * 0.4; // 上部位置
                
                writeln!(svg, r#"<text x="{}" y="{}" class="text text-score" text-anchor="start">{}</text>"#,
                         score_x, score_y, score_text).map_err(fmt_err)?;
                
                // RKS显示 - 跟分数同行，靠右
                let rks_value = score_data.rks.unwrap_or(0.0);
                let rks_text = format!("RKS: {:.2}", rks_value);
                let rks_x = pos_x + difficulty_card_width - content_padding; // 靠右对齐
                let rks_y = score_y; // 与分数在同一水平线
                
                // RKS使用渐变色
                let rks_fill = if score_data.is_phi == Some(true) { 
                    "url(#rks-gradient-ap)" 
                } else { 
                    "url(#rks-gradient)" 
                };
                
                writeln!(svg, r#"<text x="{}" y="{}" class="text text-rks" text-anchor="end" fill="{}">{}</text>"#,
                         rks_x, rks_y, rks_fill, rks_text).map_err(fmt_err)?;
                     
                // ACC显示 - 在下部位置，靠左
                let acc_value = score_data.acc.unwrap_or(0.0);
                let acc_text = format!("ACC: {:.2}%", acc_value);
                let acc_x = right_area_start + 20.0; // 靠左对齐
                let acc_y = pos_y + difficulty_card_height * 0.7; // 在卡片下部
                
                // 根据是否是AP决定填充颜色
                let acc_fill = if score_data.is_phi == Some(true) { 
                    "url(#rks-gradient-ap)" 
                } else { 
                    "#A0A0A0" 
                };
                
                writeln!(svg, r#"<text x="{}" y="{}" class="text text-acc" text-anchor="start" fill="{}">{}</text>"#,
                         acc_x, acc_y, acc_fill, acc_text).map_err(fmt_err)?;

                // 推分ACC - 与ACC同行，靠右
                if let Some(push_acc) = score_data.player_push_acc {
                    let push_acc_y = acc_y; // 与ACC在同一水平线
                    let push_acc_x = rks_x; // 与RKS同样位置（右对齐）
                    
                    let push_acc_display = if push_acc >= 100.0 {
                        if score_data.is_phi == Some(true) {
                            format!("<tspan fill=\"gold\">已 Phi</tspan>")
                        } else {
                            format!("<tspan fill=\"gold\">-> 100.00%</tspan>")
                        }
                    } else {
                        format!(r#"<tspan fill="url(#rks-gradient-push)">→ {:.2}%</tspan>"#, push_acc)
                    };
                    
                    writeln!(svg, r#"<text x="{}" y="{}" class="text text-push-acc" text-anchor="end">{}</text>"#,
                             push_acc_x, push_acc_y, push_acc_display).map_err(fmt_err)?;
                }

                // 不再显示FC/Phi文字标记，改用边框显示
            } else if has_difficulty_chart {
                // 有难度定数但无成绩，显示"无成绩"
                let no_data_x = right_area_center;
                let no_data_y = pos_y + difficulty_card_height / 2.0 + 5.0; // 垂直居中
                writeln!(svg, r#"<text x="{}" y="{}" class="text text-acc" text-anchor="middle" dominant-baseline="middle">无成绩</text>"#, 
                         no_data_x, no_data_y).map_err(fmt_err)?;
            }
        } else {
            // 没有数据时，显示"无谱面"
            let no_data_x = right_area_center;
            let no_data_y = pos_y + difficulty_card_height / 2.0 + 5.0; // 垂直居中
            writeln!(svg, r#"<text x="{}" y="{}" class="text text-acc" text-anchor="middle" dominant-baseline="middle">无谱面</text>"#, 
                     no_data_x, no_data_y).map_err(fmt_err)?;
        }
    }
    
    // --- 难度定数展示区域 ---
    let constants_x = cards_start_x;
    let constants_y = song_name_y; // 与歌曲名框顶部对齐
    let constants_width = difficulty_card_width;
    let constants_height = song_name_height; // 使用相同高度

    // 背景卡片
    writeln!(svg, r#"<rect x="{}" y="{}" width="{}" height="{}" rx="8" ry="8" class="constants-card" filter="url(#card-shadow)" />"#,
             constants_x, constants_y, constants_width, constants_height).map_err(fmt_err)?;
    
    // 显示定数文本
    if !constants.is_empty() {
        let text_y = constants_y + constants_height / 2.0 + 6.0; // 垂直居中，+6是微调
        
        // 计算文本位置，平均分布
        let segment_width = constants_width / constants.len() as f64;
        
        for (i, (diff_key, constant)) in constants.iter().enumerate() {
            let text_x = constants_x + segment_width * (i as f64 + 0.5);
            let diff_color = match *diff_key {
                "EZ" => "#77DD77",
                "HD" => "#87CEEB",
                "IN" => "#FFB347",
                "AT" => "#FF6961",
                _ => "#FFFFFF"
            };
            
            writeln!(svg, r#"<text x="{}" y="{}" class="text text-constants" text-anchor="middle"><tspan fill="{}">{}</tspan> {:.1}</text>"#,
                     text_x, text_y, diff_color, diff_key, constant).map_err(fmt_err)?;
        }
    } else {
        // 如果没有定数数据，也调整垂直位置
        let text_x = constants_x + constants_width / 2.0;
        let text_y = constants_y + constants_height / 2.0 + 6.0; // 垂直居中，+6是微调
        
        writeln!(svg, r#"<text x="{}" y="{}" class="text text-constants" text-anchor="middle">无定数数据</text>"#,
                 text_x, text_y).map_err(fmt_err)?;
    }

    // --- Footer ---
    let footer_y = height as f64 - padding / 2.0;
    let footer_x = width as f64 - padding;
    let time_str = local_time.format("%Y-%m-%d %H:%M:%S UTC+8").to_string(); // 使用UTC+8表示时区
    writeln!(svg, r#"<text x="{}" y="{}" class="text text-footer">Generated by Phi-Backend | {}</text>"#,
             footer_x, footer_y, time_str).map_err(fmt_err)?;

    // --- End SVG ---
    writeln!(svg, "</svg>").map_err(fmt_err)?;

    Ok(svg)
}

/// 生成排行榜SVG字符串
pub fn generate_leaderboard_svg_string(data: &LeaderboardRenderData) -> Result<String, AppError> {
    // -- 定义 fmt_err 闭包 --
    let fmt_err = |e| AppError::InternalError(format!("SVG formatting error: {}", e));
    // -- 结束定义 --
    
    let width = 1200;
    let row_height = 60;
    let header_height = 120;
    let footer_height = 40;
    let total_height = header_height + (data.entries.len() as i32 * row_height as i32) + footer_height;

    let mut svg = String::with_capacity(20000);
    svg.push_str(&format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 {} {}">"#, 
                width, total_height, width, total_height));

    // 添加渐变背景和样式
    // 使用 r##"..."## 来避免 # 颜色值与原始字符串分隔符冲突
    svg.push_str(r##"
    <defs>
        <linearGradient id="bg-gradient" x1="0%" y1="0%" x2="100%" y2="100%">
            <stop offset="0%" stop-color="#1a1a2e" />
            <stop offset="100%" stop-color="#16213e" />
        </linearGradient>
        <style>
            @font-face {
                font-family: 'NotoSansSC';
                src: url('https://fonts.gstatic.com/s/notosanssc/v36/k3kXo84MPvpLmixcA63oeALhLIiP-Q-87KaAavc.woff2') format('woff2');
            }
            .header-text { 
                font-family: 'NotoSansSC', sans-serif; 
                font-size: 48px; 
                fill: white; 
                text-anchor: middle; 
                font-weight: bold; /* 加粗标题 */
            }
            .rank-text { 
                font-family: 'NotoSansSC', sans-serif; 
                font-size: 32px; 
                fill: white; 
                text-anchor: middle; 
                font-weight: bold;
            }
            .name-text { 
                font-family: 'NotoSansSC', sans-serif; 
                font-size: 32px; 
                fill: white; 
                text-anchor: start; 
            }
            .rks-text { 
                font-family: 'NotoSansSC', sans-serif; 
                font-size: 32px; 
                fill: white; 
                text-anchor: end; 
                font-weight: bold;
            }
            .footer-text { 
                font-family: 'NotoSansSC', sans-serif; 
                font-size: 20px; 
                fill: #aaaaaa; 
                text-anchor: end; 
            }
        </style>
    </defs>
"##); // <--- 修正结束符的位置，紧跟在 </defs> 之后

    // 绘制背景
    svg.push_str(&format!(r#"<rect width="{}" height="{}" fill="url(#bg-gradient)" />"#, width, total_height));

    // 绘制标题
    svg.push_str(&format!(r#"<text x="{}" y="{}" class="header-text">{}</text>"#, 
                width / 2, header_height / 2 + 16, data.title));

    // 绘制表头分隔线
    write!(svg, r##"<line x1="20" y1="{}" x2="{}" y2="{}" stroke="#4a5568" stroke-width="2" />"##, 
            header_height, width - 20, header_height).map_err(fmt_err)?;

    // 绘制排行榜条目
    for (i, entry) in data.entries.iter().enumerate() {
        let y_pos = header_height + (i as i32 * row_height as i32);
        
        // 绘制排名
        write!(svg, r##"<text x="60" y="{}" class="rank-text">#{}</text>"##, 
                y_pos + (row_height / 2) as i32 + 10, i + 1).map_err(fmt_err)?;
        
        // 绘制玩家名
        let name_display = if entry.player_name.len() > 20 {
            format!("{}...", &entry.player_name[0..17])
        } else {
            entry.player_name.clone()
        };
        write!(svg, r##"<text x="120" y="{}" class="name-text">{}</text>"##, 
                y_pos + (row_height / 2) as i32 + 10, name_display).map_err(fmt_err)?;
        
        // 绘制RKS
        write!(svg, r##"<text x="{}" y="{}" class="rks-text">{:.2}</text>"##, 
                width - 60, y_pos + (row_height / 2) as i32 + 10, entry.rks).map_err(fmt_err)?;
        
        // 如果不是最后一行，绘制分隔线
        if i < data.entries.len() - 1 {
            let line_y = y_pos + row_height as i32; // Cast here
            write!(svg, r##"<line x1="100" y1="{}" x2="{}" y2="{}" stroke="#2d3748" stroke-width="1" />"##, 
                    line_y, width - 100, line_y).map_err(fmt_err)?;
        }
    }

    // 绘制底部更新时间
    let time_str = data.update_time.format("%Y-%m-%d %H:%M:%S").to_string();
    svg.push_str(&format!(r#"<text x="{}" y="{}" class="footer-text">更新时间: {} UTC</text>"#, 
                width - 60, total_height - 15, time_str));

    svg.push_str("</svg>");
    Ok(svg)
}