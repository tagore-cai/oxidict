//! 领域模型。
//!
//! 对应原 JS 侧翻译结果的两种形态：纯译文字符串 与 结构化词典结果。
//! GPUI 视图层根据 `TranslateResult` 的变体决定渲染 `Textarea` 还是词典卡片。

use crate::language::Language;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// 发音。语音数据以字节保存，供朗读按钮直接播放。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Pronunciation {
    pub symbol: Option<String>,
    #[serde(skip)]
    pub voice: Option<Arc<Vec<u8>>>,
}

/// 词典释义条目。`trait_` 是词性（n. / v. 等），`explains` 是释义列表。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Explanation {
    #[serde(rename = "trait")]
    pub trait_: Option<String>,
    pub explains: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Sentence {
    pub source: Option<String>,
    pub target: Option<String>,
}

/// 结构化词典结果（Google / Bing 词典 / 有道 / 剑桥等）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DictResult {
    pub pronunciations: Vec<Pronunciation>,
    pub explanations: Vec<Explanation>,
    pub associations: Vec<String>,
    pub sentence: Vec<Sentence>,
}

impl DictResult {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.pronunciations.is_empty()
            && self.explanations.is_empty()
            && self.associations.is_empty()
            && self.sentence.is_empty()
    }
}

/// 翻译结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TranslateResult {
    Plain(String),
    Dict(DictResult),
}

impl TranslateResult {
    /// 展平成纯文本，用于复制、朗读、写入历史库。
    pub fn as_text(&self) -> String {
        match self {
            TranslateResult::Plain(s) => s.clone(),
            TranslateResult::Dict(d) => {
                let mut out = String::new();
                for p in &d.pronunciations {
                    if let Some(sym) = &p.symbol {
                        out.push_str(sym);
                        out.push('\n');
                    }
                }
                for e in &d.explanations {
                    if let Some(t) = &e.trait_ {
                        out.push_str(t);
                        out.push(' ');
                    }
                    out.push_str(&e.explains.join("; "));
                    out.push('\n');
                }
                out.trim().to_string()
            }
        }
    }
}

impl From<String> for TranslateResult {
    fn from(s: String) -> Self {
        TranslateResult::Plain(s)
    }
}

impl From<&str> for TranslateResult {
    fn from(s: &str) -> Self {
        TranslateResult::Plain(s.to_string())
    }
}

impl From<DictResult> for TranslateResult {
    fn from(d: DictResult) -> Self {
        TranslateResult::Dict(d)
    }
}

/// 流式增量回调。LLM 类服务边生成边推送，视图层追加渲染。
pub type StreamSink = Arc<dyn Fn(String) + Send + Sync>;

/// 服务实例配置。原 JS 里就是 `options.config`，值来自 config.json 中该实例的对象。
pub type ServiceConfig = serde_json::Map<String, serde_json::Value>;

/// 四类请求共有的配置访问逻辑。
///
/// 原实现里所有服务都从 `options.config` 里取 key，这里统一成方法，
/// 服务代码不再直接触碰 `serde_json::Map`。
pub trait HasConfig {
    fn config(&self) -> &ServiceConfig;

    fn str(&self, key: &str) -> Option<&str> {
        self.config().get(key).and_then(|v| v.as_str())
    }

    fn require_str(&self, key: &str, hint: &str) -> crate::error::Result<String> {
        match self.str(key) {
            Some(s) if !s.trim().is_empty() => Ok(s.to_string()),
            _ => Err(crate::error::Error::MissingConfig(format!("{} ({})", hint, key))),
        }
    }

    fn bool(&self, key: &str) -> bool {
        self.config().get(key).and_then(|v| v.as_bool()).unwrap_or(false)
    }

    fn number(&self, key: &str) -> Option<f64> {
        self.config().get(key).and_then(|v| v.as_f64())
    }
}


/// 翻译调用参数。
#[derive(Clone)]
pub struct TranslateRequest {
    pub text: String,
    pub from: Language,
    pub to: Language,
    /// 语言检测给出的实际源语言，部分服务（Google、百度）用它替代 `from`。
    pub detect: Option<Language>,
    pub config: ServiceConfig,
    pub on_stream: Option<StreamSink>,
}

impl TranslateRequest {
    pub fn new(text: impl Into<String>, from: Language, to: Language) -> Self {
        Self {
            text: text.into(),
            from,
            to,
            detect: None,
            config: ServiceConfig::new(),
            on_stream: None,
        }
    }

    pub fn with_config(mut self, config: ServiceConfig) -> Self {
        self.config = config;
        self
    }

    pub fn with_detect(mut self, detect: Option<Language>) -> Self {
        self.detect = detect;
        self
    }

    pub fn with_stream(mut self, sink: StreamSink) -> Self {
        self.on_stream = Some(sink);
        self
    }

}

impl HasConfig for TranslateRequest {
    fn config(&self) -> &ServiceConfig {
        &self.config
    }
}

/// OCR 识别调用参数。
#[derive(Clone)]
pub struct RecognizeRequest {
    pub image: Vec<u8>,
    pub language: Language,
    pub config: ServiceConfig,
}

impl HasConfig for RecognizeRequest {
    fn config(&self) -> &ServiceConfig {
        &self.config
    }
}

/// TTS 调用参数。
#[derive(Clone)]
pub struct TtsRequest {
    pub text: String,
    pub language: Language,
    pub config: ServiceConfig,
}

impl HasConfig for TtsRequest {
    fn config(&self) -> &ServiceConfig {
        &self.config
    }
}

/// 生词本（收藏）调用参数。
#[derive(Clone)]
pub struct CollectionRequest {
    pub source: String,
    pub target: String,
    pub config: ServiceConfig,
}

impl HasConfig for CollectionRequest {
    fn config(&self) -> &ServiceConfig {
        &self.config
    }
}

/// 历史记录条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: i64,
    pub text: String,
    pub source: String,
    pub target: String,
    pub service: String,
    pub result: String,
    pub timestamp: i64,
}
