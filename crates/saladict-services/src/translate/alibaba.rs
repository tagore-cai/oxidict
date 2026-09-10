//! 阿里云机器翻译（通用版）。
//!
//! 移植自 `src/services/translate/alibaba/index.jsx`。
//! HMAC-SHA1 签名流程与 JS 侧 `crypto-js` 1:1 对应，使用
//! `saladict_net::sign` 提供的 `alibaba_escape` / `hmac_sha1_base64`。

use crate::Translator;
use async_trait::async_trait;
use saladict_core::HasConfig as _;
use saladict_core::map_language;
use saladict_core::schema::ConfigField;
use saladict_core::{Error, Language, Result, TranslateRequest, TranslateResult};
use saladict_net::{alibaba_escape, alibaba_nonce, get_value, hmac_sha1_base64, iso_utc_now};
use serde_json::Value;

pub struct Alibaba;

const ENDPOINT: &str = "http://mt.cn-hangzhou.aliyuncs.com/";
const URL_PATH: &str = "api/translate/web/general";

#[async_trait]
impl Translator for Alibaba {
    fn id(&self) -> &str {
        "alibaba"
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        vec![
            ConfigField::secret("accesskey_id", "AccessKey ID"),
            ConfigField::secret("accesskey_secret", "AccessKey Secret"),
        ]
    }

    fn map_language(&self, lang: Language) -> String {
        map_language!(lang, {
            ZhCn => "zh",
            ZhTw => "zh-tw",
            Ja => "ja",
            En => "en",
            Ko => "ko",
            Fr => "fr",
            Es => "es",
            Ru => "ru",
            De => "de",
            It => "it",
            Tr => "tr",
            PtPt => "pt",
            PtBr => "pt",
            Vi => "vi",
            Id => "id",
            Th => "th",
            Ms => "ms",
            Ar => "ar",
            Hi => "hi",
            MnMo => "mn",
            Km => "km",
            NbNo => "no",
            NnNo => "no",
            Fa => "fa",
            Sv => "sv",
            Pl => "pl",
            Nl => "nl",
            He => "he",
        })
    }

    async fn translate(&self, req: TranslateRequest) -> Result<TranslateResult> {
        // 原 JS：accesskey_id / accesskey_secret 为空时 throw 固定文案。
        let accesskey_id = req.str("accesskey_id").unwrap_or("");
        let accesskey_secret = req.str("accesskey_secret").unwrap_or("");
        if accesskey_id.is_empty() || accesskey_secret.is_empty() {
            return Err(Error::Service(
                "Please configure AccessKey ID and AccessKey Secret".into(),
            ));
        }

        let from = self.map_language(req.from);
        let to = self.map_language(req.to);
        let timestamp = iso_utc_now();
        let nonce = alibaba_nonce();

        // 与 JS 一致：text / timestamp 先用 encodeURIComponent 编码，其余参数原样拼接，
        // 之后再对整串做二次转义（alibaba_escape）。
        let query = format!(
            "AccessKeyId={}&Action=TranslateGeneral&Format=JSON&FormatType=text&Scene=general\
             &SignatureMethod=HMAC-SHA1&SignatureNonce={}&SignatureVersion=1.0&SourceLanguage={}\
             &SourceText={}&TargetLanguage={}&Timestamp={}&Version=2018-10-12",
            accesskey_id,
            nonce,
            from,
            urlencoding::encode(&req.text),
            to,
            urlencoding::encode(&timestamp),
        );

        // stringToSign = "GET&" + encodeURIComponent("/") + "&" + encodeURIComponent(query) + 替换
        let string_to_sign = format!(
            "GET&{}&{}",
            urlencoding::encode("/"),
            alibaba_escape(&query)
        );

        // HmacSHA1(stringToSign, accesskey_secret + "&")
        let signature =
            hmac_sha1_base64(format!("{}&", accesskey_secret).as_bytes(), &string_to_sign);

        let url = format!(
            "{}{}?{}&Signature={}",
            ENDPOINT,
            URL_PATH,
            query,
            urlencoding::encode(&signature)
        );

        let res: Value = get_value(url).await?;

        // get_value 内部已经 check()（非 2xx 转 Http 错误）；这里再核对业务 Code。
        let code = res.get("Code").and_then(|v| v.as_str()).unwrap_or("");
        if code == "200" {
            let translated = res
                .get("Data")
                .and_then(|d| d.get("Translated"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            Ok(TranslateResult::Plain(translated))
        } else {
            Err(Error::Service(
                serde_json::to_string(&res).unwrap_or_default(),
            ))
        }
    }
}
