//! 配置存储。
//!
//! 沿用 saladict v4 的 `config.json`（`<config_dir>/allen.town.focus.saladict/config.json`），
//! 键名与结构保持完全一致，这样老用户的配置、服务实例、历史库可以无缝迁移。

use crate::error::{Error, Result};
use crate::language::Language;
use once_cell::sync::OnceCell;
use parking_lot::RwLock;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const APP_ID: &str = "allen.town.focus.saladict";

/// 配置键名。集中在此避免散落字符串。
pub mod keys {
    pub const TRANSLATE_SERVICE_LIST: &str = "translate_service_list";
    pub const RECOGNIZE_SERVICE_LIST: &str = "recognize_service_list";
    pub const TTS_SERVICE_LIST: &str = "tts_service_list";
    pub const COLLECTION_SERVICE_LIST: &str = "collection_service_list";

    pub const TRANSLATE_SOURCE_LANGUAGE: &str = "translate_source_language";
    pub const TRANSLATE_TARGET_LANGUAGE: &str = "translate_target_language";
    pub const TRANSLATE_REMEMBER_LANGUAGE: &str = "translate_remember_language";
    pub const TRANSLATE_CLOSE_ON_BLUR: &str = "translate_close_on_blur";
    pub const TRANSLATE_ALWAYS_ON_TOP: &str = "translate_always_on_top";
    pub const TRANSLATE_WINDOW_WIDTH: &str = "translate_window_width";
    pub const TRANSLATE_WINDOW_HEIGHT: &str = "translate_window_height";
    pub const TRANSLATE_WINDOW_POSITION_X: &str = "translate_window_position_x";
    pub const TRANSLATE_WINDOW_POSITION_Y: &str = "translate_window_position_y";
    pub const TRANSLATE_AUTO_COPY: &str = "translate_auto_copy";
    pub const TRANSLATE_DETECT_ENGINE: &str = "translate_detect_engine";

    pub const CLIPBOARD_MONITOR: &str = "clipboard_monitor";
    pub const PROXY_ENABLE: &str = "proxy_enable";
    pub const PROXY_HOST: &str = "proxy_host";
    pub const PROXY_PORT: &str = "proxy_port";
    pub const PROXY_USERNAME: &str = "proxy_username";
    pub const PROXY_PASSWORD: &str = "proxy_password";
    pub const NO_PROXY: &str = "no_proxy";
    pub const SERVER_PORT: &str = "server_port";
    pub const UI_LANGUAGE: &str = "ui_language";
    pub const APP_THEME: &str = "app_theme";
    pub const APP_FONT: &str = "app_font";
    pub const APP_FALLBACK_FONT: &str = "app_fallback_font";
    pub const APP_FONT_SIZE: &str = "app_font_size";
    pub const APP_LANGUAGE: &str = "app_language";
    pub const AUTOSTART: &str = "autostart";
    pub const DEV_MODE: &str = "dev_mode";

    // 翻译窗口行为
    pub const DYNAMIC_TRANSLATE: &str = "dynamic_translate";
    pub const INCREMENTAL_TRANSLATE: &str = "incremental_translate";
    pub const TRANSLATE_DELETE_NEWLINE: &str = "translate_delete_newline";
    pub const TRANSLATE_FONT_SIZE: &str = "translate_font_size";
    pub const TRANSLATE_REVERT_ENTER: &str = "translate_revert_enter";
    pub const TRANSLATE_SECOND_LANGUAGE: &str = "translate_second_language";
    pub const TRANSLATE_HIDE_WINDOW: &str = "translate_hide_window";
    pub const TRANSLATE_REMEMBER_WINDOW_SIZE: &str = "translate_remember_window_size";
    pub const TRANSLATE_WINDOW_POSITION: &str = "translate_window_position";
    pub const HIDE_SOURCE: &str = "hide_source";
    pub const HIDE_LANGUAGE: &str = "hide_language";
    pub const HISTORY_DISABLE: &str = "history_disable";

    // OCR 窗口行为
    pub const RECOGNIZE_AUTO_COPY: &str = "recognize_auto_copy";
    pub const RECOGNIZE_CLOSE_ON_BLUR: &str = "recognize_close_on_blur";
    pub const RECOGNIZE_DELETE_NEWLINE: &str = "recognize_delete_newline";
    pub const RECOGNIZE_HIDE_WINDOW: &str = "recognize_hide_window";
    pub const RECOGNIZE_LANGUAGE: &str = "recognize_language";

    // 备份
    pub const BACKUP_TYPE: &str = "backup_type";
    pub const WEBDAV_URL: &str = "webdav_url";
    pub const WEBDAV_USERNAME: &str = "webdav_username";
    pub const WEBDAV_PASSWORD: &str = "webdav_password";
    pub const ALIYUN_ACCESS_TOKEN: &str = "aliyun_access_token";
    pub const ALIYUN_REFRESH_TOKEN: &str = "aliyun_refresh_token";

    // 托盘
    pub const TRAY_CLICK_EVENT: &str = "tray_click_event";
    pub const IGNORE_UPDATER_VERSION: &str = "ignore_updater_version";

    /// 快捷键配置前缀，实际键形如 `hotkey_selection_translate`。
    pub const HOTKEY_PREFIX: &str = "hotkey_";
}

static STORE: OnceCell<Arc<ConfigStore>> = OnceCell::new();

/// 进程级配置单例。
pub fn config() -> Arc<ConfigStore> {
    STORE
        .get()
        .expect("ConfigStore 未初始化，请在使用前调用 ConfigStore::init")
        .clone()
}

pub fn set_global(store: Arc<ConfigStore>) {
    let _ = STORE.set(store);
}

pub struct ConfigStore {
    path: PathBuf,
    data: RwLock<Map<String, Value>>,
}

impl ConfigStore {
    /// 加载（或创建）默认路径下的 config.json。
    pub fn init() -> Result<Arc<Self>> {
        let dir = dirs::config_dir()
            .ok_or_else(|| Error::Config("无法定位系统配置目录".into()))?
            .join(APP_ID);
        std::fs::create_dir_all(&dir)?;
        Self::init_with_path(dir.join("config.json"))
    }

    pub fn init_with_path(path: PathBuf) -> Result<Arc<Self>> {
        let data = match std::fs::read_to_string(&path) {
            Ok(s) => serde_json::from_str::<Map<String, Value>>(&s).unwrap_or_default(),
            Err(_) => Map::new(),
        };
        let store = Arc::new(Self {
            path,
            data: RwLock::new(data),
        });
        store.apply_defaults();
        set_global(store.clone());
        Ok(store)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 配置目录，插件与历史库都挂在这里。
    pub fn app_dir(&self) -> PathBuf {
        self.path.parent().unwrap_or(Path::new(".")).to_path_buf()
    }

    pub fn plugin_dir(&self) -> PathBuf {
        self.app_dir().join("plugins")
    }

    fn apply_defaults(&self) {
        use keys::*;
        let mut data = self.data.write();
        let defaults: Vec<(&str, Value)> = vec![
            (TRANSLATE_SERVICE_LIST, json!(["google", "bing_dict"])),
            (
                RECOGNIZE_SERVICE_LIST,
                json!(["system", "tesseract", "qrcode"]),
            ),
            (TTS_SERVICE_LIST, json!(["lingva_tts"])),
            (COLLECTION_SERVICE_LIST, json!([])),
            (TRANSLATE_SOURCE_LANGUAGE, json!("auto")),
            (TRANSLATE_TARGET_LANGUAGE, json!("zh_cn")),
            (TRANSLATE_REMEMBER_LANGUAGE, json!(true)),
            (TRANSLATE_CLOSE_ON_BLUR, json!(true)),
            (TRANSLATE_ALWAYS_ON_TOP, json!(true)),
            (TRANSLATE_WINDOW_WIDTH, json!(350.0)),
            (TRANSLATE_WINDOW_HEIGHT, json!(420.0)),
            (TRANSLATE_AUTO_COPY, json!(4)),
            (TRANSLATE_DETECT_ENGINE, json!("local")),
            (CLIPBOARD_MONITOR, json!(false)),
            (PROXY_ENABLE, json!(false)),
            (PROXY_HOST, json!("127.0.0.1")),
            (PROXY_PORT, json!(1087)),
            (SERVER_PORT, json!(60606)),
            (UI_LANGUAGE, json!("zh_cn")),
            (APP_THEME, json!("system")),
            (AUTOSTART, json!(false)),
            ("hotkey_selection_translate", json!("")),
            ("hotkey_input_translate", json!("")),
            ("hotkey_ocr_recognize", json!("")),
            ("hotkey_ocr_translate", json!("")),
            // 翻译窗口行为
            (keys::DYNAMIC_TRANSLATE, json!(false)),
            (keys::INCREMENTAL_TRANSLATE, json!(false)),
            (keys::TRANSLATE_DELETE_NEWLINE, json!(0)),
            (keys::TRANSLATE_FONT_SIZE, json!(16)),
            (keys::TRANSLATE_REVERT_ENTER, json!(true)),
            (keys::TRANSLATE_SECOND_LANGUAGE, json!("zh_cn")),
            (keys::TRANSLATE_HIDE_WINDOW, json!(false)),
            (keys::TRANSLATE_REMEMBER_WINDOW_SIZE, json!(false)),
            (keys::TRANSLATE_WINDOW_POSITION, json!("")),
            (keys::TRANSLATE_AUTO_COPY, json!(4)),
            (keys::TRANSLATE_DETECT_ENGINE, json!("local")),
            (keys::HIDE_SOURCE, json!(false)),
            (keys::HIDE_LANGUAGE, json!(false)),
            (keys::HISTORY_DISABLE, json!(false)),
            // OCR 窗口行为
            (keys::RECOGNIZE_AUTO_COPY, json!(4)),
            (keys::RECOGNIZE_CLOSE_ON_BLUR, json!(false)),
            (keys::RECOGNIZE_DELETE_NEWLINE, json!(false)),
            (keys::RECOGNIZE_HIDE_WINDOW, json!(false)),
            (keys::RECOGNIZE_LANGUAGE, json!("auto")),
            // 代理详情
            (keys::PROXY_USERNAME, json!("")),
            (keys::PROXY_PASSWORD, json!("")),
            (keys::NO_PROXY, json!("")),
            // 字体
            (keys::APP_FONT, json!("default")),
            (keys::APP_FALLBACK_FONT, json!("default")),
            (keys::APP_FONT_SIZE, json!(16)),
            (keys::APP_LANGUAGE, json!("zh_cn")),
            // 备份
            (keys::BACKUP_TYPE, json!("webdav")),
            (keys::WEBDAV_URL, json!("")),
            (keys::WEBDAV_USERNAME, json!("")),
            (keys::WEBDAV_PASSWORD, json!("")),
            (keys::ALIYUN_ACCESS_TOKEN, json!("")),
            (keys::ALIYUN_REFRESH_TOKEN, json!("")),
            // 托盘与其他
            (keys::TRAY_CLICK_EVENT, json!("config")),
            (keys::IGNORE_UPDATER_VERSION, json!("")),
            (keys::DEV_MODE, json!(false)),
        ];
        for (k, v) in defaults {
            data.entry(k.to_string()).or_insert(v);
        }
        drop(data);
        let _ = self.save();
    }

    pub fn raw(&self, key: &str) -> Option<Value> {
        self.data.read().get(key).cloned()
    }

    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.raw(key)
            .and_then(|v| serde_json::from_value(v).ok())
    }

    pub fn get_or<T: DeserializeOwned>(&self, key: &str, default: T) -> T {
        self.get(key).unwrap_or(default)
    }

    pub fn set<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        {
            let mut data = self.data.write();
            data.insert(key.to_string(), json!(value));
        }
        self.save()
    }

    pub fn remove(&self, key: &str) -> Result<()> {
        {
            let mut data = self.data.write();
            data.remove(key);
        }
        self.save()
    }

    /// 原子落盘：先写临时文件再 rename，避免崩溃时写坏 config.json。
    pub fn save(&self) -> Result<()> {
        let snapshot = {
            let data = self.data.read();
            serde_json::to_string_pretty(&*data)?
        };
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, snapshot)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }

    /// 外部修改（例如老版本 Tauri 应用、手工编辑）后重新载入。
    pub fn reload(&self) -> Result<()> {
        let s = std::fs::read_to_string(&self.path)?;
        let parsed: Map<String, Value> = serde_json::from_str(&s).unwrap_or_default();
        *self.data.write() = parsed;
        Ok(())
    }

    pub fn snapshot(&self) -> Map<String, Value> {
        self.data.read().clone()
    }

    // ---------- 类型化访问器 ----------

    pub fn service_list(&self, key: &str) -> Vec<String> {
        self.get::<Vec<String>>(key).unwrap_or_default()
    }

    pub fn set_service_list(&self, key: &str, list: &[String]) -> Result<()> {
        self.set(key, &list)
    }

    /// 读取某个服务实例的配置对象。实例 key 形如 `alibaba` 或 `alibaba@r4nd0m`。
    pub fn instance_config(&self, instance: &str) -> crate::model::ServiceConfig {
        self.raw(instance)
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default()
    }

    pub fn set_instance_config(
        &self,
        instance: &str,
        cfg: crate::model::ServiceConfig,
    ) -> Result<()> {
        self.set(instance, &Value::Object(cfg))
    }

    pub fn source_language(&self) -> Language {
        self.get::<String>(keys::TRANSLATE_SOURCE_LANGUAGE)
            .and_then(|s| Language::from_code(&s))
            .unwrap_or(Language::Auto)
    }

    pub fn target_language(&self) -> Language {
        self.get::<String>(keys::TRANSLATE_TARGET_LANGUAGE)
            .and_then(|s| Language::from_code(&s))
            .unwrap_or(Language::ZhCn)
    }

    pub fn proxy(&self) -> Option<String> {
        if !self.get_or(keys::PROXY_ENABLE, false) {
            return None;
        }
        let host: String = self.get_or(keys::PROXY_HOST, "127.0.0.1".into());
        let port: u16 = self.get_or(keys::PROXY_PORT, 1087);
        Some(format!("http://{}:{}", host, port))
    }

    pub fn server_port(&self) -> u16 {
        self.get_or(keys::SERVER_PORT, 60606)
    }
}

/// 服务实例 key 解析：`alibaba@r4nd0m` -> (`alibaba`, Some(`r4nd0m`))。
pub fn parse_instance(instance: &str) -> (&str, Option<&str>) {
    match instance.split_once('@') {
        Some((name, id)) => (name, Some(id)),
        None => (instance, None),
    }
}

/// 监控 config.json，外部变化时自动 reload 并通过回调通知（供 UI 刷新）。
pub fn watch_config<F>(store: Arc<ConfigStore>, on_change: F) -> Result<()>
where
    F: Fn() + Send + 'static,
{
    use notify::{EventKind, RecursiveMode, Watcher};
    let dir = store.app_dir();
    let mut watcher =
        notify::recommended_watcher(move |res: std::result::Result<notify::Event, notify::Error>| {
            if let Ok(event) = res {
                if matches!(
                    event.kind,
                    EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                ) {
                    let _ = store.reload();
                    on_change();
                }
            }
        })
        .map_err(|e| Error::Watch(e.to_string()))?;
    watcher
        .watch(&dir, RecursiveMode::NonRecursive)
        .map_err(|e| Error::Watch(e.to_string()))?;
    // 监听器必须保持存活，detach 到后台常驻。
    std::mem::forget(watcher);
    Ok(())
}
