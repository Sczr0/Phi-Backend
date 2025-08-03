use crate::utils::error::{AppError, AppResult};
use image::{imageops, RgbaImage, Rgba, DynamicImage};
use std::fs;
use std::path::{Path, PathBuf};
use git2::Repository;

pub const COVERS_DIR: &str = "resources/covers";
const GIT_REPO_URL: &str = "https://gitee.com/Steveeee-e/phi-plugin-ill.git";
#[allow(dead_code)]
const PLACEHOLDER_COLOR: Rgba<u8> = Rgba([100, 100, 100, 255]); // 灰色占位符

// 确保本地曲绘目录存在且包含内容，否则尝试克隆
pub fn ensure_covers_available() -> AppResult<()> {
    let covers_path = Path::new(COVERS_DIR);

    // 检查目录是否存在
    if !covers_path.exists() {
        println!("本地曲绘目录 '{}' 不存在，尝试创建并克隆...", COVERS_DIR);
        fs::create_dir_all(covers_path)
            .map_err(|e| AppError::IoError(e))?;
        clone_repo(covers_path)?;
        return Ok(());
    }

    // 检查目录是否为空或只包含隐藏文件
    match fs::read_dir(covers_path) {
        Ok(entries) => {
            // 修正检查逻辑，确保能正确处理.git等隐藏文件
            let is_empty = entries.filter_map(Result::ok)
                                  .all(|entry| entry.file_name().to_string_lossy().starts_with('.'));
            if is_empty {
                println!("本地曲绘目录 '{}' 为空或只包含隐藏文件，尝试克隆...", COVERS_DIR);
                // 清理可能存在的旧的克隆失败残留
                if Path::new(COVERS_DIR).join(".git").exists() {
                    println!("清理旧的 .git 目录...");
                    fs::remove_dir_all(Path::new(COVERS_DIR).join(".git")).map_err(|e| AppError::IoError(e))?;
                }
                clone_repo(covers_path)?;
            }
        }
        Err(e) => {
            eprintln!("无法读取本地曲绘目录 '{}': {}", COVERS_DIR, e);
            return Err(AppError::IoError(e));
        }
    }

    Ok(())
}

// 克隆 Git 仓库
fn clone_repo(target_path: &Path) -> AppResult<()> {
    println!("正在从 {} 克隆曲绘仓库到 '{}'...", GIT_REPO_URL, target_path.display());
    match Repository::clone(GIT_REPO_URL, target_path) {
        Ok(_) => {
            println!("曲绘仓库克隆成功.");
            Ok(())
        }
        Err(e) => {
            eprintln!("克隆曲绘仓库失败: {}", e);
            Err(AppError::Other(format!("Git clone failed: {}", e)))
        }
    }
}

// 加载本地曲绘图片，如果找不到则返回占位图
#[allow(dead_code)]
pub fn load_local_cover(song_id: &str, size: (u32, u32)) -> RgbaImage {
    // 假设克隆后的仓库结构为 resources/covers/illLow/{song_id}.png
    // 如果不是，需要调整此路径
    let path_png = PathBuf::from(COVERS_DIR).join("illLow").join(format!("{}.png", song_id));
    let path_jpg = PathBuf::from(COVERS_DIR).join("illLow").join(format!("{}.jpg", song_id));

    let img_result: Result<DynamicImage, image::ImageError> = 
        if path_png.exists() { image::open(&path_png) } 
        else if path_jpg.exists() { image::open(&path_jpg) } 
        else { Err(image::ImageError::IoError(std::io::Error::new(std::io::ErrorKind::NotFound, "Cover not found"))) };

    match img_result {
        Ok(img) => {
            // 先将 DynamicImage 转换为 RgbaImage
            let rgba_img = img.to_rgba8();
            // 然后调整 RgbaImage 的大小
            let resized = imageops::resize(&rgba_img, size.0, size.1, imageops::FilterType::Lanczos3);
            resized // 直接返回 RgbaImage
        }
        Err(_) => {
            // 文件不存在或加载失败，返回占位图
            create_placeholder(size)
        }
    }
}

// 创建一个纯色的占位图
#[allow(dead_code)]
fn create_placeholder(size: (u32, u32)) -> RgbaImage {
    let mut placeholder = RgbaImage::new(size.0, size.1);
    // 使用循环填充，因为 imageproc 不再是依赖
    for pixel in placeholder.pixels_mut() {
        *pixel = PLACEHOLDER_COLOR;
    }
    placeholder
} 