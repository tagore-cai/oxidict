//! 讯飞开放平台文字识别（中英）。
//!
//! 移植自 `src/services/recognize/iflytek/index.jsx`。
//! 走讯飞特有签名：对 `host/date/request-line` 做 HMAC-SHA256，base64 后放入 Authorization。

use crate::Recognizer;
use saladict_core::HasConfig as _;
use saladict_net::NetErr as _;
use base64::Engine as _;
use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use chrono::Utc;
use saladict_core::map_language;
use saladict_core::schema::ConfigField;
use saladict_core::{Error, Language, Result, RecognizeRequest};
use saladict_net::{check, hmac_sha256_base64, percent_encode, post_with_headers};
use serde_json::Value;

pub struct Iflytek;

const HOST: &str = "api.xf-yun.com";
const PATH: &str = "/v1/private/sf8e6aca1";
const SERVICE: &str = "sf8e6aca1";

/// 讯飞 HMAC-SHA256 请求头签名，等价于 JS `iflytek_auth`。
fn iflytek_auth(api_key: &str, api_secret: &str, host: &str, date: &str, request_line: &str) -> String {
    let signature_origin = format!("host: {}\ndate: {}\n{}", host, date, request_line);
    let signature = hmac_sha256_base64(api_secret.as_bytes(), &signature_origin);
    let authorization_origin = format!(
        "api_key=\"{}\", algorithm=\"hmac-sha256\", headers=\"host date request-line\", signature=\"{}\"",
        api_key, signature
    );
    B64.encode(authorization_origin.as_bytes())
}

#[async_trait]
impl Recognizer for Iflytek {
    fn id(&self) -> &str {
        "iflytek"
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

        let date = Utc::now().format("%a, %d %b %Y %H:%M:%S GMT").to_string();
        let request_line = "POST /v1/private/sf8e6aca1 HTTP/1.1";
        let auth = iflytek_auth(&apikey, &apisecret, HOST, &date, request_line);

        let b64 = B64.encode(&req.image);
        let request_body = serde_json::json!({
            "header": { "app_id": appid, "status": 3 },
            "parameter": {
                SERVICE: {
                    "category": "ch_en_public_cloud",
                    "result": { "encoding": "utf8", "compress": "raw", "format": "json" },
                }
            },
            "payload": {
                format!("{}_data_1", SERVICE): { "image": b64 }
            }
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
            .send().await.net_err()?;
        let data: Value = check(resp).await?.json().await.net_err()?;

        // 响应 payload 内 result.text 为 base64，解码后是 JSON 的 pages/lines/words。
        let text_b64 = data
            .get("payload")
            .and_then(|p| p.get(SERVICE))
            .and_then(|s| s.get("result"))
            .and_then(|r| r.get("text"))
            .and_then(|t| t.as_str())
            .ok_or_else(|| Error::Service("Result payload not found".into()))?;
        let text_string = String::from_utf8_lossy(&B64.decode(text_b64).map_err(|e| Error::Service(format!("base64 解码失败: {e}")))?).to_string();
        let text_json: Value = serde_json::from_str(&text_string)
            .map_err(|e| Error::Service(format!("解析识别结果失败: {e}")))?;

        let mut out = String::new();
        if let Some(pages) = text_json.get("pages").and_then(|v| v.as_array()) {
            for page in pages {
                if let Some(lines) = page.get("lines").and_then(|v| v.as_array()) {
                    for line in lines {
                        if let Some(words) = line.get("words").and_then(|v| v.as_array()) {
                            for word in words {
                                if let Some(content) = word.get("content").and_then(|v| v.as_str()) {
                                    out.push_str(content);
                                    out.push(' ');
                                }
                            }
                            out.push('\n');
                        }
                    }
                }
            }
        }
        Ok(out.trim().to_string())
    }
}
