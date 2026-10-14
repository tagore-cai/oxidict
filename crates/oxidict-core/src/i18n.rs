//! 国际化（Fluent 本地化系统）。
//!
//! 原实现用 react-i18next + 20 个 JSON 文件；这里改用 Mozilla 的 Fluent
//! （`.ftl`），好处是内置复数/占位/回退，且字符串带语法而非纯键值。
//!
//! 语言文件放在 `crates/oxidict-core/locales/<lang>/main.ftl`，运行时通过
//! [`fluent_templates::static_loader`] 编译期嵌入，无需外部文件依赖。

use fluent_templates::{Loader, static_loader};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use unic_langid::{LanguageIdentifier, langid};

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
        let mut guard = CURRENT.lock();
        *guard = lang;
    }
}

/// 当前语言代码。
pub fn language() -> String {
    CURRENT.lock().to_string()
}

/// 取一条本地化消息。
///
/// Fluent 的 `lookup` 已内置回退（当前语言 → fallback_language → 键名），无需手动兜底。
pub fn t(key: &str) -> String {
    let lang = CURRENT.lock().clone();
    LOCALES.lookup(&lang, key)
}

/// 带变量的本地化消息。
///
/// `args` 为 (名称, 值) 对，对应 `.ftl` 里的 `{ $name }` 占位。
pub fn t_args(key: &str, args: &[(&str, &str)]) -> String {
    use fluent_templates::fluent_bundle::FluentValue;
    use std::borrow::Cow;
    use std::collections::HashMap;

    let lang = CURRENT.lock().clone();
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 两种主语言（zh-CN / en-US）的键集必须一致。
    ///
    /// 只加一侧的键在运行时表现为「界面直接显示成 key 名」；Fluent 的 fallback
    /// 只在整条链都缺失时才暴露，静默且难查。这里在测试期把两边对齐。
    /// （其余 18 个 locale 有意只翻译一部分，缺失部分按设计回退到 en-US。）
    #[test]
    fn primary_locales_share_the_same_key_set() {
        fn keys(src: &str) -> std::collections::BTreeSet<String> {
            src.lines()
                .filter_map(|line| line.split_once('='))
                .map(|(k, _)| k.trim().to_string())
                .filter(|k| !k.is_empty() && !k.starts_with('#'))
                .collect()
        }
        let zh = keys(include_str!("../locales/zh-CN/main.ftl"));
        let en = keys(include_str!("../locales/en-US/main.ftl"));
        let only_zh: Vec<_> = zh.difference(&en).cloned().collect();
        let only_en: Vec<_> = en.difference(&zh).cloned().collect();
        assert!(
            only_zh.is_empty() && only_en.is_empty(),
            "仅 zh-CN 有: {only_zh:?}\n仅 en-US 有: {only_en:?}"
        );
    }

    /// UI 实际引用的键必须在两种主语言里都能解析出文案（防拼错键名，
    /// 也是 `.ftl` 里占位符写错时的兜底）。
    #[test]
    fn ui_keys_resolve_in_both_primary_locales() {
        const KEYS: &[&str] = &[
            // 设置页：外观 / 语言
            "config-theme",
            "config-theme-system",
            "config-theme-light",
            "config-theme-dark",
            "config-language",
            "config-font",
            "config-font-size",
            "config-font-system",
            "config-hotkey-placeholder",
            // 历史页
            "config-history-search",
            "config-history-no-match",
            "config-history-copy",
            "config-history-count",
            "config-history-empty",
            // 备份 / 恢复
            "config-backup-type",
            "config-backup-type-webdav",
            "config-backup-type-local",
            "config-backup-folder",
            "config-backup-uploading",
            "config-nav-custom",
            "config-history-more",
            "config-move-to-end",
            "config-custom-edit",
            "config-custom-editing",
            "config-custom-edit-unsupported",
            "config-custom-intro",
            "config-custom-id",
            "config-custom-endpoint",
            "config-custom-method",
            "config-custom-auth",
            "config-custom-auth-name",
            "config-custom-body",
            "config-custom-extract",
            "config-custom-tls",
            "config-custom-save",
            "config-custom-delete",
            "config-custom-list",
            "config-custom-empty",
            "config-custom-saved",
            "config-custom-auth-none",
            "config-custom-auth-bearer",
            "config-custom-auth-header",
            "config-custom-auth-query",
            "config-backup-webdav-hint",
            "config-backup-server",
            "config-backup-username",
            "config-backup-password",
            "config-backup-to-cloud",
            "config-backup-from-cloud",
            "config-backup-local-hint",
            "config-backup-export",
            "config-backup-import",
            "config-backup-empty",
            "config-backup-pick",
            "config-backup-uploaded",
            "config-backup-upload-failed",
            "config-backup-list-fetching",
            "config-backup-list-count",
            "config-backup-list-failed",
            "config-backup-export-done",
            "config-backup-export-failed",
            "config-restore-confirm-remote",
            "config-restore-confirm-local",
            "config-restore-snapshot-note",
            "config-restore-confirm",
            "config-restore-restoring",
            "config-restore-done",
            "config-restore-done-local",
            "config-restore-failed",
            "common-cancel",
        ];
        for lang in [langid!("zh-CN"), langid!("en-US")] {
            for key in KEYS {
                let value = LOCALES.lookup(&lang, key);
                assert_ne!(value, *key, "[{lang}] 键未解析: {key}");
            }
        }
    }
}
