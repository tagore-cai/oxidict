//! 微软 Bing 翻译（Edge 免 token 端点）。
//!
//! 移植自 `src/services/translate/bing/index.jsx`。
//! 新端点 `edge.microsoft.com/translate/translatetext` 无需 token；
//! 请求体为纯字符串数组，`from` 被故意省略以让端点自动检测源语言。

use crate::Translator;
use async_trait::async_trait;
use saladict_core::map_language;
use saladict_core::schema::ConfigField;
use saladict_core::{Error, Language, Result, TranslateRequest, TranslateResult};
use saladict_net::NetErr as _;
use saladict_net::{check, post_with_headers};
use serde_json::Value;

pub struct Bing;

const MS_TRANSLATE_URL: &str =
    "https://edge.microsoft.com/translate/translatetext?isEnterpriseClient=false&";

/// 原 JS 里 `DEFAULT_EDGE_USER_AGENT`。
const DEFAULT_EDGE_USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/113.0.0.0 Safari/537.36 Edg/113.0.1774.42";

#[async_trait]
impl Translator for Bing {
    fn id(&self) -> &str {
        "bing"
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        // 无需任何配置。
        vec![]
    }

    fn map_language(&self, lang: Language) -> String {
        map_language!(lang, {
            ZhCn => "zh-Hans",
            ZhTw => "zh-Hant",
            En => "en",
            Ja => "ja",
            Ko => "ko",
            Fr => "fr",
            Es => "es",
            Ru => "ru",
            De => "de",
            It => "it",
            Tr => "tr",
            PtPt => "pt-pt",
            PtBr => "pt",
            Vi => "vi",
            Id => "id",
            Th => "th",
            Ms => "ms",
            Ar => "ar",
            Hi => "hi",
            MnCy => "mn-Cyrl",
            MnMo => "mn-Mong",
            Km => "km",
            NbNo => "nb",
            Fa => "fa",
            Sv => "sv",
            Pl => "pl",
            Nl => "nl",
            Uk => "uk",
            He => "he",
        })
    }

    async fn translate(&self, req: TranslateRequest) -> Result<TranslateResult> {
        if req.text.is_empty() {
            return Ok(TranslateResult::Plain(String::new()));
        }

        // 原 JS 只把 `to` 放进 URL，并省略 `from` 以启用自动检测。
        let to = self.map_language(req.to);
        let url = format!("{}to={}", MS_TRANSLATE_URL, to);

        let resp = post_with_headers(
            url,
            &[
                ("Content-Type", "application/json"),
                ("User-Agent", DEFAULT_EDGE_USER_AGENT),
            ],
        )
        .json(std::slice::from_ref(&req.text))
        .send()
        .await
        .net_err()?;
        let resp = check(resp).await?;
        let result: Value = resp.json().await.net_err()?;

        if let Some(arr) = result.as_array() {
            if let Some(text) = arr
                .first()
                .and_then(|v| v.get("translations"))
                .and_then(|v| v.get(0))
                .and_then(|v| v.get("text"))
                .and_then(|v| v.as_str())
            {
                return Ok(TranslateResult::Plain(text.trim().to_string()));
            }
        }

        Err(Error::Service(
            serde_json::to_string(&result).unwrap_or_default(),
        ))
    }
}
