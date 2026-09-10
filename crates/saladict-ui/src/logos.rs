//! 品牌 logo 资产源。
//!
//! 把 `assets/logos/` 下的 logo 在编译期通过 `include_bytes!` 嵌入二进制，并通过自定义
//! [`AssetSource`]（[`CombinedAssets`]）暴露给 gpui，uri 形如 `"logo/<file>"`。
//! 同时提供 [`logo_uri`] 把服务 id 映射到对应的 logo uri。

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::OnceLock;

use gpui_kit::{AssetSource, SharedString};

/// 全部内嵌 logo：文件名 + 编译期字节（相对 crate 根解析到仓库 `assets/logos`）。
const LOGO_FILES: &[(&str, &[u8])] = &[
    (
        "Darwin.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/logos/Darwin.svg"
        )),
    ),
    (
        "Linux.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/logos/Linux.svg"
        )),
    ),
    (
        "Windows_NT.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/logos/Windows_NT.svg"
        )),
    ),
    (
        "alibaba.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/logos/alibaba.svg"
        )),
    ),
    (
        "anki.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/logos/anki.svg"
        )),
    ),
    (
        "baidu.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/logos/baidu.svg"
        )),
    ),
    (
        "bing.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/logos/bing.svg"
        )),
    ),
    (
        "caiyun.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/logos/caiyun.svg"
        )),
    ),
    (
        "cambridge_dict.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/logos/cambridge_dict.svg"
        )),
    ),
    (
        "chatglm.png",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/logos/chatglm.png"
        )),
    ),
    (
        "claude.png",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/logos/claude.png"
        )),
    ),
    (
        "deepl.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/logos/deepl.svg"
        )),
    ),
    (
        "deepseek.png",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/logos/deepseek.png"
        )),
    ),
    (
        "ecdict.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/logos/ecdict.svg"
        )),
    ),
    (
        "eudic.png",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/logos/eudic.png"
        )),
    ),
    (
        "gemini.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/logos/gemini.svg"
        )),
    ),
    (
        "google.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/logos/google.svg"
        )),
    ),
    (
        "iflytek.png",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/logos/iflytek.png"
        )),
    ),
    (
        "lingva.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/logos/lingva.svg"
        )),
    ),
    (
        "minimax.png",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/logos/minimax.png"
        )),
    ),
    (
        "moonshot.png",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/logos/moonshot.png"
        )),
    ),
    (
        "niutrans.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/logos/niutrans.svg"
        )),
    ),
    (
        "ollama.png",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/logos/ollama.png"
        )),
    ),
    (
        "openai.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/logos/openai.svg"
        )),
    ),
    (
        "paddle.png",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/logos/paddle.png"
        )),
    ),
    (
        "qrcode.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/logos/qrcode.svg"
        )),
    ),
    (
        "simple_latex.png",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/logos/simple_latex.png"
        )),
    ),
    (
        "tencent.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/logos/tencent.svg"
        )),
    ),
    (
        "tencent_cloud.png",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/logos/tencent_cloud.png"
        )),
    ),
    (
        "tesseract.png",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/logos/tesseract.png"
        )),
    ),
    (
        "tongyi.png",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/logos/tongyi.png"
        )),
    ),
    (
        "transmart.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/logos/transmart.svg"
        )),
    ),
    (
        "volcengine.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/logos/volcengine.svg"
        )),
    ),
    (
        "yandex.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/logos/yandex.svg"
        )),
    ),
    (
        "youdao.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/logos/youdao.svg"
        )),
    ),
];

/// 已知「服务 id → logo 文件名」不一致映射；其余按 `{id}.svg` / `{id}.png` 回退。
const SPECIAL: &[(&str, &str)] = &[
    ("chatglm_cloud", "chatglm.png"),
    ("tencent_cloud", "tencent_cloud.png"),
    ("simple_latex", "simple_latex.png"),
    ("tencent_img", "tencent.svg"),
    ("baidu_img", "baidu.svg"),
    ("baidu_accurate", "baidu.svg"),
    ("tencent_accurate", "tencent.svg"),
    ("volcengine_multi_lang", "volcengine.svg"),
    ("iflytek_intsig", "iflytek.png"),
    ("iflytek_latex", "iflytek.png"),
    ("cambridge_dict", "cambridge_dict.svg"),
    ("bing_dict", "bing.svg"),
    ("gemini_cloud", "gemini.svg"),
    ("claude_cloud", "claude.png"),
    ("openai_compatible", "openai.svg"),
    ("geminipro", "gemini.svg"),
    ("ecdict", "ecdict.svg"),
    ("youdao", "youdao.svg"),
    ("transmart", "transmart.svg"),
    ("deepl_cloud", "deepl.svg"),
    ("openai_cloud", "openai.svg"),
    ("moonshot_cloud", "moonshot.png"),
    ("minimax_cloud", "minimax.png"),
    ("deepseek_cloud", "deepseek.png"),
    ("tongyi_cloud", "tongyi.png"),
    ("caiyun", "caiyun.svg"),
    ("niutrans", "niutrans.svg"),
    ("alibaba", "alibaba.svg"),
    ("volcengine", "volcengine.svg"),
    ("yandex", "yandex.svg"),
    ("lingva", "lingva.svg"),
    ("tencent", "tencent.svg"),
    ("baidu", "baidu.svg"),
    ("google", "google.svg"),
    ("bing", "bing.svg"),
    ("deepl", "deepl.svg"),
    ("openai", "openai.svg"),
    ("ollama", "ollama.png"),
    ("chatglm", "chatglm.png"),
    ("qrcode", "qrcode.svg"),
    ("tesseract", "tesseract.png"),
    ("paddle", "paddle.png"),
    ("anki", "anki.svg"),
    ("eudic", "eudic.png"),
];

/// 按文件名取内嵌 logo 字节。
fn logo_bytes(name: &str) -> Option<&'static [u8]> {
    LOGO_FILES.iter().find(|(n, _)| *n == name).map(|(_, b)| *b)
}

/// 组合资产源：以 `"logo/"` 前缀开头的路径从内嵌表取，其余回退到 gpui-kit 自带资产。
pub struct CombinedAssets {
    inner: gpui_kit::assets::Assets,
}

impl CombinedAssets {
    pub fn new() -> Self {
        Self {
            inner: gpui_kit::assets::Assets,
        }
    }
}

impl Default for CombinedAssets {
    fn default() -> Self {
        Self::new()
    }
}

impl AssetSource for CombinedAssets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        if let Some(rest) = path.strip_prefix("logo/") {
            if let Some(bytes) = logo_bytes(rest) {
                return Ok(Some(Cow::Borrowed(bytes)));
            }
        }
        self.inner.load(path)
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        self.inner.list(path)
    }
}

/// 根据服务 id 返回对应 logo 的 uri（形如 `"logo/deepl.svg"`）或 `None`（系统服务用系统图标）。
///
/// 规则：先查 [`SPECIAL`] 显式表；否则回退为 `{id}.svg`，再 `{id}.png`。
pub fn logo_uri(service_id: &str) -> Option<&'static str> {
    if service_id == "system" {
        return None;
    }
    static MAP: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    let map = MAP.get_or_init(|| {
        let mut m: HashMap<&'static str, &'static str> = HashMap::new();
        // 默认：文件名 stem 作为服务 id。
        for (name, _) in LOGO_FILES {
            if let Some(stem) = name
                .strip_suffix(".svg")
                .or_else(|| name.strip_suffix(".png"))
            {
                m.entry(stem)
                    .or_insert_with(|| Box::leak(format!("logo/{name}").into_boxed_str()));
            }
        }
        // 显式覆盖。
        for (id, fname) in SPECIAL {
            m.insert(id, Box::leak(format!("logo/{fname}").into_boxed_str()));
        }
        m
    });
    map.get(service_id).copied()
}

#[cfg(test)]
mod tests {
    use super::{logo_bytes, logo_uri, LOGO_FILES, SPECIAL};

    /// 每个内嵌 logo 都能取到非空字节（include_bytes! 落盘即存在）。
    #[test]
    fn embedded_logos_are_all_present_and_nonempty() {
        assert!(!LOGO_FILES.is_empty(), "至少有一个内嵌 logo");
        for (name, bytes) in LOGO_FILES {
            assert!(!bytes.is_empty(), "{name} 内嵌字节为空");
            // 文件名必须能被识别为 svg 或 png 扩展名。
            assert!(
                name.ends_with(".svg") || name.ends_with(".png"),
                "{name} 扩展名应为 .svg/.png"
            );
        }
    }

    /// SPECIAL 表里引用的文件名都必须真实存在于 LOGO_FILES（防止映射悬空）。
    #[test]
    fn special_entries_reference_existing_files() {
        for (id, fname) in SPECIAL {
            assert!(
                LOGO_FILES.iter().any(|(n, _)| *n == *fname),
                "SPECIAL[{id}] 引用的文件 {fname} 不在 LOGO_FILES 中"
            );
        }
    }

    /// 系统服务返回 None（用系统图标，不渲染自研 logo）。
    #[test]
    fn system_service_has_no_logo() {
        assert_eq!(logo_uri("system"), None);
    }

    /// 默认规则：文件名 stem 即服务 id，`.svg` 优先。
    #[test]
    fn default_stem_mapping() {
        assert_eq!(logo_uri("deepl"), Some("logo/deepl.svg"));
        assert_eq!(logo_uri("baidu"), Some("logo/baidu.svg"));
        assert_eq!(logo_uri("google"), Some("logo/google.svg"));
        assert_eq!(logo_uri("youdao"), Some("logo/youdao.svg"));
    }

    /// PNG 型 logo 走默认 stem 映射仍命中（`{stem}.png`）。
    #[test]
    fn default_stem_png_mapping() {
        assert_eq!(logo_uri("deepseek"), Some("logo/deepseek.png"));
        assert_eq!(logo_uri("ollama"), Some("logo/ollama.png"));
        assert_eq!(logo_uri("tesseract"), Some("logo/tesseract.png"));
    }

    /// SPECIAL 显式映射覆盖默认 stem。
    #[test]
    fn special_override_mapping() {
        assert_eq!(logo_uri("chatglm_cloud"), Some("logo/chatglm.png"));
        assert_eq!(logo_uri("openai_compatible"), Some("logo/openai.svg"));
        assert_eq!(logo_uri("tencent_img"), Some("logo/tencent.svg"));
        assert_eq!(logo_uri("baidu_img"), Some("logo/baidu.svg"));
        assert_eq!(logo_uri("deepl_cloud"), Some("logo/deepl.svg"));
    }

    /// 未知服务 id 返回 None（渲染方自行回退图标）。
    #[test]
    fn unknown_service_has_no_logo() {
        assert_eq!(logo_uri("definitely_not_a_service"), None);
        assert_eq!(logo_uri(""), None);
    }

    /// 每个已知 logo 名都可通过 logo_bytes 查到（供 UI 侧以 uri 反向取字节）。
    #[test]
    fn all_known_names_resolve() {
        for (name, _) in LOGO_FILES {
            assert!(
                logo_bytes(name).is_some(),
                "{name} 无法通过 logo_bytes 取到"
            );
        }
    }
}
