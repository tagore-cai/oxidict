//! macOS 系统 OCR。
//!
//! # 实现说明（编译通过优先）
//!
//! 本机必须保证 `cargo check` 稳定通过。macOS 原生 Vision OCR 需要 objc / CoreGraphics
//! 的 FFI 绑定，容易引入不可控的编译风险，因此这里采用「写临时 PNG + 调用外部 OCR 助手」
//! 的命令回退方案：
//!
//! 1. 把图像字节写入缓存目录的临时文件（文件名含 PID 与单调序号，见下）；
//! 2. 调用由环境变量 `OXIDICT_OCR_BIN` 指定（默认 `/usr/local/bin/oxidict-ocr`）的 OCR
//!    助手，参数为 `<png 路径> <语言码>`；
//! 3. 助手输出即识别文本，否则返回 [`oxidict_core::Error::Platform`]；
//! 4. 无论成功失败都删除临时文件。
//!
//! ## 升级为原生 Vision 的切入点
//!
//! 后续若要真正原生 OCR，可在此文件内用 `objc` / `core-graphics` 调用
//! `VNImageRequestHandler` + `VNRecognizeTextRequest` 替换下方的命令调用，对外签名不变。
//! 大致步骤：
//! - 用 `CGDataProvider` + `CGImage::new(...)` 从图像字节构造 `CGImage`；
//! - `msg_send![VNImageRequestHandler, initWithCGImage:options:]` 建请求处理器；
//! - `msg_send![VNRecognizeTextRequest, init]` 建请求，`performRequests:error:` 执行；
//! - 遍历 `results` 取 `topCandidates:` 文本拼接。

use oxidict_core::{Language, Result};
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};

/// 临时文件序号。固定文件名会让两次并发 OCR 互相覆盖同一路径，
/// 先写入的图片被后一次替换，识别结果串台；这里用 PID + 自增序号保证唯一。
static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

pub fn system_ocr(image: &[u8], lang: Language) -> Result<String> {
    fn inner(image: &[u8], lang: Language) -> anyhow::Result<String> {
        let dir = dirs::cache_dir().ok_or_else(|| anyhow::anyhow!("no cache dir"))?;
        std::fs::create_dir_all(&dir)?;
        let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let tmp = dir.join(format!("oxidict_ocr_{}_{seq}.png", std::process::id()));
        {
            let mut f =
                std::fs::File::create(&tmp).map_err(|e| anyhow::anyhow!("create tmp: {e}"))?;
            f.write_all(image)
                .map_err(|e| anyhow::anyhow!("write tmp: {e}"))?;
        }

        let bin = std::env::var("OXIDICT_OCR_BIN")
            .unwrap_or_else(|_| "/usr/local/bin/oxidict-ocr".to_string());
        let output = std::process::Command::new(&bin)
            .arg(tmp.to_string_lossy().as_ref())
            .arg(lang.code())
            .output();

        // 临时文件不再需要，成功失败都清理，避免缓存目录无限堆积。
        let _ = std::fs::remove_file(&tmp);

        match output {
            Ok(o) if o.status.success() => {
                Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
            }
            Ok(o) => Err(anyhow::anyhow!(
                "OCR helper `{bin}` error: {}",
                String::from_utf8_lossy(&o.stderr).trim()
            )),
            Err(e) => Err(anyhow::anyhow!(
                "OCR helper `{bin}` unavailable or failed: {e}. \
                 Provide a Vision-based OCR helper or implement native VNRecognizeTextRequest."
            )),
        }
    }

    inner(image, lang).map_err(|e| oxidict_core::Error::Platform(e.to_string()))
}
