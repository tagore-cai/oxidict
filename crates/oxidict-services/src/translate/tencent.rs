//! 腾讯翻译君。
//!
//! 移植自 `src/services/translate/tencent/index.jsx`，TC3-HMAC-SHA256 签名。

use crate::Translator;
use async_trait::async_trait;
use oxidict_core::map_language;
use oxidict_core::schema::ConfigField;
use oxidict_core::{Error, HasConfig, Language, Result, TranslateRequest, TranslateResult};
use serde_json::{Value, json};

pub struct Tencent;

const ENDPOINT: &str = "tmt.tencentcloudapi.com";
const ACTION: &str = "TextTranslate";
const VERSION: &str = "2018-03-21";
const REGION: &str = "ap-beijing";

#[async_trait]
impl Translator for Tencent {
    fn id(&self) -> &str {
        "tencent"
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        vec![
            ConfigField::Text {
                key: "secret_id",
                label: "Secret ID",
                placeholder: "",
                secret: false,
                required: true,
            },
            ConfigField::Text {
                key: "secret_key",
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
            ZhTw => "zh-TW",
        })
    }

    async fn translate(&self, req: TranslateRequest) -> Result<TranslateResult> {
        use oxidict_net::NetErr as _;

        let secret_id = req.require_str("secret_id", "腾讯云 Secret ID")?;
        let secret_key = req.require_str("secret_key", "腾讯云 Secret Key")?;

        let timestamp = chrono::Utc::now().timestamp();
        let date = chrono::Utc::now().format("%Y-%m-%d").to_string();

        let body = json!({
            "SourceText": req.text,
            "Source": self.map_language(req.from),
            "Target": self.map_language(req.to),
            "ProjectId": 0,
        });
        // 请求体必须与参与签名的 payload 完全一致，所以先序列化再原样发送。
        let payload = serde_json::to_string(&body)?;

        let payload_hash = oxidict_net::sha256_hex(payload.as_bytes());
        // 注意：canonicalHeaders 自带换行，其后还要再跟一个空行，与原实现逐字符一致。
        let canonical_request = format!(
            "POST\n/\n\ncontent-type:application/json\nhost:{ENDPOINT}\n\ncontent-type;host\n{payload_hash}"
        );

        let string_to_sign_prefix = format!("TC3-HMAC-SHA256\n{timestamp}\n{date}/tmt/tc3_request");
        let signature = oxidict_net::tc3_sign(
            &secret_key,
            &date,
            "tmt",
            &canonical_request,
            &string_to_sign_prefix,
        );
        let credential_scope = format!("{date}/tmt/tc3_request");
        let authorization = format!(
            "TC3-HMAC-SHA256 Credential={secret_id}/{credential_scope}, \
             SignedHeaders=content-type;host, Signature={signature}"
        );

        let resp = oxidict_net::client()
            .post(format!("https://{ENDPOINT}"))
            .header("Authorization", authorization)
            .header("Content-Type", "application/json")
            .header("X-TC-Action", ACTION)
            .header("X-TC-Timestamp", timestamp.to_string())
            .header("X-TC-Version", VERSION)
            .header("X-TC-Region", REGION)
            .body(payload)
            .send()
            .await
            .net_err()?;
        let resp = oxidict_net::check(resp).await?;
        let result: Value = resp.json().await.net_err()?;

        let target = result
            .pointer("/Response/TargetText")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::Service(result.to_string()))?
            .trim()
            .to_string();
        Ok(TranslateResult::Plain(target))
    }
}
