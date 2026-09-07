//! 微软 Bing 词典（dictionarywords 接口）。
//!
//! 移植自 `src/services/translate/bing_dict/index.jsx`。
//! 该接口只支持单词查询，返回结构化词典结果（发音 / 释义 / 变形 / 例句）。
//! 注意：原 JS 在 `from == 'auto'` 时按首字符粗略判断中英文，这里用同样的逻辑。

use crate::Translator;
use async_trait::async_trait;
use saladict_core::map_language;
use saladict_core::model::{DictResult, Pronunciation};
use saladict_core::schema::ConfigField;
use saladict_core::{Error, Language, Result, TranslateRequest, TranslateResult};
use saladict_net::get_value;
use serde_json::Value;

pub struct BingDict;

const ENDPOINT: &str = "https://www.bing.com/api/v6/dictionarywords/search";
// 原 JS 里硬编码的 appid。
const APP_ID: &str = "371E7B2AF0F9B84EC491D731DF90A55719C7D209";

#[async_trait]
impl Translator for BingDict {
    fn id(&self) -> &str {
        "bing_dict"
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        vec![]
    }

    fn map_language(&self, lang: Language) -> String {
        map_language!(lang, {
            ZhCn => "zh-cn",
            ZhTw => "zh-cn",
            En => "en-us",
        })
    }

    async fn translate(&self, req: TranslateRequest) -> Result<TranslateResult> {
        let text = &req.text;

        // 简单的语言检测：首字符是 ASCII 字母视为英文，否则继续走 auto。
        let from = if req.from == Language::Auto {
            if text
                .chars()
                .next()
                .map(|c| c.is_ascii_alphabetic())
                .unwrap_or(false)
            {
                Language::En
            } else {
                // 无法检测，原 JS 会直接返回空串。
                return Ok(TranslateResult::Plain(String::new()));
            }
        } else {
            req.from
        };

        // 只支持英文 -> 其它语言，且 source != target。
        if from != Language::En || req.to == from {
            return Ok(TranslateResult::Plain(String::new()));
        }

        let url = format!(
            "{}?q={}&appid={}&mkt=zh-cn&pname=bingdict",
            ENDPOINT,
            urlencoding::encode(text),
            APP_ID
        );

        let res: Value = get_value(url).await?;

        let value = res
            .get("value")
            .and_then(|v| v.get(0))
            .and_then(|v| v.get("meaningGroups"))
            .and_then(|v| v.as_array());
        let meaning_groups = match value {
            Some(g) if !g.is_empty() => g,
            _ => {
                return Err(Error::Service(format!("Words not yet included: {}", text)));
            }
        };

        let mut dict = DictResult::new();

        // DISPLAY_FORMAT_DEFAULT = '发音, 快速释义, 变形'，按词性把 meaningGroups 分组。
        let formats = ["发音", "快速释义", "变形"];
        let mut groups: std::collections::HashMap<&str, Vec<&Value>> =
            formats.iter().map(|f| (*f, Vec::new())).collect();
        for g in meaning_groups {
            let group = g
                .get("partsOfSpeech")
                .and_then(|v| v.get(0))
                .and_then(|v| {
                    v.get("description")
                        .and_then(|d| d.as_str())
                        .or_else(|| v.get("name").and_then(|n| n.as_str()))
                });
            if let Some(group) = group {
                if let Some(bucket) = groups.get_mut(group) {
                    bucket.push(g);
                }
            }
        }

        // 发音
        if let Some(prons) = groups.get("发音") {
            for p in prons {
                let symbol = p
                    .get("meanings")
                    .and_then(|v| v.get(0))
                    .and_then(|v| v.get("richDefinitions"))
                    .and_then(|v| v.get(0))
                    .and_then(|v| v.get("fragments"))
                    .and_then(|v| v.get(0))
                    .and_then(|v| v.get("text"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                dict.pronunciations.push(Pronunciation {
                    symbol,
                    voice: None,
                });
            }
        }

        // 快速释义
        if let Some(exps) = groups.get("快速释义") {
            for e in exps {
                let trait_ = e
                    .get("partsOfSpeech")
                    .and_then(|v| v.get(0))
                    .and_then(|v| v.get("name"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let explains = e
                    .get("meanings")
                    .and_then(|v| v.get(0))
                    .and_then(|v| v.get("richDefinitions"))
                    .and_then(|v| v.get(0))
                    .and_then(|v| v.get("fragments"))
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|f| f.get("text").and_then(|t| t.as_str()))
                            .map(|s| s.to_string())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                dict.explanations.push(saladict_core::model::Explanation {
                    trait_,
                    explains,
                });
            }
        }

        // 变形（取第一个 group 的 fragments 文本）
        if let Some(assoc) = groups.get("变形") {
            if let Some(first) = assoc.get(0) {
                if let Some(frags) = first
                    .get("meanings")
                    .and_then(|v| v.get(0))
                    .and_then(|v| v.get("richDefinitions"))
                    .and_then(|v| v.get(0))
                    .and_then(|v| v.get("fragments"))
                    .and_then(|v| v.as_array())
                {
                    for f in frags {
                        if let Some(t) = f.get("text").and_then(|v| v.as_str()) {
                            dict.associations.push(t.to_string());
                        }
                    }
                }
            }
        }

        Ok(TranslateResult::Dict(dict))
    }
}
