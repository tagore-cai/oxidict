//! Google 翻译。
//!
//! 移植自 `src/services/translate/google/index.jsx`。
//! 这个接口返回的是异构嵌套数组：有 `result[1]` 时是词典模式，否则是纯译文。

use crate::Translator;
use async_trait::async_trait;
use saladict_core::map_language;
use saladict_core::model::{DictResult, Explanation, Pronunciation, Sentence};
use saladict_core::schema::ConfigField;
use saladict_core::HasConfig as _;
use saladict_core::{Language, Result, TranslateRequest, TranslateResult};
use serde_json::Value;

pub struct Google;

const DEFAULT_URL: &str = "https://translate.google.com";

#[async_trait]
impl Translator for Google {
    fn id(&self) -> &str {
        "google"
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        vec![ConfigField::Text {
            key: "custom_url",
            label: "自定义地址",
            placeholder: DEFAULT_URL,
            secret: false,
            required: false,
        }]
    }

    fn map_language(&self, lang: Language) -> String {
        map_language!(lang, {
            ZhCn => "zh-CN",
            ZhTw => "zh-TW",
            PtPt => "pt",
            PtBr => "pt",
            MnCy => "mn",
            NbNo => "no",
            NnNo => "no",
        })
    }

    async fn translate(&self, req: TranslateRequest) -> Result<TranslateResult> {
        let custom = req
            .str("custom_url")
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .unwrap_or(DEFAULT_URL);
        let base = if custom.starts_with("http") {
            custom.to_string()
        } else {
            format!("https://{}", custom)
        };

        let from = self.map_language(req.from);
        let to = self.map_language(req.to);

        let url = format!(
            "{base}/translate_a/single?dt=at&dt=bd&dt=ex&dt=ld&dt=md&dt=qca&dt=rw&dt=rm&dt=ss&dt=t\
             &client=gtx&sl={from}&tl={to}&hl={to}&ie=UTF-8&oe=UTF-8&otf=1&ssel=0&tsel=0&kc=7&q={q}",
            base = base,
            from = from,
            to = to,
            q = urlencoding::encode(&req.text),
        );

        let result: Value = saladict_net::get_value(url).await?;

        // result[1] 存在即为词典模式。
        if result.get(1).map(|v| !v.is_null()).unwrap_or(false) {
            let mut dict = DictResult::new();

            // 发音：result[0][1][3]
            if let Some(symbol) = result
                .get(0)
                .and_then(|v| v.get(1))
                .and_then(|v| v.get(3))
                .and_then(|v| v.as_str())
            {
                dict.pronunciations.push(Pronunciation {
                    symbol: Some(symbol.to_string()),
                    voice: None,
                });
            }

            // 释义：result[1] 每项为 [词性, ?, [释义...]]
            if let Some(items) = result.get(1).and_then(|v| v.as_array()) {
                for item in items {
                    let trait_ = item.get(0).and_then(|v| v.as_str()).map(|s| s.to_string());
                    let explains = item
                        .get(2)
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|x| x.get(0).and_then(|s| s.as_str()))
                                .map(|s| s.to_string())
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    dict.explanations.push(Explanation { trait_, explains });
                }
            }

            // 例句：result[12] 每项为 [?, [[?, ?, 原文]]]
            if let Some(items) = result.get(12).and_then(|v| v.as_array()) {
                for item in items {
                    if let Some(source) = item
                        .get(1)
                        .and_then(|v| v.get(0))
                        .and_then(|v| v.get(2))
                        .and_then(|v| v.as_str())
                    {
                        dict.sentence.push(Sentence {
                            source: Some(source.to_string()),
                            target: None,
                        });
                    }
                }
            }

            return Ok(TranslateResult::Dict(dict));
        }

        // 翻译模式：拼接 result[0] 每项的 [0]
        let mut target = String::new();
        if let Some(items) = result.get(0).and_then(|v| v.as_array()) {
            for item in items {
                if let Some(seg) = item.get(0).and_then(|v| v.as_str()) {
                    target.push_str(seg);
                }
            }
        }
        Ok(TranslateResult::Plain(target.trim().to_string()))
    }
}
