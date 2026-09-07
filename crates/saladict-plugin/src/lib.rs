//! saladict-plugin：`.potext` 插件运行时，基于 gpui-shell（QuickJS）。
//!
//! 老版本在 JS 侧 `eval(main.js)` 执行插件，Rust 只负责解包与 `run_binary`。
//! 纯 Rust 重写后没有 WebView，这里改用 gpui-shell 的 JS 运行时承担插件执行，
//! 并通过作业桥把「宿主调用脚本函数」这一缺失能力补上。

pub mod adapter;
pub mod bridge;
pub mod install;
pub mod shim;

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

    #[error("宿主模块注册失败: {0}")]
    HostModule(String),
}

/// 让插件错误能沿 `saladict_core::Error` 一路向上传，
/// 服务 trait 的错误类型保持唯一。
impl From<Error> for saladict_core::Error {
    fn from(e: Error) -> Self {
        match e {
            Error::Plugin(m) => saladict_core::Error::Plugin(m),
            Error::HostModule(m) => saladict_core::Error::Plugin(m),
            Error::Io(e) => saladict_core::Error::Io(e),
            Error::Json(e) => saladict_core::Error::Json(e),
        }
    }
}

impl From<gpui_shell::HostError> for Error {
    fn from(e: gpui_shell::HostError) -> Self {
        Error::HostModule(e.to_string())
    }
}

/// 初始化插件运行时。必须在 `gpui_shell::init(cx)` 之后、加载任何插件之前调用。
///
/// 做两件事：注册宿主模块 `saladict`（`take_job` / `submit_result`），
/// 以及把插件存储目录指到 saladict 自己的配置目录下。
pub fn init(cx: &mut gpui_shell::gpui::App, store: &saladict_core::config::ConfigStore) -> Result<()> {
    gpui_shell::init(cx);
    bridge::register_host_module()?;
    gpui_shell::set_storage_path(store.plugin_dir().join("store.json"));
    Ok(())
}

/// 把已安装的插件注册进服务注册表。
///
/// 需要一个窗口来挂载插件的 `ScriptView`——gpui-shell 的作业调度依附于渲染循环，
/// 没有窗口就不会 drain，插件的 await 永远不返回。
pub fn load_installed(
    runtime: &std::rc::Rc<gpui_shell::ShellRuntime>,
    window: &mut gpui_shell::gpui::Window,
    cx: &mut gpui_shell::gpui::App,
    store: &saladict_core::config::ConfigStore,
) {
    use crate::adapter::ShellService;
    use crate::shim::PluginKind;
    use gpui_shell::ShellRuntime;

    let kinds = [
        PluginKind::Translate,
        PluginKind::Recognize,
        PluginKind::Tts,
        PluginKind::Collection,
    ];

    for kind in kinds {
        for plugin_id in install::installed(kind, store) {
            let dir = store.plugin_dir().join(kind.dir_name()).join(&plugin_id);
            match runtime.load_application(&dir, "worker.js") {
                Ok(app) => match ShellRuntime::mount_application(runtime, &app, window, cx) {
                    Ok(_view) => {
                        let service = ShellService::new(plugin_id.clone(), kind);
                        match kind {
                            PluginKind::Translate => saladict_services::services()
                                .register_translator(std::sync::Arc::new(service)),
                            PluginKind::Recognize => saladict_services::services()
                                .register_recognizer(std::sync::Arc::new(service)),
                            PluginKind::Tts => saladict_services::services()
                                .register_tts(std::sync::Arc::new(service)),
                            PluginKind::Collection => saladict_services::services()
                                .register_collector(std::sync::Arc::new(service)),
                        }
                        log::info!("插件 {} 加载成功", plugin_id);
                    }
                    Err(e) => log::error!("插件 {} 挂载失败: {}", plugin_id, e),
                },
                Err(e) => log::error!("插件 {} 加载失败: {}", plugin_id, e),
            }
        }
    }
}
