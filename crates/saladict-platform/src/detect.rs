//! 语言检测。离线完成，用 lingua 限定与上游一致的语言集合。

use once_cell::sync::Lazy;
use saladict_core::Language;

static DETECTOR: Lazy<lingua::LanguageDetector> = Lazy::new(|| {
    lingua::LanguageDetectorBuilder::from_languages(&[
        lingua::Language::Chinese,
        lingua::Language::Japanese,
        lingua::Language::English,
        lingua::Language::Korean,
        lingua::Language::French,
        lingua::Language::Spanish,
        lingua::Language::German,
        lingua::Language::Russian,
        lingua::Language::Italian,
        lingua::Language::Portuguese,
        lingua::Language::Turkish,
        lingua::Language::Arabic,
        lingua::Language::Vietnamese,
        lingua::Language::Thai,
        lingua::Language::Indonesian,
        lingua::Language::Malay,
        lingua::Language::Hindi,
        lingua::Language::Mongolian,
        lingua::Language::Persian,
        lingua::Language::Nynorsk,
        lingua::Language::Bokmal,
        lingua::Language::Ukrainian,
    ])
    .with_preloaded_language_models()
    .build()
});

/// 检测文本语言。检测不出或置信度过低时返回 None。
pub fn detect(text: &str) -> Option<Language> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let detected = DETECTOR.detect_language_of(text)?;
    Some(map(detected))
}

fn map(lang: lingua::Language) -> Language {
    match lang {
        lingua::Language::Chinese => Language::ZhCn,
        lingua::Language::Japanese => Language::Ja,
        lingua::Language::English => Language::En,
        lingua::Language::Korean => Language::Ko,
        lingua::Language::French => Language::Fr,
        lingua::Language::Spanish => Language::Es,
        lingua::Language::German => Language::De,
        lingua::Language::Russian => Language::Ru,
        lingua::Language::Italian => Language::It,
        lingua::Language::Portuguese => Language::PtPt,
        lingua::Language::Turkish => Language::Tr,
        lingua::Language::Arabic => Language::Ar,
        lingua::Language::Vietnamese => Language::Vi,
        lingua::Language::Thai => Language::Th,
        lingua::Language::Indonesian => Language::Id,
        lingua::Language::Malay => Language::Ms,
        lingua::Language::Hindi => Language::Hi,
        lingua::Language::Mongolian => Language::MnCy,
        lingua::Language::Persian => Language::Fa,
        lingua::Language::Nynorsk => Language::NnNo,
        lingua::Language::Bokmal => Language::NbNo,
        lingua::Language::Ukrainian => Language::Uk,
        // 没有兜底分支：lingua 关掉了默认 feature，`Language` 只包含上面
        // 这些启用的语种，匹配已穷尽。日后新增语种会直接编译失败，
        // 提醒同步维护这里的映射，而不是悄悄回落到英语。
    }
}
