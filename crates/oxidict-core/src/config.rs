//! 配置存储。
//!
//! 沿用 oxidict v4 的 `config.json`（`<config_dir>/net.oxidict.app/config.json`，
//! 见 [`APP_ID`]），键名与结构保持完全一致，这样老用户的配置、服务实例、
//! 历史库可以无缝迁移。

use crate::error::{Error, Result};
use crate::language::Language;
use once_cell::sync::OnceCell;
use parking_lot::RwLock;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const APP_ID: &str = "net.oxidict.app";

/// 配置键名。集中在此避免散落字符串。
pub mod keys {
    pub const TRANSLATE_SERVICE_LIST: &str = "translate_service_list";
    pub const RECOGNIZE_SERVICE_LIST: &str = "recognize_service_list";
    pub const TTS_SERVICE_LIST: &str = "tts_service_list";
    pub const COLLECTION_SERVICE_LIST: &str = "collection_service_list";
    /// 配置驱动的自定义服务定义（数组，见 `oxidict-services` 的 `translate::custom`）。
    ///
    /// 存这里而不是单独文件：与原版「所有配置都在 config.json」的约定一致，
    /// 也因此能被现有的导出/同步/恢复链路一并带上，不需要额外处理。
    pub const CUSTOM_SERVICES: &str = "custom_services";

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
    // 界面语言统一走 APP_LANGUAGE（对齐原版 app_language；ui_language 曾是
    // 重复设计，已删除，老配置里的该键会被忽略）。
    pub const APP_THEME: &str = "app_theme";
    // 以下三个外观键为兼容原版 config.json 而保留（默认值会补齐、不会被清掉），
    // 但当前版本不读取：字体族需要给全部窗口统一注入 font family；字号要生效
    // 得把整个 UI 的 px() 迁到 rems()（gpui 的全局缩放基于 rem），收益小、
    // 回归面大。详见 README「兼容但尚未生效的配置项」。
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
    /// 本地目录后端的目标目录（可指向 iCloud / OneDrive 等云盘同步目录）。
    pub const BACKUP_FOLDER: &str = "backup_folder";
    pub const WEBDAV_URL: &str = "webdav_url";
    pub const WEBDAV_USERNAME: &str = "webdav_username";
    pub const WEBDAV_PASSWORD: &str = "webdav_password";
    // 阿里云盘备份已砍除：原版授权链依赖第三方中转服务器（代持 client_secret）
    // 与他人注册的 client_id，不可复用；自建需阿里云盘开放平台账号。老配置里的
    // aliyun_* 键会被忽略。

    // 托盘
    pub const TRAY_CLICK_EVENT: &str = "tray_click_event";
    pub const IGNORE_UPDATER_VERSION: &str = "ignore_updater_version";

    // 外观与系统集成（对齐原版 General 页）
    /// 窗口毛玻璃/透明（macOS Blurred 材质；新开窗口生效）。
    pub const TRANSPARENT: &str = "transparent";
    /// 隐藏 Dock 图标（macOS Accessory 策略，交互入口只剩托盘）。
    pub const HIDE_DOCK_ICON: &str = "hide_dock_icon";

    /// 快捷键配置前缀，实际键形如 `hotkey_selection_translate`。
    pub const HOTKEY_PREFIX: &str = "hotkey_";

    /// 四类窗口/动作的完整快捷键键名。
    ///
    /// 散落的 `"hotkey_selection_translate"` 字面量容易与上面的前缀定义
    /// 漂移，这里集中给出，注册与默认值都引用同一份常量。
    pub const HOTKEY_SELECTION_TRANSLATE: &str = "hotkey_selection_translate";
    pub const HOTKEY_INPUT_TRANSLATE: &str = "hotkey_input_translate";
    pub const HOTKEY_OCR_RECOGNIZE: &str = "hotkey_ocr_recognize";
    pub const HOTKEY_OCR_TRANSLATE: &str = "hotkey_ocr_translate";
}

static STORE: OnceCell<Arc<ConfigStore>> = OnceCell::new();

/// 进程级配置单例。
///
/// 未初始化时 panic：这是**应用入口**的契约（`main` 起服务前必须调
/// `ConfigStore::init`）。库代码里"读一下就有、没有也照样能跑"的场合请用
/// [`store_opt`]。
pub fn config() -> Arc<ConfigStore> {
    STORE
        .get()
        .expect("ConfigStore 未初始化，请在使用前调用 ConfigStore::init")
        .clone()
}

/// 全局配置（若尚未初始化则 `None`）。
///
/// 给库函数读取**可选**设置用（例如 HTTP 客户端的代理配置）。`config()` 在
/// 未初始化时会 panic，对这类"没有代理就直连"的读取过于激进——测试环境、
/// 独立工具函数都可能在没有初始化存储的情况下调用到。
pub fn store_opt() -> Option<Arc<ConfigStore>> {
    STORE.get().cloned()
}

pub fn set_global(store: Arc<ConfigStore>) {
    let _ = STORE.set(store);
}

/// 默认应用数据目录（`<config_dir>/<APP_ID>`）。不依赖 ConfigStore::init，
/// 供日志初始化等"配置装载前"的路径需求使用。
pub fn default_app_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join(APP_ID))
}

/// 单实例锁文件路径。app 入口与托盘「重启」共用；重启前必须先移除锁，
/// 否则新进程会因 PID 存活判定为「已有实例」而退出。
pub fn runtime_lock_path() -> PathBuf {
    dirs::runtime_dir()
        .or_else(dirs::cache_dir)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("oxidict-app.lock")
}

pub struct ConfigStore {
    path: PathBuf,
    data: RwLock<Map<String, Value>>,
    /// 串行化落盘。
    ///
    /// 「取快照」与「rename 到目标路径」必须处于同一临界区：否则两个并发
    /// `set` 会各自快照、再先后 rename，先取的旧快照若最后落地，就会把后写
    /// 的键整段抹掉（配置丢失，且内存与磁盘从此不一致）。
    save_lock: parking_lot::Mutex<()>,
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
        // 全局单例只初始化一次。重复调用直接返回既有实例，避免出现「本次返回值
        // 与 `config()` 指向两个不同 store」的隐性分裂：那样配置的读与写会落在
        // 互不可见的两份内存映射上，表现为「改了设置却不生效」。
        if let Some(existing) = STORE.get() {
            return Ok(existing.clone());
        }
        let data = match std::fs::read_to_string(&path) {
            Ok(s) => match serde_json::from_str::<Map<String, Value>>(&s) {
                Ok(m) => m,
                Err(e) => {
                    // 解析失败不能 `unwrap_or_default()` 了事：紧随其后的
                    // `apply_defaults` 会立刻落盘，把用户的全部配置覆盖成默认值。
                    // 先把损坏原文改名留档，再以默认配置启动。
                    log::error!("配置文件 {} 解析失败: {e}", path.display());
                    let backup = path.with_extension("json.corrupt");
                    match std::fs::rename(&path, &backup) {
                        Ok(()) => log::error!(
                            "已将损坏的配置另存为 {}，本次以默认配置启动",
                            backup.display()
                        ),
                        Err(e) => log::error!("另存损坏配置失败（将直接覆盖）: {e}"),
                    }
                    Map::new()
                }
            },
            // 文件不存在是首次启动的正常路径，不记错误。
            Err(_) => Map::new(),
        };
        let store = Arc::new(Self {
            path,
            data: RwLock::new(data),
            save_lock: parking_lot::Mutex::new(()),
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
            // 自定义服务默认为空数组：用户往这里追加 spec 即增加服务。
            (CUSTOM_SERVICES, json!([])),
            (TRANSLATE_SOURCE_LANGUAGE, json!("auto")),
            (TRANSLATE_TARGET_LANGUAGE, json!("zh_cn")),
            (TRANSLATE_REMEMBER_LANGUAGE, json!(true)),
            (TRANSLATE_CLOSE_ON_BLUR, json!(true)),
            (TRANSLATE_ALWAYS_ON_TOP, json!(true)),
            (TRANSLATE_WINDOW_WIDTH, json!(350.0)),
            (TRANSLATE_WINDOW_HEIGHT, json!(420.0)),
            // 值域对齐原版：disable | source | target | source_target
            (TRANSLATE_AUTO_COPY, json!("disable")),
            (TRANSLATE_DETECT_ENGINE, json!("local")),
            (CLIPBOARD_MONITOR, json!(false)),
            (PROXY_ENABLE, json!(false)),
            (PROXY_HOST, json!("127.0.0.1")),
            (PROXY_PORT, json!(1087)),
            (SERVER_PORT, json!(60606)),
            (APP_THEME, json!("system")),
            (AUTOSTART, json!(false)),
            (keys::HOTKEY_SELECTION_TRANSLATE, json!("")),
            (keys::HOTKEY_INPUT_TRANSLATE, json!("")),
            (keys::HOTKEY_OCR_RECOGNIZE, json!("")),
            (keys::HOTKEY_OCR_TRANSLATE, json!("")),
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
            (keys::HIDE_SOURCE, json!(false)),
            (keys::HIDE_LANGUAGE, json!(false)),
            (keys::HISTORY_DISABLE, json!(false)),
            // OCR 窗口行为
            // 对齐原版 bool：识别完成后复制结果
            (keys::RECOGNIZE_AUTO_COPY, json!(false)),
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
            (keys::BACKUP_FOLDER, json!("")),
            (keys::WEBDAV_URL, json!("")),
            (keys::WEBDAV_USERNAME, json!("")),
            (keys::WEBDAV_PASSWORD, json!("")),
            // 托盘与其他
            (keys::TRAY_CLICK_EVENT, json!("config")),
            (keys::IGNORE_UPDATER_VERSION, json!("")),
            (keys::DEV_MODE, json!(false)),
            // 外观与系统集成（对齐原版默认值：透明开、隐藏 Dock 开）
            (keys::TRANSPARENT, json!(true)),
            (keys::HIDE_DOCK_ICON, json!(true)),
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
        self.raw(key).and_then(|v| serde_json::from_value(v).ok())
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
    ///
    /// 整个「快照 -> 写临时文件 -> rename」在 `save_lock` 内串行执行，
    /// 防止并发 `set` 的快照互相覆盖（见 `save_lock` 字段的说明）。
    pub fn save(&self) -> Result<()> {
        let _guard = self.save_lock.lock();
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
    ///
    /// 解析失败时**返回错误并保持现有内存数据不变**，不要把整个配置清空成
    /// 默认值：监听线程可能在文件写到一半时触发 reload。
    pub fn reload(&self) -> Result<()> {
        let s = std::fs::read_to_string(&self.path)?;
        let parsed: Map<String, Value> = serde_json::from_str(&s)
            .map_err(|e| Error::Config(format!("配置文件解析失败: {e}")))?;
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

    /// 代理地址。若配置了用户名/密码，则拼成 `http://user:pass@host:port`。
    ///
    /// 带凭据的代理在企业网络里是常态，缺了它代理基本不可用。
    pub fn proxy(&self) -> Option<String> {
        if !self.get_or(keys::PROXY_ENABLE, false) {
            return None;
        }
        let host: String = self.get_or(keys::PROXY_HOST, "127.0.0.1".into());
        let port: u16 = self.get_or(keys::PROXY_PORT, 1087);
        let username: String = self.get_or(keys::PROXY_USERNAME, String::new());
        let password: String = self.get_or(keys::PROXY_PASSWORD, String::new());

        if username.is_empty() {
            return Some(format!("http://{}:{}", host, port));
        }
        // 对 userinfo 做百分号编码，避免密码里的特殊字符破坏 URL。
        let user = urlencoding::encode(&username);
        let pass = urlencoding::encode(&password);
        if password.is_empty() {
            Some(format!("http://{}@{}:{}", user, host, port))
        } else {
            Some(format!("http://{}:{}@{}:{}", user, pass, host, port))
        }
    }

    /// 不走代理的主机列表（逗号分隔），供上层 HTTP 客户端使用。
    pub fn no_proxy(&self) -> Option<String> {
        let v: String = self.get_or(keys::NO_PROXY, String::new());
        if v.trim().is_empty() { None } else { Some(v) }
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

/// 判断事件是否代表 `config.json` 自身发生了变化。
///
/// [`watch_config`] 监听整个应用目录，而该目录里还住着 `logs/`（日志持续写入）、
/// `history.db`（翻译即写入）与 `plugins/`、单实例锁等。若不过滤，就会出现
/// 「自己写日志 → 触发 reload → 回调重新应用设置 → 再写日志」的自激循环。
/// 因此只认文件名 `config.json`（用文件名而非完整路径比较，避开 macOS 上
/// `/tmp` 与 `/private/tmp` 之类的符号链接差异）。
fn is_config_json_change(event: &notify::Event) -> bool {
    use notify::EventKind;
    matches!(
        event.kind,
        EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
    ) && event
        .paths
        .iter()
        .any(|p| p.file_name().is_some_and(|n| n == "config.json"))
}

/// 监控 config.json，外部变化时自动 reload 并通过回调通知（供 UI 刷新）。
///
/// 只对 `config.json` 自身的事件作出反应，过滤规则见 [`is_config_json_change`]：
/// 回调方通常会重新应用设置（写配置/写日志），不过滤会自激。
pub fn watch_config<F>(store: Arc<ConfigStore>, on_change: F) -> Result<()>
where
    F: Fn() + Send + 'static,
{
    use notify::{RecursiveMode, Watcher};
    let dir = store.app_dir();
    let mut watcher = notify::recommended_watcher(
        move |res: std::result::Result<notify::Event, notify::Error>| {
            if let Ok(event) = res
                && is_config_json_change(&event)
            {
                let _ = store.reload();
                on_change();
            }
        },
    )
    .map_err(|e| Error::Watch(e.to_string()))?;
    watcher
        .watch(&dir, RecursiveMode::NonRecursive)
        .map_err(|e| Error::Watch(e.to_string()))?;
    // 监听器必须保持存活，detach 到后台常驻。
    std::mem::forget(watcher);
    Ok(())
}

#[cfg(test)]
mod tests {
    // 测试中 unwrap 直观且失败即测试失败，豁免。
    #![allow(clippy::unwrap_used)]

    use super::*;
    use notify::event::{CreateKind, ModifyKind, RemoveKind};
    use notify::{Event, EventKind};
    use std::path::PathBuf;

    fn event(kind: EventKind, path: &str) -> Event {
        Event {
            kind,
            paths: vec![PathBuf::from(path)],
            attrs: Default::default(),
        }
    }

    /// 生产事故防线：目录里除 config.json 外还有 logs/history.db 等高频写入文件，
    /// 只有 config.json 自身的事件才允许触发 reload（否则日志写入会自激）。
    #[test]
    fn only_config_json_triggers_watch_callback() {
        let dir = "/tmp/oxidict-app-dir";
        assert!(is_config_json_change(&event(
            EventKind::Modify(ModifyKind::Any),
            &format!("{dir}/config.json")
        )));
        assert!(is_config_json_change(&event(
            EventKind::Create(CreateKind::File),
            &format!("{dir}/config.json")
        )));
        assert!(is_config_json_change(&event(
            EventKind::Remove(RemoveKind::File),
            &format!("{dir}/config.json")
        )));
        // 事件可能同时带多个路径（例如 rename 的源与目标），任一命中即可。
        let mut rename = event(
            EventKind::Modify(ModifyKind::Name(notify::event::RenameMode::Both)),
            &format!("{dir}/config.json.tmp"),
        );
        rename
            .paths
            .push(PathBuf::from(format!("{dir}/config.json")));
        assert!(is_config_json_change(&rename));

        // 日志、历史库、锁文件、插件目录都不应触发。
        for name in ["logs/oxidict.log", "history.db", "oxidict-app.lock"] {
            assert!(
                !is_config_json_change(&event(
                    EventKind::Modify(ModifyKind::Any),
                    &format!("{dir}/{name}")
                )),
                "{name} 不应触发 reload"
            );
        }
        // 同名但带后缀的临时文件不触发（保存流程会先写 .tmp 再改名到
        // config.json，只有最终落地的那次改名才应对应一次 reload）。
        for name in ["config.json.tmp", "config.json.bak", "config.json.1"] {
            assert!(
                !is_config_json_change(&event(
                    EventKind::Modify(ModifyKind::Any),
                    &format!("{dir}/{name}")
                )),
                "{name} 不应触发 reload"
            );
        }
        // 非写入类事件（如 Access）忽略。
        assert!(!is_config_json_change(&event(
            EventKind::Access(notify::event::AccessKind::Any),
            &format!("{dir}/config.json")
        )));
    }
}
