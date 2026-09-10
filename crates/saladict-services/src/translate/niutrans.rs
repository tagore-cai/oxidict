//! 小牛翻译。
//!
//! 移植自 `src/services/translate/niutrans/index.jsx`，接口无需签名。

use crate::Translator;
use async_trait::async_trait;
use saladict_core::map_language;
use saladict_core::schema::ConfigField;
use saladict_core::{Error, HasConfig, Language, Result, TranslateRequest, TranslateResult};
use serde_json::{Value, json};

pub struct Niutrans;

#[async_trait]
impl Translator for Niutrans {
    fn id(&self) -> &str {
        "niutrans"
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        vec![
            ConfigField::Switch {
                key: "https",
                label: "使用 HTTPS",
                default: true,
            },
            ConfigField::Text {
                key: "apikey",
                label: "API Key",
                placeholder: "",
                secret: true,
                required: true,
            },
        ]
    }

    fn map_language(&self, lang: Language) -> String {
        map_language!(lang, {
            ZhCn => "zh",
            ZhTw => "cht",
            MnCy => "mn",
            MnMo => "mo",
            NbNo => "nb",
            NnNo => "nn",
        })
    }

    async fn translate(&self, req: TranslateRequest) -> Result<TranslateResult> {
        let https = req.bool("https");
        let apikey = req.require_str("apikey", "小牛翻译 API Key")?;
        let url = format!(
            "{}://api.niutrans.com/NiuTransServer/translation",
            if https { "https" } else { "http" }
        );

        let body = json!({
            "from": self.map_language(req.from),
            "to": self.map_language(req.to),
            "apikey": apikey,
            "src_text": req.text,
        });

        let result: Value = saladict_net::post_json_value(&url, &body).await?;
        let translated = result
            .get("tgt_text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::Service(result.to_string()))?
            .trim()
            .to_string();
        Ok(TranslateResult::Plain(translated))
    }
}
