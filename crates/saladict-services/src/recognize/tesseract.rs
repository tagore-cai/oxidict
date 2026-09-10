//! Tesseract 本地 OCR。
//!
//! 移植自 `src/services/recognize/tesseract/index.jsx`。
//! 原实现走 `tesseract.js` 在浏览器/WebView 内做识别；这里改为把图片写入临时文件，
//! 直接调用系统已安装的 `tesseract` 命令行（语言码沿用 tesseract 的 `chi_sim`/`eng` 等）。

use crate::Recognizer;
use async_trait::async_trait;
use saladict_core::map_language;
use saladict_core::schema::ConfigField;
use saladict_core::{Error, Language, RecognizeRequest, Result};
use std::env::temp_dir;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

pub struct Tesseract;

#[async_trait]
impl Recognizer for Tesseract {
    fn id(&self) -> &str {
        "tesseract"
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        // 复用系统已安装的 tesseract，无需额外配置。
        vec![]
    }

    fn map_language(&self, lang: Language) -> String {
        map_language!(lang, {
            Auto => "eng",
            ZhCn => "chi_sim",
            ZhTw => "chi_tra",
            En => "eng",
            Ja => "jpn",
            Ko => "kor",
            Fr => "fra",
            Es => "spa",
            Ru => "rus",
            De => "deu",
            It => "ita",
            Tr => "tur",
            PtPt => "por",
            PtBr => "por",
            Vi => "vie",
            Id => "ind",
            Th => "tha",
            Ms => "msa",
            Ar => "ara",
            Hi => "hin",
            Uk => "ukr",
            He => "heb",
        })
    }

    async fn recognize(&self, req: RecognizeRequest) -> Result<String> {
        let lang = self.map_language(req.language);

        let input: PathBuf = temp_dir().join(format!("saladict_tesseract_{}.png", uuid()));
        let output_base: PathBuf = temp_dir().join(format!("saladict_tesseract_out_{}", uuid()));

        fs::write(&input, &req.image)
            .map_err(|e| Error::Service(format!("写入临时图片失败: {e}")))?;

        let status = Command::new("tesseract")
            .arg(&input)
            .arg(&output_base)
            .arg("-l")
            .arg(&lang)
            .output();

        // 无论成败都尽量清理输入文件。
        let _ = fs::remove_file(&input);

        let output = status
            .map_err(|e| Error::Service(format!("调用 tesseract 失败，请确认已安装: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let _ = fs::remove_file(output_base.with_extension("txt"));
            return Err(Error::Service(format!("tesseract 执行失败: {stderr}")));
        }

        let out_path = output_base.with_extension("txt");
        let text = fs::read_to_string(&out_path)
            .map_err(|e| Error::Service(format!("读取 tesseract 结果失败: {e}")))?;
        let _ = fs::remove_file(&out_path);

        // 与原文一致：中文结果去掉空格。
        if req.language == Language::ZhCn || req.language == Language::ZhTw {
            Ok(text.replace(' ', "").trim().to_string())
        } else {
            Ok(text.trim().to_string())
        }
    }
}

fn uuid() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:x}", n)
}
