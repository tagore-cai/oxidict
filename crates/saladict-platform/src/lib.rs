//! saladict-platform：跨平台系统能力抽象层。
//!
//! 本 crate 把 saladict 桌面端依赖的系统能力（划词、截图、OCR、全局快捷键、
//! 剪切板监听、鼠标事件、语言检测）收敛成一组与 UI 框架（GPUI/Tauri）无关的
//! 自由函数，按 `cfg(target_os = ...)` 分派到对应平台实现。
//!
//! # 平台能力映射
//!
//! | 能力 | macOS | Windows | Linux |
//! | ---- | ----- | ------- | ----- |
//! | [`selection::selected_text`] 选中文本 | `selection` crate（内部走 AX） | 剪切板模拟（`selection` crate） | 剪切板模拟（`selection` crate） |
//! | [`screenshot::capture_screen`] / `capture_region` 截图 | `screenshots` crate 封装原生 API | 同上 | 同上 |
//! | [`ocr::system_ocr`] 系统 OCR | **命令回退**（见 `ocr/macos.rs` 注释） | `windows` crate → `Windows.Media.Ocr` | `tesseract` 命令行 |
//! | [`hotkey::register`] 全局快捷键 | `rdev` 事件总线 | `rdev` 事件总线 | `rdev` 事件总线 |
//! | [`mouse::cursor_position`] / `start_selection_listener` | `rdev` + `mouse_position` | 同上 | 同上 |
//! | [`clipboard::start_monitor`] 剪切板监听 | `arboard` 轮询去抖 | `arboard` 轮询去抖 | `arboard` 轮询去抖 |
//! | [`detect::detect`] 语言检测 | `lingua` 离线检测 | `lingua` 离线检测 | `lingua` 离线检测 |
//!
//! ## 关于 macOS OCR 的实现选择
//!
//! macOS 上原生的 Vision OCR 需要 objc/CoreGraphics FFI 绑定，存在不可控的编译风险。
//! 为保证本机 `cargo check` 稳定通过（编译通过优先于实现华丽），`ocr/macos.rs` 目前采用
//! 「写临时 PNG + 调用外部 OCR 助手」的命令回退方案，并在文件中注释了升级为原生
//! `VNRecognizeTextRequest` 的切入点。其余平台分支仅保证代码正确，无法在本机验证。

pub mod selection;
pub mod ocr;
pub mod screenshot;
pub mod hotkey;
pub mod clipboard;
pub mod mouse;
pub mod detect;
