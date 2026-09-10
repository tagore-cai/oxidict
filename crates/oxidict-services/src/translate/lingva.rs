//! Lingva Translate 翻译（Google 翻译的开源前端，非 LLM）。
//!
//! 移植自 `src/services/translate/lingva/`。
//! 普通 HTTP 接口：`GET {endpoint}/api/v1/{from}/{to}/{text}`，返回 `{ translation }`。

use crate::Translator;
use async_trait::async_trait;
use oxidict_core::HasConfig as _;
use oxidict_core::map_language;
use oxidict_core::schema::ConfigField;
use oxidict_core::{Language, Result, TranslateRequest, TranslateResult};
use serde_json::Value;
use urlencoding::encode as url_encode;

pub struct Lingva;

const DEFAULT_ENDPOINT: &str = "lingva.ml";

#[async_trait]
impl Translator for Lingva {
    fn id(&self) -> &str {
        "lingva"
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        vec![ConfigField::Text {
            key: "requestPath",
            label: "接口地址",
            placeholder: DEFAULT_ENDPOINT,
            secret: false,
            required: false,
        }]
    }

    fn map_language(&self, lang: Language) -> String {
        map_language!(lang, {
            Auto => "auto",
            ZhCn => "zh",
            ZhTw => "zh_HANT",
            En => "en",
            Ja => "ja",
            Ko => "ko",
            Fr => "fr",
            Es => "es",
            Ru => "ru",
            De => "de",
            It => "it",
            Tr => "tr",
            PtPt => "pt",
            PtBr => "pt",
            Vi => "vi",
            Id => "id",
            Th => "th",
            Ms => "ms",
            Ar => "ar",
            Hi => "hi",
            MnCy => "mn",
            MnMo => "mn",
            Km => "km",
            NbNo => "no",
            NnNo => "no",
            Fa => "fa",
            Sv => "sv",
            Pl => "pl",
            Nl => "nl",
            Uk => "uk",
            He => "he",
        })
    }

    async fn translate(&self, req: TranslateRequest) -> Result<TranslateResult> {
        let raw = req
            .str("requestPath")
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .unwrap_or(DEFAULT_ENDPOINT);
        let endpoint = if raw.starts_with("http") {
            raw.to_string()
        } else {
            format!("https://{}", raw)
        };

        let from = self.map_language(req.from);
        let to = self.map_language(req.to);
        // 与 JS 一致：先把 '/' 转义成 '@@'，再 URL 编码，回来时再还原。
        let plain = req.text.replace('/', "@@");
        let encoded = url_encode(&plain);

        let url = format!(
            "{endpoint}/api/v1/{from}/{to}/{encoded}",
            endpoint = endpoint,
            from = from,
            to = to,
            encoded = encoded,
        );

        let result: Value = oxidict_net::get_value(url).await?;
        let translation = result
            .get("translation")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                oxidict_core::Error::Service(format!("lingva: 无翻译结果: {}", result))
            })?;
        Ok(TranslateResult::Plain(
            translation.replace("@@", "/").trim().to_string(),
        ))
    }
}
