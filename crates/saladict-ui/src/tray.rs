//! 系统托盘：图标 + 右键菜单，菜单点击通过 channel 桥接进 GPUI。
//!
//! tray-icon 在 macOS 上必须在主线程创建（GPUI 事件循环所在线程），因此
//! [`install`] 由 `saladict_ui::launch` 在 `gpui_kit::init` 之后调用。菜单事件
//! 由独立线程从 `MenuEvent::receiver()` 读取，转成 [`TrayCommand`] 发到 channel，
//! 再由 GPUI 侧的命令循环消费——这样 GUI 状态变更都发生在 GPUI 执行器上。

use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicPtr, Ordering};

use tokio::sync::mpsc::UnboundedSender;
use tray_icon::menu::{
    CheckMenuItem, CheckMenuItemBuilder, Menu, MenuEvent, MenuId, MenuItemBuilder,
};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

/// 托盘菜单触发的命令。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCommand {
    SelectionTranslate,
    InputTranslate,
    OcrRecognize,
    OcrTranslate,
    ToggleClipboardMonitor,
    OpenSettings,
    Quit,
}

/// 托盘运行期状态：勾选项句柄 + 当前剪切板监听句柄。
///
/// `CheckMenuItem` 不是 `Send`/`Sync`（内部持有平台原生菜单对象），不能进普通
/// `static`/`Mutex`。它只在主线程被访问（创建与切换都在 GPUI 主线程），故用裸指针
/// 常驻保存即可。托盘图标本身用 `Box::leak` 常驻。
struct TrayState {
    clipboard_item: CheckMenuItem,
    monitor: Option<saladict_platform::clipboard::MonitorHandle>,
}

static TRAY_STATE: OnceLock<AtomicPtr<TrayState>> = OnceLock::new();

/// 创建托盘图标与菜单，并起线程把菜单事件转成 [`TrayCommand`] 发到 `sender`。
///
/// 调用方需保证在主线程（GPUI run 闭包内、`gpui_kit::init` 之后）调用本函数。
pub fn install(sender: UnboundedSender<TrayCommand>) -> anyhow::Result<()> {
    let icon = build_icon()?;

    let selection = MenuItemBuilder::new()
        .text("划词翻译")
        .id(MenuId::new("selection_translate"))
        .enabled(true)
        .build();
    let input = MenuItemBuilder::new()
        .text("输入翻译")
        .id(MenuId::new("input_translate"))
        .enabled(true)
        .build();
    let ocr = MenuItemBuilder::new()
        .text("截图 OCR")
        .id(MenuId::new("ocr_recognize"))
        .enabled(true)
        .build();
    let ocr_translate = MenuItemBuilder::new()
        .text("截图翻译")
        .id(MenuId::new("ocr_translate"))
        .enabled(true)
        .build();
    let clipboard = CheckMenuItemBuilder::new()
        .text("监听剪切板")
        .id(MenuId::new("toggle_clipboard"))
        .checked(false)
        .enabled(true)
        .build();
    let settings = MenuItemBuilder::new()
        .text("偏好设置")
        .id(MenuId::new("open_settings"))
        .enabled(true)
        .build();
    let quit = MenuItemBuilder::new()
        .text("退出")
        .id(MenuId::new("quit"))
        .enabled(true)
        .build();

    let menu = Menu::new();
    menu.append_items(&[
        &selection,
        &input,
        &ocr,
        &ocr_translate,
        &clipboard,
        &settings,
        &quit,
    ])
    .map_err(|e| anyhow::anyhow!("菜单组装失败: {e}"))?;

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("沙拉翻译")
        .with_icon(icon)
        .build()
        .map_err(|e| anyhow::anyhow!("托盘图标创建失败: {e}"))?;
    // 图标常驻进程生命周期，drop 会导致托盘消失。
    let _tray: &'static TrayIcon = Box::leak(Box::new(tray));

    let state = Box::leak(Box::new(TrayState {
        clipboard_item: clipboard,
        monitor: None,
    }));
    let _ = TRAY_STATE.set(AtomicPtr::new(state as *mut TrayState));

    std::thread::Builder::new()
        .name("saladict-tray".into())
        .spawn(move || {
            let receiver = MenuEvent::receiver();
            loop {
                let Ok(event) = receiver.recv() else {
                    // 通道关闭（极少发生），结束线程。
                    break;
                };
                let cmd = match event.id {
                    id if id == MenuId::new("selection_translate") => TrayCommand::SelectionTranslate,
                    id if id == MenuId::new("input_translate") => TrayCommand::InputTranslate,
                    id if id == MenuId::new("ocr_recognize") => TrayCommand::OcrRecognize,
                    id if id == MenuId::new("ocr_translate") => TrayCommand::OcrTranslate,
                    id if id == MenuId::new("toggle_clipboard") => TrayCommand::ToggleClipboardMonitor,
                    id if id == MenuId::new("open_settings") => TrayCommand::OpenSettings,
                    id if id == MenuId::new("quit") => TrayCommand::Quit,
                    _ => continue,
                };
                if sender.send(cmd).is_err() {
                    break;
                }
            }
        })
        .expect("failed to spawn tray event thread");

    Ok(())
}

/// 切换剪切板监听（由 `ToggleClipboardMonitor` 命令驱动），并同步菜单勾选状态。
pub fn toggle_clipboard_monitor() {
    let Some(atomic) = TRAY_STATE.get() else {
        log::warn!("托盘尚未初始化，无法切换剪切板监听");
        return;
    };
    let ptr = atomic.load(Ordering::Relaxed);
    // 仅在主线程访问，安全。
    let state = unsafe { &mut *ptr };
    if state.monitor.is_some() {
        state.monitor.take().unwrap().stop();
        state.clipboard_item.set_checked(false);
        log::info!("剪切板监听已关闭");
    } else {
        let handle = saladict_platform::clipboard::start_monitor(Arc::new(|text| {
            log::info!("剪切板捕获: {} 字符", text.chars().count());
        }));
        state.monitor = Some(handle);
        state.clipboard_item.set_checked(true);
        log::info!("剪切板监听已开启");
    }
}

/// 纯色占位图标（工程暂无自带图标资源，画一个沙拉绿方块）。
fn build_icon() -> anyhow::Result<Icon> {
    const SIZE: u32 = 32;
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    // #2EC27E
    for _ in 0..(SIZE * SIZE) {
        rgba.extend_from_slice(&[0x2E, 0xC2, 0x7E, 0xFF]);
    }
    Icon::from_rgba(rgba, SIZE, SIZE).map_err(|e| anyhow::anyhow!("图标生成失败: {e}"))
}
