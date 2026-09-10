//! 讯飞公式识别（LaTeX）。
//!
//! 移植自 `src/services/recognize/iflytek_latex/index.jsx`。
//! 走 rest-api.xfyun.cn 的 digest 签名：在 `host date request-line digest` 上做 HMAC-SHA256。

use crate::Recognizer;
use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use chrono::Utc;
use saladict_core::map_language;
use saladict_core::schema::ConfigField;
use saladict_core::HasConfig as _;
use saladict_core::{Error, Language, RecognizeRequest, Result};
use saladict_net::NetErr as _;
use saladict_net::{check, hmac_sha256_base64, post_with_headers};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub struct IflytekLatex;

const URL: &str = "https://rest-api.xfyun.cn/v2/itr";
const HOST: &str = "rest-api.xfyun.cn";

#[async_trait]
impl Recognizer for IflytekLatex {
    fn id(&self) -> &str {
        "iflytek_latex"
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        vec![
            ConfigField::secret("appid", "App ID"),
            ConfigField::secret("apisecret", "API Secret"),
            ConfigField::secret("apikey", "API Key"),
        ]
    }

    fn map_language(&self, lang: Language) -> String {
        map_language!(lang, {
            Auto => "zh_cn",
            ZhCn => "zh_cn",
            ZhTw => "zh_tw",
            En => "en",
        })
    }

    async fn recognize(&self, req: RecognizeRequest) -> Result<String> {
        let appid = req.require_str("appid", "iFlyTek App ID")?;
        let apisecret = req.require_str("apisecret", "iFlyTek API Secret")?;
        let apikey = req.require_str("apikey", "iFlyTek API Key")?;

        let b64 = B64.encode(&req.image);
        let body = serde_json::json!({
            "common": { "app_id": appid },
            "business": { "ent": "teach-photo-print", "aue": "raw" },
            "data": { "image": b64 }
        });
        let payload = serde_json::to_string(&body)
            .map_err(|e| Error::Service(format!("序列化请求体失败: {e}")))?;

        let date = Utc::now().format("%a, %d %b %Y %H:%M:%S GMT").to_string();
        let request_line = "POST /v2/itr HTTP/1.1";

        // digest = base64(sha256(payload))
        let mut hasher = Sha256::new();
        hasher.update(payload.as_bytes());
        let digest = format!("SHA-256={}", B64.encode(hasher.finalize()));

        let signature_origin = format!(
            "host: {}\ndate: {}\n{}\ndigest: {}",
            HOST, date, request_line, digest
        );
        let signature = hmac_sha256_base64(apisecret.as_bytes(), &signature_origin);
        let authorization = format!(
            "api_key=\"{}\", algorithm=\"hmac-sha256\", headers=\"host date request-line digest\", signature=\"{}\"",
            apikey, signature
        );

        let resp = post_with_headers(
            URL,
            &[
                ("Content-Type", "application/json"),
                ("Accept", "application/json,version=1.0"),
                ("Host", HOST),
                ("Date", &date),
                ("Digest", &digest),
                ("Authorization", &authorization),
            ],
        )
        .body(payload)
        .send()
        .await
        .net_err()?;
        let result: Value = check(resp).await?.json().await.net_err()?;

        let regions = result
            .get("data")
            .and_then(|d| d.get("region"))
            .and_then(|v| v.as_array())
            .ok_or_else(|| Error::Service(serde_json::to_string(&result).unwrap_or_default()))?;

        let mut target = String::new();
        for region in regions {
            if let Some(content) = region
                .get("recog")
                .and_then(|r| r.get("content"))
                .and_then(|v| v.as_str())
            {
                target.push_str(content);
                target.push('\n');
            }
        }
        let target = target
            .replace(" ifly-latex-begin ", "")
            .replace(" ifly-latex-end ", "");
        Ok(target.trim().to_string())
    }
}
