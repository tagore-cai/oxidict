//! 截图。
//!
//! 全屏/区域截图用 `screenshots` crate（内部封装各平台原生 API）。
//! macOS 上游还提供交互式框选（`screencapture -i`），这里一并保留：
//! 框选完成后从临时文件读回图像字节。

use oxidict_core::{Error, Result};

/// 截取整个主屏。
pub fn capture_screen() -> Result<Vec<u8>> {
    let screens = screenshots::Screen::all().map_err(|e| Error::Platform(e.to_string()))?;
    let screen = screens
        .into_iter()
        .next()
        .ok_or_else(|| Error::Platform("没有可用显示器".into()))?;
    let image = screen
        .capture()
        .map_err(|e| Error::Platform(format!("截图失败: {e}")))?;
    encode_png(&image)
}

/// 截取指定区域（物理像素坐标）。
pub fn capture_region(x: i32, y: i32, width: u32, height: u32) -> Result<Vec<u8>> {
    let screens = screenshots::Screen::all().map_err(|e| Error::Platform(e.to_string()))?;
    let screen = screens
        .into_iter()
        .next()
        .ok_or_else(|| Error::Platform("没有可用显示器".into()))?;
    let image = screen
        .capture_area(x, y, width, height)
        .map_err(|e| Error::Platform(format!("区域截图失败: {e}")))?;
    encode_png(&image)
}

/// 弹出系统级框选（仅 macOS），用户拖出区域后返回该区域的图像。
pub fn capture_interactive() -> Result<Vec<u8>> {
    if !cfg!(target_os = "macos") {
        return Err(Error::Platform(
            "交互式框选当前仅在 macOS 提供，其它平台请使用 capture_region".to_string(),
        ));
    }
    let dir = dirs::cache_dir()
        .ok_or_else(|| Error::Platform("无法定位缓存目录".to_string()))?
        .join(oxidict_core::config::APP_ID);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("screenshot_cut.png");
    let _ = std::fs::remove_file(&path);

    let status = std::process::Command::new("screencapture")
        .args(["-i", "-x"])
        .arg(&path)
        .status()
        .map_err(|e| Error::Platform(format!("调用 screencapture 失败: {e}")))?;
    if !status.success() || !path.exists() {
        // 用户按 Esc 取消属于正常路径，返回空结果而不是错误。
        return Err(Error::Platform("截图已取消".to_string()));
    }
    let bytes = std::fs::read(&path)?;
    let _ = std::fs::remove_file(&path);
    Ok(bytes)
}

fn encode_png(image: &screenshots::Image) -> Result<Vec<u8>> {
    let rgba = image::RgbaImage::from_raw(image.width(), image.height(), image.rgba().clone())
        .ok_or_else(|| Error::Platform("截图数据长度与尺寸不一致".to_string()))?;
    let mut png = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut png);
    rgba.write_to(&mut cursor, image::ImageFormat::Png)
        .map_err(|e| Error::Platform(format!("PNG 编码失败: {e}")))?;
    let _ = std::io::Write::flush(&mut cursor);
    Ok(png)
}
