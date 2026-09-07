//! 全局快捷键（注册制，`global-hotkey`）。
//!
//! # 为什么不用 rdev
//!
//! rdev 的全局键盘 tap 回调会在自己的线程里调 HIToolbox 查键盘布局
//! （`string_from_code`），而这些 API 断言必须在主队列执行——实测一敲键即
//! `dispatch_assert_queue_fail` → SIGILL 闪退（有 .ips 崩溃报告佐证）。
//! `global-hotkey` 走 `RegisterEventHotKey` 注册制：只收到匹配的组合键事件，
//! 没有逐键回调，天然规避该问题。
//!
//! 快捷键字符串沿用用户配置里的写法：`Ctrl+Alt+T`、`Command+Shift+D`、`Alt+X`。

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;

struct Registry {
    manager: Option<GlobalHotKeyManager>,
    /// 快捷键名 -> (HotKey, 事件 id)
    by_name: HashMap<String, (HotKey, u32)>,
    /// 事件 id -> 回调
    handlers: HashMap<u32, Arc<dyn Fn() + Send + Sync>>,
}

static REGISTRY: Lazy<Mutex<Registry>> = Lazy::new(|| {
    Mutex::new(Registry {
        manager: None,
        by_name: HashMap::new(),
        handlers: HashMap::new(),
    })
});

/// 注册全局快捷键。
///
/// `accelerator` 形如 `Ctrl+Alt+T`、`Cmd+Shift+D`；`CommandOrControl` 在
/// macOS 上等价于 `Cmd`，其它平台等价于 `Ctrl`。重复注册同一组合键会失败并告警。
pub fn register(name: &str, accelerator: &str, handler: Arc<dyn Fn() + Send + Sync>) {
    let Some(hotkey) = parse(accelerator) else {
        log::warn!("无法解析快捷键 {name}: {accelerator}");
        return;
    };

    let mut registry = REGISTRY.lock();
    if registry.manager.is_none() {
        match GlobalHotKeyManager::new() {
            Ok(manager) => {
                registry.manager = Some(manager);
                start_event_loop();
            }
            Err(e) => {
                log::error!("全局快捷键管理器创建失败: {e}");
                return;
            }
        }
    }

    let manager = registry.manager.as_ref().unwrap();
    match manager.register(hotkey) {
        Ok(()) => {
            log::info!("已注册快捷键 {accelerator} -> {name}");
            registry.handlers.insert(hotkey.id(), handler);
            registry.by_name.insert(name.to_string(), (hotkey, hotkey.id()));
        }
        Err(e) => {
            // 常见原因：组合键已被其它应用（或本应用重复）注册。
            log::warn!("注册快捷键 {name} ({accelerator}) 失败: {e}");
        }
    }
}

/// 注销快捷键。
pub fn unregister(name: &str) {
    let mut registry = REGISTRY.lock();
    if let Some((hotkey, id)) = registry.by_name.remove(name) {
        registry.handlers.remove(&id);
        if let Some(manager) = &registry.manager {
            let _ = manager.unregister(hotkey);
        }
    }
}

fn start_event_loop() {
    std::thread::Builder::new()
        .name("saladict-hotkey".into())
        .spawn(|| {
            // crossbeam 的 receiver 是 'static 的，直接循环 recv。
            let receiver = GlobalHotKeyEvent::receiver();
            while let Ok(event) = receiver.recv() {
                if event.state() != HotKeyState::Pressed {
                    continue;
                }
                let handler = REGISTRY.lock().handlers.get(&event.id).cloned();
                if let Some(handler) = handler {
                    handler();
                }
            }
        })
        .expect("failed to spawn hotkey event thread");
}

/// 解析快捷键字符串。
fn parse(accelerator: &str) -> Option<HotKey> {
    let mut mods = Modifiers::empty();
    let mut key: Option<Code> = None;

    for part in accelerator.split('+') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => mods |= Modifiers::CONTROL,
            "shift" => mods |= Modifiers::SHIFT,
            "alt" | "option" | "opt" => mods |= Modifiers::ALT,
            "cmd" | "meta" | "super" | "command" => mods |= Modifiers::META,
            "commandorcontrol" | "cmdorctrl" => {
                if cfg!(target_os = "macos") {
                    mods |= Modifiers::META;
                } else {
                    mods |= Modifiers::CONTROL;
                }
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
    Some(HotKey::new(Some(mods), key?))
}

fn parse_key(name: &str) -> Option<Code> {
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
        "space" => Some(Code::Space),
        "enter" | "return" => Some(Code::Enter),
        "tab" => Some(Code::Tab),
        "esc" | "escape" => Some(Code::Escape),
        "backspace" => Some(Code::Backspace),
        "up" => Some(Code::ArrowUp),
        "down" => Some(Code::ArrowDown),
        "left" => Some(Code::ArrowLeft),
        "right" => Some(Code::ArrowRight),
        "minus" => Some(Code::Minus),
        "equal" => Some(Code::Equal),
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

fn letter(c: u8) -> Option<Code> {
    Some(match c {
        b'a' => Code::KeyA,
        b'b' => Code::KeyB,
        b'c' => Code::KeyC,
        b'd' => Code::KeyD,
        b'e' => Code::KeyE,
        b'f' => Code::KeyF,
        b'g' => Code::KeyG,
        b'h' => Code::KeyH,
        b'i' => Code::KeyI,
        b'j' => Code::KeyJ,
        b'k' => Code::KeyK,
        b'l' => Code::KeyL,
        b'm' => Code::KeyM,
        b'n' => Code::KeyN,
        b'o' => Code::KeyO,
        b'p' => Code::KeyP,
        b'q' => Code::KeyQ,
        b'r' => Code::KeyR,
        b's' => Code::KeyS,
        b't' => Code::KeyT,
        b'u' => Code::KeyU,
        b'v' => Code::KeyV,
        b'w' => Code::KeyW,
        b'x' => Code::KeyX,
        b'y' => Code::KeyY,
        b'z' => Code::KeyZ,
        _ => return None,
    })
}

fn digit(c: u8) -> Option<Code> {
    Some(match c {
        b'0' => Code::Digit0,
        b'1' => Code::Digit1,
        b'2' => Code::Digit2,
        b'3' => Code::Digit3,
        b'4' => Code::Digit4,
        b'5' => Code::Digit5,
        b'6' => Code::Digit6,
        b'7' => Code::Digit7,
        b'8' => Code::Digit8,
        b'9' => Code::Digit9,
        _ => return None,
    })
}

fn fn_key(n: u32) -> Option<Code> {
    Some(match n {
        1 => Code::F1,
        2 => Code::F2,
        3 => Code::F3,
        4 => Code::F4,
        5 => Code::F5,
        6 => Code::F6,
        7 => Code::F7,
        8 => Code::F8,
        9 => Code::F9,
        10 => Code::F10,
        11 => Code::F11,
        12 => Code::F12,
        13 => Code::F13,
        14 => Code::F14,
        15 => Code::F15,
        16 => Code::F16,
        17 => Code::F17,
        18 => Code::F18,
        19 => Code::F19,
        20 => Code::F20,
        21 => Code::F21,
        22 => Code::F22,
        23 => Code::F23,
        24 => Code::F24,
        _ => return None,
    })
}
