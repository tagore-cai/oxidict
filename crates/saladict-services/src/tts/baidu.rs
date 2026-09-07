//! 百度 TTS。
//!
//! 移植自 `src/services/tts/baidu/index.jsx`。
//! 调用 `fanyi.baidu.com/gettts` 接口，参数 `lan`/`text`/`spd`，
//! 接口直接返回二进制音频（mp3），原实现无需鉴权与签名。

use crate::Tts;
use async_trait::async_trait;
use saladict_core::map_language;
use saladict_core::{Language, Result, TtsRequest};

pub struct Baidu;

const ENDPOINT: &str = "https://fanyi.baidu.com/gettts";

#[async_trait]
impl Tts for Baidu {
    fn id(&self) -> &str {
        "baidu"
    }

    fn map_language(&self, lang: Language) -> String {
        map_language!(lang, {
            ZhCn => "zh",
            ZhTw => "cht",
            Ja => "jp",
            Ko => "kor",
            Fr => "fra",
            Vi => "vie",
            MnCy => "mn",
            NbNo => "no",
            NnNo => "no",
        })
    }

    async fn tts(&self, req: TtsRequest) -> Result<Vec<u8>> {
        let lang = self.map_language(req.language);
        let url = format!(
            "{}?lan={}&text={}&spd=5",
            ENDPOINT,
            lang,
            urlencoding::encode(&req.text)
        );
        let bytes = saladict_net::get_bytes(url).await?;
        Ok(bytes.to_vec())
    }
}
