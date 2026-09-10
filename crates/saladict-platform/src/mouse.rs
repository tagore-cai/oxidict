//! 鼠标能力：光标位置。
//!
//! 原 rdev 的鼠标释放钩子随 rdev 一并撤除（rdev 的键盘 tap 在新版 macOS
//! 上有主队列断言崩溃问题，见 hotkey.rs 顶部说明）。
//! 划词触发改为「快捷键 / 托盘菜单 → selected_text()」，与原版热键划词行为一致。

/// 当前光标所在的屏幕物理坐标。
pub fn cursor_position() -> (i32, i32) {
    use mouse_position::mouse_position::Mouse;
    match Mouse::get_mouse_position() {
        mouse_position::mouse_position::Mouse::Position { x, y } => (x, y),
        mouse_position::mouse_position::Mouse::Error => (0, 0),
    }
}

/// 显示器信息：`((原点 x, y), (宽, 高), scale_factor)`。
///
/// 坐标均为系统全局坐标（macOS 为 points、Windows/Linux 为物理像素），
/// 与 [`cursor_position`] 同域；跨到窗口逻辑坐标的换算由调用方按平台处理。
pub type MonitorInfo = ((i32, i32), (u32, u32), f32);

/// 光标所在显示器的信息。光标取不到或显示器枚举失败时返回 `None`。
pub fn monitor_at_cursor() -> Option<MonitorInfo> {
    let (mx, my) = cursor_position();
    let d = screenshots::DisplayInfo::from_point(mx, my).ok()?;
    Some(((d.x, d.y), (d.width, d.height), d.scale_factor))
}
