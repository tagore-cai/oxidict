//! Anki 生词本（AnkiConnect）。
//!
//! 移植自 `src/services/collection/anki/index.jsx`。
//! 通过本地 HTTP 接口（127.0.0.1:8765）调用 AnkiConnect 的
//! `createDeck` / `createModel` / `addNote` 动作，把单词写入 `Pot` 牌组。

use crate::Collector;
use async_trait::async_trait;
use saladict_core::schema::ConfigField;
use saladict_core::{CollectionRequest, Result};
use serde_json::json;

pub struct Anki;

const DEFAULT_PORT: u16 = 8765;
const DECK: &str = "Pot";
const MODEL: &str = "Pot Card 2";

#[async_trait]
impl Collector for Anki {
    fn id(&self) -> &str {
        "anki"
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        vec![ConfigField::Number {
            key: "port",
            label: "端口",
            default: DEFAULT_PORT as f64,
            min: 1.0,
            max: 65535.0,
        }]
    }

    async fn collect(&self, req: CollectionRequest) -> Result<()> {
        let port = req
            .config
            .get("port")
            .and_then(|v| v.as_u64())
            .map(|p| p as u16)
            .unwrap_or(DEFAULT_PORT);
        let url = format!("http://127.0.0.1:{}", port);

        // 创建牌组（已存在则忽略）。
        self.connect(&url, "createDeck", json!({ "deck": DECK }))
            .await?;
        // 创建卡片模型（已存在则忽略）。
        self.connect(
            &url,
            "createModel",
            json!({
                "modelName": MODEL,
                "inOrderFields": ["Front", "Back", "Symbol1", "Voice1", "Symbol2", "Voice2"],
                "isCloze": false,
                "cardTemplates": [{
                    "Name": MODEL,
                    "Front": "{{Front}}",
                    "Back": "{{FrontSide}}<br>{{Symbol1}} {{Voice1}}<br>{{Symbol2}} {{Voice2}}<hr id=answer>{{Back}}"
                }]
            }),
        )
        .await?;

        // 添加笔记。新内核里 target 已是纯文本，直接作为 Back；无发音音频。
        self.connect(
            &url,
            "addNote",
            json!({
                "note": {
                    "deckName": DECK,
                    "modelName": MODEL,
                    "fields": {
                        "Front": req.source,
                        "Back": req.target,
                    },
                    "audio": []
                }
            }),
        )
        .await?;

        Ok(())
    }
}

impl Anki {
    /// 调用一次 AnkiConnect 动作；仅网络层错误会上抛，逻辑错误按 JS 原实现忽略。
    async fn connect(&self, url: &str, action: &str, params: serde_json::Value) -> Result<()> {
        let body = json!({ "action": action, "version": 6, "params": params });
        let _ = saladict_net::post_json_value(url, &body).await?;
        Ok(())
    }
}
