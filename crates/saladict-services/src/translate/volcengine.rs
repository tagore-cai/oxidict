//! 火山翻译。
//!
//! 移植自 `src/services/translate/volcengine/index.jsx`。
//! 签名与 AWS SigV4 同族，但密钥派生链首环是 `HMAC(secret, date)`（没有 AWS4 前缀），
//! 所以不走 `aws_sign`，在这里按原实现逐步构造。

use crate::Translator;
use async_trait::async_trait;
use saladict_core::map_language;
use saladict_core::schema::ConfigField;
use saladict_core::{Error, HasConfig, Language, Result, TranslateRequest, TranslateResult};
use serde_json::{Value, json};

pub struct Volcengine;

const HOST: &str = "open.volcengineapi.com";
const SERVICE: &str = "translate";
const REGION: &str = "cn-north-1";
const SERVICE_VERSION: &str = "2020-06-01";

#[async_trait]
impl Translator for Volcengine {
    fn id(&self) -> &str {
        "volcengine"
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        vec![
            ConfigField::Text {
                key: "appid",
                label: "App ID",
                placeholder: "",
                secret: false,
                required: true,
            },
            ConfigField::Text {
                key: "secret",
                label: "Secret Key",
                placeholder: "",
                secret: true,
                required: true,
            },
        ]
    }

    fn map_language(&self, lang: Language) -> String {
        map_language!(lang, {
            ZhCn => "zh",
            ZhTw => "zh-Hans",
        })
    }

    async fn translate(&self, req: TranslateRequest) -> Result<TranslateResult> {
        use saladict_net::NetErr as _;

        let appid = req.require_str("appid", "火山翻译 App ID")?;
        let secret = req.require_str("secret", "火山翻译 Secret Key")?;

        let body = json!({
            "TargetLanguage": self.map_language(req.to),
            "TextList": [req.text],
        });
        let body_str = serde_json::to_string(&body)?;
        let body_hash = saladict_net::sha256_hex(body_str.as_bytes());

        let format_date = saladict_net::aws_now();
        let short_date = saladict_net::aws_date();

        // 参与签名的头按小写字母序排列，与原实现一致。
        let signed_headers = "content-type;host;x-content-sha256;x-date";
        let canonical_headers = format!(
            "content-type:application/json\nhost:{HOST}\nx-content-sha256:{body_hash}\nx-date:{format_date}\n"
        );

        let norm_query = format!("Action=TranslateText&Version={SERVICE_VERSION}");
        let canonical_request =
            format!("POST\n/\n{norm_query}\n{canonical_headers}\n{signed_headers}\n{body_hash}");
        let hashed_canonical = saladict_net::sha256_hex(canonical_request.as_bytes());

        let credential_scope = format!("{short_date}/{REGION}/{SERVICE}/request");
        let string_to_sign =
            format!("HMAC-SHA256\n{format_date}\n{credential_scope}\n{hashed_canonical}");

        // 密钥派生链：kDate = HMAC(secret, date)，注意没有 AWS4 前缀。
        let k_date = saladict_net::hmac_sha256_raw(secret.as_bytes(), short_date.as_bytes());
        let k_region = saladict_net::hmac_sha256_raw(&k_date, REGION.as_bytes());
        let k_service = saladict_net::hmac_sha256_raw(&k_region, SERVICE.as_bytes());
        let signing_key = saladict_net::hmac_sha256_raw(&k_service, b"request");
        let signature = saladict_net::hmac_sha256_hex(&signing_key, &string_to_sign);

        let authorization = format!(
            "HMAC-SHA256 Credential={appid}/{credential_scope}, SignedHeaders={signed_headers}, Signature={signature}"
        );

        let url = format!("https://{HOST}/?{norm_query}");
        let resp = saladict_net::client()
            .post(&url)
            .header("Authorization", authorization)
            .header("Content-Type", "application/json")
            .header("X-Content-Sha256", &body_hash)
            .header("X-Date", &format_date)
            .body(body_str)
            .send()
            .await
            .net_err()?;
        let resp = saladict_net::check(resp).await?;
        let result: Value = resp.json().await.net_err()?;

        let list = result
            .get("TranslationList")
            .and_then(|v| v.as_array())
            .ok_or_else(|| Error::Service(result.to_string()))?;
        let target = list
            .iter()
            .filter_map(|item| item.get("Translation").and_then(|v| v.as_str()))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string();
        Ok(TranslateResult::Plain(target))
    }
}
