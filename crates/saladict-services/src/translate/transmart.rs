//! 腾讯交互翻译（Transmart）。
//!
//! 移植自 `src/services/translate/transmart/index.jsx`，可选用户名与令牌。

use crate::Translator;
use async_trait::async_trait;
use saladict_core::schema::ConfigField;
use saladict_core::{Error, HasConfig, Result, TranslateRequest, TranslateResult};
use serde_json::{Value, json};

pub struct Transmart;

const CLIENT_KEY: &str =
    "browser-chrome-110.0.0-Mac OS-df4bd4c5-a65d-44b2-a40f-42f34f3535f2-1677486696487";

#[async_trait]
impl Translator for Transmart {
    fn id(&self) -> &str {
        "transmart"
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        vec![
            ConfigField::Text {
                key: "username",
                label: "用户名",
                placeholder: "可选",
                secret: false,
                required: false,
            },
            ConfigField::Text {
                key: "token",
                label: "Token",
                placeholder: "可选",
                secret: true,
                required: false,
            },
        ]
    }

    async fn translate(&self, req: TranslateRequest) -> Result<TranslateResult> {
        let mut header = json!({
            "fn": "auto_translation",
            "client_key": CLIENT_KEY,
        });
        // 用户名与令牌都填写时才带上鉴权，与原实现一致。
        if let (Some(user), Some(token)) = (req.str("username"), req.str("token"))
            && !user.is_empty()
            && !token.is_empty()
        {
            header["user"] = json!(user);
            header["token"] = json!(token);
        }

        let body = json!({
            "header": header,
            "type": "plain",
            "source": {
                "lang": self.map_language(req.from),
                "text_list": [req.text],
            },
            "target": {
                "lang": self.map_language(req.to),
            },
        });

        let result: Value =
            saladict_net::post_json_value("https://transmart.qq.com/api/imt", &body).await?;
        let lines = result
            .get("auto_translation")
            .and_then(|v| v.as_array())
            .ok_or_else(|| Error::Service(result.to_string()))?;
        let target = lines
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string();
        Ok(TranslateResult::Plain(target))
    }
}
