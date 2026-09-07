//! Google TTS。
//!
//! 移植自 `src/services/tts/google/index.jsx` 与 `token.js`。
//! `translate_tts` 接口需要 `tk` 参数（由文本与 TKK 种子算出的防伪令牌），
//! 否则会被服务端拒绝。这里把 `token.js` 的 `sM`/`xr` 完整平移到 Rust。

use crate::Tts;
use async_trait::async_trait;
use saladict_core::map_language;
use saladict_core::{Language, Result, TtsRequest};

pub struct Google;

const ENDPOINT: &str = "https://translate.google.com/translate_tts";

#[async_trait]
impl Tts for Google {
    fn id(&self) -> &str {
        "google"
    }

    fn map_language(&self, lang: Language) -> String {
        map_language!(lang, {
            ZhCn => "zh-CN",
            ZhTw => "zh-TW",
            MnCy => "mn",
            NbNo => "no",
            NnNo => "no",
        })
    }

    async fn tts(&self, req: TtsRequest) -> Result<Vec<u8>> {
        let lang = self.map_language(req.language);
        let tk = google_tk(&req.text);
        let url = format!(
            "{}?tl={}&q={}&ie=UTF-8&client=t&total=1&idx=0&tk={}",
            ENDPOINT,
            lang,
            urlencoding::encode(&req.text),
            tk
        );
        let bytes = saladict_net::get_bytes(url).await?;
        Ok(bytes.to_vec())
    }
}

/// 计算 Google TTS 的 `tk` 防伪令牌。
///
/// 移植自 `token.js` 的 `sM`/`xr`。原 JS 从 `window.TKK` 读取种子，
/// 该值在 saladict 里固定为 `"0"`，这里直接以 `0` 代入。
fn google_tk(text: &str) -> String {
    // 把文本按 UTF-16 码元展开成与 JS `charCodeAt` 一致的整数序列。
    let e: Vec<i32> = {
        let units: Vec<u16> = text.encode_utf16().collect();
        let mut out = Vec::new();
        let mut i = 0;
        while i < units.len() {
            let l = units[i] as i32;
            if l < 128 {
                out.push(l);
            } else if l < 2048 {
                out.push((l >> 6) | 192);
            } else if (l & 0xFC00) == 0xD800
                && i + 1 < units.len()
                && (units[i + 1] as i32 & 0xFC00) == 0xDC00
            {
                let l = 0x10000 + ((l & 0x3FF) << 10) + (units[i + 1] as i32 & 0x3FF);
                out.push((l >> 18) | 240);
                out.push((l >> 12 & 63) | 128);
                out.push((l >> 6 & 63) | 128);
                out.push((l & 63) | 128);
                i += 1;
            } else {
                out.push((l >> 12) | 224);
                out.push((l >> 6 & 63) | 128);
                out.push((l & 63) | 128);
            }
            i += 1;
        }
        out
    };

    let mut a: i32 = 0; // TKK 第一段（saladict 固定为 0）
    for &ei in &e {
        a = a.wrapping_add(ei);
        a = xr(a, &[10, 6]);
    }
    a = xr(a, &[3, 11, 15]);
    // a ^= TKK 第二段（此处为 0，无影响）

    let mut au = a as i64;
    if au < 0 {
        au = (au & 0x7FFF_FFFF) + 0x8000_0000;
    }
    au %= 1_000_000;
    format!("{}.{}", au, au)
}

/// 对应 `token.js` 的 `xr`：对 `a` 做若干次「左移 + 加」的 32 位混淆。
fn xr(mut a: i32, shifts: &[u32]) -> i32 {
    for &d in shifts {
        a = a.wrapping_add(a.wrapping_shl(d));
    }
    a
}
