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
