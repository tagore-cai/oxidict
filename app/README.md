# app（`oxidict` 可执行文件）

**可执行入口与启动编排。** 整个包只有一个 `src/main.rs`（294 行），它不做业务逻辑，
只负责把各子系统按正确顺序拉起来、提供进程级基础设施（日志 / 单实例锁 / CLI）、接好注入点。

内部结构与启动顺序的完整说明见 [ARCHITECTURE.md](./ARCHITECTURE.md)。

## 运行

```bash
cargo run -p oxidict                      # 完整应用（配置 + 服务 + HTTP + 托盘 + 主窗口）
cargo run -p oxidict -- --translate "hello"   # CLI 模式：翻译一段文本并退出
RUST_LOG=info cargo run -p oxidict        # 看启动链路（默认级别是 warn）
```

CLI 模式与 GUI 模式**共用同一份配置、服务选择与历史库**——`--translate` 写进的就是
`history.db` 里你平时看到的那张表。

## 启动时会做什么

```
日志 → （--translate 则走 CLI 分支并退出）
     → 单实例锁（runtime_dir/cache_dir 下的 oxidict-app.lock）
     → ConfigStore::init()            # config.json
     → i18n::init_from_config()       # 界面语言
     → History::init_default()        # history.db
     → init_builtin_services()        # 内置 50 个服务 + custom_services
     → plugin::load_installed()       # 已安装的 .potext
     → oxidict_server::start_from_config()   # 127.0.0.1:60606，失败只 warn
     → 托盘命令 channel + 剪切板监听 + 4 个全局快捷键
     → GPUI 事件循环（阻塞在这里）：init → Dock 策略 → ui::launch
```

**顺序不是随意的**，有几处硬约束（例如 `dock::apply_from_config` 只能在 GPUI `.run()` 回调内，
提前调用会在启动时 panic）。改启动顺序前请读 [ARCHITECTURE.md §3](./ARCHITECTURE.md)。

## 失败策略

| 位置 | 失败时 |
| --- | --- |
| 本地 HTTP 服务（端口被占） | 记 warn，继续启动 |
| 插件加载 | 记日志，跳过该插件 |
| 日志文件打不开 | 降级为只输出 stdout |
| 单实例锁创建失败 | 记 warn，放弃单实例保护（不让应用打不开） |
| 配置 / 历史库初始化失败 | `expect` → 进程终止（无法继续维持不变量） |

## 单实例锁

- 锁文件：`<runtime_dir 或 cache_dir>/oxidict-app.lock`，内容为 PID。
- 崩溃残留的**脏锁会自动清理**（Unix：`kill(pid, 0)` 探测）；**Windows 上不会**，
  需手工删除锁文件。
- 托盘「重启」会先移除锁再拉起新进程（否则新进程会判定为「已有实例」而退出）。

## 打包（macOS）

```bash
./scripts/bundle.sh
codesign --force --deep --sign - target/release/bundle/macos/Oxidict.app
open target/release/bundle/macos/Oxidict.app
```

必须用 `.app` 运行而不是裸二进制：裸二进制没有 bundle 身份，TCC（输入监控 / 辅助功能）无法把
授权稳定记住。详见根 [README](../README.md) 的「首次运行授权」。

## 依赖

本包是唯一依赖全部 7 个 crate 的包，也是唯一允许依赖 `oxidict-ui` 的包：

```
app → ui, server, plugin, services, platform, core, gpui-kit
```

`libc` 是**唯一**的非 workspace 依赖，且仅用于单实例锁的 `kill(pid, 0)` 存活探测（Unix）。

## 测试

本包**没有测试**。可提取为纯函数、值得补测的候选是 `lock_is_stale` 与 `create_lock_file`
（只依赖文件系统，容易覆盖）。
