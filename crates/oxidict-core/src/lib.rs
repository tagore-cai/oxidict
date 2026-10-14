//! oxidict-core：与 UI 框架无关的领域内核。
//!
//! 包含统一语言码表、配置存储、历史库、领域模型与错误类型。
//! 这一层不依赖 GPUI，也不依赖 Tauri，可独立测试。

use serde::{Deserialize, Serialize};

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

/// HTTP 客户端指纹档位（写在服务实例配置的 `tls` 字段里）。
///
/// 默认 `Native`：不模拟浏览器，调官方 API 时足够。**网页端点**常被 WAF 按
/// TLS（JA3/JA4）与 HTTP/2 特征判定，默认的非浏览器 ClientHello 会吃到 403
/// 或连接重置，这时需要模拟真实浏览器。
///
/// 为什么放在 core 而不是网络层：请求结构体要携带它，而 `oxidict-net` 依赖
/// core（反向不成立），放网络层会造成循环依赖或要求每个调用点自己传。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TlsProfile {
    /// 原生指纹（不模拟浏览器）。
    #[default]
    Native,
    /// 模拟 Chrome。
    Chrome,
    /// 模拟 Safari。
    Safari,
}

impl TlsProfile {
    /// 从配置值解析。**未知值一律回退 `Native`**：宁可指纹不匹配，
    /// 也不要因为一个拼写错误就让所有请求"假装是 Safari"——那会把排查方向带偏。
    pub fn from_code(code: &str) -> Self {
        match code.trim().to_ascii_lowercase().as_str() {
            "chrome" => Self::Chrome,
            "safari" => Self::Safari,
            _ => Self::Native,
        }
    }

    /// 写回配置用的取值。
    pub fn code(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Chrome => "chrome",
            Self::Safari => "safari",
        }
    }

    /// 是否模拟浏览器指纹。
    pub fn is_emulated(self) -> bool {
        !matches!(self, Self::Native)
    }
}

#[cfg(test)]
mod tests {
    // 测试中 unwrap 直观且失败即测试失败，豁免。
    #![allow(clippy::unwrap_used)]

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

    /// 指纹档位的解析必须**保守回退**：不认识的值（含空串、拼写错误、
    /// 手改配置留下的历史值）一律当作「不模拟」。
    #[test]
    fn tls_profile_falls_back_to_native() {
        assert_eq!(TlsProfile::from_code("chrome"), TlsProfile::Chrome);
        assert_eq!(TlsProfile::from_code("  Chrome "), TlsProfile::Chrome);
        assert_eq!(TlsProfile::from_code("SAFARI"), TlsProfile::Safari);
        for unknown in ["", "native", "firefox", "Chrome131", "true", "1"] {
            assert_eq!(
                TlsProfile::from_code(unknown),
                TlsProfile::Native,
                "{unknown} 应回退 Native"
            );
        }
    }

    /// 取值与 serde 表示必须一致（配置里存的就是 `code()` 的字符串）。
    #[test]
    fn tls_profile_code_round_trips_through_json() {
        for profile in [TlsProfile::Native, TlsProfile::Chrome, TlsProfile::Safari] {
            let text = serde_json::to_string(&profile).unwrap();
            assert_eq!(
                text,
                format!("\"{}\"", profile.code()),
                "serde 表示与 code() 不一致"
            );
            let back: TlsProfile = serde_json::from_str(&text).unwrap();
            assert_eq!(back, profile);
        }
        assert_eq!(TlsProfile::default(), TlsProfile::Native);
        assert!(!TlsProfile::Native.is_emulated());
        assert!(TlsProfile::Chrome.is_emulated());
    }
}
