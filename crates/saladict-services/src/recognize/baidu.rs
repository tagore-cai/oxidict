//! 百度通用文字识别。
//!
//! 移植自 `src/services/recognize/baidu/index.jsx`。
//! 流程：先用 client_id/secret 换 access_token，再带 token 表单提交 base64 图片。

use crate::Recognizer;
use async_trait::async_trait;
use base64::Engine as _;
use saladict_core::map_language;
use saladict_core::schema::ConfigField;
use saladict_core::HasConfig as _;
use saladict_core::{Error, Language, RecognizeRequest, Result};
use saladict_net::NetErr as _;
use saladict_net::{check, post_with_headers};
use serde_json::Value;

pub struct Baidu;

const TOKEN_URL: &str = "https://aip.baidubce.com/oauth/2.0/token";
const OCR_URL: &str = "https://aip.baidubce.com/rest/2.0/ocr/v1/general_basic";

#[async_trait]
impl Recognizer for Baidu {
    fn id(&self) -> &str {
        "baidu"
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        vec![
            ConfigField::secret("client_id", "API Key"),
            ConfigField::secret("client_secret", "Secret Key"),
        ]
    }

    fn map_language(&self, lang: Language) -> String {
        map_language!(lang, {
            Auto => "CHN_ENG",
            ZhCn => "CHN_ENG",
            ZhTw => "CHN_ENG",
            En => "ENG",
            Ja => "JAP",
            Ko => "KOR",
            Fr => "FRE",
            Es => "SPA",
            Ru => "RUS",
            De => "GER",
            It => "ITA",
            PtPt => "POR",
            PtBr => "POR",
        })
    }

    async fn recognize(&self, req: RecognizeRequest) -> Result<String> {
        let client_id = req.require_str("client_id", "Baidu API Key")?;
        let client_secret = req.require_str("client_secret", "Baidu Secret Key")?;

        // 1) 换取 access_token（POST + query 参数，与原 Tauri fetch 行为一致）。
        let token_resp = post_with_headers(
            TOKEN_URL,
            &[
                ("Content-Type", "application/json"),
                ("Accept", "application/json"),
            ],
        )
        .query(&[
            ("grant_type", "client_credentials"),
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
        ])
        .send()
        .await
        .net_err()?;
        let token_val: Value = check(token_resp).await?.json().await.net_err()?;
        let token = token_val
            .get("access_token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::Service("Get Access Token Failed!".into()))?;

        // 2) 提交图片识别。
        let b64 = base64::engine::general_purpose::STANDARD.encode(&req.image);
        let lang = self.map_language(req.language);
        let resp = post_with_headers(
            OCR_URL,
            &[("Content-Type", "application/x-www-form-urlencoded")],
        )
        .query(&[("access_token", token)])
        .form(&[
            ("language_type", lang.as_str()),
            ("detect_direction", "false"),
            ("image", b64.as_str()),
        ])
        .send()
        .await
        .net_err()?;
        let result: Value = check(resp).await?.json().await.net_err()?;

        let words = result
            .get("words_result")
            .and_then(|v| v.as_array())
            .ok_or_else(|| Error::Service(serde_json::to_string(&result).unwrap_or_default()))?;

        let mut target = String::new();
        for item in words {
            if let Some(w) = item.get("words").and_then(|v| v.as_str()) {
                target.push_str(w);
                target.push('\n');
            }
        }
        Ok(target.trim().to_string())
    }
}
