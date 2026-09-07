//! 系统 OCR。
//!
//! 移植自 `src/services/recognize/system/index.jsx`。
//! 原实现按 `osType` 分派到 Tauri 命令 `system_ocr`，并在 Linux/Windows 上对
//! 中文（及 Windows 上的日文）结果去掉多余空格。这里直接复用工程已有的平台层
//! `saladict_platform::ocr::system_ocr`，由平台层内部按语言分派到 macos/windows/linux
//! 的具体实现，不再自行构造命令调用。

use crate::Recognizer;
use async_trait::async_trait;
use saladict_core::schema::ConfigField;
use saladict_core::{Error, Language, Result, RecognizeRequest};
use saladict_platform::ocr::system_ocr;

pub struct System;

#[async_trait]
impl Recognizer for System {
    fn id(&self) -> &str {
        "system"
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        // 系统 OCR 无需配置，直接可用。
        vec![]
    }

    async fn recognize(&self, req: RecognizeRequest) -> Result<String> {
        let mut result = system_ocr(&req.image, req.language)
            .map_err(|e| Error::Platform(e.to_string()))?;

        // 与原文一致：中文（及 Windows 上的日文）结果去掉空格，避免逐字间隔。
        if req.language == Language::ZhCn || req.language == Language::ZhTw {
            result = result.replace(' ', "");
        }
        #[cfg(target_os = "windows")]
        if req.language == Language::Ja {
            result = result.replace(' ', "");
        }

        Ok(result.trim().to_string())
    }
}
