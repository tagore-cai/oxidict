//! 火山引擎通用 OCR。
//!
//! 移植自 `src/services/recognize/volcengine/index.jsx`。
//! 签名与火山翻译同族（密钥派生链首环为 `HMAC(secret, date)`，无 AWS4 前缀），
//! 走 `visual.volcengineapi.com`。响应取 `data.line_texts` 拼接。

use crate::Recognizer;
use async_trait::async_trait;
use base64::Engine as _;
use saladict_core::map_language;
use saladict_core::schema::ConfigField;
use saladict_core::HasConfig as _;
use saladict_core::{Error, Language, RecognizeRequest, Result};
use saladict_net::{
    aws_date, aws_now, hmac_sha256_hex, hmac_sha256_raw, post_with_headers, sha256_hex,
};
use serde_json::Value;
use urlencoding::encode as url_encode;

pub struct Volcengine;

const HOST: &str = "visual.volcengineapi.com";
const SERVICE: &str = "cv";
const REGION: &str = "cn-north-1";
const CONTENT_TYPE: &str = "application/x-www-form-urlencoded";

#[async_trait]
impl Recognizer for Volcengine {
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
            ConfigField::secret("secret", "Secret Key"),
        ]
    }

    fn map_language(&self, lang: Language) -> String {
        map_language!(lang, {
            Auto => "auto",
            ZhCn => "zh_cn",
            ZhTw => "zh_tw",
            En => "en",
        })
    }

    async fn recognize(&self, req: RecognizeRequest) -> Result<String> {
        let appid = req.require_str("appid", "Volcengine App ID")?;
        let secret = req.require_str("secret", "Volcengine Secret Key")?;

        let b64 = base64::engine::general_purpose::STANDARD.encode(&req.image);
        let value = volcengine_query(&appid, &secret, "OCRNormal", "2020-08-26", &b64).await?;

        let data = value
            .get("data")
            .ok_or_else(|| Error::Service(serde_json::to_string(&value).unwrap_or_default()))?;
        let line_texts = data
            .get("line_texts")
            .and_then(|v| v.as_array())
            .ok_or_else(|| Error::Service(serde_json::to_string(&value).unwrap_or_default()))?;

        let mut texts = String::new();
        for t in line_texts {
            if let Some(s) = t.as_str() {
                texts.push_str(s);
                texts.push('\n');
            }
        }
        Ok(texts.trim().to_string())
    }
}

/// 火山 OCR 的签名与请求发送；`action`/`version` 区分普通与多语种接口。
async fn volcengine_query(
    appid: &str,
    secret: &str,
    action: &str,
    version: &str,
    image_b64: &str,
) -> Result<Value> {
    let body = format!(
        "image_base64={}&approximate_pixel=0&mode=default&filter_thresh=80",
        url_encode(image_b64)
    );
    let body_hash = sha256_hex(body.as_bytes());

    let x_date = aws_now();
    let date = aws_date();

    let credential_scope = format!("{date}/{REGION}/{SERVICE}/request");
    let signed_headers = "content-type;host;x-content-sha256;x-date";

    let signed_str = format!(
        "content-type:{CONTENT_TYPE}\nhost:{HOST}\nx-content-sha256:{body_hash}\nx-date:{x_date}\n"
    );

    let canonical_request = format!(
        "POST\n/\nAction={action}&Version={version}\n{signed_str}\n{signed_headers}\n{body_hash}"
    );
    let hashed_canonical = sha256_hex(canonical_request.as_bytes());

    // 密钥派生链：kDate = HMAC(secret, date)，无 AWS4 前缀。
    let k_date = hmac_sha256_raw(secret.as_bytes(), date.as_bytes());
    let k_region = hmac_sha256_raw(&k_date, REGION.as_bytes());
    let k_service = hmac_sha256_raw(&k_region, SERVICE.as_bytes());
    let signing_key = hmac_sha256_raw(&k_service, b"request");

    let signing_str = format!("HMAC-SHA256\n{x_date}\n{credential_scope}\n{hashed_canonical}");
    let sign = hmac_sha256_hex(&signing_key, &signing_str);

    let authorization = format!(
        "HMAC-SHA256 Credential={appid}/{credential_scope}, SignedHeaders={signed_headers}, Signature={sign}"
    );

    let url = format!("https://{HOST}/?Action={action}&Version={version}");
    let resp = post_with_headers(&url, &[("Content-Type", CONTENT_TYPE)])
        .header("X-Date", &x_date)
        .header("X-Content-Sha256", &body_hash)
        .header("Authorization", &authorization)
        .body(body)
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

    resp.json()
        .await
        .map_err(|e| Error::Service(format!("Http Request Error\n{e}")))
}
