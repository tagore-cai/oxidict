//! DeepL 翻译。
//!
//! 原实现提供三种模式：free（网页 jsonrpc 接口）、api（官方 API Key）、deeplx（自建服务），
//! 由配置项 `type` 选择。

use crate::Translator;
use async_trait::async_trait;
use saladict_core::map_language;
use saladict_core::schema::ConfigField;
use saladict_core::{Error, HasConfig, Language, Result, TranslateRequest, TranslateResult};
use serde_json::{Value, json};

pub struct DeepL;

const JSONRPC_URL: &str = "https://www2.deepl.com/jsonrpc";

#[async_trait]
impl Translator for DeepL {
    fn id(&self) -> &str {
        "deepl"
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        vec![
            ConfigField::Select {
                key: "type",
                label: "接口类型",
                options: &[
                    ("free", "免费接口"),
                    ("api", "API Key"),
                    ("deeplx", "DeepLX"),
                ],
                default: "free",
            },
            ConfigField::Text {
                key: "authKey",
                label: "Auth Key",
                placeholder: "",
                secret: true,
                required: false,
            },
            ConfigField::Text {
                key: "customUrl",
                label: "DeepLX 地址",
                placeholder: "http://localhost:1188/translate",
                secret: false,
                required: false,
            },
        ]
    }

    fn map_language(&self, lang: Language) -> String {
        map_language!(lang, {
            ZhCn => "ZH",
            ZhTw => "ZH",
            Ja => "JA",
            En => "EN",
            Ko => "KO",
            Fr => "FR",
            Es => "ES",
            Ru => "RU",
            De => "DE",
            It => "IT",
            Tr => "TR",
            PtPt => "PT-PT",
            PtBr => "PT-BR",
            Id => "ID",
            Sv => "SV",
            Pl => "PL",
            Nl => "NL",
            Uk => "UK",
        })
    }

    async fn translate(&self, req: TranslateRequest) -> Result<TranslateResult> {
        let from = self.map_language(req.from);
        let to = self.map_language(req.to);
        let service_type = req.str("type").unwrap_or("free").to_string();
        match service_type.as_str() {
            "api" => {
                let key = req.require_str("authKey", "DeepL Auth Key")?;
                translate_by_key(&req.text, &from, &to, &key).await
            }
            "deeplx" => {
                let url = req.require_str("customUrl", "DeepLX 地址")?;
                translate_by_deeplx(&req.text, &from, &to, &url).await
            }
            _ => translate_by_free(&req.text, &from, &to).await,
        }
    }
}

/// 统计文本中 `i` 的个数，DeepL 用它计算时间戳。
fn get_i_count(text: &str) -> usize {
    text.matches('i').count()
}

/// DeepL 的防重放时间戳，算法与原实现一致。
fn get_time_stamp(i_count: usize) -> u64 {
    let ts = chrono::Utc::now().timestamp_millis() as u64;
    if i_count != 0 {
        let i_count = i_count + 1;
        ts - (ts % i_count as u64) + i_count as u64
    } else {
        ts
    }
}

async fn translate_by_free(text: &str, from: &str, to: &str) -> Result<TranslateResult> {
    use saladict_net::NetErr as _;

    let rand = saladict_net::alibaba_nonce();
    let source = if from != "auto" {
        from.get(..2).unwrap_or(from).to_string()
    } else {
        "auto".to_string()
    };
    let target = to.get(..2).unwrap_or(to).to_string();

    let body = json!({
        "jsonrpc": "2.0",
        "method": "LMT_handle_texts",
        "params": {
            "splitting": "newlines",
            "lang": {
                "source_lang_user_selected": source,
                "target_lang": target,
            },
            "texts": [{ "text": text, "requestAlternatives": 3 }],
            "timestamp": get_time_stamp(get_i_count(text)),
        },
        "id": rand,
    });

    // 原实现的字符级混淆：按随机数决定 method 字段后的空格形态。
    let mut body_str = serde_json::to_string(&body)?;
    if (rand + 5).is_multiple_of(29) || (rand + 3).is_multiple_of(13) {
        body_str = body_str.replace(r#""method":""#, r#""method" : ""#);
    } else {
        body_str = body_str.replace(r#""method":""#, r#""method": ""#);
    }

    let resp = saladict_net::client()
        .post(JSONRPC_URL)
        .header("Content-Type", "application/json")
        .body(body_str)
        .send()
        .await
        .net_err()?;
    let resp = saladict_net::check(resp).await?;
    let result: Value = resp.json().await.net_err()?;
    let translated = result
        .pointer("/result/texts/0/text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::Service(result.to_string()))?
        .trim()
        .to_string();
    Ok(TranslateResult::Plain(translated))
}

async fn translate_by_deeplx(
    text: &str,
    from: &str,
    to: &str,
    url: &str,
) -> Result<TranslateResult> {
    let body = json!({
        "source_lang": from,
        "target_lang": to,
        "text": text,
    });
    let result: Value = saladict_net::post_json_value(url, &body).await?;
    let translated = result
        .get("data")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::Service(result.to_string()))?
        .to_string();
    Ok(TranslateResult::Plain(translated))
}

async fn translate_by_key(text: &str, from: &str, to: &str, key: &str) -> Result<TranslateResult> {
    use saladict_net::NetErr as _;

    let url = if key.ends_with(":fx") {
        "https://api-free.deepl.com/v2/translate"
    } else if key.ends_with(":dp") {
        "https://api.deepl-pro.com/v2/translate"
    } else {
        "https://api.deepl.com/v2/translate"
    };

    let mut body = json!({
        "text": [text],
        "target_lang": to,
    });
    if from != "auto" {
        body["source_lang"] = json!(from);
    }

    let auth = format!("DeepL-Auth-Key {}", key);
    let resp = saladict_net::post_with_headers(url, &[("Authorization", auth.as_str())])
        .json(&body)
        .send()
        .await
        .net_err()?;
    let resp = saladict_net::check(resp).await?;
    let result: Value = resp.json().await.net_err()?;
    let translated = result
        .pointer("/translations/0/text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::Service(result.to_string()))?
        .trim()
        .to_string();
    Ok(TranslateResult::Plain(translated))
}
