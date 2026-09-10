//! 二维码识别（QR Code）。
//!
//! 移植自 `src/services/recognize/qrcode/index.jsx`。
//! 原实现在 WebView 内用 `jsQR` 解码；这里改用 Rust 的 `rqrr`，先以 `image` crate
//! 解码内存图片，再交给 `rqrr` 扫描。原文：识别失败或存在多个二维码时抛错。

use crate::Recognizer;
use async_trait::async_trait;
use image::DynamicImage;
use rqrr::PreparedImage;
use saladict_core::schema::ConfigField;
use saladict_core::{Error, RecognizeRequest, Result};

pub struct Qrcode;

#[async_trait]
impl Recognizer for Qrcode {
    fn id(&self) -> &str {
        "qrcode"
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        // 本地解码，无需配置。
        vec![]
    }

    async fn recognize(&self, req: RecognizeRequest) -> Result<String> {
        let img: DynamicImage = image::load_from_memory(&req.image)
            .map_err(|e| Error::Service(format!("图片解码失败: {e}")))?;
        let img = img.to_luma8();

        let mut prepared = PreparedImage::prepare(img);
        let grids = prepared.detect_grids();
        if grids.is_empty() {
            return Err(Error::Service(
                "QR code not recognized or multiple QR codes exist".into(),
            ));
        }

        let mut out = String::new();
        for grid in grids {
            let (_meta, content) = grid
                .decode()
                .map_err(|e| Error::Service(format!("二维码解析失败: {e}")))?;
            out.push_str(&content);
            out.push('\n');
        }
        // 与原文一致：失败抛错，成功返回解码内容（多个时拼接）。
        Ok(out.trim().to_string())
    }
}
