//! 全局快捷键（观察流制，`keytap`）。
//!
//! # 为什么是 keytap（两任前身的死因）
//!
//! - **rdev**：回调在 OS 线程直接执行用户代码，其内部调 HIToolbox 查键盘布局
//!   （`string_from_code`），而这些 API 断言必须在主队列执行——实测一敲键即
//!   `dispatch_assert_queue_fail` → SIGILL 闪退（有 .ips 报告佐证）。已移除。
//! - **global-hotkey**：注册制本身没问题（macOS `RegisterEventHotKey`、Windows
//!   `RegisterHotKey`），但其 Linux 后端只有 `x11-dl`——Wayland（Ubuntu 22.04+ /
//!   Fedora 默认会话）下完全失效，快捷键整个不可用，违背三平台对等目标。已移除。
//! - **keytap**：macOS CGEventTap（专用 CFRunLoop 线程）+ Windows
//!   `WH_KEYBOARD_LL`（消息泵线程）+ Linux evdev（`/dev/input/event*` 直读，
//!   X11 与 Wayland 原生可用）；事件经 channel 分发，用户线程自行拉取，
//!   OS 线程零用户代码；`Drop` 即注销，权限不足返回 typed
//!   `Error::PermissionDenied`，不静默失败。
//!
//! # 权限要求
//!
//! - macOS：CGEventTap 需要「输入监控」权限（划词功能本就需要「辅助功能」，
//!   同一系统设置面板，多勾一项）。未授权时 `Tap::new()` 返错并记日志。
//! - Linux：evdev 需要用户在 `input` 组（`sudo usermod -aG input $USER`）。
//! - Windows：无需额外权限。
//!
//! # 触发语义
//!
//! 与原注册制等价：按下组合键的非修饰键瞬间触发一次，auto-repeat
//! （`KeyRepeat`）不触发。修饰键左右键归一化处理，且为超集匹配——
//! 按 `Shift+Alt+X` 也能触发已注册的 `Alt+X`（比注册制的精确匹配更宽容）。
//!
//! 快捷键字符串沿用用户配置写法：`Ctrl+Alt+T`、`Command+Shift+D`、`Alt+X`。

use keytap::{EventKind, Key, Tap};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// 归一化后的修饰键（左右键合一）。
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
enum Mod {
    Control,
    Shift,
    Alt,
    Meta,
}

/// 一个已注册的快捷键绑定。
struct Binding {
    /// 需要按住的修饰键（归一化后）。
    mods: HashSet<Mod>,
    /// 触发键（非修饰键）。
    key: Key,
    handler: Arc<dyn Fn() + Send + Sync>,
}

struct Registry {
    /// name -> 绑定。事件线程每收到非修饰键 KeyDown 时查此表。
    bindings: Mutex<HashMap<String, Binding>>,
    /// 事件线程是否已启动（Tap 只创建一次，Drop 前常驻）。
    started: AtomicBool,
}

static REGISTRY: Lazy<Registry> = Lazy::new(|| Registry {
    bindings: Mutex::new(HashMap::new()),
    started: AtomicBool::new(false),
});

/// 注册全局快捷键。
///
/// `accelerator` 形如 `Ctrl+Alt+T`、`Cmd+Shift+D`；`CommandOrControl` 在
/// macOS 上等价于 `Cmd`，其它平台等价于 `Ctrl`。同名重复注册会覆盖旧绑定。
pub fn register(name: &str, accelerator: &str, handler: Arc<dyn Fn() + Send + Sync>) {
    let Some((mods, key)) = parse(accelerator) else {
        log::warn!("无法解析快捷键 {name}: {accelerator}");
        return;
    };

    REGISTRY.bindings.lock().insert(
        name.to_string(),
        Binding {
            mods,
            key,
            handler,
        },
    );
    log::info!("已注册快捷键 {accelerator} -> {name}");

    // Tap 只创建一次；失败时重置标志，允许用户授权后通过重新注册重试。
    if !REGISTRY.started.swap(true, Ordering::SeqCst) {
        start_event_loop();
    }
}

/// 注销快捷键。
pub fn unregister(name: &str) {
    REGISTRY.bindings.lock().remove(name);
}

fn start_event_loop() {
    std::thread::Builder::new()
        .name("saladict-hotkey".into())
        .spawn(|| {
            // Tap 在事件线程内创建：内含 ShutdownGuard（Drop 即停 OS 监听），
            // 不可 Clone，线程与 tap 同生命周期最干净。
            let tap = match Tap::new() {
                Ok(tap) => tap,
                Err(e) => {
                    log::error!(
                        "全局快捷键监听创建失败: {e}。macOS 需在 系统设置->隐私与安全性->输入监控 \
                         中授权本应用；Linux 需将用户加入 input 组"
                    );
                    // 允许下次 register 重试。
                    REGISTRY.started.store(false, Ordering::SeqCst);
                    return;
                }
            };

            // 当前按住的修饰键（归一化）。keytap 把 FlagsChanged 合成为
            // 修饰键的 KeyDown/KeyUp，直接据此维护即可。
            let mut held: HashSet<Mod> = HashSet::new();

            for event in tap.iter() {
                match event.kind {
                    EventKind::KeyDown(key) if is_modifier(key) => {
                        held.insert(normalize_modifier(key));
                    }
                    EventKind::KeyUp(key) if is_modifier(key) => {
                        held.remove(&normalize_modifier(key));
                    }
                    // 非修饰键按下：匹配所有满足条件的绑定。
                    // auto-repeat 走 KeyRepeat（不在此分支），天然不重复触发。
                    EventKind::KeyDown(key) => {
                        // 不持锁调用 handler：handler 里可能再调 register/unregister。
                        let hits: Vec<Arc<dyn Fn() + Send + Sync>> = {
                            let bindings = REGISTRY.bindings.lock();
                            bindings
                                .values()
                                .filter(|b| b.key == key && b.mods.is_subset(&held))
                                .map(|b| b.handler.clone())
                                .collect()
                        };
                        for handler in hits {
                            handler();
                        }
                    }
                    EventKind::KeyRepeat(_) | EventKind::KeyUp(_) => {}
                }
            }
        })
        .expect("failed to spawn hotkey event thread");
}

fn is_modifier(key: Key) -> bool {
    matches!(
        key,
        Key::ShiftLeft
            | Key::ShiftRight
            | Key::ControlLeft
            | Key::ControlRight
            | Key::AltLeft
            | Key::AltRight
            | Key::MetaLeft
            | Key::MetaRight
    )
}

fn normalize_modifier(key: Key) -> Mod {
    match key {
        Key::ControlLeft | Key::ControlRight => Mod::Control,
        Key::ShiftLeft | Key::ShiftRight => Mod::Shift,
        Key::AltLeft | Key::AltRight => Mod::Alt,
        _ => Mod::Meta,
    }
}

/// 解析快捷键字符串为（修饰键集合, 触发键）。
fn parse(accelerator: &str) -> Option<(HashSet<Mod>, Key)> {
    let mut mods = HashSet::new();
    let mut key: Option<Key> = None;

    for part in accelerator.split('+') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => {
                mods.insert(Mod::Control);
            }
            "shift" => {
                mods.insert(Mod::Shift);
            }
            "alt" | "option" | "opt" => {
                mods.insert(Mod::Alt);
            }
            "cmd" | "meta" | "super" | "command" => {
                mods.insert(Mod::Meta);
            }
            "commandorcontrol" | "cmdorctrl" => {
                mods.insert(if cfg!(target_os = "macos") {
                    Mod::Meta
                } else {
                    Mod::Control
                });
            }
            other => {
                if key.is_some() {
                    // 多个非修饰键，视为非法。
                    return None;
                }
                key = Some(parse_key(other)?);
            }
        }
    }

    Some((mods, key?))
}

fn parse_key(name: &str) -> Option<Key> {
    let bytes = name.as_bytes();
    if bytes.len() == 1 {
        let c = bytes[0];
        if c.is_ascii_lowercase() {
            return letter(c);
        }
        if c.is_ascii_digit() {
            return digit(c);
        }
    }
    match name {
        "space" => Some(Key::Space),
        "enter" | "return" => Some(Key::Enter),
        "tab" => Some(Key::Tab),
        "esc" | "escape" => Some(Key::Escape),
        "backspace" => Some(Key::Backspace),
        "up" => Some(Key::ArrowUp),
        "down" => Some(Key::ArrowDown),
        "left" => Some(Key::ArrowLeft),
        "right" => Some(Key::ArrowRight),
        "minus" => Some(Key::Minus),
        "equal" => Some(Key::Equal),
        name if name.starts_with('f') => {
            let n: u32 = name[1..].parse().ok()?;
            if !(1..=24).contains(&n) {
                return None;
            }
            fn_key(n)
        }
        _ => None,
    }
}

fn letter(c: u8) -> Option<Key> {
    Some(match c {
        b'a' => Key::A,
        b'b' => Key::B,
        b'c' => Key::C,
        b'd' => Key::D,
        b'e' => Key::E,
        b'f' => Key::F,
        b'g' => Key::G,
        b'h' => Key::H,
        b'i' => Key::I,
        b'j' => Key::J,
        b'k' => Key::K,
        b'l' => Key::L,
        b'm' => Key::M,
        b'n' => Key::N,
        b'o' => Key::O,
        b'p' => Key::P,
        b'q' => Key::Q,
        b'r' => Key::R,
        b's' => Key::S,
        b't' => Key::T,
        b'u' => Key::U,
        b'v' => Key::V,
        b'w' => Key::W,
        b'x' => Key::X,
        b'y' => Key::Y,
        b'z' => Key::Z,
        _ => return None,
    })
}

fn digit(c: u8) -> Option<Key> {
    Some(match c {
        b'0' => Key::Digit0,
        b'1' => Key::Digit1,
        b'2' => Key::Digit2,
        b'3' => Key::Digit3,
        b'4' => Key::Digit4,
        b'5' => Key::Digit5,
        b'6' => Key::Digit6,
        b'7' => Key::Digit7,
        b'8' => Key::Digit8,
        b'9' => Key::Digit9,
        _ => return None,
    })
}

fn fn_key(n: u32) -> Option<Key> {
    Some(match n {
        1 => Key::F1,
        2 => Key::F2,
        3 => Key::F3,
        4 => Key::F4,
        5 => Key::F5,
        6 => Key::F6,
        7 => Key::F7,
        8 => Key::F8,
        9 => Key::F9,
        10 => Key::F10,
        11 => Key::F11,
        12 => Key::F12,
        13 => Key::F13,
        14 => Key::F14,
        15 => Key::F15,
        16 => Key::F16,
        17 => Key::F17,
        18 => Key::F18,
        19 => Key::F19,
        20 => Key::F20,
        21 => Key::F21,
        22 => Key::F22,
        23 => Key::F23,
        24 => Key::F24,
        _ => return None,
    })
}
