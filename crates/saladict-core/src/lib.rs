//! saladict-core：与 UI 框架无关的领域内核。
//!
//! 包含统一语言码表、配置存储、历史库、领域模型与错误类型。
//! 这一层不依赖 GPUI，也不依赖 Tauri，可独立测试。

pub mod backup;
pub mod i18n;
pub mod config;
pub mod error;
pub mod history;
pub mod language;
pub mod model;
pub mod runtime;
pub mod schema;

pub use error::{Error, Result};
pub use language::Language;
pub use model::{
    HasConfig,
    CollectionRequest, DictResult, Explanation, HistoryEntry, Pronunciation, RecognizeRequest,
    Sentence, ServiceConfig, StreamSink, TranslateRequest, TranslateResult, TtsRequest,
};

/// 服务实例的元信息，供注册表与设置面板使用。
#[derive(Debug, Clone)]
pub struct ServiceMeta {
    /// 服务唯一标识，例如 `google`、`volcengine_ocr`。
    pub id: &'static str,
    /// 展示名，走 i18n key。
    pub name: &'static str,
    /// 是否需要 API Key 等配置才能使用。
    pub needs_config: bool,
}

/// 四大类服务。config.json 里的 `*_service_list` 直接映射到这里。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServiceKind {
    Translate,
    Recognize,
    Tts,
    Collection,
}

impl ServiceKind {
    pub fn list_key(self) -> &'static str {
        match self {
            ServiceKind::Translate => config::keys::TRANSLATE_SERVICE_LIST,
            ServiceKind::Recognize => config::keys::RECOGNIZE_SERVICE_LIST,
            ServiceKind::Tts => config::keys::TTS_SERVICE_LIST,
            ServiceKind::Collection => config::keys::COLLECTION_SERVICE_LIST,
        }
    }

    /// 插件目录名，与老版本 `plugins/<type>/<name>` 保持一致。
    pub fn dir_name(self) -> &'static str {
        match self {
            ServiceKind::Translate => "translate",
            ServiceKind::Recognize => "recognize",
            ServiceKind::Tts => "tts",
            ServiceKind::Collection => "collection",
        }
    }
}
