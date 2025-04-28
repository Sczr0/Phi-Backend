use crate::models::rks::RksRecord;
use crate::utils::error::AppError;
use crate::utils::cover_loader;
use resvg::usvg::{self, Options as UsvgOptions, fontdb};
use resvg::{render, tiny_skia::{Pixmap, Transform}};
use std::path::PathBuf;
use std::rc::Rc;
use chrono::{DateTime, Utc};
use std::fmt::Write;
use std::collections::HashSet;
use itertools::Itertools;
use std::fs;
use base64::{Engine as _};
use std::path::Path;

pub struct PlayerStats {
    pub ap_top_3_avg: Option<f64>,
    pub best_27_avg: Option<f64>,
    pub real_rks: Option<f64>,
    pub player_name: Option<String>,
    pub update_time: DateTime<Utc>,
    pub n: u32,  // 请求的 Best N 数量
    pub ap_top_3_scores: Vec<RksRecord>, // 添加 AP Top 3 的具体成绩
}

// 常量定义
const FONTS_DIR: &str = "resources/fonts";
const MAIN_FONT_NAME: &str = "思源黑体 CN";
const COVER_ASPECT_RATIO: f64 = 512.0 / 270.0;

// Helper function to generate a single score card SVG group
fn generate_card_svg(
    svg: &mut String, 
    score: &RksRecord, 
    index: usize, 
    card_x: u32, 
    card_y: u32, 
    card_width: u32, 
    is_ap_card: bool, // Flag to indicate if this is for the AP section
    is_ap_score: bool // Flag to indicate if the score itself is AP
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

    // Accuracy (带最小推分acc)
    let acc_text = if !is_ap_score && score.acc < 100.0 && score.difficulty_value > 0.0 { // 只有定数>0时才计算推分
        // 计算最小推分acc
        let min_push_acc = calculate_min_push_acc(score.rks, score.difficulty_value);
        
        // 如果最小推分acc非常接近100，直接显示 -> 100.00%
        if min_push_acc > 99.995 {
             format!("Acc: {:.2}% -> 100.00%", score.acc)
        }
        // 如果两者差值非常小(小于0.005，对应四舍五入后两位不变)，则展示三位小数
        else if (min_push_acc - score.acc).abs() < 0.005 {
            format!("Acc: {:.2}% -> {:.3}%", score.acc, min_push_acc)
        } else {
            format!("Acc: {:.2}% -> {:.2}%", score.acc, min_push_acc)
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
) -> Result<String, AppError> {
    let width = 1200; 
    let header_height = 120; 
    let ap_title_height = 50; // Height for the "AP Top 3" title
    let footer_height = 50;  
    let main_card_padding_outer = 12; 
    let ap_card_padding_outer = 12;
    let columns = 3;
    
    // Calculate card width based on outer padding
    let main_card_width = (width - main_card_padding_outer * (columns + 1)) / columns;

    // Calculate approximate card height based on text block height needed for alignment
    // These values should match those used inside generate_card_svg
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

    // 定义 AP 卡片的起始 Y 坐标（在 AP section group 内）
    let ap_card_start_y = ap_card_padding_outer; // 使用外边距作为起始 Y

    let ap_section_height = if !stats.ap_top_3_scores.is_empty() { 
        ap_card_start_y + calculated_card_height + ap_card_padding_outer 
    } else { 0 };

    let rows = (scores.len() as u32 + columns - 1) / columns;
    let content_height = (calculated_card_height + main_card_padding_outer) * rows.max(1); // Min 1 row

    let total_height = header_height + ap_section_height + content_height + footer_height + 10;

    let mut svg = String::new();
    let fmt_err = |e| AppError::InternalError(format!("SVG formatting error: {}", e));

    writeln!(
        svg,
        r#"<svg width="{}" height="{}" viewBox="0 0 {} {}" xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink">"#,
        width, total_height, width, total_height
    ).map_err(fmt_err)?;
    
    // --- Definitions (Styles, Gradients, Filters, Font) ---
    writeln!(svg, "<defs>").map_err(fmt_err)?;
    
    writeln!(svg, r#"<linearGradient id="bg-gradient" x1="0%" y1="0%" x2="100%" y2="100%">"#).map_err(fmt_err)?;
    writeln!(svg, "<stop offset=\"0%\" stop-color=\"#141826\" />").map_err(fmt_err)?;
    writeln!(svg, "<stop offset=\"100%\" stop-color=\"#252E48\" />").map_err(fmt_err)?;
    writeln!(svg, r#"</linearGradient>"#).map_err(fmt_err)?;

    // Shadow Filter Definition
    writeln!(svg, r#"<filter id="card-shadow" x="-10%" y="-10%" width="120%" height="130%">"#).map_err(fmt_err)?;
    writeln!(svg, "<feDropShadow dx=\"0\" dy=\"3\" stdDeviation=\"3\" flood-color=\"#000000\" flood-opacity=\"0.25\" />").map_err(fmt_err)?;
    writeln!(svg, r#"</filter>"#).map_err(fmt_err)?;

    // Font style - No longer embedding font data here
    writeln!(svg, "<style>").map_err(fmt_err)?;
    write!(
        svg,
        r#"
        /* <![CDATA[ */
        /* @font-face removed - Relying on fontdb loading */
        svg {{ background: url(#bg-gradient); }}
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
        /* AP gradient stops styled inline now
        #ap-gradient stop:nth-child(1) {{ stop-color: #FFDA63; }}
        #ap-gradient stop:nth-child(2) {{ stop-color: #D1913C; }}
        */
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
        /* Difficulty Tags Styles are removed */

        * {{ font-family: "{font_name}", "Microsoft YaHei", "SimHei", "DengXian", Arial, sans-serif; }}
        /* ]]> */
        "#,
        font_name = MAIN_FONT_NAME
    ).map_err(fmt_err)?;
    writeln!(svg, "</style>").map_err(fmt_err)?;

    // Define AP card stroke gradient with inline styles for stops
    writeln!(svg, r#"<linearGradient id="ap-gradient" x1="0%" y1="0%" x2="100%" y2="100%">"#).map_err(fmt_err)?;
    writeln!(svg, "<stop offset=\"0%\" style=\"stop-color:#FFDA63\" />").map_err(fmt_err)?; // Inline style
    writeln!(svg, "<stop offset=\"100%\" style=\"stop-color:#D1913C\" />").map_err(fmt_err)?; // Inline style
    writeln!(svg, r#"</linearGradient>"#).map_err(fmt_err)?;

    writeln!(svg, "</defs>").map_err(fmt_err)?;
    
    // --- Background ---
    writeln!(svg, r#"<rect width="100%" height="100%" fill="url(#bg-gradient)"/>"#).map_err(fmt_err)?;

    // --- Header ---
    let player_name = stats.player_name.as_deref().unwrap_or("Phigros Player");
    let real_rks = stats.real_rks.unwrap_or(0.0);
    writeln!(svg, r#"<text x="40" y="55" class="text-title">{}({:.2})</text>"#, escape_xml(player_name), real_rks).map_err(fmt_err)?;

    // ----  AP Top 3 Avg 显示 ----
    let ap_text = match stats.ap_top_3_avg { // 使用 PlayerStats 中的 ap_top_3_avg
        Some(avg) => format!("AP Top 3 Avg: {:.4}", avg),
        None => "AP Top 3 Avg: N/A".to_string(),
    };
    writeln!(svg, r#"<text x="40" y="85" class="text-stat">{}</text>"#, ap_text).map_err(fmt_err)?;

    // ---- 调整 Best 27 Avg 的 Y 坐标 ----
    let b27_avg_str = stats.best_27_avg.map_or("N/A".to_string(), |avg| format!("{:.4}", avg));
    let bn_text = format!("Best 27 Avg: {}", b27_avg_str);
    // 将 Y 坐标调整回原来的 110
    writeln!(svg, r#"<text x="40" y="110" class="text-stat">{}</text>"#, bn_text).map_err(fmt_err)?;

    // 更新时间位置调整为与左上角的下边缘对齐
    let update_time = format!("Updated at {}", stats.update_time.format("%Y/%m/%d %H:%M:%S"));
    writeln!(svg, r#"<text x="{}" y="110" class="text-time">{}</text>"#, width - 30, update_time).map_err(fmt_err)?;

    // 分割线位置保持不变
    writeln!(svg, "<line x1='40' y1='{}' x2='{}' y2='{}' stroke='#333848' stroke-width='1' stroke-opacity='0.7'/>", 
             header_height, width - 40, header_height).map_err(fmt_err)?;

    // --- AP Top 3 Section ---
    let ap_section_start_y = header_height + 15; // section group 的起始位置不变
    if !stats.ap_top_3_scores.is_empty() {
        writeln!(svg, r#"<g id="ap-top-3-section" transform="translate(0, {})">"#, ap_section_start_y).map_err(fmt_err)?;
        // ---- 修改：移除标题文本 ----
        // writeln!(svg, r#"<text x="40" y="25" class="text-section-title">AP Top 3</text>"#).map_err(fmt_err)?;
        
        // 使用新的 ap_card_start_y
        for (idx, score) in stats.ap_top_3_scores.iter().take(3).enumerate() {
            let x_pos = ap_card_padding_outer + idx as u32 * (main_card_width + ap_card_padding_outer);
            // 传递新的起始 Y 坐标
            generate_card_svg(&mut svg, score, idx, x_pos, ap_card_start_y, main_card_width, true, true)?;
        }
        writeln!(svg, r#"</g>"#).map_err(fmt_err)?;
    }

    // --- Main Score Cards Section ---
    // 调整 Main Section 的起始 Y 坐标以适应 AP section 高度变化
    let main_content_start_y = header_height + ap_section_height + 15; // 使用更新后的 ap_section_height
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
        generate_card_svg(&mut svg, score, index, x, y, main_card_width, false, is_ap_score)?;
    }

    // --- Footer ---
    let footer_y = (total_height - footer_height / 2 + 10) as f64; // 增加偏移量，使其向下移动
    let generated_text = format!("Generated by Phi-Backend at {}", chrono::Local::now().format("%Y/%m/%d %H:%M:%S"));
    writeln!(svg, r#"<text x="{}" y="{:.1}" class="text-footer" text-anchor="middle">{}</text>"#, width / 2, footer_y, generated_text).map_err(fmt_err)?;

    writeln!(svg, "</svg>").map_err(fmt_err)?;

    Ok(svg)
}

// --- SVG 渲染函数 ---

pub fn render_svg_to_png(svg_data: String) -> Result<Vec<u8>, AppError> {
    // 创建字体数据库并加载系统字体
    let mut font_db = fontdb::Database::new();
    
    // 尝试加载系统字体
    font_db.load_system_fonts();
    
    // 加载自定义字体
    load_custom_fonts(&mut font_db)?;
    
    // 设置字体搜索路径，优先使用自定义字体，然后是系统字体
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
    
    // 解析SVG并创建渲染树
    let tree = usvg::Tree::from_data(&svg_data.as_bytes(), &opts, &font_db_rc)
        .map_err(|e| AppError::InternalError(format!("Failed to parse SVG: {}", e)))?;

    // 创建像素图并渲染
    let pixmap_size = tree.size().to_int_size();
    let mut pixmap = Pixmap::new(pixmap_size.width(), pixmap_size.height())
        .ok_or_else(|| AppError::InternalError("Failed to create pixmap".to_string()))?;

    render(&tree, Transform::default(), &mut pixmap.as_mut());

    // 编码为PNG并返回
    pixmap.encode_png()
        .map_err(|e| AppError::InternalError(format!("Failed to encode PNG: {}", e)))
}

// 辅助函数：转义 XML 特殊字符
fn escape_xml(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

// 辅助函数：加载自定义字体到字体数据库
fn load_custom_fonts(font_db: &mut fontdb::Database) -> Result<(), AppError> {
    let fonts_dir = PathBuf::from(FONTS_DIR);
    
    if !fonts_dir.exists() {
        log::warn!("Fonts directory does not exist: {}", fonts_dir.display()); // Use warn instead of error if system fonts are fallback
        return Ok(()); // Allow proceeding without custom fonts if dir is missing
    }
    
    // 尝试加载fonts目录中的所有字体文件
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
                 continue; // Skip problematic entries
             }
         };
        
        let path = entry.path();
        
        if path.is_file() && path.extension().map_or(false, |ext| ext == "ttf" || ext == "otf") {
             log::debug!("Loading custom font: {}", path.display());
             match font_db.load_font_file(&path) {
                 Ok(_) => fonts_loaded = true,
                 Err(e) => {
                     // Log a more critical error if font loading fails
                     log::error!("CRITICAL: Failed to load custom font file '{}': {}. Text rendering might be incorrect.", path.display(), e);
                     // Optionally, you could return an error here if custom font is mandatory:
                     // return Err(AppError::InternalError(format!("Failed to load mandatory font {}: {}", path.display(), e)));
                 }
             }
        }
    }
    
    if !fonts_loaded {
        log::warn!("No custom fonts were successfully loaded from {}", fonts_dir.display());
    }
    
    Ok(())
}

// Helper to find the first font file in a directory (can likely be removed too if load_custom_fonts handles errors gracefully)
fn find_first_font_file(dir: &Path) -> Result<Option<PathBuf>, AppError> {
    if !dir.exists() {
        return Ok(None);
    }
    let entries = fs::read_dir(dir)
        .map_err(|e| AppError::InternalError(format!("Failed to read directory {}: {}", dir.display(), e)))?;

    for entry in entries {
        let entry = entry.map_err(|e| AppError::InternalError(format!("Failed to read directory entry: {}", e)))?;
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == "ttf" || ext == "otf" {
                    return Ok(Some(path));
                }
            }
        }
    }
    Ok(None)
}

// 添加一个函数用于计算最小推分acc
fn calculate_min_push_acc(current_rks: f64, difficulty_value: f64) -> f64 {
    // RKS计算公式: RKS = ((acc - 55.0) / 45.0)^2 * 定数
    // 需要求解: acc使得RKS增加0.01
    // 即: ((acc - 55.0) / 45.0)^2 * 定数 = current_rks + 0.01

    // 1. 计算目标RKS，确保不会因精度问题导致目标低于当前
    let target_rks = (current_rks + 0.01).max(current_rks + 1e-9); // Add a small epsilon

    // 处理定数为0或负数的情况，避免除零或无效计算
    if difficulty_value <= 0.0 {
        return 100.0; // 如果定数无效，无法推分，返回100%
    }
    
    // 计算目标RKS / 定数，处理可能为负的情况（理论上不应发生，但防御性编程）
    let rks_ratio = target_rks / difficulty_value;
    if rks_ratio < 0.0 {
         return 100.0; // 如果比率小于0，无法开方，返回100%
    }
    
    // 2. 反推需要达到的acc
    // target_rks = ((acc - 55.0) / 45.0)^2 * difficulty_value
    // ((acc - 55.0) / 45.0)^2 = target_rks / difficulty_value
    // (acc - 55.0) / 45.0 = sqrt(target_rks / difficulty_value)
    // acc - 55.0 = 45.0 * sqrt(target_rks / difficulty_value)
    // acc = 55.0 + 45.0 * sqrt(target_rks / difficulty_value)
    
    let min_acc = 55.0 + 45.0 * rks_ratio.sqrt();
    
    // 限制在100.0以内，因为acc不能超过100%
    // 同时确保结果不低于当前acc（因为目标是增加rks）
    min_acc.max(55.0).min(100.0) // Ensure acc is at least 55.0
}