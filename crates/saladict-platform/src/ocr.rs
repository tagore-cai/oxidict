//! 系统 OCR：对图像做文字识别，按平台分派到对应实现。

use saladict_core::{Language, Result};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "linux")]
mod linux;

/// 对 `image`（PNG/JPEG 等编码字节）做系统 OCR，返回识别出的文本。
///
/// `lang` 为内部语言码；传 [`Language::Auto`] 时由系统按用户/默认语言决定。
pub fn system_ocr(image: &[u8], lang: Language) -> Result<String> {
    #[cfg(target_os = "macos")]
    {
        macos::system_ocr(image, lang)
    }
    #[cfg(target_os = "windows")]
    {
        windows::system_ocr(image, lang)
    }
    #[cfg(target_os = "linux")]
    {
        linux::system_ocr(image, lang)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = (image, lang);
        Err(saladict_core::Error::Platform(
            "OCR is not supported on this platform",
        ))
    }
}
