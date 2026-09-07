//! Lingva TTS。
//!
//! 移植自 `src/services/tts/lingva/index.jsx`。
//! Lingva 是开源的 Google 翻译前端，音频接口 `/api/v1/audio/{lang}/{text}`
//! 直接返回 JSON `{ audio: "<base64>" }`，这里解出 base64 还原成音频字节。

use crate::Tts;
use async_trait::async_trait;
use base64::Engine;
use saladict_core::map_language;
use saladict_core::schema::ConfigField;
use saladict_core::{Error, Language, Result, TtsRequest};

pub struct Lingva;

const DEFAULT_ENDPOINT: &str = "lingva.ml";

#[async_trait]
impl Tts for Lingva {
    fn id(&self) -> &str {
        "lingva"
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        vec![ConfigField::Text {
            key: "request_path",
            label: "请求地址",
            placeholder: DEFAULT_ENDPOINT,
            secret: false,
            required: false,
        }]
    }

    fn map_language(&self, lang: Language) -> String {
        map_language!(lang, {
            ZhCn => "zh",
            ZhTw => "zh_HANT",
            MnCy => "mn",
            NbNo => "no",
            NnNo => "no",
        })
    }

    async fn tts(&self, req: TtsRequest) -> Result<Vec<u8>> {
        let mut endpoint = req
            .config
            .get("request_path")
            .and_then(|v| v.as_str())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .unwrap_or(DEFAULT_ENDPOINT)
            .to_string();
        if !endpoint.starts_with("http") {
            endpoint = format!("https://{}", endpoint);
        }

        let lang = self.map_language(req.language);
        let url = format!(
            "{}/api/v1/audio/{}/{}",
            endpoint,
            lang,
            urlencoding::encode(&req.text)
        );

        let value = saladict_net::get_value(url).await?;
        let audio = value
            .get("audio")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::Service("Lingva 响应缺少 audio 字段".into()))?;

        base64::engine::general_purpose::STANDARD
            .decode(audio)
            .map_err(|e| Error::Service(format!("音频解码失败: {}", e)))
    }
}
