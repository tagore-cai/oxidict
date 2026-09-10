//! 系统托盘：图标 + 右键菜单，菜单点击通过 channel 桥接进 GPUI。
//!
//! tray-icon 在 macOS 上必须在主线程创建（GPUI 事件循环所在线程），因此
//! [`install`] 由 `oxidict_ui::launch` 在 `gpui_kit::init` 之后调用。菜单事件
//! 由独立线程从 `MenuEvent::receiver()` 读取，转成 [`TrayCommand`] 发到 channel，
//! 再由 GPUI 侧的命令循环消费——这样 GUI 状态变更都发生在 GPUI 执行器上。

use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicPtr, Ordering};

use oxidict_core::i18n::{t, t_args};

use tokio::sync::mpsc::UnboundedSender;
use tray_icon::menu::{
    CheckMenuItem, CheckMenuItemBuilder, Menu, MenuEvent, MenuId, MenuItemBuilder,
    PredefinedMenuItem, Submenu,
};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

/// 托盘菜单触发的命令。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCommand {
    SelectionTranslate,
    InputTranslate,
    OcrRecognize,
    OcrTranslate,
    ToggleClipboardMonitor,
    OpenSettings,
    /// 设置 translate_auto_copy（托盘「自动复制」子菜单，单选组）。
    SetAutoCopy(AutoCopyMode),
    /// 检查更新（打开更新窗口）。
    CheckUpdate,
    /// 查看日志（dev_mode 时出现在菜单，打开日志目录）。
    ViewLog,
    /// 重启应用（先移除单实例锁再拉起新进程退出）。
    Restart,
    Quit,
}

/// `translate_auto_copy` 的取值域，对齐原版 disable | source | target | source_target。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoCopyMode {
    Source,
    Target,
    SourceTarget,
    Disable,
}

impl AutoCopyMode {
    pub fn as_str(self) -> &'static str {
        match self {
            AutoCopyMode::Source => "source",
            AutoCopyMode::Target => "target",
            AutoCopyMode::SourceTarget => "source_target",
            AutoCopyMode::Disable => "disable",
        }
    }

    fn from_config() -> Self {
        let v = oxidict_core::config::config().get_or(
            oxidict_core::config::keys::TRANSLATE_AUTO_COPY,
            String::from("disable"),
        );
        match v.as_str() {
            "source" => AutoCopyMode::Source,
            "target" => AutoCopyMode::Target,
            "source_target" => AutoCopyMode::SourceTarget,
            _ => AutoCopyMode::Disable,
        }
    }
}

/// 托盘运行期状态：勾选项句柄 + 当前剪切板监听句柄。
///
/// `CheckMenuItem` 不是 `Send`/`Sync`（内部持有平台原生菜单对象），不能进普通
/// `static`/`Mutex`。它只在主线程被访问（创建与切换都在 GPUI 主线程），故用裸指针
/// 常驻保存即可。托盘图标本身用 `Box::leak` 常驻。
struct TrayState {
    clipboard_item: CheckMenuItem,
    /// 自动复制单选组：顺序与 [`AutoCopyMode`] 四个值一一对应。
    copy_items: [CheckMenuItem; 4],
    monitor: Option<oxidict_platform::clipboard::MonitorHandle>,
}

static TRAY_STATE: OnceLock<AtomicPtr<TrayState>> = OnceLock::new();

/// 全局命令发送端：托盘安装时存入，供快捷键录制等非托盘模块发命令复用。
///
/// `UnboundedSender` 是 `Clone + Send + Sync`，全局持有无碍；命令循环唯一，
/// 不存在多发送端语义分歧。
static COMMAND_TX: OnceLock<UnboundedSender<TrayCommand>> = OnceLock::new();

/// 向 GPUI 命令循环发送一条托盘命令。
///
/// 返回 `false` 表示托盘未安装（如 CLI 模式）或命令循环已退出，调用方自行
/// 决定是否记日志。
pub fn send_command(cmd: TrayCommand) -> bool {
    match COMMAND_TX.get() {
        Some(tx) => tx.send(cmd).is_ok(),
        None => false,
    }
}

/// 创建托盘图标与菜单，并起线程把菜单事件转成 [`TrayCommand`] 发到 `sender`。
///
/// 调用方需保证在主线程（GPUI run 闭包内、`gpui_kit::init` 之后）调用本函数。
pub fn install(sender: UnboundedSender<TrayCommand>) -> anyhow::Result<()> {
    // 先存全局发送端，快捷键热更新依赖它（即使托盘创建失败也已就绪）。
    let _ = COMMAND_TX.set(sender.clone());
    let icon = build_icon()?;

    let selection = MenuItemBuilder::new()
        .text(t("tray-selection-translate"))
        .id(MenuId::new("selection_translate"))
        .enabled(true)
        .build();
    let input = MenuItemBuilder::new()
        .text(t("tray-input-translate"))
        .id(MenuId::new("input_translate"))
        .enabled(true)
        .build();
    let ocr = MenuItemBuilder::new()
        .text(t("tray-ocr-recognize"))
        .id(MenuId::new("ocr_recognize"))
        .enabled(true)
        .build();
    let ocr_translate = MenuItemBuilder::new()
        .text(t("tray-ocr-translate"))
        .id(MenuId::new("ocr_translate"))
        .enabled(true)
        .build();
    let clipboard = CheckMenuItemBuilder::new()
        .text(t("tray-clipboard-monitor"))
        .id(MenuId::new("toggle_clipboard"))
        .checked(false)
        .enabled(true)
        .build();

    // 自动复制子菜单：单选组，选中态来自 translate_auto_copy 当前值。
    let mode = AutoCopyMode::from_config();
    let build_copy_item = |id: &str, label: String, checked: bool| {
        CheckMenuItemBuilder::new()
            .text(label)
            .id(MenuId::new(id))
            .checked(checked)
            .enabled(true)
            .build()
    };
    let copy_source = build_copy_item(
        "copy_source",
        t("tray-copy-source").to_string(),
        mode == AutoCopyMode::Source,
    );
    let copy_target = build_copy_item(
        "copy_target",
        t("tray-copy-target").to_string(),
        mode == AutoCopyMode::Target,
    );
    let copy_source_target = build_copy_item(
        "copy_source_target",
        t("tray-copy-source-target").to_string(),
        mode == AutoCopyMode::SourceTarget,
    );
    let copy_disable = build_copy_item(
        "copy_disable",
        t("tray-copy-disable").to_string(),
        mode == AutoCopyMode::Disable,
    );
    let auto_copy = Submenu::with_id(MenuId::new("auto_copy"), t("tray-auto-copy"), true);
    auto_copy
        .append_items(&[
            &copy_source,
            &copy_target,
            &copy_source_target,
            &PredefinedMenuItem::separator(),
            &copy_disable,
        ])
        .map_err(|e| {
            anyhow::anyhow!(t_args("tray-error-menu-build", &[("err", &e.to_string())]))
        })?;

    let settings = MenuItemBuilder::new()
        .text(t("tray-settings"))
        .id(MenuId::new("open_settings"))
        .enabled(true)
        .build();
    let check_update = MenuItemBuilder::new()
        .text(t("tray-check-update"))
        .id(MenuId::new("check_update"))
        .enabled(true)
        .build();
    // 查看日志：仅开发者模式显示（对齐原版 add_developer_menu）。
    let dev_mode =
        oxidict_core::config::config().get_or(oxidict_core::config::keys::DEV_MODE, false);
    let view_log = MenuItemBuilder::new()
        .text(t("tray-view-log"))
        .id(MenuId::new("view_log"))
        .enabled(true)
        .build();
    let restart = MenuItemBuilder::new()
        .text(t("tray-restart"))
        .id(MenuId::new("restart"))
        .enabled(true)
        .build();
    let quit = MenuItemBuilder::new()
        .text(t("tray-quit"))
        .id(MenuId::new("quit"))
        .enabled(true)
        .build();

    let menu = Menu::new();
    menu.append_items(&[
        &selection,
        &input,
        &clipboard,
        &auto_copy,
        &PredefinedMenuItem::separator(),
        &ocr,
        &ocr_translate,
        &PredefinedMenuItem::separator(),
        &settings,
        &check_update,
    ])
    .map_err(|e| anyhow::anyhow!(t_args("tray-error-menu-build", &[("err", &e.to_string())])))?;
    if dev_mode {
        menu.append(&view_log).map_err(|e| {
            anyhow::anyhow!(t_args("tray-error-menu-build", &[("err", &e.to_string())]))
        })?;
    }
    // 原来这一行没有 `?`：追加分隔符/重启/退出失败会被静默吞掉，
    // 表现为托盘菜单缺项但无任何日志。
    menu.append_items(&[&PredefinedMenuItem::separator(), &restart, &quit])
        .map_err(|e| {
            anyhow::anyhow!(t_args("tray-error-menu-build", &[("err", &e.to_string())]))
        })?;

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        // 左键不弹菜单：左键点击走 tray_click_event 分发（对齐原版
        // on_tray_click），菜单只留给右键。
        .with_menu_on_left_click(false)
        .with_tooltip(t("app-name"))
        .with_icon(icon)
        .build()
        .map_err(|e| {
            anyhow::anyhow!(t_args("tray-error-icon-build", &[("err", &e.to_string())]))
        })?;
    // 图标常驻进程生命周期，drop 会导致托盘消失。
    let _tray: &'static TrayIcon = Box::leak(Box::new(tray));

    let state = Box::leak(Box::new(TrayState {
        clipboard_item: clipboard,
        copy_items: [copy_source, copy_target, copy_source_target, copy_disable],
        monitor: None,
    }));
    let _ = TRAY_STATE.set(AtomicPtr::new(state as *mut TrayState));

    std::thread::Builder::new()
        .name("oxidict-tray".into())
        .spawn(move || {
            let receiver = MenuEvent::receiver();
            // 通道关闭（极少发生）即结束线程。
            while let Ok(event) = receiver.recv() {
                let cmd = match event.id {
                    id if id == MenuId::new("selection_translate") => {
                        TrayCommand::SelectionTranslate
                    }
                    id if id == MenuId::new("input_translate") => TrayCommand::InputTranslate,
                    id if id == MenuId::new("ocr_recognize") => TrayCommand::OcrRecognize,
                    id if id == MenuId::new("ocr_translate") => TrayCommand::OcrTranslate,
                    id if id == MenuId::new("toggle_clipboard") => {
                        TrayCommand::ToggleClipboardMonitor
                    }
                    id if id == MenuId::new("open_settings") => TrayCommand::OpenSettings,
                    id if id == MenuId::new("copy_source") => {
                        TrayCommand::SetAutoCopy(AutoCopyMode::Source)
                    }
                    id if id == MenuId::new("copy_target") => {
                        TrayCommand::SetAutoCopy(AutoCopyMode::Target)
                    }
                    id if id == MenuId::new("copy_source_target") => {
                        TrayCommand::SetAutoCopy(AutoCopyMode::SourceTarget)
                    }
                    id if id == MenuId::new("copy_disable") => {
                        TrayCommand::SetAutoCopy(AutoCopyMode::Disable)
                    }
                    id if id == MenuId::new("check_update") => TrayCommand::CheckUpdate,
                    id if id == MenuId::new("view_log") => TrayCommand::ViewLog,
                    id if id == MenuId::new("restart") => TrayCommand::Restart,
                    id if id == MenuId::new("quit") => TrayCommand::Quit,
                    _ => continue,
                };
                if sender.send(cmd).is_err() {
                    break;
                }
            }
        })
        .expect("failed to spawn tray event thread");

    // 左键点击：按 tray_click_event 分发（对齐原版 on_tray_click 语义）。
    // 独立线程读图标事件通道；config() 是 Arc 全局单例，跨线程读取安全。
    std::thread::Builder::new()
        .name("oxidict-tray-click".into())
        .spawn(|| {
            let receiver = TrayIconEvent::receiver();
            for event in receiver.iter() {
                // 只响应左键释放。
                let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                else {
                    continue;
                };
                let action = oxidict_core::config::config()
                    .get::<String>(oxidict_core::config::keys::TRAY_CLICK_EVENT)
                    .unwrap_or_default();
                let cmd = match action.as_str() {
                    "translate" => TrayCommand::InputTranslate,
                    "ocr_recognize" => TrayCommand::OcrRecognize,
                    "ocr_translate" => TrayCommand::OcrTranslate,
                    "disable" => continue,
                    // 缺省（含未设置/非法值）：打开设置，与原版一致。
                    _ => TrayCommand::OpenSettings,
                };
                if !send_command(cmd) {
                    break;
                }
            }
        })
        .expect("failed to spawn tray click thread");

    Ok(())
}

/// 切换剪切板监听（由 `ToggleClipboardMonitor` 命令驱动），并同步菜单勾选状态。
pub fn toggle_clipboard_monitor() {
    let Some(atomic) = TRAY_STATE.get() else {
        log::warn!("{}", t("tray-warn-not-init"));
        return;
    };
    let ptr = atomic.load(Ordering::Relaxed);
    // SAFETY: ptr 来自 init() 时 `Box::into_raw`（非空、释放权归本模块，进程
    // 生命周期内不回收）；所有调用点（托盘菜单回调、GPUI 命令循环）都在
    // 主线程串行执行，同一时刻不存在其他引用，`&mut` 不构成别名违例。
    let state = unsafe { &mut *ptr };
    match state.monitor.take() {
        Some(handle) => {
            handle.stop();
            state.clipboard_item.set_checked(false);
            log::info!("{}", t("tray-clipboard-off"));
        }
        _ => {
            let handle = oxidict_platform::clipboard::start_monitor(Arc::new(|text| {
                log::info!(
                    "{}",
                    t_args(
                        "tray-clipboard-captured",
                        &[("count", &text.chars().count().to_string())]
                    )
                );
            }));
            state.monitor = Some(handle);
            state.clipboard_item.set_checked(true);
            log::info!("{}", t("tray-clipboard-on"));
        }
    }
}

/// 同步「自动复制」单选组的勾选态（GPUI 主线程调用）。
pub fn set_auto_copy_checked(mode: AutoCopyMode) {
    let Some(atomic) = TRAY_STATE.get() else {
        return;
    };
    let ptr = atomic.load(Ordering::Relaxed);
    // SAFETY: 同 toggle_clipboard_monitor——Box::into_raw 的指针由本模块
    // 独占释放，调用点都在 GPUI 主线程串行执行，无并发别名。
    let state = unsafe { &mut *ptr };
    let modes = [
        AutoCopyMode::Source,
        AutoCopyMode::Target,
        AutoCopyMode::SourceTarget,
        AutoCopyMode::Disable,
    ];
    for (item, m) in state.copy_items.iter().zip(modes) {
        item.set_checked(m == mode);
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
    Icon::from_rgba(rgba, SIZE, SIZE)
        .map_err(|e| anyhow::anyhow!(t_args("tray-error-icon-gen", &[("err", &e.to_string())])))
}
