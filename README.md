# saladict-rs

沙拉翻译的 Rust + gpui-kit 原生重写版。目标是去掉 WebView（Tauri 1.6 + React 19），
把 251 个前端文件收敛为一组纯 Rust crate，同时沿用原有 `config.json`、
历史库结构与 `.potext` 插件包，保证老用户配置无缝迁移。

## 运行

```bash
cargo run -p saladict          # 内核模式：配置 + 服务 + 剪切板/快捷键 + 外部调用 HTTP
cargo check --workspace        # 全量编译检查（当前 0 error）
```

## 分层

```
app/                     可执行入口（内核模式，GPUI 窗口层待接入）
crates/
  saladict-core          语言码表 / config.json / sqlite 历史 / 领域模型 / ConfigField
  saladict-net           reqwest 封装 + 签名（HMAC-SHA1/256、MD5、AWS SigV4、TC3、阿里云转义）
  saladict-platform      三平台能力：划词 / 截图 / 系统 OCR / 全局快捷键 / 剪切板 / 语言检测
  saladict-services      Translator / Recognizer / Tts / Collector trait + 注册表 + 全量内置服务
  saladict-plugin        .potext 插件运行时（gpui-shell QuickJS + 作业桥）
  saladict-server        127.0.0.1:60606 外部调用接口（路由与老版本一致）
  saladict-ui            GPUI 窗口层（建设中）
```

## 与原实现的对应关系

| 原实现 | 本工程 |
| --- | --- |
| JS 侧 `crypto-js` 签名 | `saladict-net/src/sign.rs` |
| 30 个翻译服务（`src/services/translate/*`） | `saladict-services/src/translate/*.rs` |
| 15 个 OCR 服务 | `saladict-services/src/recognize/*.rs` |
| 30 份手写 `Config.jsx` 表单 | 声明式 `ConfigField`（`saladict-core/src/schema.rs`） |
| 散落 5 处的语言码映射 | `saladict-core/src/language.rs` 单一枚举 + `map_language!` |
| `eval(main.js)` 插件 | gpui-shell QuickJS + 控制权反转作业桥（`saladict-plugin/src/bridge.rs`） |
| Tauri 窗口/托盘/快捷键 | `saladict-platform` + `saladict-ui`（UI 层建设中） |

## 当前进度

- 内置服务：**翻译 30/30、OCR 15/15、TTS 3/3、生词本 2/2 全部完成**，与原实现对齐。
- 平台层：macOS 已编译验证（划词/截图/OCR/快捷键/剪切板/语言检测），
  Windows/Linux 分支就绪待目标机器验证（macOS OCR 暂用外部命令回退，Vision 接入点已注明）。
- UI 层：翻译窗口、托盘（划词/输入翻译、OCR、剪切板监听开关、退出）、
  Notify/Updater/Recognize 窗口已落地；`cargo run -p saladict` 直接启动。
- 端到端已验证：`saladict --translate "..."` 真实翻译出正确译文并写入历史库
  （与老版本同一个 config.json / history.db）。
- 快捷键与托盘事件统一走 TrayCommand 通道（tokio mpsc）桥进 GPUI。
- 依赖策略：gpui-kit 整体走 git rev cbdf5baa（gpui-shell 只存在于该 rev；
  crates.io 的 0.6.0 与 git main API 不同，不可混用）。
- 本轮新增：截图 OCR 触发链路（托盘 → `screencapture -i` 框选 → Recognize 窗口）、
  LLM 流式增量上屏（on_stream → channel → GPUI 卡片实时更新）、
  `.potext` 插件宿主（legacy utils 宿主代发 HTTP，脚本零网络权限）。
- **Config 窗口完成**：五页设置（四类服务实例增删 + 通用设置），
  表单由服务的 `config_schema()` 声明式驱动——原版 30 份手写 Config.jsx 归一。
  托盘「偏好设置」直达。**MVP 闭环**：托盘划词翻译、多服务并行、OCR、
  插件、历史、配置全部可用。
- 未完成：识别结果回填翻译、打包分发、i18n/拖拽排序等打磨。

## 关于 gpui-shell

gpui-shell 不在 crates.io（crates.io 同名包 0.1.0 是 3 行 main.rs 的占位包），
以 git 依赖引入并与 gpui-kit 统一到同一 rev。注意它的边界：插件入口是
`ScriptView`，`HostValue` 只承载纯数据，没有「Rust 调脚本函数」的公开接口——
因此 `.potext` 兼容层用控制权反转：脚本 `await take_job(...)` 从宿主队列取作业，
算完 `submit_result(...)` 回传，等待发生在 channel 上而非轮询。
