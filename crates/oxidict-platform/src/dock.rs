//! macOS Dock 图标可见性（对齐原版 `hide_dock_icon`）。
//!
//! 原版用 tauri 的 `set_activation_policy(SystemTray)`：应用不出现在 Dock，
//! 但窗口仍可创建与聚焦，交互入口只剩托盘。这里等价实现：
//! `NSApplicationActivationPolicy::Accessory`（SystemTray 的原生名）。
//!
//! Windows 没有任务栏隐藏的等价语义（原版 `skip_taskbar` 需窗口句柄，收益低）；
//! Linux 桌面环境差异过大。两者均为 no-op。
//!
//! # 线程约定
//!
//! `setActivationPolicy` 必须在主线程调用。调用点只有两处：main 启动序列
//! （主线程）与 GPUI 事件循环回调（也在主线程），见 [`set_visible`] 内的
//! 主线程检查兜底。

/// 设置 Dock 图标可见性。
///
/// `visible = false` 时应用从 Dock 消失（Accessory 策略）；托盘图标不受影响。
pub fn set_visible(visible: bool) {
    #[cfg(target_os = "macos")]
    {
        use objc2::MainThreadMarker;
        use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};

        // 主线程检查：拿不到 MainThreadMarker 说明在非主线程，拒绝执行而不是
        // 静默产生未定义行为。
        let Some(mtm) = MainThreadMarker::new() else {
            log::warn!("dock::set_visible 必须在主线程调用，本次调用已忽略");
            return;
        };
        // SAFETY：sharedApplication 要求主线程，mtm 已证明。
        let app = NSApplication::sharedApplication(mtm);
        let policy = if visible {
            NSApplicationActivationPolicy::Regular
        } else {
            NSApplicationActivationPolicy::Accessory
        };
        // 当前 objc2-app-kit 版本已把 setActivationPolicy 标为 safe（内部自带
        // MainThreadMarker 约束），无需 unsafe 块。
        app.setActivationPolicy(policy);
        log::info!("Dock icon {visible}");
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = visible;
    }
}

/// 按配置应用 Dock 图标可见性（启动序列与 General 页开关共用）。
pub fn apply_from_config() {
    let hide =
        oxidict_core::config::config().get_or(oxidict_core::config::keys::HIDE_DOCK_ICON, false);
    set_visible(!hide);
}
