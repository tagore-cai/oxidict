//! 快捷键录制组件。
//!
//! 对齐原版「点击输入框 → 直接按组合键自动录入」的交互：可聚焦的槽，聚焦后
//! 按下组合键即合成 `Ctrl+Alt+T` 风格字符串写入值，按 `Backspace`/`Delete`
//! 清空。替代设置页里「手动敲字符串」的普通 [`Input`](gpui_kit::component::input::Input)。
//!
//! 只捕获**含修饰键**的组合（避免把误按的单个字母录进去），并通过
//! `prevent_default` + `stop_propagation` 屏蔽默认输入行为。
//!
//! 职责分离：这里负责**录入**（组合键 → config 字符串），平台全局快捷键
//! （`oxidict_platform::hotkey`，keytap 观察流）负责**生效**。产物字符串
//! 直接兼容 `hotkey::parse`。

use crate::tray::{TrayCommand, send_command};
use gpui_kit::component::{ActiveTheme, Sizable, Size, StyledExt};
use gpui_kit::{
    App, Context, Empty, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement,
    KeyDownEvent, Keystroke, ParentElement, Render, RenderOnce, SharedString, StyleRefinement,
    Styled, Window, div, px,
};

/// 把 gpui 的按键事件合成 `Oxidict` 配置用的加速键字符串（`Ctrl+Alt+T`）。
///
/// - 只接受**含修饰键**的组合（无修饰键的单个字符不构成热键，返回 `None`）。
/// - 键名映射为 `oxidict_platform::hotkey::parse` 认识的名称。
///
/// 未在 `parse` 支持范围内的键返回 `None`。
pub fn to_accelerator(keystroke: &Keystroke) -> Option<String> {
    let m = keystroke.modifiers;
    if !m.modified() {
        return None;
    }
    let key = normalize_key(&keystroke.key)?;
    let mut parts: Vec<&str> = Vec::with_capacity(4);
    if m.shift {
        parts.push("Shift");
    }
    if m.control {
        parts.push("Ctrl");
    }
    if m.alt {
        parts.push("Alt");
    }
    if m.platform {
        // macOS 显示 Command；hotkey::parse 对 cmd/command 均归一为 Meta。
        parts.push("Cmd");
    }
    parts.push(&key);
    Some(parts.join("+"))
}

/// 是否应清空快捷键（Backspace / Delete）。
#[inline]
pub fn is_clear_key(cx_key: &str) -> bool {
    matches!(cx_key.to_ascii_lowercase().as_str(), "backspace" | "delete")
}

/// 快捷键绑定名 -> 触发命令。与 main.rs 启动注册共用，收敛映射到一处。
fn command_for(name: &str) -> Option<TrayCommand> {
    Some(match name {
        "selection_translate" => TrayCommand::SelectionTranslate,
        "input_translate" => TrayCommand::InputTranslate,
        "ocr_recognize" => TrayCommand::OcrRecognize,
        "ocr_translate" => TrayCommand::OcrTranslate,
        _ => return None,
    })
}

/// 应用一个快捷键绑定——**生效链路的唯一入口**（热更新）。
///
/// 非空加速键即注册（platform registry 同名覆盖旧绑定），空串注销。handler
/// 经托盘命令 channel 桥接进 GPUI 命令循环，与托盘菜单、剪切板开关同路。
/// 配置写入由调用方负责（通常紧邻调用 `config().set`），本函数只管「生效」。
///
/// 启动时的批量注册（main.rs）也走这里，保证映射与语义只此一份。
pub fn apply_hotkey(name: &str, accelerator: &str) {
    let accel = accelerator.trim();
    if accel.is_empty() {
        oxidict_platform::hotkey::unregister(name);
        log::info!("已注销快捷键 {name}");
        return;
    }
    let Some(cmd) = command_for(name) else {
        log::warn!("未知快捷键绑定名: {name}");
        return;
    };
    oxidict_platform::hotkey::register(
        name,
        accel,
        std::sync::Arc::new(move || {
            if !send_command(cmd) {
                log::warn!("快捷键命令发送失败（命令循环可能已退出）: {cmd:?}");
            }
        }),
    );
}

fn normalize_key(key: &str) -> Option<String> {
    let lower = key.to_ascii_lowercase();
    if lower.len() == 1 {
        let c = lower.as_bytes()[0];
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            return Some(lower);
        }
        // 标点（positional）：直接以符号本身入配置，兼容原版 keyMap 产出
        // 与 `hotkey::parse` 的解析（`+` 是分隔符，无法表示，跳过）。
        if matches!(
            c,
            b'`' | b'-' | b'=' | b'[' | b']' | b'\\' | b';' | b'\'' | b',' | b'.' | b'/'
        ) {
            return Some(lower);
        }
    }
    match lower.as_str() {
        "space" => Some("space".into()),
        "enter" | "return" => Some("enter".into()),
        "tab" => Some("tab".into()),
        "esc" | "escape" => Some("esc".into()),
        "up" | "down" | "left" | "right" => Some(lower),
        "minus" | "-" => Some("minus".into()),
        "equal" | "=" => Some("equal".into()),
        "backspace" | "delete" => Some("backspace".into()),
        // 小键盘：对齐原版 keyMap 产出（Num0..Num9）。
        s if s.starts_with("numpad") => {
            let n: u32 = s["numpad".len()..].parse().ok()?;
            if (0..=9).contains(&n) {
                Some(format!("num{n}"))
            } else {
                None
            }
        }
        // 导航与编辑键。
        "home" | "end" | "insert" | "capslock" | "pageup" | "pagedown" => Some(lower),
        s if s.len() > 1 && s.starts_with('f') => {
            let n: u32 = s[1..].parse().ok()?;
            if (1..=24).contains(&n) {
                return Some(format!("f{n}"));
            }
            None
        }
        _ => None,
    }
}

/// 值变化回调：参数是新的加速键字符串与 App 访问（可开通知窗）。
pub type OnChange = Box<dyn Fn(&str, &mut App) + Send + Sync>;

/// 快捷键录制状态：可聚焦、按组合键更新值。
pub struct HotkeyRecorderState {
    focus_handle: FocusHandle,
    value: String,
    /// 值变化回调（写配置、冲突检测、注册生效等）。回调运行在主线程，
    /// 第二个参数是 App 访问（可开通知窗）。
    on_change: Option<OnChange>,
}

impl HotkeyRecorderState {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        Self {
            focus_handle,
            value: String::new(),
            on_change: None,
        }
    }

    /// 当前绑定的快捷键字符串（`Ctrl+Alt+T`），未绑定则为空。
    pub fn current_value(&self) -> &str {
        &self.value
    }

    pub fn set_value(&mut self, value: impl Into<String>, cx: &mut Context<Self>) {
        let value = value.into();
        // 值未变化（如重复回填初始值）不触发回调，避免重复写配置/注册。
        let changed = self.value != value;
        self.value = value;
        if changed && let Some(cb) = &self.on_change {
            cb(&self.value, cx);
        }
        cx.notify();
    }

    /// 值变化回调（链式设置）。
    pub fn on_change(mut self, cb: impl Fn(&str, &mut App) + Send + Sync + 'static) -> Self {
        self.on_change = Some(Box::new(cb));
        self
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if is_clear_key(&event.keystroke.key) {
            window.prevent_default();
            cx.stop_propagation();
            self.set_value(String::new(), cx);
            return;
        }
        if let Some(accel) = to_accelerator(&event.keystroke) {
            window.prevent_default();
            cx.stop_propagation();
            self.set_value(accel, cx);
        }
    }
}

impl Focusable for HotkeyRecorderState {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for HotkeyRecorderState {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

/// 录制槽 UI：显示当前值/占位符，聚焦后可录入组合键。
#[derive(IntoElement)]
pub struct HotkeyRecorder {
    state: Entity<HotkeyRecorderState>,
    placeholder: String,
    style: StyleRefinement,
    size: Size,
}

impl HotkeyRecorder {
    pub fn new(state: &Entity<HotkeyRecorderState>) -> Self {
        Self {
            state: state.clone(),
            placeholder: String::from("点击后按组合键"),
            style: StyleRefinement::default(),
            size: Size::Medium,
        }
    }

    /// 占位文本（未绑定任何键时显示）。
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = text.into();
        self
    }
}

impl Sizable for HotkeyRecorder {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl Styled for HotkeyRecorder {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for HotkeyRecorder {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = self.state;
        let style = self.style;
        let size = self.size;
        let placeholder = self.placeholder;
        let value = state.read(cx).value.clone();
        let focus_handle = state.read(cx).focus_handle.clone();

        let (border, fg, muted) = {
            let t = cx.theme();
            (
                t.colors.border,
                t.colors.foreground,
                t.colors.muted_foreground,
            )
        };

        // 与 gpui_kit 输入框的尺寸梯度对齐（Small≈28px、Medium≈32px）。
        let (height, text_size) = match size {
            Size::XSmall => (px(24.), px(12.)),
            Size::Small => (px(28.), px(12.)),
            Size::Large => (px(36.), px(13.)),
            _ => (px(32.), px(13.)),
        };

        let is_empty = value.is_empty();
        let display = if is_empty { placeholder } else { value };

        div()
            .id(("hotkey-recorder-slot", state.entity_id()))
            .track_focus(&focus_handle)
            .on_key_down(window.listener_for(&state, HotkeyRecorderState::on_key_down))
            .w(px(180.))
            .h(height)
            .px_2()
            .cursor_pointer()
            .rounded_md()
            .border_1()
            .border_color(border)
            .items_center()
            .text_size(text_size)
            .refine_style(&style)
            .child(
                div()
                    .text_size(text_size)
                    .text_color(if is_empty { muted } else { fg })
                    .child(SharedString::from(display)),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // `Modifiers` 仅供本模块的测试构造 Keystroke，故只在 test 下导入，
    // 避免非 test 构建出现 unused import。
    use gpui_kit::Modifiers;

    fn ks(key: &str, m: impl Fn(&mut Modifiers)) -> Keystroke {
        let mut modifiers = Modifiers::none();
        m(&mut modifiers);
        Keystroke {
            modifiers,
            key: key.to_string(),
            key_char: None,
        }
    }

    #[test]
    fn requires_modifier() {
        assert_eq!(to_accelerator(&ks("a", |_| {})), None);
        assert_eq!(to_accelerator(&ks("1", |_| {})), None);
    }

    #[test]
    fn composes_modifiers() {
        assert_eq!(
            to_accelerator(&ks("t", |m| {
                m.control = true;
                m.alt = true;
            })),
            Some("Ctrl+Alt+t".into())
        );
        assert_eq!(
            to_accelerator(&ks("t", |m| {
                m.shift = true;
                m.platform = true;
            })),
            Some("Shift+Cmd+t".into())
        );
        assert_eq!(
            to_accelerator(&ks("space", |m| m.alt = true)),
            Some("Alt+space".into())
        );
    }

    #[test]
    fn maps_special_and_fn_keys() {
        assert_eq!(
            to_accelerator(&ks("f5", |m| m.control = true)),
            Some("Ctrl+f5".into())
        );
        assert_eq!(
            to_accelerator(&ks("F12", |m| m.control = true)),
            Some("Ctrl+f12".into())
        );
        assert_eq!(
            to_accelerator(&ks("up", |m| m.control = true)),
            Some("Ctrl+up".into())
        );
    }

    #[test]
    fn rejects_unknown_or_out_of_range() {
        assert_eq!(to_accelerator(&ks("f30", |m| m.control = true)), None);
        assert_eq!(to_accelerator(&ks("%%%", |m| m.control = true)), None);
        assert_eq!(to_accelerator(&ks("NumpadA", |m| m.control = true)), None);
    }

    #[test]
    fn maps_punctuation_and_numpad() {
        // 标点以符号本身入配置（对齐原版 keyMap 产出）。
        assert_eq!(
            to_accelerator(&ks(",", |m| m.control = true)),
            Some("Ctrl+,".into())
        );
        assert_eq!(
            to_accelerator(&ks("`", |m| m.control = true)),
            Some("Ctrl+`".into())
        );
        assert_eq!(
            to_accelerator(&ks("Numpad1", |m| m.control = true)),
            Some("Ctrl+num1".into())
        );
        assert_eq!(
            to_accelerator(&ks("PageUp", |m| m.control = true)),
            Some("Ctrl+pageup".into())
        );
    }

    #[test]
    fn clear_key_detection() {
        assert!(is_clear_key("backspace"));
        assert!(is_clear_key("Delete"));
        assert!(is_clear_key("BACKSPACE"));
        assert!(!is_clear_key("a"));
    }
}
