//! Gemini Pro。
//!
//! 移植自 `src/services/translate/geminipro/index.jsx`，走 Google Generative Language
//! 原生协议。流式用 `alt=sse`（官方支持的 SSE 输出），比原实现里对 JSON 数组做正则
//! 切片的方案稳定。

use super::openai_compatible as compat;
use crate::Translator;
use async_trait::async_trait;
use saladict_core::schema::ConfigField;
use saladict_core::{Error, HasConfig, Result, TranslateRequest, TranslateResult};
use serde_json::{json, Value};

pub struct Geminipro;

const DEFAULT_MODEL_PATH: &str =
    "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash";

#[async_trait]
impl Translator for Geminipro {
    fn id(&self) -> &str {
        "geminipro"
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        vec![
            ConfigField::Text {
                key: "requestPath",
                label: "API 地址",
                placeholder: DEFAULT_MODEL_PATH,
                secret: false,
                required: false,
            },
            ConfigField::Text {
                key: "apiKey",
                label: "API Key",
                placeholder: "",
                secret: true,
                required: true,
            },
            ConfigField::Switch {
                key: "stream",
                label: "流式输出",
                default: true,
            },
        ]
    }

    async fn translate(&self, req: TranslateRequest) -> Result<TranslateResult> {
        use saladict_net::NetErr as _;
        use futures::StreamExt;

        let api_key = req.require_str("apiKey", "Gemini API Key")?;
        let stream = req.bool("stream");
        let base = req
            .str("requestPath")
            .filter(|s| !s.trim().is_empty())
            .map(|s| {
                if s.starts_with("http") {
                    s.to_string()
                } else {
                    format!("https://{}", s)
                }
            })
            .unwrap_or_else(|| DEFAULT_MODEL_PATH.to_string());
        let base = base.trim_end_matches('/').to_string();

        let url = if stream {
            format!("{base}:streamGenerateContent?alt=sse&key={api_key}")
        } else {
            format!("{base}:generateContent?key={api_key}")
        };

        // 提示词是 parts 结构，模板替换与 OpenAI 共享实现一致。
        let from = req.from.code().to_string();
        let to = req.to.code().to_string();
        let detect = compat::language_display(req.detect.unwrap_or(req.from)).to_string();
        let prompts: Vec<(String, String)> = compat::build_messages(&req, compat::prompts_from(&req));
        let _ = (&from, &to);

        let contents: Vec<Value> = prompts
            .iter()
            .map(|(role, content)| {
                json!({
                    // Gemini 没有 system 角色，system 提示并入 user 轮次。
                    "role": if role == "assistant" { "model" } else { "user" },
                    "parts": [{ "text": content }],
                })
            })
            .collect();

        let body = json!({
            "contents": contents,
            "safetySettings": [
                { "category": "HARM_CATEGORY_HARASSMENT", "threshold": "BLOCK_NONE" },
                { "category": "HARM_CATEGORY_HATE_SPEECH", "threshold": "BLOCK_NONE" },
                { "category": "HARM_CATEGORY_SEXUALLY_EXPLICIT", "threshold": "BLOCK_NONE" },
                { "category": "HARM_CATEGORY_DANGEROUS_CONTENT", "threshold": "BLOCK_NONE" },
            ],
        });

        if !stream {
            let result: Value = saladict_net::post_json_value(&url, &body).await?;
            let text = result
                .pointer("/candidates/0/content/parts/0/text")
                .and_then(|v| v.as_str())
                .ok_or_else(|| Error::Service(result.to_string()))?
                .trim()
                .to_string();
            return Ok(TranslateResult::Plain(text));
        }

        let resp = saladict_net::client()
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .net_err()?;
        let resp = saladict_net::check(resp).await?;
        let mut stream = saladict_net::sse_text(resp);
        let mut target = String::new();
        while let Some(line) = stream.next().await {
            let line = line?;
            if let Some(payload) = saladict_net::sse_payload(&line) {
                if let Ok(chunk) = serde_json::from_str::<Value>(payload) {
                    if let Some(text) = chunk
                        .pointer("/candidates/0/content/parts/0/text")
                        .and_then(|v| v.as_str())
                    {
                        target.push_str(text);
                        if let Some(sink) = req.on_stream.as_ref() {
                            sink(format!("{}_", target));
                        }
                    }
                }
            }
        }
        Ok(TranslateResult::Plain(target.trim().to_string()))
    }
}
