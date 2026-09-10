//! 腾讯云图片翻译 OCR。
//!
//! 移植自 `src/services/recognize/tencent_img/index.jsx`。
//! 与高精度 OCR 同族签名（TC3-HMAC-SHA256），但 action 为 `ImageTranslate`，
//! 调用的是 `tmt` 服务。返回的 `ImageRecord.Value` 同时含源/目标文本，按语言自动决定取哪一类。

use crate::Recognizer;
use async_trait::async_trait;
use base64::Engine as _;
use saladict_core::HasConfig as _;
use saladict_core::map_language;
use saladict_core::schema::ConfigField;
use saladict_core::{Error, Language, RecognizeRequest, Result};
use saladict_net::{post_with_headers, sha256_hex, tc3_sign, uuid_v4};
use serde_json::Value;

pub struct TencentImg;

const ENDPOINT: &str = "tmt.tencentcloudapi.com";
const SERVICE: &str = "tmt";
const REGION: &str = "ap-beijing";
const ACTION: &str = "ImageTranslate";
const VERSION: &str = "2018-03-21";

#[async_trait]
impl Recognizer for TencentImg {
    fn id(&self) -> &str {
        "tencent_img"
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
            ZhTw => "zh-TW",
            En => "en",
            Ja => "ja",
            Ko => "ko",
            Fr => "fr",
            Es => "es",
            Ru => "ru",
            De => "de",
            It => "it",
            PtPt => "pt",
            PtBr => "pt",
            Vi => "vi",
            Th => "th",
            Ms => "ms",
        })
    }

    async fn recognize(&self, req: RecognizeRequest) -> Result<String> {
        let secret_id = req.require_str("secret_id", "Tencent Secret ID")?;
        let secret_key = req.require_str("secret_key", "Tencent Secret Key")?;

        let now = chrono::Utc::now();
        let timestamp = now.timestamp();
        let date = now.format("%Y-%m-%d").to_string();

        let mapped = self.map_language(req.language);
        let target = if mapped == "auto" {
            "zh".to_string()
        } else {
            mapped.clone()
        };

        let b64 = base64::engine::general_purpose::STANDARD.encode(&req.image);
        let payload = format!(
            "{{\"SessionUuid\":\"{}\",\"Scene\":\"doc\",\"Data\":\"{}\",\"Source\":\"auto\",\"Target\":\"{}\",\"ProjectId\":0}}",
            uuid_v4(),
            b64,
            target
        );
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

        let value = result
            .get("Response")
            .and_then(|r| r.get("ImageRecord"))
            .and_then(|r| r.get("Value"))
            .and_then(|v| v.as_array());
        match value {
            Some(arr) => {
                let mut source = String::new();
                let mut target_txt = String::new();
                for item in arr {
                    if let Some(t) = item.get("SourceText").and_then(|v| v.as_str()) {
                        source.push_str(t);
                        source.push('\n');
                    }
                    if let Some(t) = item.get("TargetText").and_then(|v| v.as_str()) {
                        target_txt.push_str(t);
                        target_txt.push('\n');
                    }
                }
                if mapped == "auto" {
                    Ok(source.trim().to_string())
                } else {
                    Ok(target_txt.trim().to_string())
                }
            }
            None => Err(Error::Service(
                serde_json::to_string(&result).unwrap_or_default(),
            )),
        }
    }
}
