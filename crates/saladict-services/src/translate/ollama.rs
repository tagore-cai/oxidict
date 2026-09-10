//! Ollama（本地 LLM）。
//!
//! 移植自 `src/services/translate/ollama/index.jsx`，走 Ollama 原生 `/api/chat` 协议。

use super::openai_compatible as compat;
use crate::Translator;
use async_trait::async_trait;
use saladict_core::schema::ConfigField;
use saladict_core::{Error, HasConfig, Result, TranslateRequest, TranslateResult};
use serde_json::{json, Value};

pub struct Ollama;

#[async_trait]
impl Translator for Ollama {
    fn id(&self) -> &str {
        "ollama"
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        vec![
            ConfigField::Text {
                key: "requestPath",
                label: "Ollama 地址",
                placeholder: "http://localhost:11434",
                secret: false,
                required: true,
            },
            ConfigField::Text {
                key: "model",
                label: "模型",
                placeholder: "qwen2.5:7b",
                secret: false,
                required: true,
            },
            ConfigField::Switch {
                key: "stream",
                label: "流式输出",
                default: false,
            },
        ]
    }

    async fn translate(&self, req: TranslateRequest) -> Result<TranslateResult> {
        let request_path = req.require_str("requestPath", "Ollama 地址")?;
        let model = req.require_str("model", "Ollama 模型名")?;

        let mut host = compat::normalize_url(&request_path, false);
        // /api/chat 挂在 host 根上，去掉调用方可能带的路径尾部。
        while host.ends_with('/') {
            host.pop();
        }
        let url = format!("{}/api/chat", host);

        let messages = compat::build_messages(&req, compat::prompts_from(&req));
        let stream = req.bool("stream");
        let body = json!({
            "model": model,
            "stream": stream,
            "messages": messages
                .iter()
                .map(|(role, content)| json!({"role": role, "content": content}))
                .collect::<Vec<Value>>(),
        });

        // 非流式一次返回完整内容；流式由 Ollama 以 NDJSON 输出，
        // 这里沿用非流式端点取最终结果，流式增量在服务层补齐。
        if !stream {
            return compat::ollama_chat(&url, body).await;
        }
        use saladict_net::NetErr as _;
        let resp = saladict_net::client()
            .post(&url)
            .json(&body)
            .send()
            .await
            .net_err()?;
        let resp = saladict_net::check(resp).await?;
        let text = resp.text().await.net_err()?;
        let mut target = String::new();
        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let chunk: Value = serde_json::from_str(line.trim())
                .map_err(|e| Error::Service(format!("Ollama 流式响应解析失败: {e}")))?;
            if let Some(delta) = chunk.pointer("/message/content").and_then(|v| v.as_str()) {
                target.push_str(delta);
                if let Some(sink) = req.on_stream.as_ref() {
                    sink(format!("{}_", target));
                }
            }
        }
        Ok(TranslateResult::Plain(target.trim().to_string()))
    }
}
