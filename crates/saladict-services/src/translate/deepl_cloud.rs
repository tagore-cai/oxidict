//! DeepL（沙拉云中转）翻译。
//!
//! 移植自 `src/services/translate/deepl_cloud/`。
//! 同样走沙拉在线中转的 OpenAI 兼容 `/api/chat/completions`，但模型固定为 `deepL-pro`，
//! 且只发一条不含 `role` 的消息，其 `content` 是 DeepL 风格的 JSON：`{"text","sl","tl"}`。
//! 鉴权头 `Authorization: Bearer <apiKey>`（配置面板未暴露 apiKey，由沙拉账号会话兜底）。
//! 响应解析同 OpenAI 兼容格式：`choices[0].message.content` / `choices[0].delta.content`。

use crate::Translator;
use saladict_core::HasConfig as _;
use async_trait::async_trait;
use futures::StreamExt;
use saladict_core::map_language;
use saladict_core::schema::ConfigField;
use saladict_core::{Error, Language, Result, TranslateRequest, TranslateResult};
use serde_json::{json, Value};

pub struct DeeplCloud;

/// 沙拉云中转的 chat/completions 端点。配置里的 `requestPath` 可覆盖。
const DEFAULT_ENDPOINT: &str = "https://api.saladict.com/api/chat/completions";
const DEFAULT_MODEL: &str = "deepL-pro";

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
impl Translator for DeeplCloud {
    fn id(&self) -> &str {
        "deepl_cloud"
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        // 原 Config.jsx 除 instance_name 外无可编辑项：model 固定 deepL-pro，promptList 固定。
        vec![]
    }

    fn map_language(&self, lang: Language) -> String {
        // 这里不直接用于 prompt（deepL 用 sl/tl 的裸内部码），保留 info.ts 映射以备他用。
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

        // DeepL 风格的消息体：裸内部码作为 sl/tl，文本经 JSON 转义。
        let sl = req.from.code();
        let tl = req.to.code();
        let payload = serde_json::to_string(&serde_json::json!({
            "text": req.text,
            "sl": sl,
            "tl": tl,
        }))?;
        let messages = json!([{"content": payload}]);

        let stream = req.on_stream.is_some();
        let body = json!({
            "model": model,
            "messages": messages,
            "stream": stream,
        });

        let auth = format!("Bearer {}", api_key);
        let headers = [
            ("Content-Type", "application/json"),
            ("Authorization", auth.as_str()),
        ];

        let rb = saladict_net::post_with_headers(url, &headers).json(&body);
        let resp = rb
            .send()
            .await
            .map_err(|e| Error::Network(e.to_string()))?;
        let resp = saladict_net::check(resp).await?;

        // 流式：边解析 SSE 增量边推送。
        if let Some(sink) = &req.on_stream {
            let mut full = String::new();
            let mut s = saladict_net::sse_text(resp);
            while let Some(line) = s.next().await {
                let line = line?;
                if let Some(data) = saladict_net::sse_payload(&line) {
                    if let Ok(v) = serde_json::from_str::<Value>(data) {
                        if let Some(delta) = extract_content(&v) {
                            full.push_str(delta);
                            sink(delta.to_string());
                        }
                    }
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
            .ok_or_else(|| Error::Service(format!("deepl_cloud: 无翻译结果: {}", text)))?;
        Ok(TranslateResult::Plain(content.trim().to_string()))
    }
}
