//! 百度翻译（通用版）。
//!
//! 移植自 `src/services/translate/baidu/index.jsx`。
//! 签名 = md5(appid + text + salt + secret)，请求以 GET + query 形式发送。

use crate::Translator;
use async_trait::async_trait;
use oxidict_core::HasConfig as _;
use oxidict_core::map_language;
use oxidict_core::schema::ConfigField;
use oxidict_core::{Error, Language, Result, TranslateRequest, TranslateResult};
use oxidict_net::{get_value, md5_hex, uuid_v4};
use serde_json::Value;

pub struct Baidu;

const ENDPOINT: &str = "https://fanyi-api.baidu.com/api/trans/vip/translate";

#[async_trait]
impl Translator for Baidu {
    fn id(&self) -> &str {
        "baidu"
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        vec![
            ConfigField::secret("appid", "AppID"),
            ConfigField::secret("secret", "Secret"),
        ]
    }

    fn map_language(&self, lang: Language) -> String {
        map_language!(lang, {
            ZhCn => "zh",
            ZhTw => "cht",
            En => "en",
            Ja => "jp",
            Ko => "kor",
            Fr => "fra",
            Es => "spa",
            Ru => "ru",
            De => "de",
            It => "it",
            Tr => "tr",
            PtPt => "pt",
            PtBr => "pot",
            Vi => "vie",
            Id => "id",
            Th => "th",
            Ms => "may",
            Ar => "ar",
            Hi => "hi",
            Km => "hkm",
            NbNo => "nob",
            NnNo => "nno",
            Fa => "per",
            Sv => "swe",
            Pl => "pl",
            Nl => "nl",
            Uk => "ukr",
            He => "heb",
        })
    }

    async fn translate(&self, req: TranslateRequest) -> Result<TranslateResult> {
        let appid = req.str("appid").unwrap_or("");
        let secret = req.str("secret").unwrap_or("");
        if appid.is_empty() || secret.is_empty() {
            return Err(Error::Service("Please configure appid and secret".into()));
        }

        let from = self.map_language(req.from);
        let to = self.map_language(req.to);
        let salt = uuid_v4();
        let sign = md5_hex(&format!("{}{}{}{}", appid, req.text, salt, secret));

        let url = format!(
            "{}?q={}&from={}&to={}&appid={}&salt={}&sign={}",
            ENDPOINT,
            urlencoding::encode(&req.text),
            from,
            to,
            appid,
            salt,
            sign,
        );

        let res: Value = get_value(url).await?;

        if let Some(trans) = res.get("trans_result").and_then(|v| v.as_array()) {
            let mut target = String::new();
            for item in trans {
                if let Some(dst) = item.get("dst").and_then(|v| v.as_str()) {
                    target.push_str(dst);
                    target.push('\n');
                }
            }
            Ok(TranslateResult::Plain(target.trim().to_string()))
        } else {
            Err(Error::Service(
                serde_json::to_string(&res).unwrap_or_default(),
            ))
        }
    }
}
