//! 彩云小译（Caiyun Translate）。
//!
//! 移植自 `src/services/translate/caiyun/index.jsx`。
//! 鉴权头为 `x-authorization: token <token>`，请求体带 `trans_type`。

use crate::Translator;
use async_trait::async_trait;
use oxidict_core::HasConfig as _;
use oxidict_core::map_language;
use oxidict_core::schema::ConfigField;
use oxidict_core::{Error, Language, Result, TranslateRequest, TranslateResult};
use oxidict_net::NetErr as _;
use oxidict_net::{check, post_with_headers};
use serde_json::Value;

pub struct Caiyun;

const ENDPOINT: &str = "https://api.interpreter.caiyunai.com/v1/translator";

#[async_trait]
impl Translator for Caiyun {
    fn id(&self) -> &str {
        "caiyun"
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        vec![ConfigField::secret("token", "Token")]
    }

    fn map_language(&self, lang: Language) -> String {
        map_language!(lang, {
            ZhCn => "zh",
            ZhTw => "zh",
            En => "en",
            Ja => "ja",
        })
    }

    async fn translate(&self, req: TranslateRequest) -> Result<TranslateResult> {
        let token = req.str("token").unwrap_or("");
        if token.is_empty() {
            return Err(Error::Service("Please configure token".into()));
        }

        let from = self.map_language(req.from);
        let to = self.map_language(req.to);

        let body = serde_json::json!({
            "source": [req.text.clone()],
            "trans_type": format!("{}2{}", from, to),
            "request_id": "demo",
            "detect": true,
        });

        let auth = format!("token {}", token);
        let resp = post_with_headers(
            ENDPOINT,
            &[
                ("content-type", "application/json"),
                ("x-authorization", auth.as_str()),
            ],
        )
        .json(&body)
        .send()
        .await
        .net_err()?;
        let resp = check(resp).await?;
        let result: Value = resp.json().await.net_err()?;

        if let Some(target) = result
            .get("target")
            .and_then(|v| v.as_array())
            .and_then(|a| a.first())
            .and_then(|v| v.as_str())
        {
            Ok(TranslateResult::Plain(target.to_string()))
        } else {
            Err(Error::Service(
                serde_json::to_string(&result).unwrap_or_default(),
            ))
        }
    }
}
