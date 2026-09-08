//! saladict-ui：GPUI 窗口层。
//!
//! 对应原实现的 7 个窗口。窗口参数（尺寸、是否置顶、位置记忆）读自
//! config.json，与老版本行为一致。

pub mod translate_window;
pub mod tray;
pub mod notify;
pub mod updater;
pub mod recognize;
pub mod plugin_host;
pub mod config_window;
pub mod input_translate;

use gpui_kit::component::Root;
use gpui_kit::AppContext as _;
use gpui_kit::{Global, WeakEntity};
use gpui_kit::{
    px, App, Bounds, Point, Size, TitlebarOptions, WindowBounds, WindowKind, WindowOptions,
};
use saladict_core::config::{config, keys};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::tray::{toggle_clipboard_monitor, TrayCommand};
use crate::translate_window::TranslateWindow;

/// 全局保存翻译窗口的弱引用，命令循环据此「复用已打开的窗口」。
struct TranslateWindowHandle(WeakEntity<TranslateWindow>);
impl Global for TranslateWindowHandle {}

/// 把文本投递到已打开的翻译窗口（不存在则忽略并告警）。
pub fn translate_text(text: &str, cx: &mut App) {
    if let Some(handle) = cx.try_global::<TranslateWindowHandle>() {
        if let Some(view) = handle.0.upgrade() {
            // Entity::update 不提供 Window，用 with_window 取到该实体当前关联的窗口。
            if cx
                .with_window(view.entity_id(), |window, cx| {
                    view.update(cx, |view, cx| view.translate_text(text, window, cx));
                })
                .is_none()
            {
                log::warn!("翻译窗口当前没有关联窗口，无法翻译");
            }
        } else {
            log::warn!("翻译窗口已被销毁，无法复用");
        }
    } else {
        log::warn!("翻译窗口尚未初始化，无法翻译");
    }
}

/// 打开翻译窗口。
///
/// 尺寸取 `translate_window_width/height`，置顶跟随 `translate_always_on_top`，
/// 位置记忆在 `translate_window_position_x/y`（与老版本键名一致）。
pub fn open_translate_window(cx: &mut App) {
    let width: f32 = config().get_or(keys::TRANSLATE_WINDOW_WIDTH, 350.0);
    let height: f32 = config().get_or(keys::TRANSLATE_WINDOW_HEIGHT, 420.0);
    let always_on_top: bool = config().get_or(keys::TRANSLATE_ALWAYS_ON_TOP, true);

    let bounds = Bounds {
        origin: Point {
            x: px(config().get_or(keys::TRANSLATE_WINDOW_POSITION_X, 100.0)),
            y: px(config().get_or(keys::TRANSLATE_WINDOW_POSITION_Y, 100.0)),
        },
        size: Size {
            width: px(width),
            height: px(height),
        },
    };

    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some("沙拉翻译".into()),
            appears_transparent: true,
            ..Default::default()
        }),
        focus: false,
        show: true,
        kind: if always_on_top {
            WindowKind::PopUp
        } else {
            WindowKind::Normal
        },
        ..Default::default()
    };

    cx.open_window(options, |window, cx| {
        let view = cx.new(|cx| translate_window::TranslateWindow::new(window, cx));
        // 记录弱引用，后续托盘/快捷键命令可复用此窗口。
        cx.set_global(TranslateWindowHandle(view.downgrade()));
        cx.new(|cx| Root::new(view, window, cx))
    })
    .expect("打开翻译窗口失败");
}

/// GPUI 启动入口：打开主窗口，安装托盘，并启动命令消费循环。
///
/// `tx`/`rx` 是 `app` 创建的托盘命令 channel：`tx` 给托盘菜单事件线程与全局快捷键
/// 回调复用，`rx` 在本函数里被 `cx.spawn` 的消费循环持有。
pub fn launch(cx: &mut App, tx: UnboundedSender<TrayCommand>, mut rx: UnboundedReceiver<TrayCommand>) {
    open_translate_window(cx);

    // 托盘必须在主线程、gpui_kit::init 之后创建（macOS 限制）。
    if let Err(e) = tray::install(tx) {
        log::error!("托盘初始化失败: {e}");
    }

    cx.spawn(async move |cx| {
        while let Some(cmd) = rx.recv().await {
            match cmd {
                TrayCommand::SelectionTranslate => {
                    // selected_text() 是阻塞调用（macOS 走 AX/剪切板），丢到 tokio
                    // 线程池，JoinHandle 不依赖 tokio 线程局部，可在 GPUI 上 await。
                    let text = match saladict_core::runtime::handle()
                        .spawn_blocking(|| saladict_platform::selection::selected_text())
                        .await
                    {
                        Ok(Ok(text)) => text,
                        Ok(Err(e)) => {
                            log::warn!("取词失败: {e}");
                            continue;
                        }
                        Err(e) => {
                            log::warn!("取词任务被取消: {e}");
                            continue;
                        }
                    };
                    let _ = cx.update(|cx| translate_text(&text, cx));
                }
                TrayCommand::Quit => {
                    cx.update(|cx| cx.quit());
                }
                TrayCommand::ToggleClipboardMonitor => {
                    toggle_clipboard_monitor();
                }
                // 交互式框选是阻塞等待（macOS 走 `screencapture -i`），必须丢到
                // tokio 线程池，不能卡住 GPUI 主线程。
                TrayCommand::OcrRecognize | TrayCommand::OcrTranslate => {
                    match saladict_core::runtime::handle()
                        .spawn_blocking(|| saladict_platform::screenshot::capture_interactive())
                        .await
                    {
                        Ok(Ok(bytes)) if !bytes.is_empty() => {
                            let _ = cx.update(|cx| {
                                crate::recognize::open_recognize_with_image(cx, bytes)
                            });
                        }
                        Ok(Err(e)) => log::warn!("截图已取消或失败: {e}"),
                        Ok(_) => log::warn!("截图为空"),
                        Err(e) => log::warn!("截图任务被取消: {e}"),
                    }
                }
                TrayCommand::InputTranslate => {
                    let _ = cx.update(|cx| input_translate::open_input_translate(cx));
                }
                TrayCommand::OpenSettings => {
                    let _ = cx.update(|cx| crate::config_window::open_config(cx));
                }
                other => {
                    log::info!("收到未处理的托盘命令: {other:?}");
                }
            }
        }
    })
    .detach();
}
