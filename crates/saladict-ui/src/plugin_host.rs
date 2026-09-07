//! 插件宿主窗口。
//!
//! gpui-shell 的作业调度依附于渲染循环：插件的 `await take_job(...)` 要在
//! 窗口渲染帧之间才会被 drain。所以已安装的插件 ScriptView 挂载在一个
//! 常驻的小宿主窗口里（对用户几乎不可见），保证调度器持续运转。

use gpui_kit::component::Root;
use gpui_kit::AppContext as _;
use gpui_kit::{div, px, App, Bounds, IntoElement, Point, Render, Size, Styled, TitlebarOptions, WindowOptions};
use saladict_core::config::ConfigStore;
use std::sync::Arc;

/// 插件宿主根视图：本身不渲染内容，只为插件 ScriptView 提供渲染循环。
pub struct PluginHostView;

impl Render for PluginHostView {
    fn render(&mut self, _: &mut gpui_kit::Window, _: &mut gpui_kit::Context<Self>) -> impl IntoElement {
        div().size_full()
    }
}

/// 打开插件宿主窗口并加载已安装的 `.potext` 插件。
///
/// 插件加载失败只记日志，不影响应用其余部分——这是插件系统的基本原则：
/// 一个坏插件不能拖垮宿主。
pub fn open_plugin_host(cx: &mut App, store: Arc<ConfigStore>) {
    let options = WindowOptions {
        window_bounds: Some(gpui_kit::WindowBounds::Windowed(Bounds {
            origin: Point { x: px(24.0), y: px(980.0) },
            size: Size { width: px(72.0), height: px(28.0) },
        })),
        titlebar: Some(TitlebarOptions {
            title: Some("saladict-plugins".into()),
            appears_transparent: true,
            ..Default::default()
        }),
        focus: false,
        show: true,
        ..Default::default()
    };

    cx.open_window(options, |window, cx| {
        let runtime = match gpui_shell::ShellRuntime::new(cx) {
            Ok(rt) => rt,
            Err(e) => {
                log::error!("脚本运行时创建失败，插件不可用: {e}");
                let view = cx.new(|_| PluginHostView);
                return cx.new(|cx| Root::new(view, window, cx));
            }
        };
        saladict_plugin::load_installed(&runtime, window, cx, &store);
        let view = cx.new(|_| PluginHostView);
        cx.new(|cx| Root::new(view, window, cx))
    })
    .expect("打开插件宿主窗口失败");
}
