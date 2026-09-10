//! 百度图文翻译（图片翻译）。
//!
//! 移植自 `src/services/recognize/baidu_img/index.jsx`。
//! 原实现读取缓存里的截图 `pot_screenshot_cut.png` 计算签名；这里直接复用传入图片字节。

use crate::Recognizer;
use async_trait::async_trait;
use oxidict_core::HasConfig as _;
use oxidict_core::map_language;
use oxidict_core::schema::ConfigField;
use oxidict_core::{Error, Language, RecognizeRequest, Result};
use oxidict_net::NetErr as _;
use oxidict_net::{check, md5_hex, md5_hex_bytes, post_with_headers, uuid_v4};
use serde_json::Value;

pub struct BaiduImg;

const URL: &str = "https://fanyi-api.baidu.com/api/trans/sdk/picture";

#[async_trait]
impl Recognizer for BaiduImg {
    fn id(&self) -> &str {
        "baidu_img"
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        vec![
            ConfigField::secret("appid", "App ID"),
            ConfigField::secret("secret", "Secret"),
        ]
    }

    fn map_language(&self, lang: Language) -> String {
        map_language!(lang, {
            Auto => "auto",
            ZhCn => "zh",
            ZhTw => "cht",
            En => "en",
            Ja => "jp",
            Ko => "kor",
            Fr => "fra",
            Es => "spa",
            Ru => "ru",
            De => "de",
            It => "it",
            Tr => "tr",
            PtPt => "pt",
            PtBr => "pot",
            Vi => "vie",
            Id => "id",
            Th => "th",
            Ms => "may",
            Ar => "ar",
            Hi => "hi",
        })
    }

    async fn recognize(&self, req: RecognizeRequest) -> Result<String> {
        let appid = req.require_str("appid", "Baidu ImageTranslate App ID")?;
        let secret = req.require_str("secret", "Baidu ImageTranslate Secret")?;

        // 签名：md5(appid + md5(file) + salt + "APICUIDmac" + secret)
        let file_md5 = md5_hex_bytes(&req.image);
        let salt = uuid_v4();
        let sign_raw = format!("{}{}{}APICUIDmac{}", appid, file_md5, salt, secret);
        let sign = md5_hex(&sign_raw);

        let lang = self.map_language(req.language);
        let to: String = if lang == "auto" {
            "zh".to_string()
        } else {
            lang.clone()
        };

        let part = reqwest::multipart::Part::bytes(req.image.clone())
            .file_name("pot_screenshot_cut.png")
            .mime_str("image/png")
            .map_err(|e| Error::Service(format!("构造图片表单失败: {e}")))?;
        let form = reqwest::multipart::Form::new()
            .part("image", part)
            .text("from", "auto")
            .text("to", to)
            .text("appid", appid)
            .text("salt", salt)
            .text("cuid", "APICUID")
            .text("mac", "mac")
            .text("version", "3")
            .text("sign", sign);

        let resp = post_with_headers(URL, &[("token", "")])
            .multipart(form)
            .send()
            .await
            .net_err()?;
        let result: Value = check(resp).await?.json().await.net_err()?;

        let data = result
            .get("data")
            .ok_or_else(|| Error::Service(serde_json::to_string(&result).unwrap_or_default()))?;
        let src = data.get("sumSrc").and_then(|v| v.as_str());
        let dst = data.get("sumDst").and_then(|v| v.as_str());
        match (lang.as_str(), src, dst) {
            (_, Some(s), _) if lang == "auto" => Ok(s.trim().to_string()),
            (_, _, Some(d)) => Ok(d.trim().to_string()),
            _ => Err(Error::Service(
                serde_json::to_string(&result).unwrap_or_default(),
            )),
        }
    }
}
