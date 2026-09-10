//! 统一语言码表。
//!
//! 原 JS 实现里语言映射散落在 5 处：`utils/language.ts` 的内部码、
//! 每个服务 `info.ts` 的 `Language` 枚举、`recognize/system` 的三套平台码、
//! `utils/lang_detect.js` 的厂商码、以及 Rust 侧 `lang_detect.rs` 的 lingua 映射。
//! 这里收敛为单一内部码枚举，各服务只需声明「内部码 -> 厂商码」的偏差项。

use serde::{Deserialize, Serialize};
use std::fmt;

/// 内部语言码。与 saladict v4 `languageList` 一一对应，保证既有 config.json 兼容。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    #[default]
    Auto,
    ZhCn,
    ZhTw,
    En,
    Ja,
    Ko,
    Fr,
    Es,
    Ru,
    De,
    It,
    Tr,
    PtPt,
    PtBr,
    Vi,
    Id,
    Th,
    Ms,
    Ar,
    Hi,
    Km,
    MnCy,
    NbNo,
    NnNo,
    Fa,
    Sv,
    Pl,
    Nl,
    Uk,
    He,
    MnMo,
}

impl Language {
    /// 内部码字符串，写入 `config.json` 与跨服务传递时使用。
    pub fn code(self) -> &'static str {
        match self {
            Language::Auto => "auto",
            Language::ZhCn => "zh_cn",
            Language::ZhTw => "zh_tw",
            Language::En => "en",
            Language::Ja => "ja",
            Language::Ko => "ko",
            Language::Fr => "fr",
            Language::Es => "es",
            Language::Ru => "ru",
            Language::De => "de",
            Language::It => "it",
            Language::Tr => "tr",
            Language::PtPt => "pt_pt",
            Language::PtBr => "pt_br",
            Language::Vi => "vi",
            Language::Id => "id",
            Language::Th => "th",
            Language::Ms => "ms",
            Language::Ar => "ar",
            Language::Hi => "hi",
            Language::Km => "km",
            Language::MnCy => "mn_cy",
            Language::NbNo => "nb_no",
            Language::NnNo => "nn_no",
            Language::Fa => "fa",
            Language::Sv => "sv",
            Language::Pl => "pl",
            Language::Nl => "nl",
            Language::Uk => "uk",
            Language::He => "he",
            Language::MnMo => "mn_mo",
        }
    }

    pub fn from_code(code: &str) -> Option<Language> {
        Some(match code {
            "auto" => Language::Auto,
            "zh_cn" | "zh-cn" | "zh" => Language::ZhCn,
            "zh_tw" | "zh-tw" => Language::ZhTw,
            "en" | "en-us" | "en-gb" => Language::En,
            "ja" | "ja-jp" => Language::Ja,
            "ko" => Language::Ko,
            "fr" => Language::Fr,
            "es" => Language::Es,
            "ru" => Language::Ru,
            "de" => Language::De,
            "it" => Language::It,
            "tr" => Language::Tr,
            "pt_pt" => Language::PtPt,
            "pt_br" => Language::PtBr,
            "vi" => Language::Vi,
            "id" => Language::Id,
            "th" => Language::Th,
            "ms" => Language::Ms,
            "ar" => Language::Ar,
            "hi" => Language::Hi,
            "km" => Language::Km,
            "mn_cy" => Language::MnCy,
            "nb_no" => Language::NbNo,
            "nn_no" => Language::NnNo,
            "fa" => Language::Fa,
            "sv" => Language::Sv,
            "pl" => Language::Pl,
            "nl" => Language::Nl,
            "uk" => Language::Uk,
            "he" => Language::He,
            "mn_mo" => Language::MnMo,
            _ => return None,
        })
    }

    /// 全部可选语言（不含 `Auto`），用于设置面板的下拉。
    pub fn all() -> &'static [Language] {
        &[
            Language::Auto,
            Language::ZhCn,
            Language::ZhTw,
            Language::En,
            Language::Ja,
            Language::Ko,
            Language::Fr,
            Language::Es,
            Language::Ru,
            Language::De,
            Language::It,
            Language::Tr,
            Language::PtPt,
            Language::PtBr,
            Language::Vi,
            Language::Id,
            Language::Th,
            Language::Ms,
            Language::Ar,
            Language::Hi,
            Language::Km,
            Language::MnCy,
            Language::NbNo,
            Language::NnNo,
            Language::Fa,
            Language::Sv,
            Language::Pl,
            Language::Nl,
            Language::Uk,
            Language::He,
            Language::MnMo,
        ]
    }

    /// 给 LLM 类服务构造 prompt 用的英文名。
    pub fn english_name(self) -> &'static str {
        match self {
            Language::Auto => "the source language",
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
            Language::PtBr => "Portuguese (Brazil)",
            Language::Vi => "Vietnamese",
            Language::Id => "Indonesian",
            Language::Th => "Thai",
            Language::Ms => "Malay",
            Language::Ar => "Arabic",
            Language::Hi => "Hindi",
            Language::Km => "Khmer",
            Language::MnCy => "Mongolian",
            Language::NbNo => "Norwegian Bokmål",
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

    /// 国旗图标代号，对应原 `LanguageFlag`。
    pub fn flag(self) -> &'static str {
        match self {
            Language::ZhCn | Language::ZhTw | Language::MnMo => "cn",
            Language::En => "gb",
            Language::Ja => "jp",
            Language::Ko => "kr",
            Language::PtPt => "pt",
            Language::PtBr => "br",
            Language::Vi => "vn",
            Language::Ar => "ae",
            Language::Hi => "in",
            Language::Km => "kh",
            Language::MnCy => "mn",
            Language::NbNo | Language::NnNo => "no",
            Language::Fa => "ir",
            Language::Sv => "se",
            Language::Uk => "ua",
            Language::He => "il",
            Language::Auto => "auto",
            other => other.code(),
        }
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

/// 把一个「内部码 -> 厂商码」的偏差表编译成匹配分支。
///
/// 未列出的语言回退为内部码本身，这样大多数服务只需声明少数几项。
///
/// ```ignore
/// fn map(&self, lang: Language) -> String {
///     map_language!(lang, {
///         ZhCn => "zh-Hans",
///         ZhTw => "zh-Hant",
///     })
/// }
/// ```
#[macro_export]
macro_rules! map_language {
    ($lang:expr, { $($variant:ident => $code:expr),* $(,)? }) => {
        match $lang {
            $( $crate::language::Language::$variant => ::std::string::ToString::to_string($code), )*
            other => other.code().to_string(),
        }
    };
}

impl Language {
    /// 全部语言，UI 语言选择器的数据源（顺序与原 languageList 一致）。
    pub const ALL: &'static [Language] = &[
        Language::Auto,
        Language::ZhCn,
        Language::ZhTw,
        Language::En,
        Language::Ja,
        Language::Ko,
        Language::Fr,
        Language::Es,
        Language::Ru,
        Language::De,
        Language::It,
        Language::Tr,
        Language::PtPt,
        Language::PtBr,
        Language::Vi,
        Language::Id,
        Language::Th,
        Language::Ms,
        Language::Ar,
        Language::Hi,
        Language::Km,
        Language::MnCy,
        Language::NbNo,
        Language::NnNo,
        Language::Fa,
        Language::Sv,
        Language::Pl,
        Language::Nl,
        Language::Uk,
        Language::He,
        Language::MnMo,
    ];

    /// UI 显示名（简体中文界面）。
    pub fn display_name(self) -> &'static str {
        match self {
            Language::Auto => "自动检测",
            Language::ZhCn => "简体中文",
            Language::ZhTw => "繁体中文",
            Language::En => "英语",
            Language::Ja => "日语",
            Language::Ko => "韩语",
            Language::Fr => "法语",
            Language::Es => "西班牙语",
            Language::Ru => "俄语",
            Language::De => "德语",
            Language::It => "意大利语",
            Language::Tr => "土耳其语",
            Language::PtPt => "葡萄牙语",
            Language::PtBr => "巴西葡萄牙语",
            Language::Vi => "越南语",
            Language::Id => "印尼语",
            Language::Th => "泰语",
            Language::Ms => "马来语",
            Language::Ar => "阿拉伯语",
            Language::Hi => "印地语",
            Language::Km => "高棉语",
            Language::MnCy => "蒙古语（西里尔）",
            Language::NbNo => "挪威语（书面）",
            Language::NnNo => "挪威语（新）",
            Language::Fa => "波斯语",
            Language::Sv => "瑞典语",
            Language::Pl => "波兰语",
            Language::Nl => "荷兰语",
            Language::Uk => "乌克兰语",
            Language::He => "希伯来语",
            Language::MnMo => "蒙古语",
        }
    }
}
