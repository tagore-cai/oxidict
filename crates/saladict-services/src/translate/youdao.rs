//! 有道翻译。
//!
//! 移植自 `src/services/translate/youdao/index.jsx`，SHA256(v3) 签名；
//! 词典模式下会顺带拉取英式/美式发音音频。

use crate::Translator;
use async_trait::async_trait;
use saladict_core::map_language;
use saladict_core::model::{DictResult, Explanation, Pronunciation, Sentence};
use saladict_core::schema::ConfigField;
use saladict_core::{Error, HasConfig, Language, Result, TranslateRequest, TranslateResult};
use serde_json::Value;
use std::sync::Arc;

pub struct Youdao;

const URL: &str = "https://openapi.youdao.com/api";

#[async_trait]
impl Translator for Youdao {
    fn id(&self) -> &str {
        "youdao"
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        vec![
            ConfigField::Text {
                key: "appkey",
                label: "应用 ID",
                placeholder: "",
                secret: false,
                required: true,
            },
            ConfigField::Text {
                key: "key",
                label: "应用密钥",
                placeholder: "",
                secret: true,
                required: true,
            },
        ]
    }

    fn map_language(&self, lang: Language) -> String {
        map_language!(lang, {
            ZhCn => "zh-CHS",
            ZhTw => "zh-CHT",
        })
    }

    async fn translate(&self, req: TranslateRequest) -> Result<TranslateResult> {
        use saladict_net::NetErr as _;

        let appkey = req.require_str("appkey", "有道应用 ID")?;
        let key = req.require_str("key", "有道应用密钥")?;

        let text = req.text.trim().to_string();
        let curtime = chrono::Utc::now().timestamp().to_string();
        let salt = saladict_net::uuid_v4();

        let sign_raw = format!("{}{}{}{}{}", appkey, truncate(&text), salt, curtime, key);
        let sign = saladict_net::sha256_hex(sign_raw.as_bytes());

        let url = format!(
            "{URL}?q={q}&from={from}&to={to}&appKey={appkey}&salt={salt}&sign={sign}&signType=v3&curtime={curtime}",
            q = urlencoding::encode(&text),
            from = self.map_language(req.from),
            to = self.map_language(req.to),
            appkey = urlencoding::encode(&appkey),
            salt = urlencoding::encode(&salt),
            sign = urlencoding::encode(&sign),
            curtime = curtime,
        );

        let resp = saladict_net::client().get(&url).send().await.net_err()?;
        let resp = saladict_net::check(resp).await?;
        let result: Value = resp.json().await.net_err()?;

        // 词典模式
        if result
            .get("isWord")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            let mut dict = DictResult::new();
            let basic = result.get("basic").cloned().unwrap_or(Value::Null);

            let mut has_voice = false;
            for (region_key, phonetic_key, speech_key) in [
                ("UK", "uk-phonetic", "uk-speech"),
                ("US", "us-phonetic", "us-speech"),
            ] {
                let phonetic = basic.get(phonetic_key).and_then(|v| v.as_str());
                if let Some(symbol) = phonetic {
                    let voice = match basic.get(speech_key).and_then(|v| v.as_str()) {
                        Some(url) => saladict_net::get_bytes(url).await.ok(),
                        None => None,
                    };
                    has_voice = has_voice || voice.is_some();
                    dict.pronunciations.push(Pronunciation {
                        symbol: Some(format!("{region_key} {symbol}")),
                        voice: voice.map(Arc::new),
                    });
                }
            }
            if !has_voice && let Some(symbol) = basic.get("phonetic").and_then(|v| v.as_str()) {
                dict.pronunciations.push(Pronunciation {
                    symbol: Some(symbol.to_string()),
                    voice: None,
                });
            }

            if let Some(explains) = basic.get("explains").and_then(|v| v.as_array()) {
                for item in explains {
                    let line = item.as_str().unwrap_or_default();
                    let mut trait_ = String::new();
                    let first = line.split(' ').next().unwrap_or_default();
                    if first.ends_with('.') {
                        trait_ = first.to_string();
                    }
                    let explains: Vec<String> = line
                        .replace(&trait_, "")
                        .trim()
                        .split('；')
                        .map(|s| s.to_string())
                        .collect();
                    dict.explanations.push(Explanation {
                        trait_: if trait_.is_empty() {
                            None
                        } else {
                            Some(trait_)
                        },
                        explains,
                    });
                }
            }

            if let Some(wfs) = basic.get("wfs").and_then(|v| v.as_array()) {
                for wf in wfs {
                    let name = wf
                        .pointer("/wf/name")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default();
                    let value = wf
                        .pointer("/wf/value")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default();
                    dict.associations.push(format!("{} {}", name, value));
                }
            }
            if let Some(exam) = basic.get("exam_type").and_then(|v| v.as_array()) {
                let joined = exam
                    .iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                dict.associations.push(joined);
            }

            return Ok(TranslateResult::Dict(dict));
        }

        // 翻译模式
        let translations = result
            .get("translation")
            .and_then(|v| v.as_array())
            .ok_or_else(|| Error::Service(result.to_string()))?;
        let target = translations
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string();
        let _ = Sentence::default();
        Ok(TranslateResult::Plain(target))
    }
}

/// 有道签名要求：超过 20 字符的文本取「前 10 + 长度 + 后 10」。
fn truncate(q: &str) -> String {
    let chars: Vec<char> = q.chars().collect();
    let len = chars.len();
    if len <= 20 {
        q.to_string()
    } else {
        let head: String = chars[..10].iter().collect();
        let tail: String = chars[len - 10..].iter().collect();
        format!("{}{}{}", head, len, tail)
    }
}
