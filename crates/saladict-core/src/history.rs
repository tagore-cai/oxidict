//! 翻译历史。
//!
//! 原实现由 `tauri-plugin-sql` 在 JS 侧惰性建表，表结构保持完全一致，
//! 老版本生成的 `history.db` 可直接继续使用。

use crate::error::Result;
use crate::model::HistoryEntry;
use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::sync::Arc;

const CREATE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS history(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    text TEXT NOT NULL,
    source TEXT NOT NULL,
    target TEXT NOT NULL,
    service TEXT NOT NULL,
    result TEXT NOT NULL,
    timestamp INTEGER NOT NULL
)"#;

static DB: OnceCell<Arc<History>> = OnceCell::new();

pub struct History {
    conn: Mutex<Connection>,
}

impl History {
    pub fn init(path: PathBuf) -> Result<Arc<Self>> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(&path)?;
        conn.execute(CREATE_SQL, [])?;
        let history = Arc::new(Self {
            conn: Mutex::new(conn),
        });
        let _ = DB.set(history.clone());
        Ok(history)
    }

    /// 默认位置：`<config_dir>/allen.town.focus.saladict/history.db`
    pub fn init_default() -> Result<Arc<Self>> {
        let dir = dirs::config_dir()
            .ok_or_else(|| crate::error::Error::Config("无法定位系统配置目录".into()))?
            .join(crate::config::APP_ID);
        Self::init(dir.join("history.db"))
    }

    pub fn global() -> Arc<Self> {
        DB.get()
            .expect("History 未初始化，请先调用 History::init")
            .clone()
    }

    pub fn add(
        &self,
        text: &str,
        source: &str,
        target: &str,
        service: &str,
        result: &str,
    ) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO history (text, source, target, service, result, timestamp) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                text,
                source,
                target,
                service,
                result,
                chrono::Utc::now().timestamp_millis()
            ],
        )?;
        Ok(())
    }

    pub fn list(&self, limit: u32, offset: u32) -> Result<Vec<HistoryEntry>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, text, source, target, service, result, timestamp FROM history
             ORDER BY timestamp DESC LIMIT ?1 OFFSET ?2",
        )?;
        let rows = stmt.query_map(params![limit, offset], |row| {
            Ok(HistoryEntry {
                id: row.get(0)?,
                text: row.get(1)?,
                source: row.get(2)?,
                target: row.get(3)?,
                service: row.get(4)?,
                result: row.get(5)?,
                timestamp: row.get(6)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn search(&self, keyword: &str, limit: u32) -> Result<Vec<HistoryEntry>> {
        let conn = self.conn.lock();
        let pattern = format!("%{}%", keyword);
        let mut stmt = conn.prepare(
            "SELECT id, text, source, target, service, result, timestamp FROM history
             WHERE text LIKE ?1 OR result LIKE ?1 ORDER BY timestamp DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![pattern, limit], |row| {
            Ok(HistoryEntry {
                id: row.get(0)?,
                text: row.get(1)?,
                source: row.get(2)?,
                target: row.get(3)?,
                service: row.get(4)?,
                result: row.get(5)?,
                timestamp: row.get(6)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn delete(&self, id: i64) -> Result<()> {
        self.conn
            .lock()
            .execute("DELETE FROM history WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn clear(&self) -> Result<()> {
        self.conn.lock().execute("DELETE FROM history", [])?;
        Ok(())
    }

    pub fn count(&self) -> Result<i64> {
        let conn = self.conn.lock();
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM history", [], |r| r.get(0))?;
        Ok(n)
    }
}
