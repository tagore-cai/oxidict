//! 腾讯云通用印刷体识别（GeneralBasicOCR）。
//!
//! 移植自 `src/services/recognize/tencent/index.jsx`。
//! 签名用 `oxidict_net::sign::tc3_sign`（TC3-HMAC-SHA256 链式派生）。

use crate::Recognizer;
use async_trait::async_trait;
use base64::Engine as _;
use chrono::Utc;
use oxidict_core::HasConfig as _;
use oxidict_core::map_language;
use oxidict_core::schema::ConfigField;
use oxidict_core::{Error, Language, RecognizeRequest, Result};
use oxidict_net::NetErr as _;
use oxidict_net::{check, post_with_headers, sha256_hex, tc3_sign};
use serde_json::Value;

pub struct Tencent;

const ENDPOINT: &str = "ocr.tencentcloudapi.com";
const SERVICE: &str = "ocr";
const REGION: &str = "ap-beijing";
const ACTION: &str = "GeneralBasicOCR";
const VERSION: &str = "2018-11-19";

#[async_trait]
impl Recognizer for Tencent {
    fn id(&self) -> &str {
        "tencent"
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        vec![
            ConfigField::secret("secret_id", "Secret ID"),
            ConfigField::secret("secret_key", "Secret Key"),
        ]
    }

    fn map_language(&self, lang: Language) -> String {
        map_language!(lang, {
            Auto => "auto",
            ZhCn => "zh",
            ZhTw => "zh_rare",
            En => "auto",
            Ja => "jap",
            Ko => "kor",
            Fr => "fre",
            Es => "spa",
            Ru => "rus",
            De => "ger",
            It => "ita",
            PtPt => "por",
            PtBr => "por",
            Vi => "vie",
            Th => "tha",
            Ms => "may",
            Ar => "ara",
            Hi => "hi",
        })
    }

    async fn recognize(&self, req: RecognizeRequest) -> Result<String> {
        let secret_id = req.require_str("secret_id", "Tencent Secret ID")?;
        let secret_key = req.require_str("secret_key", "Tencent Secret Key")?;

        let now = Utc::now();
        let timestamp = now.timestamp();
        let date = now.format("%Y-%m-%d").to_string();

        let b64 = base64::engine::general_purpose::STANDARD.encode(&req.image);
        let lang = self.map_language(req.language);
        let body = serde_json::json!({ "ImageBase64": b64, "LanguageType": lang });
        let payload = serde_json::to_string(&body)
            .map_err(|e| Error::Service(format!("序列化请求体失败: {e}")))?;
        let hashed_payload = sha256_hex(payload.as_bytes());

        let canonical_request = format!(
            "POST\n/\n\ncontent-type:application/json\nhost:{}\n\ncontent-type;host\n{}",
            ENDPOINT, hashed_payload
        );
        let credential_scope = format!("{}/{}/tc3_request", date, SERVICE);
        let string_to_sign_prefix = format!("TC3-HMAC-SHA256\n{}\n{}", timestamp, credential_scope);
        let signature = tc3_sign(
            &secret_key,
            &date,
            SERVICE,
            &canonical_request,
            &string_to_sign_prefix,
        );
        let authorization = format!(
            "TC3-HMAC-SHA256 Credential={}/{}/tc3_request, SignedHeaders=content-type;host, Signature={}",
            secret_id, date, signature
        );

        let resp = post_with_headers(
            format!("https://{}", ENDPOINT),
            &[
                ("Authorization", &authorization),
                ("content-type", "application/json"),
                ("Host", ENDPOINT),
                ("X-TC-Action", ACTION),
                ("X-TC-Timestamp", &timestamp.to_string()),
                ("X-TC-Version", VERSION),
                ("X-TC-Region", REGION),
            ],
        )
        .body(payload)
        .send()
        .await
        .net_err()?;
        let result: Value = check(resp).await?.json().await.net_err()?;

        let detections = result
            .get("Response")
            .and_then(|r| r.get("TextDetections"))
            .and_then(|v| v.as_array())
            .ok_or_else(|| Error::Service(serde_json::to_string(&result).unwrap_or_default()))?;

        let mut target = String::new();
        for d in detections {
            if let Some(t) = d.get("DetectedText").and_then(|v| v.as_str()) {
                target.push_str(t);
                target.push('\n');
            }
        }
        Ok(target.trim().to_string())
    }
}
