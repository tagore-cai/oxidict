//! 国际化（Fluent 本地化系统）。
//!
//! 原实现用 react-i18next + 20 个 JSON 文件；这里改用 Mozilla 的 Fluent
//! （`.ftl`），好处是内置复数/占位/回退，且字符串带语法而非纯键值。
//!
//! 语言文件放在仓库根的 `locales/<lang>/main.ftl`，运行时通过
//! [`fluent_templates::static_loader`] 编译期嵌入，无需外部文件依赖。

use fluent_templates::{static_loader, Loader};
use once_cell::sync::Lazy;
use std::sync::Mutex;
use unic_langid::{langid, LanguageIdentifier};

// 编译期嵌入 locales/<lang>/main.ftl。fallback 为英文。
static_loader! {
    static LOCALES = {
        locales: "./locales",
        fallback_language: "en-US",
        // 去掉键前缀中的文件名部分，只保留消息 id。
        customise: |bundle| bundle.set_use_isolating(false),
    };
}

/// 支持的语言（与原工程 locales 目录一致的子集，优先填充完整翻译）。
pub const LANGUAGES: [(&str, &str); 20] = [
    ("zh-CN", "简体中文"),
    ("zh-TW", "繁體中文"),
    ("en-US", "English"),
    ("ja-JP", "日本語"),
    ("ko-KR", "한국어"),
    ("fr-FR", "Français"),
    ("de-DE", "Deutsch"),
    ("es-ES", "Español"),
    ("it-IT", "Italiano"),
    ("pt-BR", "Português (Brasil)"),
    ("pt-PT", "Português"),
    ("ru-RU", "Русский"),
    ("ar-AE", "العربية"),
    ("he-IL", "עברית"),
    ("fa-IR", "فارسی"),
    ("tr-TR", "Türkçe"),
    ("uk-UA", "Українська"),
    ("nb-NO", "Norsk bokmål"),
    ("nn-NO", "Norsk nynorsk"),
    ("tk-TM", "Türkmen"),
];

/// 当前语言。运行时可通过 [`set_language`] 切换。
static CURRENT: Lazy<Mutex<LanguageIdentifier>> = Lazy::new(|| Mutex::new(langid!("zh-CN")));

/// 设置当前语言（如 `zh-CN`、`en-US`）。无法解析的值会被忽略。
pub fn set_language(code: &str) {
    if let Ok(lang) = code.parse::<LanguageIdentifier>() {
        let mut guard = CURRENT.lock().unwrap();
        *guard = lang;
    }
}

/// 当前语言代码。
pub fn language() -> String {
    CURRENT.lock().unwrap().to_string()
}

/// 取一条本地化消息。
///
/// Fluent 的 `lookup` 已内置回退（当前语言 → fallback_language → 键名），无需手动兜底。
pub fn t(key: &str) -> String {
    let lang = CURRENT.lock().unwrap().clone();
    LOCALES.lookup(&lang, key)
}

/// 带变量的本地化消息。
///
/// `args` 为 (名称, 值) 对，对应 `.ftl` 里的 `{ $name }` 占位。
pub fn t_args(key: &str, args: &[(&str, &str)]) -> String {
    use fluent_templates::fluent_bundle::FluentValue;
    use std::borrow::Cow;
    use std::collections::HashMap;

    let lang = CURRENT.lock().unwrap().clone();
    let mut map: HashMap<Cow<'static, str>, FluentValue> = HashMap::new();
    for (name, value) in args {
        map.insert(
            Cow::Owned((*name).to_string()),
            FluentValue::from((*value).to_string()),
        );
    }
    LOCALES.lookup_with_args(&lang, key, &map)
}

/// 从 config 的 `app_language` 初始化语言。
pub fn init_from_config() {
    let code: String = crate::config::config()
        .get(crate::config::keys::APP_LANGUAGE)
        .unwrap_or_else(|| "zh-CN".into());
    set_language(&code);
}
