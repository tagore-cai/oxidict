//! 智谱 ChatGLM 翻译（BigModel 开放平台）。
//!
//! 移植自 `src/services/translate/chatglm/index.jsx`。
//! 鉴权用的是 HS256 JWT（jose 库），这里用 `hmac_sha256_raw` + base64url 手搓；
//! 支持流式（SSE）与非流式两种模式，流式时通过 `req.on_stream` 增量回传。

use crate::Translator;
use async_trait::async_trait;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use futures::StreamExt;
use saladict_core::map_language;
use saladict_core::schema::ConfigField;
use saladict_core::HasConfig as _;
use saladict_core::{Error, Language, Result, TranslateRequest, TranslateResult};
use saladict_net::NetErr as _;
use saladict_net::{check, hmac_sha256_raw, post_with_headers, sse_payload, sse_text};
use serde_json::{json, Value};

pub struct Chatglm;

const ENDPOINT: &str = "https://open.bigmodel.cn/api/paas/v4/chat/completions";
const DEFAULT_MODEL: &str = "glm-4.5-flash";

#[async_trait]
impl Translator for Chatglm {
    fn id(&self) -> &str {
        "chatglm"
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        vec![
            ConfigField::Text {
                key: "model",
                label: "Model",
                placeholder: DEFAULT_MODEL,
                secret: false,
                required: false,
            },
            ConfigField::secret("apiKey", "API Key"),
            ConfigField::Switch {
                key: "stream",
                label: "Stream",
                default: true,
            },
        ]
    }

    fn map_language(&self, lang: Language) -> String {
        map_language!(lang, {
            ZhCn => "Simplified Chinese",
            ZhTw => "Traditional Chinese",
            En => "English",
            Ja => "Japanese",
            Ko => "Korean",
            Fr => "French",
            Es => "Spanish",
            Ru => "Russian",
            De => "German",
            It => "Italian",
            Tr => "Turkish",
            PtPt => "Portuguese",
            PtBr => "Brazilian Portuguese",
            Vi => "Vietnamese",
            Id => "Indonesian",
            Th => "Thai",
            Ms => "Malay",
            Ar => "Arabic",
            Hi => "Hindi",
            MnMo => "Mongolian",
            MnCy => "Mongolian(Cyrillic)",
            Km => "Khmer",
            NbNo => "Norwegian Bokmål",
            NnNo => "Norwegian Nynorsk",
            Fa => "Persian",
            Sv => "Swedish",
            Pl => "Polish",
            Nl => "Dutch",
            Uk => "Ukrainian",
            He => "Hebrew",
        })
    }

    async fn translate(&self, req: TranslateRequest) -> Result<TranslateResult> {
        let api_key = req.require_str("apiKey", "invalid apikey")?;
        // 原 JS：apiKey 形如 "id.secret"。
        let (id, secret) = match api_key.split_once('.') {
            Some(pair) => pair,
            None => return Err(Error::Service("invalid apikey".into())),
        };

        // 模板变量替换（$text / $from / $to / $detect）。
        let detect_name = req
            .detect
            .map(|d| d.english_name())
            .unwrap_or_default()
            .to_string();
        let from_code = req.from.code().to_string();
        let to_code = req.to.code().to_string();

        let mut messages: Vec<Value> = Vec::new();
        if let Some(arr) = req.config.get("promptList").and_then(|v| v.as_array()) {
            for item in arr {
                let role = item.get("role").and_then(|v| v.as_str()).unwrap_or("user");
                let content = item
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .replace("$text", &req.text)
                    .replace("$from", &from_code)
                    .replace("$to", &to_code)
                    .replace("$detect", &detect_name);
                messages.push(json!({ "role": role, "content": content }));
            }
        }
        if messages.is_empty() {
            messages.push(json!({ "role": "user", "content": req.text.clone() }));
        }

        let model = req
            .str("model")
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .unwrap_or(DEFAULT_MODEL)
            .to_string();
        let stream = req.bool("stream");

        // ---- 构造 HS256 JWT ----
        let timestamp = chrono::Utc::now().timestamp_millis();
        let header = json!({ "alg": "HS256", "sign_type": "SIGN" });
        let payload = json!({
            "api_key": id,
            "exp": timestamp + 1000 * 60,
            "timestamp": timestamp,
        });
        let header_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header)?);
        let payload_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload)?);
        let signing_input = format!("{}.{}", header_b64, payload_b64);
        let sig = hmac_sha256_raw(secret.as_bytes(), signing_input.as_bytes());
        let sig_b64 = URL_SAFE_NO_PAD.encode(sig);
        let token = format!("{}.{}", signing_input, sig_b64);

        let body = json!({
            "model": model,
            "thinking": { "type": "disabled" },
            "messages": messages,
            "stream": stream,
        });

        let builder = post_with_headers(
            ENDPOINT,
            &[
                ("Content-Type", "application/json"),
                ("Authorization", &token),
            ],
        )
        .json(&body);

        if stream {
            let resp = builder.send().await.net_err()?;
            let resp = check(resp).await?;
            let mut stream = sse_text(resp);

            let mut target = String::new();
            let mut temp = String::new();
            while let Some(item) = stream.next().await {
                let line = item?;
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if let Some(payload) = sse_payload(line) {
                    if payload == "[DONE]" {
                        continue;
                    }
                    let data = if temp.is_empty() {
                        payload.to_string()
                    } else {
                        format!("{}{}", temp, payload)
                    };
                    match serde_json::from_str::<Value>(&data) {
                        Ok(v) => {
                            temp.clear();
                            if let Some(content) = v
                                .get("choices")
                                .and_then(|c| c.get(0))
                                .and_then(|c| c.get("delta"))
                                .and_then(|d| d.get("content"))
                                .and_then(|c| c.as_str())
                            {
                                target.push_str(content);
                                if let Some(cb) = &req.on_stream {
                                    cb(target.clone());
                                }
                            }
                        }
                        Err(_) => temp = data,
                    }
                }
            }
            Ok(TranslateResult::Plain(target.trim().to_string()))
        } else {
            let resp = builder.send().await.net_err()?;
            let resp = check(resp).await?;
            let result: Value = resp.json().await.net_err()?;

            if let Some(choices) = result.get("choices") {
                if let Some(content) = choices
                    .get(0)
                    .and_then(|c| c.get("message"))
                    .and_then(|m| m.get("content"))
                    .and_then(|c| c.as_str())
                {
                    let mut t = content.trim().to_string();
                    if t.starts_with('"') {
                        t.remove(0);
                    }
                    if t.ends_with('"') {
                        t.pop();
                    }
                    return Ok(TranslateResult::Plain(t.trim().to_string()));
                }
                return Err(Error::Service(serde_json::to_string(choices)?));
            }
            Err(Error::Service(
                serde_json::to_string(&result).unwrap_or_default(),
            ))
        }
    }
}
