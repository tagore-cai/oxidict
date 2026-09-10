//! saladict-core：与 UI 框架无关的领域内核。
//!
//! 包含统一语言码表、配置存储、历史库、领域模型与错误类型。
//! 这一层不依赖 GPUI，也不依赖 Tauri，可独立测试。

pub mod backup;
pub mod config;
pub mod error;
pub mod history;
pub mod i18n;
pub mod language;
pub mod model;
pub mod runtime;
pub mod schema;

pub use error::{Error, Result};
pub use language::Language;
pub use model::{
    CollectionRequest, DictResult, Explanation, HasConfig, HistoryEntry, Pronunciation,
    RecognizeRequest, Sentence, ServiceConfig, StreamSink, TranslateRequest, TranslateResult,
    TtsRequest,
};

/// 四大类服务。config.json 里的 `*_service_list` 直接映射到这里。
///
/// 这是全项目唯一的「服务种类」枚举：服务注册表、设置面板、`.potext` 插件
/// 目录都用它。插件层不再另立一套平行的 kind 枚举。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServiceKind {
    Translate,
    Recognize,
    Tts,
    Collection,
}

impl ServiceKind {
    /// 规范名：`translate` / `recognize` / `tts` / `collection`。
    ///
    /// 同时用于插件目录名与 `.potext` 插件必须导出的入口函数名（老插件的
    /// `main.js` 里 `function translate(...)` 等），二者在历史上一直同名。
    pub fn as_str(self) -> &'static str {
        match self {
            ServiceKind::Translate => "translate",
            ServiceKind::Recognize => "recognize",
            ServiceKind::Tts => "tts",
            ServiceKind::Collection => "collection",
        }
    }

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
        self.as_str()
    }

    /// 全部种类，供插件加载等需要遍历的场景使用。
    pub const ALL: [ServiceKind; 4] = [
        ServiceKind::Translate,
        ServiceKind::Recognize,
        ServiceKind::Tts,
        ServiceKind::Collection,
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_names_match_plugin_dirs() {
        // `as_str` 同时是插件目录名与 .potext 入口函数名，三者必须与老版本一致，
        // 否则已安装的插件会找不到目录或导出函数。
        assert_eq!(ServiceKind::Translate.as_str(), "translate");
        assert_eq!(ServiceKind::Recognize.as_str(), "recognize");
        assert_eq!(ServiceKind::Tts.as_str(), "tts");
        assert_eq!(ServiceKind::Collection.as_str(), "collection");
        for kind in ServiceKind::ALL {
            assert_eq!(kind.dir_name(), kind.as_str());
        }
    }

    #[test]
    fn kind_list_keys_are_distinct() {
        // 四类服务的启用列表是 config.json 里四个不同的键，重复会让
        // 「启用某个翻译服务」意外覆盖另一类的列表。
        let mut seen = std::collections::HashSet::new();
        for kind in ServiceKind::ALL {
            assert!(
                seen.insert(kind.list_key()),
                "重复的 list_key: {}",
                kind.as_str()
            );
        }
        assert_eq!(seen.len(), ServiceKind::ALL.len());
    }

    #[test]
    fn parse_instance_splits_name_and_id() {
        assert_eq!(config::parse_instance("google"), ("google", None));
        assert_eq!(
            config::parse_instance("alibaba@r4nd0m"),
            ("alibaba", Some("r4nd0m"))
        );
        // 多个 @ 时按第一个切分，后缀原样保留。
        assert_eq!(config::parse_instance("a@b@c"), ("a", Some("b@c")));
    }
}
