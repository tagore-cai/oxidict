//! Claude（沙拉云中转）翻译。
//!
//! 移植自 `src/services/translate/claude_cloud/`。
//! 走沙拉在线中转的 OpenAI 兼容 `/api/chat/completions`，鉴权头 `Authorization: Bearer <apiKey>`，
//! SSE 增量字段为 `choices[0].delta.content`（中转已把 Anthropic 原生格式归一化）。

use crate::Translator;
use async_trait::async_trait;
use futures::StreamExt;
use saladict_core::HasConfig as _;
use saladict_core::map_language;
use saladict_core::schema::ConfigField;
use saladict_core::{Error, Language, Result, TranslateRequest, TranslateResult};
use serde_json::{Value, json};

pub struct ClaudeCloud;

/// 沙拉云中转的 chat/completions 端点。配置里的 `requestPath` 可覆盖。
const DEFAULT_ENDPOINT: &str = "https://api.saladict.com/api/chat/completions";
const DEFAULT_MODEL: &str = "claude-4.5-haiku";
const DEFAULT_TEMP: f64 = 0.1;

const SYSTEM_PROMPT: &str = "You are a professional translation engine, please translate the text into a colloquial, professional, elegant and fluent content, without the style of machine translation. You must only translate the text content, never interpret it.";

const MODELS: &[(&str, &str)] = &[
    ("claude-4.5-haiku", "claude-4.5-haiku"),
    ("claude-4.5-sonnet", "claude-4.5-sonnet"),
];

/// 从一条 OpenAI 兼容的 chunk 里取出增量文本：先试 `delta.content`（流式），再试 `message.content`（非流式）。
fn extract_content(v: &Value) -> Option<&str> {
    let choice = v.get("choices")?.get(0)?;
    if let Some(c) = choice
        .get("delta")
        .and_then(|d| d.get("content"))
        .and_then(|c| c.as_str())
    {
        return Some(c);
    }
    choice
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
}

#[async_trait]
impl Translator for ClaudeCloud {
    fn id(&self) -> &str {
        "claude_cloud"
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        vec![
            ConfigField::secret("apiKey", "API Key"),
            ConfigField::Select {
                key: "model",
                label: "模型",
                options: MODELS,
                default: DEFAULT_MODEL,
            },
            ConfigField::Switch {
                key: "stream",
                label: "流式输出",
                default: false,
            },
            ConfigField::Number {
                key: "temperature",
                label: "温度",
                default: DEFAULT_TEMP,
                min: 0.0,
                max: 2.0,
            },
        ]
    }

    fn map_language(&self, lang: Language) -> String {
        map_language!(lang, {
            Auto => "Auto",
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
        let url = req
            .str("requestPath")
            .filter(|s| !s.is_empty())
            .unwrap_or(DEFAULT_ENDPOINT)
            .to_string();
        let api_key = req.str("apiKey").unwrap_or("").to_string();
        let model = req
            .str("model")
            .filter(|s| !s.is_empty())
            .unwrap_or(DEFAULT_MODEL)
            .to_string();
        let temperature = req
            .config
            .get("temperature")
            .and_then(|v| v.as_f64())
            .unwrap_or(DEFAULT_TEMP);

        let to_lang = self.map_language(req.to);
        let user_content = format!("Translate into {}:\n\"\"\"\n{}\n\"\"\"", to_lang, req.text);
        let messages = json!([
            {"role": "system", "content": SYSTEM_PROMPT},
            {"role": "user", "content": user_content},
        ]);

        let stream = req.bool("stream") || req.on_stream.is_some();
        let mut body = json!({
            "model": model,
            "messages": messages,
            "stream": stream,
        });
        body["temperature"] = json!(temperature);

        let auth = format!("Bearer {}", api_key);
        let headers = [
            ("Content-Type", "application/json"),
            ("Authorization", auth.as_str()),
        ];

        let rb = saladict_net::post_with_headers(url, &headers).json(&body);
        let resp = rb.send().await.map_err(|e| Error::Network(e.to_string()))?;
        let resp = saladict_net::check(resp).await?;

        // 流式：边解析 SSE 增量边推送。
        if let Some(sink) = &req.on_stream {
            let mut full = String::new();
            let mut s = saladict_net::sse_text(resp);
            while let Some(line) = s.next().await {
                let line = line?;
                if let Some(data) = saladict_net::sse_payload(&line)
                    && let Ok(v) = serde_json::from_str::<Value>(data)
                    && let Some(delta) = extract_content(&v)
                {
                    full.push_str(delta);
                    sink(delta.to_string());
                }
            }
            return Ok(TranslateResult::Plain(full.trim().to_string()));
        }

        // 非流式：直接解析完整 JSON。
        let text = resp
            .text()
            .await
            .map_err(|e| Error::Network(e.to_string()))?;
        let v: Value = serde_json::from_str(&text)?;
        let content = extract_content(&v)
            .ok_or_else(|| Error::Service(format!("claude_cloud: 无翻译结果: {}", text)))?;
        Ok(TranslateResult::Plain(content.trim().to_string()))
    }
}
