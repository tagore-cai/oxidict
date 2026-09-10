//! 讯飞开放平台手写/文档识别（Intsig）。
//!
//! 移植自 `src/services/recognize/iflytek_intsig/index.jsx`。
//! 复用讯飞 `iflytek_auth` 签名，接口为 `hh_ocr_recognize_doc`，结果取 `whole_text`。

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
use saladict_net::{check, hmac_sha256_base64, percent_encode, post_with_headers};
use serde_json::Value;

pub struct IflytekIntsig;

const HOST: &str = "api.xf-yun.com";
const PATH: &str = "/v1/private/hh_ocr_recognize_doc";
const SERVICE: &str = "hh_ocr_recognize_doc";

fn iflytek_auth(
    api_key: &str,
    api_secret: &str,
    host: &str,
    date: &str,
    request_line: &str,
) -> String {
    let signature_origin = format!("host: {}\ndate: {}\n{}", host, date, request_line);
    let signature = hmac_sha256_base64(api_secret.as_bytes(), &signature_origin);
    let authorization_origin = format!(
        "api_key=\"{}\", algorithm=\"hmac-sha256\", headers=\"host date request-line\", signature=\"{}\"",
        api_key, signature
    );
    B64.encode(authorization_origin.as_bytes())
}

#[async_trait]
impl Recognizer for IflytekIntsig {
    fn id(&self) -> &str {
        "iflytek_intsig"
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
            Auto => "auto",
            ZhCn => "zh_cn",
            ZhTw => "zh_tw",
            En => "en",
            Ja => "ja",
            Ko => "ko",
            Fr => "fr",
            Es => "es",
            Ru => "ru",
            De => "de",
            It => "it",
            Tr => "tr",
            PtPt => "pt_pt",
            PtBr => "pt_br",
            Vi => "vi",
            Id => "id",
            Th => "th",
            Ms => "ms",
            Ar => "ar",
            Hi => "hi",
        })
    }

    async fn recognize(&self, req: RecognizeRequest) -> Result<String> {
        let appid = req.require_str("appid", "iFlyTek App ID")?;
        let apisecret = req.require_str("apisecret", "iFlyTek API Secret")?;
        let apikey = req.require_str("apikey", "iFlyTek API Key")?;

        let date = Utc::now().format("%a, %d %b %Y %H:%M:%S GMT").to_string();
        let request_line = "POST /v1/private/hh_ocr_recognize_doc HTTP/1.1";
        let auth = iflytek_auth(&apikey, &apisecret, HOST, &date, request_line);

        let b64 = B64.encode(&req.image);
        let request_body = serde_json::json!({
            "header": { "app_id": appid, "status": 3 },
            "parameter": {
                SERVICE: {
                    "recognizeDocumentRes": { "encoding": "utf8", "compress": "raw", "format": "json" },
                }
            },
            "payload": { "image": { "image": b64 } }
        });
        let payload = serde_json::to_string(&request_body)
            .map_err(|e| Error::Service(format!("序列化请求体失败: {e}")))?;

        let url = format!(
            "https://{}{}?authorization={}&host={}&date={}",
            HOST,
            PATH,
            auth,
            HOST,
            percent_encode(&date)
        );

        let resp = post_with_headers(&url, &[("content-type", "application/json")])
            .body(payload)
            .send()
            .await
            .net_err()?;
        let data: Value = check(resp).await?.json().await.net_err()?;

        let text_b64 = data
            .get("payload")
            .and_then(|p| p.get(SERVICE))
            .and_then(|s| s.get("text"))
            .and_then(|t| t.as_str())
            .ok_or_else(|| Error::Service("Result payload not found".into()))?;
        let text_string = String::from_utf8_lossy(
            &B64.decode(text_b64)
                .map_err(|e| Error::Service(format!("base64 解码失败: {e}")))?,
        )
        .to_string();
        let text_json: Value = serde_json::from_str(&text_string)
            .map_err(|e| Error::Service(format!("解析识别结果失败: {e}")))?;

        let whole = text_json
            .get("whole_text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        Ok(whole.trim().to_string())
    }
}
