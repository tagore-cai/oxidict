//! 欧路词典生词本（Eudic / 法语词典开放 API）。
//!
//! 移植自 `src/services/collection/eudic/index.jsx`。
//! 走 `api.frdic.com` 开放接口：先按名称查找/创建生词本分类（category），
//! 再把单词加入该分类。鉴权用请求头 `Authorization: <token>`。

use crate::Collector;
use async_trait::async_trait;
use saladict_core::schema::ConfigField;
use saladict_core::{CollectionRequest, Error, Result};
use serde_json::json;

pub struct Eudic;

const CATEGORY_URL: &str = "https://api.frdic.com/api/open/v1/studylist/category";
const WORDS_URL: &str = "https://api.frdic.com/api/open/v1/studylist/words";

#[async_trait]
impl Collector for Eudic {
    fn id(&self) -> &str {
        "eudic"
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        vec![
            ConfigField::Text {
                key: "name",
                label: "生词本名称",
                placeholder: "Saladict",
                secret: false,
                required: false,
            },
            ConfigField::secret("token", "Token"),
        ]
    }

    async fn collect(&self, req: CollectionRequest) -> Result<()> {
        let name = req
            .config
            .get("name")
            .and_then(|v| v.as_str())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .unwrap_or("Saladict")
            .to_string();
        let token = req
            .config
            .get("token")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| Error::MissingConfig("eudic token 未配置".into()))?;

        let category_id = self.check_category(&name, token).await?;
        self.add_word(category_id, &req.source, token).await?;
        Ok(())
    }
}

impl Eudic {
    /// 查找名为 `name` 的分类，没有则创建，返回其 id。
    async fn check_category(&self, name: &str, token: &str) -> Result<i64> {
        let headers = [
            ("Content-Type", "application/json"),
            ("Authorization", token),
        ];

        let resp = saladict_net::get_with_headers(CATEGORY_URL, &headers)
            .query(&[("language", "en")])
            .send()
            .await
            .map_err(net_err)?;
        let resp = saladict_net::check(resp).await?;
        let result: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| Error::Network(e.to_string()))?;

        let data = result
            .get("data")
            .and_then(|v| v.as_array())
            .ok_or_else(|| Error::Service("获取生词本分类失败".into()))?;
        for item in data {
            if item.get("name").and_then(|v| v.as_str()) == Some(name)
                && let Some(id) = item.get("id").and_then(|v| v.as_i64())
            {
                return Ok(id);
            }
        }

        // 未找到，创建分类。
        let create = saladict_net::post_with_headers(CATEGORY_URL, &headers)
            .json(&json!({ "language": "en", "name": name }))
            .send()
            .await
            .map_err(net_err)?;
        let create = saladict_net::check(create).await?;
        let created: serde_json::Value = create
            .json()
            .await
            .map_err(|e| Error::Network(e.to_string()))?;
        created
            .get("data")
            .and_then(|v| v.get("id"))
            .and_then(|v| v.as_i64())
            .ok_or_else(|| Error::Service("创建生词本分类失败".into()))
    }

    /// 把单词加入指定分类。
    async fn add_word(&self, category_id: i64, word: &str, token: &str) -> Result<()> {
        let headers = [
            ("Content-Type", "application/json"),
            ("Authorization", token),
        ];
        let body = json!({
            "id": category_id,
            "language": "en",
            "words": [word],
        });
        let resp = saladict_net::post_with_headers(WORDS_URL, &headers)
            .json(&body)
            .send()
            .await
            .map_err(net_err)?;
        let resp = saladict_net::check(resp).await?;
        let result: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| Error::Network(e.to_string()))?;
        match result.get("message").and_then(|v| v.as_str()) {
            Some(_) => Ok(()),
            None => Err(Error::Service("加入单词失败".into())),
        }
    }
}

/// reqwest 错误转 `Error::Network`，对齐 `saladict-net` 的内部约定。
fn net_err(e: reqwest::Error) -> Error {
    Error::Network(e.to_string())
}
