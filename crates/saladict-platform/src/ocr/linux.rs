//! Linux 系统 OCR：调用 `tesseract` 命令行。
//!
//! 注意：本分支无法在本机（macOS）验证，仅保证代码正确。

use anyhow::Context;
use saladict_core::{Language, Result};

pub fn system_ocr(image: &[u8], lang: Language) -> Result<String> {
    fn inner(image: &[u8], lang: Language) -> anyhow::Result<String> {
        let dir = dirs::cache_dir().context("no cache dir")?;
        let tmp = dir.join("saladict_ocr_tmp.png");
        std::fs::write(&tmp, image).context("write tmp png")?;

        let mut cmd = std::process::Command::new("tesseract");
        cmd.arg(tmp.to_string_lossy().as_ref()).arg("stdout");
        if lang != Language::Auto {
            cmd.arg("-l").arg(lang.code());
        }

        let out = cmd
            .output()
            .map_err(|e| anyhow::anyhow!("tesseract not installed or failed: {e}"))?;

        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
        } else {
            let stderr = String::from_utf8_lossy(&out.stderr);
            if stderr.contains("data") {
                Err(anyhow::anyhow!(
                    "tesseract language data not installed, try `apt install tesseract-ocr-{}`",
                    lang.code()
                ))
            } else {
                Err(anyhow::anyhow!("tesseract error: {stderr}"))
            }
        }
    }

    inner(image, lang).map_err(|e| saladict_core::Error::Platform(e.to_string()))
}
