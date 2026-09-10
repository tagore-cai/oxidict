//! OpenAI 兼容接口的共享实现。
//!
//! 对应原 `openai_compatible.js`：所有走 chat/completions 的服务（openai、
//! ollama、geminipro 以及各 *_cloud 中转）共用这段逻辑——提示词模板替换、
//! 可选流式输出、响应解析。差异只在端点地址与鉴权头。

use saladict_core::{Error, Language, Result, StreamSink, TranslateRequest, TranslateResult};
use saladict_net::NetErr as _;
use serde_json::{Value, json};

/// LLM 提示词里的语言用完整英文名，翻译质量比语言码好。
pub fn language_display(lang: Language) -> &'static str {
    match lang {
        Language::Auto => "auto",
        Language::ZhCn => "Simplified Chinese",
        Language::ZhTw => "Traditional Chinese",
        Language::En => "English",
        Language::Ja => "Japanese",
        Language::Ko => "Korean",
        Language::Fr => "French",
        Language::Es => "Spanish",
        Language::Ru => "Russian",
        Language::De => "German",
        Language::It => "Italian",
        Language::Tr => "Turkish",
        Language::PtPt => "Portuguese",
        Language::PtBr => "Brazilian Portuguese",
        Language::Vi => "Vietnamese",
        Language::Id => "Indonesian",
        Language::Th => "Thai",
        Language::Ms => "Malay",
        Language::Ar => "Arabic",
        Language::Hi => "Hindi",
        Language::Km => "Khmer",
        Language::MnCy => "Mongolian (Cyrillic)",
        Language::NbNo => "Norwegian Bokmal",
        Language::NnNo => "Norwegian Nynorsk",
        Language::Fa => "Persian",
        Language::Sv => "Swedish",
        Language::Pl => "Polish",
        Language::Nl => "Dutch",
        Language::Uk => "Ukrainian",
        Language::He => "Hebrew",
        Language::MnMo => "Mongolian",
    }
}

/// 默认提示词，与原实现一致：系统提示 + 用户消息。
pub fn default_prompts() -> Value {
    json!([
        {
            "role": "system",
            "content": "You are a professional translation engine, please translate the text into a colloquial, professional, elegant and fluent content, without the style of machine translation. You must only translate the text content, never interpret it."
        },
        {
            "role": "user",
            "content": "Translate into $to:\n\"\"\"\n$text\n\"\"\""
        }
    ])
}

/// 把配置里的 promptList（或默认模板）填入实际文本。
pub fn build_messages(req: &TranslateRequest, prompts: Value) -> Vec<(String, String)> {
    let from = req.from.code().to_string();
    let to = req.to.code().to_string();
    let detect = req
        .detect
        .map(language_display)
        .unwrap_or("auto")
        .to_string();

    let list = match prompts {
        Value::Array(a) => a,
        // 不变量：default_prompts() 返回的内置常量是 JSON Array。
        _ => default_prompts()
            .as_array()
            .expect("内置默认 prompts 是 JSON Array")
            .clone(),
    };

    list.into_iter()
        .filter_map(|item| {
            let role = item.get("role")?.as_str()?.to_string();
            let content = item.get("content")?.as_str()?.to_string();
            let content = content
                .replace("$text", &req.text)
                .replace("$from", &from)
                .replace("$to", &to)
                .replace("$detect", &detect);
            Some((role, content))
        })
        .collect()
}

/// 归一化请求地址：补协议头，保证指向 chat/completions。
pub fn normalize_url(request_path: &str, force_chat_completions: bool) -> String {
    let mut path = request_path.trim().to_string();
    if !path.starts_with("http://") && !path.starts_with("https://") {
        path = format!("https://{}", path);
    }
    if force_chat_completions && !path.ends_with("/chat/completions") {
        if !path.ends_with('/') {
            path.push('/');
        }
        path.push_str("v1/chat/completions");
    }
    path
}

/// 调用 chat/completions 并解析响应，`stream` 为真时增量回调。
pub async fn chat_completions(
    url: &str,
    headers: &[(&str, &str)],
    body: Value,
    stream: bool,
    sink: Option<&StreamSink>,
) -> Result<TranslateResult> {
    let mut rb = saladict_net::post_with_headers(url, headers).json(&body);
    if !stream {
        rb = rb.header("Accept", "application/json");
    }
    let resp = rb.send().await.net_err()?;

    if stream {
        let resp = saladict_net::check(resp).await?;
        use futures::StreamExt;
        let mut stream = saladict_net::sse_text(resp);
        let mut target = String::new();
        while let Some(line) = stream.next().await {
            let line = line?;
            if let Some(payload) = saladict_net::sse_payload(&line)
                && let Ok(chunk) = serde_json::from_str::<Value>(payload)
                && let Some(delta) = chunk
                    .pointer("/choices/0/delta/content")
                    .and_then(|v| v.as_str())
            {
                target.push_str(delta);
                if let Some(sink) = sink {
                    sink(format!("{}_", target));
                }
            }
        }
        Ok(TranslateResult::Plain(target.trim().to_string()))
    } else {
        let resp = saladict_net::check(resp).await?;
        let result: Value = resp.json().await.net_err()?;
        let content = result
            .pointer("/choices/0/message/content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::Service(result.to_string()))?
            .trim()
            .to_string();
        // 与原实现一致：去掉模型可能自作主张加的引号。
        let content = content
            .strip_prefix('"')
            .unwrap_or(&content)
            .strip_suffix('"')
            .unwrap_or(&content)
            .to_string();
        Ok(TranslateResult::Plain(content))
    }
}

/// 构造标准 chat/completions 请求体。
pub fn build_body(model: &str, stream: bool, messages: &[(String, String)], extra: Value) -> Value {
    let mut body = json!({
        "model": model,
        "stream": stream,
        "messages": messages
            .iter()
            .map(|(role, content)| json!({"role": role, "content": content}))
            .collect::<Vec<_>>(),
    });
    if let (Value::Object(base), Value::Object(ext)) = (&mut body, &extra) {
        for (k, v) in ext {
            base.insert(k.clone(), v.clone());
        }
    }
    body
}

/// 从配置读取 promptList；没有配置时用默认模板。
pub fn prompts_from(req: &TranslateRequest) -> Value {
    req.config
        .get("promptList")
        .cloned()
        .unwrap_or_else(default_prompts)
}

/// Ollama 走原生 /api/chat 协议，响应结构与 OpenAI 不同，单独解析。
pub async fn ollama_chat(url: &str, body: Value) -> Result<TranslateResult> {
    let resp = saladict_net::post_json_value(url, &body).await?;
    let content = resp
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    Ok(TranslateResult::Plain(content.trim().to_string()))
}
