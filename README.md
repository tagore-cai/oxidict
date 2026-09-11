# Oxidict

原 Saladict（沙拉翻译）的 Rust + gpui-kit 原生重写版。目标是去掉 WebView（Tauri 1.6 + React 19），
把 251 个前端文件收敛为一组纯 Rust crate。`config.json` 键名/结构、历史库与 `.potext`
插件包格式与原版保持一致；注意 APP_ID 已改为 `net.oxidict.app`，老配置需手动把
`<config_dir>/allen.town.focus.saladict` 拷贝为 `<config_dir>/net.oxidict.app`。

## 运行

```bash
cargo run -p oxidict          # 内核模式：配置 + 服务 + 剪切板/快捷键 + 外部调用 HTTP
cargo check --workspace        # 全量编译检查（当前 0 error）
```

## 构建要求

- 工具链由 `rust-toolchain.toml` 钉死（1.98.1 + rustfmt + clippy），rustup 会自动安装对应版本
- macOS 10.15+，需 Xcode Command Line Tools
- 平台支持：macOS 已编译验证；Windows / Linux 分支就绪待目标机器验证

## 打包（macOS）

```bash
./scripts/bundle.sh            # 产出 target/release/bundle/macos/Oxidict.app
# 制作 dmg：
hdiutil create -srcfolder target/release/bundle/macos/Oxidict.app \
  -volname Oxidict target/release/bundle/Oxidict.dmg
```

说明：

- 脚本内部执行 `cargo build --release -p oxidict`，再按 `Info.plist` 组装 `.app`
- 应用图标缺失时自动降级为系统默认图标；要自定义图标，把 `icon.icns`
  放到 `assets/icon.icns`（可用 `iconutil -c icns` 从 iconset 生成）
- 未做 codesign / 公证：首次打开需右键 → 打开，绕过 Gatekeeper 提示

## 首次运行授权（macOS）

全局快捷键走 CGEventTap，划词翻译走 Accessibility，两者都需要用户在系统设置里授权：

1. 用 `.app` 运行（不是裸二进制）。裸二进制（`./target/release/oxidict`）没有
   bundle 身份，TCC 无法把授权稳定记住；建议先 ad-hoc 签名再运行：

   ```bash
   ./scripts/bundle.sh
   codesign --force --deep --sign - target/release/bundle/macos/Oxidict.app
   open target/release/bundle/macos/Oxidict.app
   ```

2. 首次启动会**主动弹出授权请求**；若没弹，手动到：
   - **系统设置 → 隐私与安全性 → 输入监控**：勾选 Oxidict（全局快捷键）
   - **系统设置 → 隐私与安全性 → 辅助功能**：勾选 Oxidict（划词取选中文本）

3. 授权后**重启应用**（TCC 变更对已运行进程不生效）。重新编译会使权限失效，
   需再次授权。

未授权时应用其它功能可用，仅全局快捷键不可用，日志会输出一次明确提示
（不再反复刷屏）。

## 开源协议

[MIT](./LICENSE)。本项目为原 Saladict 的独立 Rust 重写：未复用原版代码，
仅兼容其 `config.json`、历史库结构与 `.potext` 插件包格式。

## 分层

```
app/                     可执行入口（内核模式，GPUI 窗口层待接入）
crates/
  oxidict-core          语言码表 / config.json / sqlite 历史 / 领域模型 / ConfigField
  oxidict-net           reqwest 封装 + 签名（HMAC-SHA1/256、MD5、AWS SigV4、TC3、阿里云转义）
  oxidict-platform      三平台能力：划词 / 截图 / 系统 OCR / 全局快捷键 / 剪切板 / 语言检测
  oxidict-services      Translator / Recognizer / Tts / Collector trait + 注册表 + 全量内置服务
  oxidict-plugin        .potext 插件运行时（gpui-shell QuickJS + 作业桥）
  oxidict-server        127.0.0.1:60606 外部调用接口（路由与老版本一致）
  oxidict-ui            GPUI 窗口层（建设中）
```

## 与原实现的对应关系

| 原实现 | 本工程 |
| --- | --- |
| JS 侧 `crypto-js` 签名 | `oxidict-net/src/sign.rs` |
| 30 个翻译服务（`src/services/translate/*`） | `oxidict-services/src/translate/*.rs` |
| 15 个 OCR 服务 | `oxidict-services/src/recognize/*.rs` |
| 30 份手写 `Config.jsx` 表单 | 声明式 `ConfigField`（`oxidict-core/src/schema.rs`） |
| 散落 5 处的语言码映射 | `oxidict-core/src/language.rs` 单一枚举 + `map_language!` |
| `eval(main.js)` 插件 | rquickjs QuickJS 宿主（`oxidict-plugin`，saladict 全局对象注入） |
| Tauri 窗口/托盘/快捷键 | `oxidict-platform` + `oxidict-ui`（UI 层建设中） |

## 当前进度

- 内置服务：**翻译 30/30、OCR 15/15、TTS 3/3、生词本 2/2 全部完成**，与原实现对齐。
- 平台层：macOS 已编译验证（划词/截图/OCR/快捷键/剪切板/语言检测），
  Windows/Linux 分支就绪待目标机器验证（macOS OCR 暂用外部命令回退，Vision 接入点已注明）。
- UI 层：翻译窗口、托盘（划词/输入翻译、OCR、剪切板监听开关、退出）、
  Notify/Updater/Recognize 窗口已落地；`cargo run -p oxidict` 直接启动。
- 端到端已验证：`oxidict --translate "..."` 真实翻译出正确译文并写入历史库
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
