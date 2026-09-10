//! saladict-plugin：`.potext` 插件运行时，基于 rquickjs（QuickJS）。
//!
//! 插件就是老格式的 `main.js` 脚本：宿主把源码作为脚本 eval，
//! 入口函数（`translate` / `recognize` / `tts` / `collection`）落在
//! globalThis 上直接调用——与原版 Tauri 宿主的 eval 语义一致。
//! 运行时不依赖窗口与渲染循环，CLI / HTTP / UI 三种上下文行为一致。

pub mod adapter;
pub mod host;
pub mod install;
pub mod runtime;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("插件错误: {0}")]
    Plugin(String),

    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON 错误: {0}")]
    Json(#[from] serde_json::Error),
}

/// 让插件错误能沿 `saladict_core::Error` 一路向上传，
/// 服务 trait 的错误类型保持唯一。
impl From<Error> for saladict_core::Error {
    fn from(e: Error) -> Self {
        match e {
            Error::Plugin(m) => saladict_core::Error::Plugin(m),
            Error::Io(e) => saladict_core::Error::Io(e),
            Error::Json(e) => saladict_core::Error::Json(e),
        }
    }
}

/// 把已安装的插件注册进服务注册表。同步阻塞，可在任意线程调用。
///
/// 单个插件加载失败只记日志，不影响其余插件。返回加载成功的插件 id。
pub fn load_installed(store: &saladict_core::config::ConfigStore) -> Vec<String> {
    use saladict_core::ServiceKind;

    let mut loaded = Vec::new();
    for kind in ServiceKind::ALL {
        for plugin_id in install::installed(kind, store) {
            let dir = store.plugin_dir().join(kind.dir_name()).join(&plugin_id);
            match runtime::load(&plugin_id, &dir) {
                Ok(()) => {
                    let service = adapter::PluginService::new(plugin_id.clone(), kind);
                    match kind {
                        ServiceKind::Translate => saladict_services::services()
                            .register_translator(std::sync::Arc::new(service)),
                        ServiceKind::Recognize => saladict_services::services()
                            .register_recognizer(std::sync::Arc::new(service)),
                        ServiceKind::Tts => {
                            saladict_services::services().register_tts(std::sync::Arc::new(service))
                        }
                        ServiceKind::Collection => saladict_services::services()
                            .register_collector(std::sync::Arc::new(service)),
                    }
                    loaded.push(plugin_id.clone());
                    log::info!("插件 {} 加载成功", plugin_id);
                }
                Err(e) => log::error!("插件 {} 加载失败: {}", plugin_id, e),
            }
        }
    }
    loaded
}
