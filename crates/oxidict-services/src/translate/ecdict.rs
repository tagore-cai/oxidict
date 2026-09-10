//! ECDICT 英汉词典（通过 pot-app 的本地词典接口）。
//!
//! 移植自 `src/services/translate/ecdict/index.jsx`。
//! 原实现向 `https://pot-app.com/api/dict` 投递 `{ text }`，
//! 服务端返回结构化词典结果（发音 / 释义 / 例句等），
//! 这里直接反序列化为 `DictResult` 透传给 UI。
//!
//! 数据来源：pot-app 内置的 ECDICT 词库（本地 sqlite）。本进程只做转发，
//! 不直接读取文件——若将来要改为直连本地 sqlite，需引入 rusqlite 依赖。

use crate::Translator;
use async_trait::async_trait;
use oxidict_core::map_language;
use oxidict_core::model::DictResult;
use oxidict_core::schema::ConfigField;
use oxidict_core::{Language, Result, TranslateRequest, TranslateResult};
use oxidict_net::NetErr as _;

pub struct Ecdict;

const ENDPOINT: &str = "https://pot-app.com/api/dict";

#[async_trait]
impl Translator for Ecdict {
    fn id(&self) -> &str {
        "ecdict"
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        vec![]
    }

    fn map_language(&self, lang: Language) -> String {
        map_language!(lang, {
            ZhCn => "zh",
            ZhTw => "zh",
            En => "en",
        })
    }

    async fn translate(&self, req: TranslateRequest) -> Result<TranslateResult> {
        let resp =
            oxidict_net::post_with_headers(ENDPOINT, &[("Content-Type", "application/json")])
                .json(&serde_json::json!({ "text": req.text }))
                .send()
                .await
                .net_err()?;
        let resp = oxidict_net::check(resp).await?;
        let result: serde_json::Value = resp.json().await.net_err()?;

        let dict: DictResult = serde_json::from_value(result)?;
        Ok(TranslateResult::Dict(dict))
    }
}
