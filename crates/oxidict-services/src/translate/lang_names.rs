//! 语言名映射：把内部语言码转成**英文语言名**。
//!
//! 沙拉云中转类服务（`relay.rs`）与用户自定义服务（`custom.rs`）都用它填
//! `{{from}}` / `{{to}}` 占位符——提示词里写 "Translate into Japanese" 比写
//! `ja` 效果好，而这些服务不接受语言码。
//!
//! 未列出的语言回退到内部码本身（见 `map_language!` 的兜底分支）。

use oxidict_core::Language;
use oxidict_core::map_language;

/// 英文语言名。未覆盖的语言回退为其内部码。
pub fn english_name(lang: Language) -> String {
    map_language!(lang, {
        Auto => "Auto",
        ZhCn => "Simplified Chinese",
        ZhTw => "Traditional Chinese",
        En => "English",
        Ja => "Japanese",
        Ko => "Korean",
        Fr => "French",
        Es => "Spanish",
        Ru => "Russian",
        De => "German",
        It => "Italian",
        Tr => "Turkish",
        PtPt => "Portuguese",
        PtBr => "Brazilian Portuguese",
        Vi => "Vietnamese",
        Id => "Indonesian",
        Th => "Thai",
        Ms => "Malay",
        Ar => "Arabic",
        Hi => "Hindi",
        MnMo => "Mongolian",
        MnCy => "Mongolian(Cyrillic)",
        Km => "Khmer",
        NbNo => "Norwegian Bokmål",
        NnNo => "Norwegian Nynorsk",
        Fa => "Persian",
        Sv => "Swedish",
        Pl => "Polish",
        Nl => "Dutch",
        Uk => "Ukrainian",
        He => "Hebrew",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_core_languages_to_english_names() {
        assert_eq!(english_name(Language::ZhCn), "Simplified Chinese");
        assert_eq!(english_name(Language::ZhTw), "Traditional Chinese");
        assert_eq!(english_name(Language::Ja), "Japanese");
        assert_eq!(english_name(Language::PtBr), "Brazilian Portuguese");
        assert_eq!(english_name(Language::Auto), "Auto");
    }

    /// 未在表里的语言必须回退成内部码而不是空串——空串会让提示词变成
    /// "Translate into "，服务侧只会当成没给目标语言。
    #[test]
    fn falls_back_to_code_not_empty() {
        for lang in Language::ALL {
            let name = english_name(*lang);
            assert!(!name.trim().is_empty(), "{lang:?} 映射为空");
        }
    }
}
