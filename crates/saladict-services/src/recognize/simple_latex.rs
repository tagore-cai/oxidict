//! SimpleTex 公式识别（LaTeX OCR）。
//!
//! 移植自 `src/services/recognize/simple_latex/index.jsx`。
//! 原实现从 AppCache 读取截图 `pot_screenshot_cut.png` 并以 multipart 上传；这里直接复用
//! 传入的图片字节作为 `file` 字段。响应结构为 `{ res: { latex } }`，失败时抛出原始 JSON。

use crate::Recognizer;
use async_trait::async_trait;
use saladict_core::HasConfig as _;
use saladict_core::map_language;
use saladict_core::schema::ConfigField;
use saladict_core::{Error, Language, Result, RecognizeRequest};
use saladict_net::{check, post_with_headers};
use serde_json::Value;

pub struct SimpleLatex;

const URL: &str = "https://server.simpletex.cn/api/latex_ocr/v2";

#[async_trait]
impl Recognizer for SimpleLatex {
    fn id(&self) -> &str {
        "simple_latex"
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        vec![ConfigField::Text {
            key: "token",
            label: "Token",
            placeholder: "",
            secret: true,
            required: false,
        }]
    }

    fn map_language(&self, lang: Language) -> String {
        map_language!(lang, {
            Auto => "zh_cn",
            ZhCn => "zh_cn",
            ZhTw => "zh_tw",
            En => "en",
        })
    }

    async fn recognize(&self, req: RecognizeRequest) -> Result<String> {
        let token = req.str("token").unwrap_or_default();

        let part = reqwest::multipart::Part::bytes(req.image.clone())
            .file_name("pot_screenshot_cut.png")
            .mime_str("image/png")
            .map_err(|e| Error::Service(format!("构造图片表单失败: {e}")))?;
        let form = reqwest::multipart::Form::new().part("file", part);

        let resp = post_with_headers(URL, &[("token", token)])
            .multipart(form)
            .send()
            .await
            .map_err(|e| Error::Service(format!("Http Request Error\n{e}")))?;
        let result: Value = check(resp)
            .await
            .map_err(|e| Error::Service(format!("Http Request Error\n{e}")))?
            .json()
            .await
            .map_err(|e| Error::Service(format!("Http Request Error\n{e}")))?;

        let latex = result
            .get("res")
            .and_then(|res| res.get("latex"))
            .and_then(|v| v.as_str());
        match latex {
            Some(s) => Ok(s.trim().to_string()),
            None => Err(Error::Service(serde_json::to_string(&result).unwrap_or_default())),
        }
    }
}
