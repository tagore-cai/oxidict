//! 把 JS 插件适配成四大类服务 trait。
//!
//! 注册表里内置服务和脚本插件完全同构：调用方拿到的是
//! `Arc<dyn Translator>`，不需要知道背后是 Rust 还是 JavaScript。

use crate::runtime::{self, CallRequest};
use async_trait::async_trait;
use base64::Engine;
use oxidict_core::{Error, Result, ServiceKind, TranslateRequest, TranslateResult};
use oxidict_services::{Collector, Recognizer, Translator, Tts};
use serde_json::Value as Json;

/// 脚本插件的公共部分：插件 id + 注册到服务表的 id + 类型。
#[derive(Clone)]
pub struct PluginService {
    pub plugin_id: String,
    pub service_id: String,
    pub kind: ServiceKind,
}

impl PluginService {
    pub fn new(plugin_id: impl Into<String>, kind: ServiceKind) -> Self {
        let plugin_id = plugin_id.into();
        Self {
            service_id: plugin_id.clone(),
            plugin_id,
            kind,
        }
    }

    /// 调用插件入口函数。输入对 recognize 是 base64 图片，其余是文本。
    async fn call(
        &self,
        input: String,
        from: &str,
        to: &str,
        config: &serde_json::Map<String, Json>,
    ) -> Result<Json> {
        runtime::call(CallRequest {
            plugin_id: self.plugin_id.clone(),
            func: self.kind.as_str(),
            input,
            from: from.to_string(),
            to: to.to_string(),
            config: config.clone(),
        })
        .await
        .map_err(oxidict_core::Error::from)
    }
}

#[async_trait]
impl Translator for PluginService {
    fn id(&self) -> &str {
        &self.service_id
    }

    async fn translate(&self, req: TranslateRequest) -> Result<TranslateResult> {
        let from = req.from.code().to_string();
        let to = req.to.code().to_string();
        let value = self.call(req.text.clone(), &from, &to, &req.config).await?;

        // 字符串是直接译文；对象按词典结构解释。
        match value {
            Json::String(s) => Ok(TranslateResult::Plain(s)),
            Json::Null => Ok(TranslateResult::Plain(String::new())),
            other => {
                if let Some(s) = other.as_str() {
                    return Ok(TranslateResult::Plain(s.to_string()));
                }
                let dict = oxidict_core::model::DictResult {
                    pronunciations: other
                        .get("pronunciations")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or_default(),
                    explanations: other
                        .get("explanations")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or_default(),
                    associations: other
                        .get("associations")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or_default(),
                    sentence: other
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
impl Recognizer for PluginService {
    fn id(&self) -> &str {
        &self.service_id
    }

    async fn recognize(&self, req: oxidict_core::RecognizeRequest) -> Result<String> {
        let image = base64::engine::general_purpose::STANDARD.encode(&req.image);
        let lang = req.language.code().to_string();
        let value = self.call(image, &lang, &lang, &req.config).await?;
        Ok(value.as_str().unwrap_or_default().to_string())
    }
}

#[async_trait]
impl Tts for PluginService {
    fn id(&self) -> &str {
        &self.service_id
    }

    async fn tts(&self, req: oxidict_core::TtsRequest) -> Result<Vec<u8>> {
        let lang = req.language.code().to_string();
        let value = self
            .call(req.text.clone(), &lang, &lang, &req.config)
            .await?;
        // 插件可以返回 base64 字符串或字节数组。
        match value {
            Json::String(s) => base64::engine::general_purpose::STANDARD
                .decode(s)
                .map_err(|e| Error::Plugin(format!("插件返回的音频不是合法 base64: {e}"))),
            Json::Array(items) => Ok(items
                .iter()
                .filter_map(|v| v.as_u64().map(|n| n as u8))
                .collect()),
            other => Err(Error::Plugin(format!("插件返回的音频类型不支持: {other}"))),
        }
    }
}

#[async_trait]
impl Collector for PluginService {
    fn id(&self) -> &str {
        &self.service_id
    }

    async fn collect(&self, req: oxidict_core::CollectionRequest) -> Result<()> {
        self.call(req.source.clone(), "auto", "auto", &req.config)
            .await?;
        Ok(())
    }
}
