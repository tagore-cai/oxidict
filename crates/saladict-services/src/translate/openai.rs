//! OpenAI 翻译。
//!
//! 移植自 `src/services/translate/openai/index.jsx`，基于 openai_compatible 共享层。
//! `service` 为 `openai` 时自动补 `/v1/chat/completions` 并用 Bearer 鉴权；
//! 为 `azure` 类时使用 `api-key` 头。

use super::openai_compatible as compat;
use crate::Translator;
use async_trait::async_trait;
use saladict_core::schema::ConfigField;
use saladict_core::HasConfig as _;
use saladict_core::{Result, TranslateRequest, TranslateResult};

pub struct Openai;

const DEFAULT_ARGUMENTS: &str = r#"{"temperature": 0}"#;

#[async_trait]
impl Translator for Openai {
    fn id(&self) -> &str {
        "openai"
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        vec![
            ConfigField::Select {
                key: "service",
                label: "服务类型",
                options: &[("openai", "OpenAI 兼容"), ("azure", "Azure OpenAI")],
                default: "openai",
            },
            ConfigField::Text {
                key: "requestPath",
                label: "请求地址",
                placeholder: "https://api.openai.com",
                secret: false,
                required: true,
            },
            ConfigField::Text {
                key: "model",
                label: "模型",
                placeholder: "gpt-4o-mini",
                secret: false,
                required: true,
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
            ConfigField::Text {
                key: "requestArguments",
                label: "额外请求参数 (JSON)",
                placeholder: DEFAULT_ARGUMENTS,
                secret: false,
                required: false,
            },
        ]
    }

    async fn translate(&self, req: TranslateRequest) -> Result<TranslateResult> {
        let service = req.str("service").unwrap_or("openai").to_string();
        let request_path = req.require_str("requestPath", "OpenAI 请求地址")?;
        let model = req.str("model").unwrap_or("gpt-4o-mini").to_string();
        let api_key = req.require_str("apiKey", "OpenAI API Key")?;
        let stream = req.bool("stream");

        // openai like api 不强制 /v1，只有官方 openai 服务才补全端点。
        let force_completions = service == "openai";
        let url = compat::normalize_url(&request_path, force_completions);

        let headers = if service == "openai" {
            vec![
                ("Content-Type", "application/json".to_string()),
                ("Authorization", format!("Bearer {}", api_key)),
            ]
        } else {
            vec![
                ("Content-Type", "application/json".to_string()),
                ("api-key", api_key.clone()),
            ]
        };

        let messages = compat::build_messages(&req, compat::prompts_from(&req));
        let extra: serde_json::Value = req
            .str("requestArguments")
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_else(|| serde_json::from_str(DEFAULT_ARGUMENTS).unwrap());
        let body = compat::build_body(&model, stream, &messages, extra);

        let headers: Vec<(&str, &str)> = headers.iter().map(|(k, v)| (*k, v.as_str())).collect();

        compat::chat_completions(&url, &headers, body, stream, req.on_stream.as_ref()).await
    }
}
