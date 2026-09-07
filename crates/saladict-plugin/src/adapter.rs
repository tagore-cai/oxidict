//! 把 gpui-shell 插件适配成四大类服务 trait。
//!
//! 注册表里内置服务和脚本插件因此完全同构：调用方拿到的是
//! `Arc<dyn Translator>`，不需要知道背后是 Rust 还是 JavaScript。

use crate::bridge::{bridge_for, config_to_host_value, host_to_json, next_job_id, Job};
use crate::shim::PluginKind;
use saladict_core::{Error, Result};
use async_trait::async_trait;
use gpui_shell::HostValue;
use saladict_core::{TranslateRequest, TranslateResult};
use saladict_services::{Collector, Recognizer, Translator, Tts};
use std::time::Duration;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// 脚本插件的公共部分：插件 id + 注册到服务表的 id + 类型。
#[derive(Clone)]
pub struct ShellService {
    pub plugin_id: String,
    pub service_id: String,
    pub kind: PluginKind,
}

impl ShellService {
    pub fn new(plugin_id: impl Into<String>, kind: PluginKind) -> Self {
        let plugin_id = plugin_id.into();
        Self {
            service_id: plugin_id.clone(),
            plugin_id,
            kind,
        }
    }

    async fn call(
        &self,
        text: String,
        from: &str,
        to: &str,
        config: &serde_json::Map<String, serde_json::Value>,
        image: Option<String>,
    ) -> Result<HostValue> {
        let job = Job {
            id: next_job_id(),
            kind: self.kind.as_str(),
            text,
            from: from.to_string(),
            to: to.to_string(),
            config: config_to_host_value(config),
            image,
        };
        Ok(bridge_for(&self.plugin_id).call(job, DEFAULT_TIMEOUT).await?)
    }
}

#[async_trait]
impl Translator for ShellService {
    fn id(&self) -> &str {
        &self.service_id
    }

    async fn translate(&self, req: TranslateRequest) -> Result<TranslateResult> {
        let from = req.from.code().to_string();
        let to = req.to.code().to_string();
        let value = self
            .call(req.text.clone(), &from, &to, &req.config, None)
            .await?;

        // 字符串是直接译文；对象按词典结构解释。
        match value {
            HostValue::Str(s) => Ok(TranslateResult::Plain(s)),
            HostValue::Null => Ok(TranslateResult::Plain(String::new())),
            other => {
                let json = host_to_json(&other);
                if let Some(s) = json.as_str() {
                    return Ok(TranslateResult::Plain(s.to_string()));
                }
                let dict = saladict_core::model::DictResult {
                    pronunciations: json
                        .get("pronunciations")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or_default(),
                    explanations: json
                        .get("explanations")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or_default(),
                    associations: json
                        .get("associations")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or_default(),
                    sentence: json
                        .get("sentence")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or_default(),
                };
                Ok(TranslateResult::Dict(dict))
            }
        }
    }
}

#[async_trait]
impl Recognizer for ShellService {
    fn id(&self) -> &str {
        &self.service_id
    }

    async fn recognize(&self, req: saladict_core::RecognizeRequest) -> Result<String> {
        use base64::Engine;
        let image = base64::engine::general_purpose::STANDARD.encode(&req.image);
        let lang = req.language.code().to_string();
        let value = self
            .call(String::new(), &lang, &lang, &req.config, Some(image))
            .await?;
        match value {
            HostValue::Str(s) => Ok(s),
            other => Ok(host_to_json(&other)
                .as_str()
                .unwrap_or_default()
                .to_string()),
        }
    }
}

#[async_trait]
impl Tts for ShellService {
    fn id(&self) -> &str {
        &self.service_id
    }

    async fn tts(&self, req: saladict_core::TtsRequest) -> Result<Vec<u8>> {
        let lang = req.language.code().to_string();
        let value = self
            .call(req.text.clone(), &lang, &lang, &req.config, None)
            .await?;
        // 插件可以返回 base64 字符串或字节数组。
        match value {
            HostValue::Str(s) => {
                use base64::Engine;
                base64::engine::general_purpose::STANDARD
                    .decode(s)
                    .map_err(|e| Error::Plugin(format!("插件返回的音频不是合法 base64: {}", e)))
            }
            HostValue::Array(items) => Ok(items
                .iter()
                .filter_map(|v| match v {
                    HostValue::Number(n) => Some(*n as u8),
                    _ => None,
                })
                .collect()),
            other => Err(Error::Plugin(format!(
                "插件返回的音频类型不支持: {:?}",
                other
            ))),
        }
    }
}

#[async_trait]
impl Collector for ShellService {
    fn id(&self) -> &str {
        &self.service_id
    }

    async fn collect(&self, req: saladict_core::CollectionRequest) -> Result<()> {
        self.call(
            req.source.clone(),
            "auto",
            "auto",
            &req.config,
            None,
        )
        .await?;
        Ok(())
    }
}
