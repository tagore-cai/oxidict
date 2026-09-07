//! 剑桥词典（Cambridge Dictionary）。
//!
//! 移植自 `src/services/translate/cambridge_dict/index.jsx`。
//! 原实现用 `DOMParser` + `querySelectorAll` 解析 HTML；本工程未引入 HTML 解析器
//! （且按约定不得修改 Cargo.toml 增加依赖），这里用「按 class 取元素 + 标签栈匹配」
//! 的手写方式尽量还原其结构提取逻辑。语义上保持：发音 / 释义（含词性）/ 例句，
//! 并对单词发音音频做最优努力下载（失败则留空）。
//! 注：原 JS 的 `<b>` 加粗包装在纯文本结果里省略，仅保留文字。

use crate::Translator;
use async_trait::async_trait;
use saladict_core::map_language;
use saladict_core::model::{DictResult, Explanation, Pronunciation, Sentence};
use saladict_core::schema::ConfigField;
use saladict_core::{Error, Language, Result, TranslateRequest, TranslateResult};
use std::sync::Arc;

pub struct CambridgeDict;

const ENDPOINT: &str = "https://dictionary.cambridge.org/search/direct/";

// ---------------------------------------------------------------------------
// 极简 HTML 辅助：按 class 取元素内部 HTML，标签用栈匹配。
// ---------------------------------------------------------------------------

/// 在 `html` 的 `search..` 区间里找到第一个 `class="..."` 且其 class 列表含 `token`
/// 的位置（返回相对 `html` 的绝对下标）。
fn find_class_pos(html: &str, search: usize, token: &str) -> Option<usize> {
    let mut i = search;
    while i < html.len() {
        if html[i..].starts_with("class=\"") {
            let cls_start = i + 7;
            let cls_end = html[cls_start..].find('"')? + cls_start;
            let cls = &html[cls_start..cls_end];
            if cls.split_whitespace().any(|c| c == token) {
                return Some(i);
            }
            i = cls_end + 1;
        } else {
            i += 1;
        }
    }
    None
}

/// 给定 `class="..."` 的起始下标，返回该元素的内部 HTML（用标签栈匹配到对应闭合标签）。
fn inner_html(html: &str, class_pos: usize) -> Option<String> {
    let cls_start = class_pos + 7;
    let cls_end = html[cls_start..].find('"')? + cls_start;
    let gt = html[cls_end + 1..].find('>')? + cls_end + 1;
    let open_tag_start = html[..class_pos].rfind('<')?;
    let tag_name = html[open_tag_start + 1..]
        .split(|c| c == ' ' || c == '>')
        .next()?;
    let inner_start = gt + 1;
    let open = format!("<{}", tag_name);
    let close = format!("</{}>", tag_name);
    let mut depth = 1usize;
    let mut i = inner_start;
    while i < html.len() {
        if html[i..].starts_with(&close) {
            depth -= 1;
            if depth == 0 {
                return Some(html[inner_start..i].to_string());
            }
            i += close.len();
        } else if html[i..].starts_with(&open) {
            depth += 1;
            i += open.len();
        } else {
            i += 1;
        }
    }
    None
}

/// 取所有 class 含 `token` 的元素的内部 HTML。
fn elements_inner(html: &str, token: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut search = 0;
    while let Some(pos) = find_class_pos(html, search, token) {
        if let Some(inner) = inner_html(html, pos) {
            out.push(inner);
        }
        search = pos + 1;
    }
    out
}

/// 去掉所有标签并把空白折叠成单个空格。
fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ => {
                if !in_tag {
                    out.push(c);
                }
            }
        }
    }
    let mut collapsed = String::with_capacity(out.len());
    let mut prev_space = false;
    for c in out.chars() {
        if c.is_whitespace() {
            if !prev_space && !collapsed.is_empty() {
                collapsed.push(' ');
            }
            prev_space = true;
        } else {
            collapsed.push(c);
            prev_space = false;
        }
    }
    collapsed.trim().to_string()
}

/// 取第一个 class 含 `token` 的元素的纯文本。
fn text_of(html: &str, token: &str) -> Option<String> {
    let pos = find_class_pos(html, 0, token)?;
    inner_html(html, pos).map(|s| strip_tags(&s))
}

/// 在某个片段里从 `from` 下标开始找 `src="..."` 的值。
fn attr_src_from(s: &str, from: usize) -> Option<String> {
    let rest = &s[from..];
    let pos = rest.find("src=\"")?;
    let start = from + pos + 5;
    let end = s[start..].find('"')? + start;
    Some(s[start..end].to_string())
}

/// 把音频地址归一化到 cambridge 域名。
fn normalize_voice(v: &str) -> String {
    if let Some(proto_end) = v.find("://") {
        let after = &v[proto_end + 3..];
        if let Some(slash) = after.find('/') {
            return format!("https://dictionary.cambridge.org{}", &after[slash..]);
        }
    }
    if v.starts_with('/') {
        return format!("https://dictionary.cambridge.org{}", v);
    }
    v.to_string()
}

#[async_trait]
impl Translator for CambridgeDict {
    fn id(&self) -> &str {
        "cambridge_dict"
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        vec![]
    }

    fn map_language(&self, lang: Language) -> String {
        map_language!(lang, {
            ZhCn => "chinese-simplified",
            ZhTw => "chinese-traditional",
            En => "english",
        })
    }

    async fn translate(&self, req: TranslateRequest) -> Result<TranslateResult> {
        let text = &req.text;

        // 简单检测：首字符为 ASCII 字母视为英文，否则无法翻译返回空。
        let from = if req.from == Language::Auto {
            if text
                .chars()
                .next()
                .map(|c| c.is_ascii_alphabetic())
                .unwrap_or(false)
            {
                Language::En
            } else {
                return Ok(TranslateResult::Plain(String::new()));
            }
        } else {
            req.from
        };

        // 只支持英文 -> 其它语言，且 source != target。
        if from != Language::En || req.to == from || text.split(' ').count() > 1 {
            return Ok(TranslateResult::Plain(String::new()));
        }

        let from_code = self.map_language(from);
        let to_code = self.map_language(req.to);
        let url = format!(
            "{}?datasetsearch={}-{}&q={}",
            ENDPOINT,
            from_code,
            to_code,
            urlencoding::encode(text)
        );

        let html = saladict_net::get_text(url).await?;

        let entries = elements_inner(&html, "entry-body__el");
        if entries.is_empty() {
            return Err(Error::Service(format!("Words not yet included: {}", text)));
        }

        let mut dict = DictResult::new();

        for entry in &entries {
            // 发音（每个 .dpron-i）
            for block in elements_inner(entry, "dpron-i") {
                let region = text_of(&block, "region");
                let symbol = text_of(&block, "pron");
                let voice = if let Some(p) = block.find("daud") {
                    attr_src_from(&block, p).map(|v| normalize_voice(&v))
                } else {
                    None
                };
                let voice = match voice {
                    Some(url) => saladict_net::get_bytes(&url)
                        .await
                        .ok()
                        .map(|b| Arc::new(b.to_vec())),
                    None => None,
                };
                dict.pronunciations.push(Pronunciation {
                    symbol: symbol.or(region),
                    voice,
                });
            }

            // 词性
            let word_pos = text_of(entry, "posgram");

            // 释义块（.def-block.ddef_block）
            for def in elements_inner(entry, "ddef_block") {
                // 跳过带 data-wl-senseid="panel" 的面板块
                if def.contains("data-wl-senseid")
                    && def.contains("panel")
                {
                    continue;
                }
                let eng_def = text_of(&def, "ddef_d").unwrap_or_default();
                let trait_ = word_pos.clone().filter(|w| !w.is_empty()).or_else(|| {
                    Some(eng_def.replace(char::is_whitespace, " ").trim().to_string())
                });
                let mut explains = vec![eng_def];
                if let Some(trans) = text_of(&def, "dtrans-se") {
                    for part in trans.split(';') {
                        let p = part.trim().to_string();
                        if !p.is_empty() {
                            explains.push(p);
                        }
                    }
                }
                dict.explanations.push(Explanation {
                    trait_,
                    explains,
                });
            }

            // 例句（第一个 .eg）
            for def in elements_inner(entry, "ddef_block") {
                if let Some(p) = def.find("examp") {
                    if def[p..].contains("eg") {
                        if let Some(eg) = text_of(&def, "eg") {
                            dict.sentence.push(Sentence {
                                source: Some(eg),
                                target: None,
                            });
                        }
                    }
                }
            }
        }

        Ok(TranslateResult::Dict(dict))
    }
}
