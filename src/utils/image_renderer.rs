use crate::models::rks::RksRecord;
use crate::utils::error::AppError;
use crate::utils::rks_utils;
use crate::utils::cover_loader;
use resvg::usvg::{self, Options as UsvgOptions, fontdb};
use resvg::{render, tiny_skia::{Pixmap, Transform}};
use std::path::{PathBuf, Path}; // Ensure Path is imported
use std::rc::Rc;
// 确保导入了 FixedOffset 和 TimeZone
use chrono::{DateTime, Utc, FixedOffset/*, TimeZone*/};
use std::fmt::Write;
use std::collections::{HashMap, HashSet}; // 导入 HashMap
// use itertools::Itertools; // Remove unused import
use std::fs;
use base64::{engine::general_purpose::STANDARD as base64_engine, Engine as _}; // Added
// 导入 rand 相关
use rand::seq::SliceRandom;
use rand::thread_rng;
use crate::models::player_archive::RKSRankingEntry;


#[allow(dead_code)]
pub struct PlayerStats {
    pub ap_top_3_avg: Option<f64>,
    pub best_27_avg: Option<f64>,
    pub real_rks: Option<f64>,
    pub player_name: Option<String>,
    pub update_time: DateTime<Utc>,
    pub n: u32,  // 请求的 Best N 数量
    pub ap_top_3_scores: Vec<RksRecord>, // 添加 AP Top 3 的具体成绩
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

    let card_class = if is_ap_score { "card card-ap" } else { "card" };

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
         writeln!(svg, "<rect x='{}' y='{}' width='{:.1}' height='{:.1}' fill='#333' rx='4' ry='4'/>",
                  cover_x, cover_y, cover_size_w, cover_size_h).map_err(fmt_err)?;
    }

    // Text content positioning
    let text_x = cover_x + cover_size_w + 15.0; // Padding between cover and text
    let text_width = (card_width as f64) - text_x - card_padding; // Available width for text

    // Calculate Y positions for text lines to align with cover
    let song_name_y = cover_y + text_line_height_song * 0.75; // Adjust baseline alignment
    let score_y = song_name_y + text_line_height_score * 0.8 + text_block_spacing;
    let acc_y = score_y + text_line_height_acc + text_block_spacing;
    let level_y = acc_y + text_line_height_level + text_block_spacing;

    // Song Name - 截断逻辑
    let max_char_width_approx = 11.0; // 大致估计每个字符的平均宽度
    let max_name_len = (text_width / max_char_width_approx).max(8.0) as usize; // 计算能容纳的最大字符数
    let song_name = if score.song_name.chars().count() > max_name_len {
        // 使用 saturating_sub 避免负数长度
        format!("{}...", score.song_name.chars().take(max_name_len.saturating_sub(3)).collect::<String>())
    } else {
        score.song_name.clone()
    };

    // 移除 textLength 和 lengthAdjust
    writeln!(svg, r#"<text x="{}" y="{:.1}" class="text-songname">{}</text>"#, text_x, song_name_y, escape_xml(&song_name)).map_err(fmt_err)?;

    // Score
    let score_text = score.score.map_or("N/A".to_string(), |s| format!("{:.0}", s));
    writeln!(svg, r#"<text x="{}" y="{:.1}" class="text-score">{}</text>"#, text_x, score_y, score_text).map_err(fmt_err)?;

    // Accuracy (带推分acc)
    let acc_text = if !is_ap_score && score.acc < 100.0 && score.difficulty_value > 0.0 { // 只有定数>0时才显示推分
        // 如果有预计算的推分ACC，优先使用
        let push_acc = if let Some(pa) = pre_calculated_push_acc {
            pa
        } else {
            // 否则使用旧算法计算
            rks_utils::calculate_min_push_acc(score.rks, score.difficulty_value)
        };

        // 如果推分acc非常接近100，直接显示 -> 100.00%
        if push_acc > 99.995 {
             format!("Acc: {:.2}% -> 100.00%", score.acc)
        }
        // 如果两者差值非常小(小于0.005，对应四舍五入后两位不变)，则展示三位小数
        else if (push_acc - score.acc).abs() < 0.005 {
            format!("Acc: {:.2}% -> {:.3}%", score.acc, push_acc)
        } else {
            format!("Acc: {:.2}% -> {:.2}%", score.acc, push_acc)
        }
    } else {
        // AP或者已满分或者定数为0，只显示当前acc
        format!("Acc: {:.2}%", score.acc)
    };
    writeln!(svg, r#"<text x="{}" y="{:.1}" class="text-acc">{}</text>"#, text_x, acc_y, acc_text).map_err(fmt_err)?;

    // Level & RKS
    // 获取难度标签文本
    let difficulty_text = match &score.difficulty {
        diff if diff.eq_ignore_ascii_case("EZ") => "EZ",
        diff if diff.eq_ignore_ascii_case("HD") => "HD",
        diff if diff.eq_ignore_ascii_case("IN") => "IN",
        diff if diff.eq_ignore_ascii_case("AT") => "AT",
        _ => "??"
    };

    // Level & RKS (现在包含难度标签)
    let level_text = format!("{} Lv.{} -> {:.2}", difficulty_text, score.difficulty_value, score.rks);
    writeln!(svg, r#"<text x="{}" y="{:.1}" class="text-level">{}</text>"#, text_x, level_y, level_text).map_err(fmt_err)?;

    // Rank (Only for main scores, not AP)
    if !is_ap_card {
        let rank_text = format!("#{}", index + 1);
        writeln!(svg, r#"<text x="{}" y="{:.1}" class="text-rank">{}</text>"#, (card_width as f64) - card_padding, level_y, rank_text).map_err(fmt_err)?;
    }

    writeln!(svg, "</g>").map_err(fmt_err)?; // End card group
    Ok(())
}


// --- SVG 生成函数 ---

pub fn generate_svg_string(
    scores: &[RksRecord],
    stats: &PlayerStats,
    push_acc_map: Option<&HashMap<String, f64>>, // 新增：预先计算的推分ACC映射，键为"曲目ID-难度"
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


    let mut svg = String::new();
    let fmt_err = |e| AppError::InternalError(format!("SVG formatting error: {}", e));

    // --- 获取随机背景图 ---
    let mut background_image_href = None;
    let _background_fill = "url(#bg-gradient)".to_string(); // Prefix unused variable

    let cover_base_path = PathBuf::from(cover_loader::COVERS_DIR).join("ill"); // 指向 ill 目录
    let cover_files: Vec<PathBuf> = match fs::read_dir(&cover_base_path) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok()) // Ignore read errors for individual entries
            .map(|entry| entry.path())
            .filter(|path| path.is_file() && (path.extension() == Some("png".as_ref()) || path.extension() == Some("jpg".as_ref())))
            .collect(),
        Err(e) => {
            log::error!("读取封面目录失败 '{}': {}", cover_base_path.display(), e);
            Vec::new() // Empty vec, will fallback to gradient
        }
    };

    if !cover_files.is_empty() {
        let mut rng = thread_rng();
        if let Some(random_path) = cover_files.choose(&mut rng) { // 随机选择一个路径
            match fs::read(random_path) { // 读取随机选择的文件
                Ok(data) => {
                    let mime_type = if random_path.extension().map_or(false, |ext| ext == "png") {
                        "image/png"
                    } else {
                        "image/jpeg"
                    };
                    let base64_encoded = base64_engine.encode(&data); // Base64 编码
                    background_image_href = Some(format!("data:{};base64,{}", mime_type, base64_encoded));
                    log::info!("使用随机背景图: {}", random_path.display());
                }
                Err(e) => {
                    log::error!("读取随机背景封面文件失败 '{}': {}", random_path.display(), e);
                    // 读取失败则回退到渐变
                }
            }
        } else {
             log::warn!("无法从封面文件列表中随机选择一个");
             // Fallback to gradient if choose fails (shouldn't happen with non-empty list)
        }
    } else {
         log::warn!("在 '{}' 目录中找不到任何 .png 或 .jpg 封面用于随机背景", cover_base_path.display());
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
    writeln!(svg, r#"<linearGradient id="bg-gradient" x1="0%" y1="0%" x2="100%" y2="100%"><stop offset="0%" style="stop-color:#141826" /><stop offset="100%" style="stop-color:#252E48" /></linearGradient>"#).map_err(fmt_err)?;

    // Shadow Filter Definition
    writeln!(svg, r#"<filter id="card-shadow" x="-10%" y="-10%" width="120%" height="130%"><feDropShadow dx="0" dy="3" stdDeviation="3" flood-color="rgba(0,0,0,0.25)" flood-opacity="0.25" /></filter>"#).map_err(fmt_err)?;

    // Gaussian Blur Filter Definition
    writeln!(svg, r#"<filter id="bg-blur">"#).map_err(fmt_err)?;
    // 调整 stdDeviation 控制模糊程度, 15 是一个比较强的模糊效果
    writeln!(svg, r#"<feGaussianBlur stdDeviation="15" />"#).map_err(fmt_err)?;
    writeln!(svg, r#"</filter>"#).map_err(fmt_err)?;

    // Font style ... (保持不变) ...
    writeln!(svg, "<style>").map_err(fmt_err)?;
    write!(
        svg,
        r#"
        /* <![CDATA[ */
        svg {{ background-color: #141826; /* Fallback background color */ }}
        .card {{
            fill: #1A1E2A;
            stroke: #333848;
            stroke-width: 1;
            filter: url(#card-shadow);
            transition: all 0.3s ease;
        }}
        .card-ap {{
          stroke: url(#ap-gradient);
          stroke-width: 2;
        }}
        /* ... (其他样式保持不变) ... */
        .text-title {{ font-size: 34px; fill: white; /* font-weight: bold; */ text-shadow: 0px 2px 4px rgba(0, 0, 0, 0.4); }}
        .text-stat {{ font-size: 21px; fill: #E0E0E0; }}
        .text-time {{ font-size: 14px; fill: #999; text-anchor: end; }}
        .text-footer {{ font-size: 13px; fill: #888; }}
        .text-songname {{ font-size: 19px; fill: white; /* font-weight: bold; */ }}
        .text-score {{ font-size: 26px; fill: white; /* font-weight: bold; */ }}
        .text-acc {{ font-size: 14px; fill: #bbb; }}
        .text-level {{ font-size: 14px; fill: #bbb; }}
        .text-rank {{ font-size: 14px; fill: #999; text-anchor: end; }}
        .text-section-title {{ font-size: 21px; fill: white; /* font-weight: bold; */ }}
        * {{ font-family: "{font_name}", "Microsoft YaHei", "SimHei", "DengXian", Arial, sans-serif; }}
        /* ]]> */
        "#,
        font_name = MAIN_FONT_NAME
    ).map_err(fmt_err)?;
    writeln!(svg, "</style>").map_err(fmt_err)?;


    // Define AP card stroke gradient ... (保持不变) ...
    writeln!(svg, r#"<linearGradient id="ap-gradient" x1="0%" y1="0%" x2="100%" y2="100%">"#).map_err(fmt_err)?;
    writeln!(svg, "<stop offset=\"0%\" style=\"stop-color:#FFDA63\" />").map_err(fmt_err)?;
    writeln!(svg, "<stop offset=\"100%\" style=\"stop-color:#D1913C\" />").map_err(fmt_err)?;
    writeln!(svg, r#"</linearGradient>"#).map_err(fmt_err)?;

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
        writeln!(svg, r#"<rect width="100%" height="100%" fill="rgba(20, 24, 38, 0.7)" />"#).map_err(fmt_err)?;
    } else {
        // 回退到渐变背景
        writeln!(svg, r#"<rect width="100%" height="100%" fill="url(#bg-gradient)"/>"#).map_err(fmt_err)?;
    }
    // --- 背景结束 ---


    // --- Header ---
    let player_name = stats.player_name.as_deref().unwrap_or("Phigros Player");
    let real_rks = stats.real_rks.unwrap_or(0.0);
    writeln!(svg, r#"<text x="40" y="55" class="text-title">{}({:.2})</text>"#, escape_xml(player_name), real_rks).map_err(fmt_err)?;
    let ap_text = match stats.ap_top_3_avg {
        Some(avg) => format!("AP Top 3 Avg: {:.4}", avg),
        None => "AP Top 3 Avg: N/A".to_string(),
    };
    writeln!(svg, r#"<text x="40" y="85" class="text-stat">{}</text>"#, ap_text).map_err(fmt_err)?;
    let b27_avg_str = stats.best_27_avg.map_or("N/A".to_string(), |avg| format!("{:.4}", avg));
    let bn_text = format!("Best 27 Avg: {}", b27_avg_str);
    writeln!(svg, r#"<text x="40" y="110" class="text-stat">{}</text>"#, bn_text).map_err(fmt_err)?;

    // 修改右上角时间格式，添加 UTC 标识
    let update_time = format!("Updated at {} UTC", stats.update_time.format("%Y/%m/%d %H:%M:%S"));
    writeln!(svg, r#"<text x="{}" y="110" class="text-time">{}</text>"#, width - 30, update_time).map_err(fmt_err)?;

    writeln!(svg, "<line x1='40' y1='{}' x2='{}' y2='{}' stroke='#333848' stroke-width='1' stroke-opacity='0.7'/>",
             header_height, width - 40, header_height).map_err(fmt_err)?;


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
            
            generate_card_svg(&mut svg, score, idx, x_pos, ap_card_start_y, main_card_width, true, true, push_acc)?;
        }
        writeln!(svg, r#"</g>"#).map_err(fmt_err)?;
    }


    // --- Main Score Cards Section --- (保持不变) ...
    let main_content_start_y = header_height + ap_section_height + 15;
    let ap_score_ids: HashSet<(String, String)> = stats.ap_top_3_scores
        .iter()
        .map(|s| (s.song_id.clone(), s.difficulty.clone()))
        .collect();
    for (index, score) in scores.iter().enumerate() {
        let row = index as u32 / columns;
        let col = index as u32 % columns;
        let x = main_card_padding_outer + col * (main_card_width + main_card_padding_outer);
        let y = main_content_start_y + main_card_padding_outer + row * (calculated_card_height + main_card_padding_outer);
        let is_ap_score = ap_score_ids.contains(&(score.song_id.clone(), score.difficulty.clone()));
        
        // 获取预计算的推分ACC（如果有）
        let push_acc = push_acc_map.and_then(|map| {
            let key = format!("{}-{}", score.song_id, score.difficulty);
            map.get(&key).copied()
        });
        
        generate_card_svg(&mut svg, score, index, x, y, main_card_width, false, is_ap_score, push_acc)?;
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
    let mut font_db = fontdb::Database::new();
    font_db.load_system_fonts();
    load_custom_fonts(&mut font_db)?;

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

    let font_db_rc = Rc::new(font_db);
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

    if current_song_ill_path_png.exists() {
        if let Ok(img_data) = fs::read(&current_song_ill_path_png) {
            let base64_encoded = base64_engine.encode(&img_data);
            background_image_href = Some(format!("data:image/png;base64,{}", base64_encoded));
            log::info!("使用当前曲目曲绘作为背景: {}", current_song_ill_path_png.display());
        }
    } else if current_song_ill_path_jpg.exists() {
        if let Ok(img_data) = fs::read(&current_song_ill_path_jpg) {
            let base64_encoded = base64_engine.encode(&img_data);
            background_image_href = Some(format!("data:image/jpeg;base64,{}", base64_encoded));
            log::info!("使用当前曲目曲绘作为背景: {}", current_song_ill_path_jpg.display());
        }
    } else {
        // 如果找不到当前曲目的曲绘，则随机选一个
        let cover_base_path = PathBuf::from(cover_loader::COVERS_DIR).join("ill");
        let cover_files: Vec<PathBuf> = match fs::read_dir(&cover_base_path) {
            Ok(entries) => entries.filter_map(Result::ok).map(|entry| entry.path()).filter(|path| path.is_file() && (path.extension() == Some("png".as_ref()) || path.extension() == Some("jpg".as_ref()))).collect(),
            Err(e) => { log::error!("读取封面目录失败 '{}': {}", cover_base_path.display(), e); Vec::new() }
        };
        if !cover_files.is_empty() {
            let mut rng = thread_rng();
            if let Some(random_path) = cover_files.choose(&mut rng) {
                match fs::read(random_path) {
                    Ok(img_data) => {
                        let mime_type = if random_path.extension().map_or(false, |ext| ext == "png") { "image/png" } else { "image/jpeg" };
                        let base64_encoded = base64_engine.encode(&img_data);
                        background_image_href = Some(format!("data:{};base64,{}", mime_type, base64_encoded));
                        log::info!("使用随机背景图: {}", random_path.display());
                    },
                    Err(e) => { log::error!("读取随机背景封面文件失败 '{}': {}", random_path.display(), e); }
                }
            } else { log::warn!("无法从封面文件列表中随机选择一个"); }
        } else { log::warn!("在 '{}' 目录中找不到任何 .png 或 .jpg 封面用于随机背景", cover_base_path.display()); }
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
    writeln!(svg, r#"<filter id="bg-blur"><feGaussianBlur stdDeviation="15" /></filter>"#).map_err(fmt_err)?;
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