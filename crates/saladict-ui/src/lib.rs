//! saladict-ui：GPUI 窗口层。
//!
//! 对应原实现的 7 个窗口。窗口参数（尺寸、是否置顶、位置记忆）读自
//! config.json，与老版本行为一致。

pub mod config_window;
pub mod hotkey;
pub mod input_translate;
pub mod logos;
pub mod notify;
pub mod recognize;
pub mod translate_window;
pub mod tray;
pub mod updater;

use gpui_kit::component::Root;
use gpui_kit::AppContext as _;
use gpui_kit::{
    px, App, Bounds, Pixels, Point, Size, TitlebarOptions, WindowBounds, WindowHandle, WindowKind,
    WindowOptions,
};
use gpui_kit::{Global, WeakEntity};
use saladict_core::config::{config, keys};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::translate_window::TranslateWindow;
use crate::tray::{toggle_clipboard_monitor, TrayCommand};

/// 全局保存翻译窗口的弱引用与窗口句柄：view 供「复用已打开的窗口」，
/// window 供翻译完成后的自动隐藏（hide_window）等窗口级操作。
struct TranslateWindowHandle {
    view: WeakEntity<TranslateWindow>,
    window: Option<WindowHandle<Root>>,
}
impl Global for TranslateWindowHandle {}

/// `transparent` 配置 → 窗口背景外观：开启时 macOS 毛玻璃材质
/// （NSVisualEffectView），否则不透明。窗口创建时读取一次。
pub fn window_background_appearance() -> gpui_kit::WindowBackgroundAppearance {
    if config().get_or(keys::TRANSPARENT, true) {
        gpui_kit::WindowBackgroundAppearance::Blurred
    } else {
        gpui_kit::WindowBackgroundAppearance::Opaque
    }
}

/// 根容器背景色：transparent 开启时压低不透明度让材质层透出，
/// 否则保持主题原色。配合 [`window_background_appearance`] 使用。
pub fn window_root_bg(base: gpui_kit::Hsla) -> gpui_kit::Hsla {
    if config().get_or(keys::TRANSPARENT, true) {
        base.alpha(0.82)
    } else {
        base
    }
}

/// 把文本投递到翻译窗口：已打开则复用，已被关闭（close_on_blur /
/// translate_hide_window 都会移除窗口）则按当前配置重新打开，保证下次
/// 取词永远可用——与原版「隐藏后再次唤起」的体验一致。
pub fn translate_text(text: &str, cx: &mut App) {
    let need_open = match cx.try_global::<TranslateWindowHandle>() {
        Some(handle) => handle.view.upgrade().is_none(),
        None => true,
    };
    if need_open {
        open_translate_window(cx);
    }
    let handle = cx.global::<TranslateWindowHandle>();
    match handle.view.upgrade() {
        Some(view) => {
            // Entity::update 不提供 Window，用 with_window 取到该实体当前关联的窗口。
            if cx
                .with_window(view.entity_id(), |window, cx| {
                    view.update(cx, |view, cx| view.translate_text(text, window, cx));
                })
                .is_none()
            {
                log::warn!("翻译窗口当前没有关联窗口，无法翻译");
            }
        }
        None => log::error!("翻译窗口重开后仍不可用，无法翻译"),
    }
}

/// 光标处弹窗原点：以光标为窗口左上角，超出光标所在屏幕右/下边缘时反向
/// 偏移一个窗口宽/高，并夹回屏幕内（对齐原版 tauri window.rs 的 mouse 分支）。
///
/// 坐标域换算：mouse_position 与 DisplayInfo 同为系统全局坐标——macOS 是
/// points（与 gpui 逻辑坐标同域），Windows/Linux 是物理像素需除以
/// scale_factor。近似换算，边缘 clamp 偶有细微偏差，实机验证后校准。
fn mouse_follow_origin(width: f32, height: f32) -> Point<Pixels> {
    use saladict_platform::mouse::{cursor_position, monitor_at_cursor};
    let (cur_x, cur_y) = cursor_position();
    let Some(((mon_x, mon_y), (mon_w, mon_h), scale)) = monitor_at_cursor() else {
        return Point {
            x: px(100.0),
            y: px(100.0),
        };
    };

    #[cfg(target_os = "macos")]
    let to_logical = |v: i32| v as f32;
    #[cfg(not(target_os = "macos"))]
    let to_logical = |v: i32| v as f32 / scale.max(1.0);
    #[cfg(target_os = "macos")]
    let _ = scale;

    let (mon_x, mon_y) = (to_logical(mon_x), to_logical(mon_y));
    let (mon_w, mon_h) = (to_logical(mon_w as i32), to_logical(mon_h as i32));
    let (mut x, mut y) = (to_logical(cur_x), to_logical(cur_y));

    if x + width > mon_x + mon_w {
        x -= width;
        if x < mon_x {
            x = mon_x;
        }
    }
    if y + height > mon_y + mon_h {
        y -= height;
        if y < mon_y {
            y = mon_y;
        }
    }
    Point { x: px(x), y: px(y) }
}

/// 打开翻译窗口。
///
/// 尺寸取 `translate_window_width/height`，置顶跟随 `translate_always_on_top`；
/// 位置按 `translate_window_position` 分派：`mouse`（默认）跟随光标弹出并
/// 夹回光标所在屏幕，其他值（如 `remember`）用 `translate_window_position_x/y`。
pub fn open_translate_window(cx: &mut App) {
    let width: f32 = config().get_or(keys::TRANSLATE_WINDOW_WIDTH, 350.0);
    let height: f32 = config().get_or(keys::TRANSLATE_WINDOW_HEIGHT, 420.0);
    let always_on_top: bool = config().get_or(keys::TRANSLATE_ALWAYS_ON_TOP, true);

    let origin =
        if config().get_or(keys::TRANSLATE_WINDOW_POSITION, String::from("mouse")) == "mouse" {
            mouse_follow_origin(width, height)
        } else {
            Point {
                x: px(config().get_or(keys::TRANSLATE_WINDOW_POSITION_X, 100.0)),
                y: px(config().get_or(keys::TRANSLATE_WINDOW_POSITION_Y, 100.0)),
            }
        };

    let bounds = Bounds {
        origin,
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
        cx.set_global(TranslateWindowHandle {
            view: view.downgrade(),
            window: None,
        });
        cx.new(|cx| Root::new(view, window, cx))
    })
    .inspect(|handle| {
        // 补记窗口句柄，供翻译完成后 hide_window 等窗口级操作使用。
        cx.global_mut::<TranslateWindowHandle>().window = Some(*handle);
    })
    .expect("打开翻译窗口失败");
}

/// GPUI 启动入口：打开主窗口，安装托盘，并启动命令消费循环。
///
/// `tx`/`rx` 是 `app` 创建的托盘命令 channel：`tx` 给托盘菜单事件线程与全局快捷键
/// 回调复用，`rx` 在本函数里被 `cx.spawn` 的消费循环持有。
pub fn launch(
    cx: &mut App,
    tx: UnboundedSender<TrayCommand>,
    mut rx: UnboundedReceiver<TrayCommand>,
) {
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
                        .spawn_blocking(saladict_platform::selection::selected_text)
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
                    cx.update(|cx| translate_text(&text, cx));
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
                        .spawn_blocking(saladict_platform::screenshot::capture_interactive)
                        .await
                    {
                        Ok(Ok(bytes)) if !bytes.is_empty() => {
                            cx.update(|cx| crate::recognize::open_recognize_with_image(cx, bytes));
                        }
                        Ok(Err(e)) => log::warn!("截图已取消或失败: {e}"),
                        Ok(_) => log::warn!("截图为空"),
                        Err(e) => log::warn!("截图任务被取消: {e}"),
                    }
                }
                TrayCommand::InputTranslate => {
                    cx.update(input_translate::open_input_translate);
                }
                TrayCommand::OpenSettings => {
                    cx.update(crate::config_window::open_config);
                }
                TrayCommand::SetAutoCopy(mode) => {
                    let _ = saladict_core::config::config().set(
                        saladict_core::config::keys::TRANSLATE_AUTO_COPY,
                        &mode.as_str().to_string(),
                    );
                    // 同步单选组勾选态（CheckMenuItem 仅限主线程操作）。
                    cx.update(|_cx| tray::set_auto_copy_checked(mode));
                }
                TrayCommand::CheckUpdate => {
                    cx.update(crate::updater::open_updater);
                }
                TrayCommand::ViewLog => {
                    let dir = saladict_core::config::config().app_dir().join("logs");
                    if let Err(e) = saladict_platform::shell::open_path(&dir) {
                        log::warn!("打开日志目录失败: {e}");
                    }
                }
                TrayCommand::Restart => {
                    // 先移除单实例锁再拉起新进程：锁内 PID 尚存活时新进程会
                    // 判定为「已有实例」而直接退出。锁路径与 app 入口共用。
                    let _ = std::fs::remove_file(saladict_core::config::runtime_lock_path());
                    match std::env::current_exe()
                        .and_then(|exe| std::process::Command::new(exe).spawn())
                    {
                        Ok(_) => {
                            cx.update(|cx| cx.quit());
                        }
                        Err(e) => log::error!("重启失败: {e}"),
                    }
                } // 不写兜底分支：`TrayCommand` 一旦新增变体，这里会直接编译失败，
                  // 强迫显式决定处理方式，而不是默默落进日志里。
            }
        }
    })
    .detach();
}
