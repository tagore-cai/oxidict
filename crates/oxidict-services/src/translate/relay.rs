//! 沙拉云中转类翻译服务的**声明式**实现。
//!
//! 改造前这 9 个服务各占一个文件（合计约 1600 行），实测 `diff` 出来每个与
//! `openai_cloud.rs` 只差 **23–24 行**——且那些行全是文档注释、结构体名、
//! `DEFAULT_MODEL`、`MODELS`、`id()` 与错误串。真正的差异只有**数据**。
//!
//! 这里把它们收敛为「一张表 + 一个执行器」：新增同类中转服务只需往
//! [`RELAYS`] 里加一行。
//!
//! **变换器只做具名枚举**（[`BodyShape`] / [`FieldShape`]），不引入通用表达式
//! 或脚本语言：需要任意逻辑（多步请求、签名、解析 HTML）时应走 `.potext` 插件
//! （已有 QuickJS 宿主），而不是在这里造一门小语言。

use crate::Translator;
use async_trait::async_trait;
use futures::StreamExt;
use oxidict_core::HasConfig as _;
use oxidict_core::schema::ConfigField;
use oxidict_core::{Error, Language, Result, TranslateRequest, TranslateResult};
use serde_json::{Value, json};
use std::sync::Arc;

/// 沙拉云中转的 chat/completions 端点。配置里的 `requestPath` 可覆盖。
const RELAY_ENDPOINT: &str = "https://api.saladict.com/api/chat/completions";
const DEFAULT_TEMP: f64 = 0.1;

const SYSTEM_PROMPT: &str = "You are a professional translation engine, please translate the text into a colloquial, professional, elegant and fluent content, without the style of machine translation. You must only translate the text content, never interpret it.";

/// 请求体形态。
#[derive(Clone, Copy)]
pub enum BodyShape {
    /// OpenAI 兼容：`system` + `user` 两条消息，user 里带 "Translate into X" 指令，
    /// 并附 `temperature`；`stream` 同时受配置开关与流式回调影响。
    OpenAiChat,
    /// DeepL 中转：单条**不含 `role`** 的消息，其 `content` 是 `{"text","sl","tl"}`
    /// 的 JSON 串（用语言的裸内部码）；无 `temperature`，`stream` 只看流式回调。
    DeeplRelay,
}

/// 表单字段形态。
#[derive(Clone, Copy)]
pub enum FieldShape {
    /// API Key + 模型下拉 + 流式开关 + 温度。
    OpenAiStandard,
    /// 无可编辑项（模型与提示词都由中转侧固定）。
    None,
}

/// 一个中转服务的**全部差异**。
#[derive(Clone, Copy)]
pub struct RelaySpec {
    /// 服务 id。会写进用户的 config.json，**不可随意改名**（见测试）。
    pub id: &'static str,
    pub default_model: &'static str,
    /// 模型下拉选项 `(值, 展示名)`；空表示不暴露模型选择。
    pub models: &'static [(&'static str, &'static str)],
    pub body: BodyShape,
    pub fields: FieldShape,
}

/// 全部中转服务。新增同类服务只需加一行。
pub static RELAYS: &[RelaySpec] = &[
    RelaySpec {
        id: "openai_cloud",
        default_model: "gpt-4.1-nano",
        models: &[
            ("gpt-4.1-nano", "gpt-4.1-nano"),
            ("gpt-5-mini", "gpt-5-mini"),
            ("gpt-5", "gpt-5"),
        ],
        body: BodyShape::OpenAiChat,
        fields: FieldShape::OpenAiStandard,
    },
    RelaySpec {
        id: "claude_cloud",
        default_model: "claude-4.5-haiku",
        models: &[
            ("claude-4.5-haiku", "claude-4.5-haiku"),
            ("claude-4.5-sonnet", "claude-4.5-sonnet"),
        ],
        body: BodyShape::OpenAiChat,
        fields: FieldShape::OpenAiStandard,
    },
    RelaySpec {
        id: "gemini_cloud",
        default_model: "gemini-2.5-flash",
        models: &[
            ("gemini-2.5-flash", "gemini-2.5-flash"),
            ("gemini-2.5-pro", "gemini-2.5-pro"),
        ],
        body: BodyShape::OpenAiChat,
        fields: FieldShape::OpenAiStandard,
    },
    RelaySpec {
        id: "chatglm_cloud",
        default_model: "glm-4.5-flash",
        models: &[("glm-4.5-flash", "glm-4.5-flash"), ("glm-4.6", "glm-4.6")],
        body: BodyShape::OpenAiChat,
        fields: FieldShape::OpenAiStandard,
    },
    RelaySpec {
        id: "deepseek_cloud",
        default_model: "deepseek-v4-flash",
        models: &[("deepseek-v4-flash", "deepseek-v4-flash")],
        body: BodyShape::OpenAiChat,
        fields: FieldShape::OpenAiStandard,
    },
    RelaySpec {
        id: "minimax_cloud",
        default_model: "minimax-m2",
        models: &[("minimax-m2", "minimax-m2")],
        body: BodyShape::OpenAiChat,
        fields: FieldShape::OpenAiStandard,
    },
    RelaySpec {
        id: "moonshot_cloud",
        default_model: "kimi-k2",
        models: &[("kimi-k2", "kimi-k2")],
        body: BodyShape::OpenAiChat,
        fields: FieldShape::OpenAiStandard,
    },
    RelaySpec {
        id: "tongyi_cloud",
        default_model: "qwen3-max",
        models: &[("qwen3-max", "qwen3-max")],
        body: BodyShape::OpenAiChat,
        fields: FieldShape::OpenAiStandard,
    },
    RelaySpec {
        id: "deepl_cloud",
        default_model: "deepL-pro",
        models: &[],
        body: BodyShape::DeeplRelay,
        fields: FieldShape::None,
    },
];

pub struct Relay(&'static RelaySpec);

/// 注册全部中继服务。
pub fn register(services: &crate::Services) {
    for spec in RELAYS {
        services.register_translator(Arc::new(Relay(spec)));
    }
}

/// 从一条 OpenAI 兼容的 chunk 里取出增量文本：先试 `delta.content`（流式），
/// 再试 `message.content`（非流式）。
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
impl Translator for Relay {
    fn id(&self) -> &str {
        self.0.id
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        match self.0.fields {
            FieldShape::None => {
                // 原 Config.jsx 除 instance_name 外无可编辑项：model 与提示词固定。
                vec![]
            }
            FieldShape::OpenAiStandard => vec![
                ConfigField::secret("apiKey", "API Key"),
                ConfigField::Select {
                    key: "model",
                    label: "模型",
                    options: self.0.models,
                    default: self.0.default_model,
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
            ],
        }
    }

    /// 9 个服务的映射表逐字相同，且与自定义服务共用一份（见 `lang_names.rs`）。
    fn map_language(&self, lang: Language) -> String {
        super::lang_names::english_name(lang)
    }

    async fn translate(&self, req: TranslateRequest) -> Result<TranslateResult> {
        let url = req
            .str("requestPath")
            .filter(|s| !s.is_empty())
            .unwrap_or(RELAY_ENDPOINT)
            .to_string();
        let api_key = req.str("apiKey").unwrap_or("").to_string();
        let model = req
            .str("model")
            .filter(|s| !s.is_empty())
            .unwrap_or(self.0.default_model)
            .to_string();

        let (body, stream) = match self.0.body {
            BodyShape::OpenAiChat => {
                let temperature = req
                    .config
                    .get("temperature")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(DEFAULT_TEMP);
                let to_lang = self.map_language(req.to);
                let user_content =
                    format!("Translate into {}:\n\"\"\"\n{}\n\"\"\"", to_lang, req.text);
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
                (body, stream)
            }
            BodyShape::DeeplRelay => {
                // 裸内部码作为 sl/tl，文本经 JSON 转义后塞进 content。
                let payload = serde_json::to_string(&json!({
                    "text": req.text,
                    "sl": req.from.code(),
                    "tl": req.to.code(),
                }))?;
                let messages = json!([{ "content": payload }]);
                let stream = req.on_stream.is_some();
                (
                    json!({ "model": model, "messages": messages, "stream": stream }),
                    stream,
                )
            }
        };

        let auth = format!("Bearer {}", api_key);
        let headers = [
            ("Content-Type", "application/json"),
            ("Authorization", auth.as_str()),
        ];

        let rb = oxidict_net::post_with_headers(url, &headers).json(&body);
        let resp = rb.send().await.map_err(|e| Error::Network(e.to_string()))?;
        let resp = oxidict_net::check(resp).await?;

        // 流式：边解析 SSE 增量边推送。
        if stream && let Some(sink) = &req.on_stream {
            let mut full = String::new();
            let mut s = oxidict_net::sse_text(resp);
            while let Some(line) = s.next().await {
                let line = line?;
                if let Some(data) = oxidict_net::sse_payload(&line)
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
            .ok_or_else(|| Error::Service(format!("{}: 无翻译结果: {}", self.0.id, text)))?;
        Ok(TranslateResult::Plain(content.trim().to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 服务 id 会写进用户的 `config.json`，改名等于让已启用该服务的用户
    /// **静默丢失配置**（列表里还留着 id、注册表里却找不到）。这里把已发布的
    /// id 集合钉住，改名必须是有意识的决定。
    #[test]
    fn service_ids_are_pinned() {
        let expected = [
            "chatglm_cloud",
            "claude_cloud",
            "deepl_cloud",
            "deepseek_cloud",
            "gemini_cloud",
            "minimax_cloud",
            "moonshot_cloud",
            "openai_cloud",
            "tongyi_cloud",
        ];
        let mut ids: Vec<&str> = RELAYS.iter().map(|s| s.id).collect();
        ids.sort_unstable();
        assert_eq!(ids, expected, "中转服务的 id 集合发生了变化");
    }

    /// id 必须唯一：重复会让后注册的覆盖前者，用户启用哪个都指向同一个实现。
    #[test]
    fn service_ids_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for spec in RELAYS {
            assert!(seen.insert(spec.id), "重复的服务 id: {}", spec.id);
        }
    }

    /// 默认模型必须在下拉选项里（否则表单回显会选中一个不存在的项）；
    /// 不暴露模型选择的服务（models 为空）不受此约束。
    #[test]
    fn default_model_is_selectable() {
        for spec in RELAYS {
            if spec.models.is_empty() {
                continue;
            }
            assert!(
                spec.models.iter().any(|(v, _)| *v == spec.default_model),
                "{} 的默认模型 {} 不在 models 列表里",
                spec.id,
                spec.default_model
            );
        }
    }
}
