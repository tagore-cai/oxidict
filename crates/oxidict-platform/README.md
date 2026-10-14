# oxidict-platform

**跨平台系统能力抽象层。**

> 内部结构、逐文件职责、不变量清单与设计决策 → [ARCHITECTURE.md](./ARCHITECTURE.md)

把桌面端依赖的系统能力收敛成一组与 UI 框架（GPUI/Tauri）无关的**自由函数**，
按 `cfg(target_os = ...)` 分派到对应平台实现。

设计目标：这一层只依赖 `oxidict-core`，**不依赖 gpui-kit**——CLI、HTTP 服务、插件宿主都能调到它，
不该被拖进渲染循环。

## 依赖

```
oxidict-platform  →  oxidict-core
被依赖：oxidict-services（系统 OCR / tesseract）、oxidict-ui、app
```

外部依赖：`image`、`arboard`（剪切板）、`selection`（划词）、`keytap`（全局键盘）、`screenshots`、
`mouse_position`、`lingua`（离线语言检测）、`dirs`、`once_cell`、`parking_lot`。

平台特定依赖写在**本 crate 的 Cargo.toml**（virtual workspace manifest 不允许 `[target]` 区块）：

- `cfg(target_os = "windows")`：`windows`（`Media.Ocr` 等）
- `cfg(target_os = "macos")`：`objc2` + `objc2-app-kit`（Dock 策略）

## 平台能力矩阵

| 能力 | macOS | Windows | Linux |
| --- | --- | --- | --- |
| `selection::selected_text()` | `selection` crate（内部走 AX） | 剪切板模拟 | 剪切板模拟 |
| `screenshot::capture_screen()` / `capture_region()` | `screenshots` crate | 同左 | 同左 |
| `screenshot::capture_interactive()` | `screencapture -i` | 未实现 | 未实现 |
| `ocr::system_ocr()` | **命令回退**（见下） | `windows` crate → `Windows.Media.Ocr` | `tesseract` 命令行 |
| `hotkey::register()` | keytap（CGEventTap） | keytap（`WH_KEYBOARD_LL`） | keytap（evdev） |
| `mouse::cursor_position()` | `mouse_position` crate | 同左 | 同左 |
| `clipboard::start_monitor()` | `arboard` 轮询去抖 | 同左 | 同左 |
| `detect::detect()` | `lingua` 离线 | 同左 | 同左 |
| `dock::set_visible()` | `NSApplicationActivationPolicy::Accessory` | no-op | no-op |
| `autostart::enable()/disable()` | LaunchAgent plist | 注册表 Run 键 | `.desktop` 文件 |
| `shell::open_path()` | `open` | `explorer` | `xdg-open` |

**验证状态**：macOS 已编译 + 端到端验证；Windows / Linux 分支代码就绪，待目标机器验证。

## 模块

| 模块 | 职责 |
| --- | --- |
| `selection.rs` | 取当前焦点的选中文本 |
| `screenshot.rs` | 全屏 / 区域 / 交互式框选截图 |
| `ocr.rs` + `ocr/{macos,windows,linux}.rs` | 系统 OCR 的平台分派与实现 |
| `hotkey.rs` | 全局快捷键注册（观察流制） |
| `clipboard.rs` | 剪切板监听（轮询 + 去抖），返回 `MonitorHandle`（`Drop` 即停） |
| `mouse.rs` | 光标位置 + 光标所在显示器信息（`MonitorInfo`） |
| `detect.rs` | 离线语言检测（`lingua`） |
| `dock.rs` | macOS Dock 图标可见性（`hide_dock_icon`） |
| `autostart.rs` | 开机自启 |
| `shell.rs` | 用系统默认方式打开路径 |
| `examples/tap_probe.rs` | 快捷键 tap 的独立探针，用于排查权限/事件问题 |

## 三个「前任怎么死的」——不要回退这些选型

### 全局快捷键为什么是 `keytap`

| 候选 | 死因 |
| --- | --- |
| `rdev` | 回调在 OS 线程直接执行用户代码，其内部调 HIToolbox 查键盘布局（`string_from_code`），而这些 API 断言必须在主队列执行——实测一敲键即 `dispatch_assert_queue_fail` → SIGILL 闪退（有 `.ips` 报告佐证）。**已移除** |
| `global-hotkey` | 注册制本身没问题（macOS `RegisterEventHotKey`、Windows `RegisterHotKey`），但 Linux 后端**只有 `x11-dl`**——Wayland（Ubuntu 22.04+ / Fedora 默认会话）下快捷键完全失效，违背三平台对等目标。**已移除** |
| `keytap`（现用） | macOS CGEventTap（专用 CFRunLoop 线程）+ Windows `WH_KEYBOARD_LL`（消息泵线程）+ Linux evdev（`/dev/input/event*` 直读，X11 与 Wayland 原生可用）。事件经 channel 分发、用户线程自行拉取，**OS 线程零用户代码**；`Drop` 即注销；权限不足返回 typed `Error::PermissionDenied`，不静默失败 |

**权限要求**：

- macOS：CGEventTap 需要**输入监控**权限（划词功能本就需要辅助功能，同一面板多勾一项）。
  未授权时 `Tap::new()` 返错并记日志，不反复刷屏。
- Linux：evdev 需要用户在 `input` 组（`sudo usermod -aG input $USER`）。
- Windows：无需额外权限。

### 鼠标释放钩子为什么没了

原版用 rdev 的鼠标释放钩子触发划词。rdev 因上述崩溃问题整体撤除后，划词触发改为
「快捷键 / 托盘菜单 → `selected_text()`」，与原版热键划词行为一致。`mouse.rs` 只保留光标位置查询
（翻译窗口的「跟随光标弹出」需要它）。

### macOS OCR 为什么是命令回退

原生 Vision OCR 需要 objc/CoreGraphics FFI 绑定，存在不可控的编译风险。为保证本机 `cargo check`
稳定通过（**编译通过优先于实现华丽**），`ocr/macos.rs` 目前采用「写临时 PNG + 调用外部 OCR 助手」
的回退方案，文件内已注释升级为原生 `VNRecognizeTextRequest` 的**接入点**。要接手这块就从那里进。

## 时序与线程约定（踩过的坑）

**`dock.rs` 不得在 GPUI `Application::run` 之前调用。**

GPUI 的 macOS 平台通过 `+[GPUIApplication sharedApplication]` 把共享实例创建为自己注册的
NSApplication 子类（该类带 `platform` ivar，用于回取 MacPlatform 指针）。若此前任何代码先取过
`NSApplication::sharedApplication`（`setActivationPolicy` 就会），AppKit 会先创建出**原生
NSApplication** 单例，之后 GPUI 执行 `set_ivar("platform", ..)` 时 panic：

```
Ivar platform not found on class NSApplication
```

因此 Dock 策略在 GPUI 启动回调（`app/src/main.rs` 的 `.run(|cx| ..)`）内应用，`set_visible`
内还有主线程检查兜底。**这是启动即崩的历史事故，改动时序前先读 `dock.rs` 顶部注释。**

**权限按「负责进程」归属**：从终端/IDE 启动的裸二进制，授权会记在终端身上。所以「用户报权限失败、
开发者复现不了」是正常现象——用 `.app` 运行并 ad-hoc 签名后再验证（见根 README）。

## 测试

```bash
cargo test -p oxidict-platform
cargo run -p oxidict-platform --example tap_probe    # 快捷键探针
```

本 crate 目前只有 **1 个测试**（`hotkey.rs::parses_punctuation_and_numpad`，标点与数字键盘解析）。
平台能力大多需要真实系统与授权，无法在 CI 里跑；但**快捷键字符串解析是纯函数，可以且应该补测**——
`Ctrl+Alt+T`、`Command+Shift+D`、F1–F24、非法输入这些分支现在都没覆盖。这是本 crate 最大的测试空白。
