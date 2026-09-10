//! 腾讯云通用印刷体高精度 OCR。
//!
//! 移植自 `src/services/recognize/tencent_accurate/index.jsx`。
//! 签名走 TC3-HMAC-SHA256（密钥派生链 `HMAC(TC3+secret, date) -> service -> tc3_request`），
//! 与 `oxidict_net::tc3_sign` 一致。响应取 `Response.TextDetections[].DetectedText` 拼接。

use crate::Recognizer;
use async_trait::async_trait;
use base64::Engine as _;
use oxidict_core::HasConfig as _;
use oxidict_core::map_language;
use oxidict_core::schema::ConfigField;
use oxidict_core::{Error, Language, RecognizeRequest, Result};
use oxidict_net::{post_with_headers, sha256_hex, tc3_sign};
use serde_json::Value;

pub struct TencentAccurate;

const ENDPOINT: &str = "ocr.tencentcloudapi.com";
const SERVICE: &str = "ocr";
const REGION: &str = "ap-beijing";
const ACTION: &str = "GeneralAccurateOCR";
const VERSION: &str = "2018-11-19";

#[async_trait]
impl Recognizer for TencentAccurate {
    fn id(&self) -> &str {
        "tencent_accurate"
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        vec![
            ConfigField::secret("secret_id", "Secret ID"),
            ConfigField::secret("secret_key", "Secret Key"),
        ]
    }

    fn map_language(&self, lang: Language) -> String {
        // 请求体不使用语言参数，这里按 info.ts 原样映射以备他用。
        map_language!(lang, {
            Auto => "auto",
            ZhCn => "zh",
            ZhTw => "zh_rare",
            En => "auto",
        })
    }

    async fn recognize(&self, req: RecognizeRequest) -> Result<String> {
        let secret_id = req.require_str("secret_id", "Tencent Secret ID")?;
        let secret_key = req.require_str("secret_key", "Tencent Secret Key")?;

        let now = chrono::Utc::now();
        let timestamp = now.timestamp();
        let date = now.format("%Y-%m-%d").to_string();

        let b64 = base64::engine::general_purpose::STANDARD.encode(&req.image);
        let payload = format!("{{\"ImageBase64\":\"{}\"}}", b64);
        let hashed_payload = sha256_hex(payload.as_bytes());

        let canonical_headers = format!("content-type:application/json\nhost:{ENDPOINT}\n");
        let signed_headers = "content-type;host";
        let canonical_request =
            format!("POST\n/\n\n{canonical_headers}\n{signed_headers}\n{hashed_payload}");

        let credential_scope = format!("{date}/{SERVICE}/tc3_request");
        let string_to_sign_prefix = format!("TC3-HMAC-SHA256\n{timestamp}\n{credential_scope}");
        let signature = tc3_sign(
            &secret_key,
            &date,
            SERVICE,
            &canonical_request,
            &string_to_sign_prefix,
        );

        let authorization = format!(
            "TC3-HMAC-SHA256 Credential={secret_id}/{credential_scope}, SignedHeaders={signed_headers}, Signature={signature}"
        );

        let url = format!("https://{ENDPOINT}");
        let resp = post_with_headers(
            &url,
            &[
                ("Authorization", &authorization),
                ("content-type", "application/json"),
                ("X-TC-Action", ACTION),
                ("X-TC-Timestamp", &timestamp.to_string()),
                ("X-TC-Version", VERSION),
                ("X-TC-Region", REGION),
            ],
        )
        .body(payload)
        .send()
        .await
        .map_err(|e| Error::Service(format!("Http Request Error\n{e}")))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Service(format!(
                "Http Request Error\nHttp Status: {}\n{}",
                status.as_u16(),
                body
            )));
        }

        let result: Value = resp
            .json()
            .await
            .map_err(|e| Error::Service(format!("Http Request Error\n{e}")))?;

        let detections = result
            .get("Response")
            .and_then(|r| r.get("TextDetections"))
            .and_then(|v| v.as_array());
        match detections {
            Some(arr) => {
                let mut target = String::new();
                for item in arr {
                    if let Some(t) = item.get("DetectedText").and_then(|v| v.as_str()) {
                        target.push_str(t);
                        target.push('\n');
                    }
                }
                Ok(target.trim().to_string())
            }
            None => Err(Error::Service(
                serde_json::to_string(&result).unwrap_or_default(),
            )),
        }
    }
}
