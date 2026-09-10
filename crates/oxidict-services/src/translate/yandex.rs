//! Yandex 翻译。
//!
//! 移植自 `src/services/translate/yandex/index.tsx`，走安卓客户端接口，无需配置。

use crate::Translator;
use async_trait::async_trait;
use oxidict_core::map_language;
use oxidict_core::{Error, Language, Result, TranslateRequest, TranslateResult};
use serde_json::Value;

pub struct Yandex;

const URL: &str = "https://translate.yandex.net/api/v1/tr.json/translate";
const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

#[async_trait]
impl Translator for Yandex {
    fn id(&self) -> &str {
        "yandex"
    }

    fn map_language(&self, lang: Language) -> String {
        map_language!(lang, {
            Auto => "",
            ZhCn => "zh",
            ZhTw => "zh",
            NbNo => "no",
            NnNo => "no",
        })
    }

    async fn translate(&self, req: TranslateRequest) -> Result<TranslateResult> {
        use oxidict_net::NetErr as _;

        let id = format!("{}-0-0", oxidict_net::uuid_v4().replace('-', ""));
        let url = format!("{}?id={}&srv=android", URL, urlencoding::encode(&id));

        let resp = oxidict_net::client()
            .post(&url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("User-Agent", UA)
            .form(&[
                ("source_lang", self.map_language(req.from)),
                ("target_lang", self.map_language(req.to)),
                ("text", req.text.clone()),
            ])
            .send()
            .await
            .net_err()?;
        let resp = oxidict_net::check(resp).await?;
        let result: Value = resp.json().await.net_err()?;
        let translated = result
            .get("text")
            .and_then(|v| v.get(0))
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::Service(result.to_string()))?
            .to_string();
        Ok(TranslateResult::Plain(translated))
    }
}
