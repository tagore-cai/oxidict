# Oxidict

原 Saladict（沙拉翻译）的 Rust + gpui-kit 原生重写版。目标是去掉 WebView（Tauri 1.6 + React 19），
把 251 个前端文件收敛为一组纯 Rust crate。`config.json` 键名/结构、历史库与 `.potext`
插件包格式与原版保持一致；注意 APP_ID 已改为 `net.oxidict.app`，老配置需手动把
`<config_dir>/allen.town.focus.saladict` 拷贝为 `<config_dir>/net.oxidict.app`。

## 文档

| 文档 | 内容 |
| --- | --- |
| [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md) | 整体架构：依赖图、分层规则、运行时模型（4 套执行上下文）、5 条核心数据流、落盘布局、关键设计决策、扩展点 |
| `crates/<包>/README.md` | 该包的职责、依赖、公开 API、用法、测试 |
| `crates/<包>/ARCHITECTURE.md` | 该包的内部结构：逐文件职责、内部链路、不变量清单、设计决策记录、已知限制 |
| `app/README.md` · `app/ARCHITECTURE.md` | 可执行入口的启动编排（含顺序上的硬约束） |

八个包各有两份文档，逐包索引见下面的「[分层](#分层)」。

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

依赖是**有向无环图**，方向即「依赖」。完整依赖图与分层规则见
[docs/ARCHITECTURE.md §2](./docs/ARCHITECTURE.md)。

| 包 | 一句话职责 | 文档 |
| --- | --- | --- |
| `app`（`oxidict`） | 可执行入口：启动编排、日志、单实例锁、CLI | [README](./app/README.md) · [架构](./app/ARCHITECTURE.md) |
| `crates/oxidict-ui` | GPUI 窗口层（6 个窗口 + 托盘 + 快捷键录制 + 排版标尺） | [README](./crates/oxidict-ui/README.md) · [架构](./crates/oxidict-ui/ARCHITECTURE.md) |
| `crates/oxidict-server` | `127.0.0.1:60606` 外部调用接口（路由与原版一致） | [README](./crates/oxidict-server/README.md) · [架构](./crates/oxidict-server/ARCHITECTURE.md) |
| `crates/oxidict-plugin` | `.potext` 插件运行时（rquickjs 宿主，无窗口依赖） | [README](./crates/oxidict-plugin/README.md) · [架构](./crates/oxidict-plugin/ARCHITECTURE.md) |
| `crates/oxidict-services` | 四大类服务 trait + 注册表 + 全部 50 个内置实现 | [README](./crates/oxidict-services/README.md) · [架构](./crates/oxidict-services/ARCHITECTURE.md) |
| `crates/oxidict-platform` | 三平台系统能力（划词/截图/OCR/快捷键/剪切板/语言检测） | [README](./crates/oxidict-platform/README.md) · [架构](./crates/oxidict-platform/ARCHITECTURE.md) |
| `crates/oxidict-net` | HTTP 客户端 + 指纹模拟 + 签名 + 备份传输 | [README](./crates/oxidict-net/README.md) · [架构](./crates/oxidict-net/ARCHITECTURE.md) |
| `crates/oxidict-core` | 领域内核：语言码表 / `config.json` / sqlite 历史 / 领域模型 / i18n | [README](./crates/oxidict-core/README.md) · [架构](./crates/oxidict-core/ARCHITECTURE.md) |

三条不应被打破的规则：

- `oxidict-core` 不依赖任何内部 crate（它是叶子）
- 除 `oxidict-ui` 外，任何包都不依赖 gpui-kit（内核必须能脱离窗口运行）
- `oxidict-services` 依赖 `oxidict-platform` 是受控例外（系统 OCR 等服务确需系统能力）


## 与原实现的对应关系

| 原实现 | 本工程 |
| --- | --- |
| JS 侧 `crypto-js` 签名 | `oxidict-net/src/sign.rs` |
| 30 个翻译服务（`src/services/translate/*`） | `oxidict-services/src/translate/*.rs` |
| 15 个 OCR 服务 | `oxidict-services/src/recognize/*.rs` |
| 30 份手写 `Config.jsx` 表单 | 声明式 `ConfigField`（`oxidict-core/src/schema.rs`） |
| 散落 5 处的语言码映射 | `oxidict-core/src/language.rs` 单一枚举 + `map_language!` |
| `eval(main.js)` 插件 | rquickjs 宿主（`oxidict-plugin`，向脚本注入 `oxidict` 全局对象） |
| Tauri 窗口/托盘/快捷键 | `oxidict-platform` + `oxidict-ui` |

## 当前进度

- 内置服务：**翻译 30/30、OCR 15/15、TTS 3/3、生词本 2/2 全部完成**，与原实现对齐。
- 平台层：macOS 已编译验证（划词/截图/OCR/快捷键/剪切板/语言检测），
  Windows/Linux 分支就绪待目标机器验证（macOS OCR 暂用外部命令回退，Vision 接入点已注明）。
- UI 层：翻译窗口、托盘（划词/输入翻译、OCR、剪切板监听开关、退出）、
  Notify/Updater/Recognize 窗口已落地；`cargo run -p oxidict` 直接启动。
- 端到端已验证：`oxidict --translate "..."` 真实翻译出正确译文并写入历史库
  （与老版本同一个 config.json / history.db）。
- 快捷键与托盘事件统一走 TrayCommand 通道（tokio mpsc）桥进 GPUI。
- 依赖策略：`gpui-kit` 走 crates.io 的 `0.6`（已正式发布，不再钉 git rev）。
  历史上曾依赖 git rev `cbdf5baa` 与其上的 `gpui-shell`，现已全部移除。
- 本轮新增：截图 OCR 触发链路（托盘 → `screencapture -i` 框选 → Recognize 窗口）、
  LLM 流式增量上屏（`on_stream` → channel → GPUI 卡片实时更新）、
  `.potext` 插件宿主（rquickjs 原生模块代发 HTTP，脚本零网络权限）。
- **Config 窗口完成**：五页设置（四类服务实例增删 + 通用设置），
  表单由服务的 `config_schema()` 声明式驱动——原版 30 份手写 Config.jsx 归一。
  托盘「偏好设置」直达。**MVP 闭环**：托盘划词翻译、多服务并行、OCR、
  插件、历史、配置全部可用。
- i18n：Fluent（`fluent-templates`）编译期嵌入 20 个 locale，见
  `crates/oxidict-core/locales/`；界面语言可在「通用」页直接切换（20 项全列出）。
  **约定**：只有 `zh-CN` 与 `en-US` 需要保持完整，其余 18 个按块翻译、缺失部分
  运行时回退到 `en-US`。新增文案只补这两种即可（`i18n::tests` 会强制两者键集一致，
  并校验 UI 引用的键确实能解析）。
- 主题支持「跟随系统 / 浅色 / 深色」三态，启动与切换时都会生效。
- 未完成：识别结果回填翻译；服务实例拖拽已支持（↑/↓ 亦保留），历史页暂无分页。

## HTTP 客户端与指纹模拟

底层用 **`wreq`**（`reqwest` 的硬分叉，HTTPS 走 BoringSSL）而不是 reqwest，唯一理由是
**能模拟浏览器指纹**。

**为什么需要它**：服务一旦可以"配置一个网页端点"，端点集合就是开放的——用户能指向
`translate.google.com`、`dictionary.cambridge.org` 这类页面，而它们普遍用 WAF 按
TLS（JA3/JA4）与 HTTP/2 特征判定客户端。默认的非浏览器 ClientHello 会吃到 403 或直接
被重置连接。`wreq` 还能保留 HTTP/1 header 的**大小写**（reqwest 会小写化，部分 WAF 因此拒绝）。

**怎么用**：服务实例配置里的 `tls` 字段（`native` 默认 / `chrome` / `safari`），
设置面板会为会被拦的服务渲染成下拉。缺省或不认识的值一律回退 `native`——
宁可指纹不匹配，也不要因一个拼写错误让所有请求"假装是 Safari"。

**代价（真实存在，不要忽略）**：

| 项 | 影响 |
| --- | --- |
| 构建 | BoringSSL 原生构建，三平台都需要 `cmake` + `perl` + `libclang` + `git`（macOS 有 Xcode CLT 即可，实测 3m43s 完成全量构建） |
| 符号冲突 | 若依赖树里出现 `openssl-sys` 会链接失败，需开 wreq 的 `prefix-symbols`；当前依赖树里没有 openssl-sys |
| 根证书 | 默认用 `chromium-roots`（Chrome 的根证书集），与系统信任库不同；企业 MITM 证书场景需自行验证 |
| 供应链 | 依赖作者自维护的 `btls` fork；wreq 是硬分叉，reqwest 的安全补丁不会自动过来 |

**它解决不了什么**（别以为配上就能用）：WAF 拦人是分层的，wreq 覆盖 L1 传输层指纹与
L2 header 形态；**L3 会话/Token**（需 Cookie jar + 多步请求）、**L4 HTML 响应体**
（需选择器提取）、**L5 风控挑战**（Cloudflare/hCaptcha，纯客户端无解）、
**L6 速率限制**（需退避 + 轮换代理）都要另外做。

## 自定义服务（配置驱动）

**加一个"单次请求 + 从 JSON 取字段"的翻译服务不需要写代码**：往 config.json 的
`custom_services` 数组里追加一条 spec，重启即生效；也可以在
**偏好设置 → 自定义服务**页用表单添加（保存即生效，无需重启）。

表单覆盖常见形态（服务标识 / 地址 / 方法 / 鉴权 / 请求体模板 / 译文路径 / 指纹）；
`headers`、`fields`、`stream_delta` 这类进阶字段仍在 config.json 里手写。

```json
{
  "custom_services": [
    {
      "id": "my_mt",
      "label": "我的翻译",
      "endpoint": "https://api.example.com/v1/translate",
      "method": "POST",
      "auth": { "kind": "bearer" },
      "headers": { "X-App": "oxidict" },
      "body": { "q": "{{text}}", "source": "{{fromCode}}", "target": "{{toCode}}" },
      "extract": { "text": "data.translations.0.text" },
      "fields": [{ "kind": "secret", "key": "apiKey", "label": "API Key" }]
    }
  ]
}
```

| 字段 | 说明 |
| --- | --- |
| `id` | 服务标识，写进 `*_service_list`。**不能含 `@`**（实例与备注的分隔符） |
| `endpoint` / `method` | 必须以 `http(s)://` 开头；方法默认 `POST` |
| `auth` | 具名变换器：`none` / `bearer` / `header{name}` / `query{name}` / `hash_query{...}`（见下） |
| `headers` / `body` | 支持占位符；`body` 省略则不发送请求体（GET 常用） |
| `extract.text` | 译文路径，点或方括号写法（`data.items[0].text`） |
| `extract.html_selector` | HTML 模式：CSS 选择器取文本（与 `text` 二选一） |
| `session` | 多步会话前置请求 `{url, method, token_path}`；`token_path` 留空表示只要 Cookie |
| `extract.stream_mode` / `stream_delta` | 流式：`sse_json` 时须给增量路径 |
| `fields` | 声明式表单（`text`/`secret`/`select`/`switch`/`number`），设置面板据此渲染 |
| `tls` | 指纹档位，见上一节（会被 WAF 拦的端点填 `chrome`） |

**占位符**：`{{text}}`、`{{from}}`/`{{to}}`（英文语言名）、`{{fromCode}}`/`{{toCode}}`
（内部语言码）、`{{apiKey}}`，以及任意配置字段名。整串为占位符且字段是数字/布尔时
会保留 JSON 类型（`"temperature": "{{temperature}}"` 传出去仍是数字）。

**需要签名的接口**用 `hash_query`（明文拼接 + 哈希，结果作为查询参数发出）：

```json
{
  "id": "youdao", "endpoint": "https://openapi.youdao.com/api",
  "method": "GET",
  "auth": {
    "kind": "hash_query", "algo": "sha256", "name": "sign",
    "template": "{{appKey}}{{text}}{{salt}}{{timestamp}}{{appSecret}}",
    "extra": { "appid": "{{appid}}", "salt": "{{salt}}", "curtime": "{{timestamp}}",
               "q": "{{text}}", "from": "{{fromCode}}", "to": "{{toCode}}", "signType": "v3" }
  },
  "extract": { "text": "translation.0" },
  "fields": [
    { "kind": "text", "key": "appid", "label": "应用 ID" },
    { "kind": "secret", "key": "appKey", "label": "应用密钥" },
    { "kind": "secret", "key": "appSecret", "label": "签名密钥" }
  ]
}
```

`{{salt}}`（每次请求一个 UUID）与 `{{timestamp}}`（Unix 秒）在一次请求内只生成一次——
这是签名能对上的前提。哈希支持 `md5` 与 `sha256`。

**已知边界**（不是 bug，是刻意的能力边界）：

- `{{fromCode}}`/`{{toCode}}` 是内部码（`zh_cn` 这种下划线形式），不是 BCP-47；
- 没有自定义语言映射；
- 提取是 **JSON 路径或 CSS 选择器二选一**：`extract.html_selector` 设置后响应
  按 HTML 解析（词典类网页端点走这里），`text` 字段被忽略；
- **多步会话**用 `session` 字段：先打一发前置请求（拿会话 Cookie 或一个注入
  主请求的 `{{token}}`），Cookie 由共享 cookie_store 自动带过去。但**任意编排**
  的多步流程（条件、循环、状态机）仍走 `.potext` 插件——把 spec 做成通用脚本
  语言，会让"配错了很难查"取代"配不了"成为主要问题；
- 签名支持 `md5` / `sha256` / `hmac_sha256`；其它签名协议走插件；
- 签名类鉴权（`hash_query`）**不能在设置面板的表单里编辑**，只能写
  config.json：表单若照原样保存会把签名配置丢掉，那比不能编辑更糟。

单条 spec 不合法时**只跳过并在日志里指出下标**（`custom_services[2]（id="x"）已跳过: ...`），
不会让应用起不来；与内置服务同名的也会被跳过，避免悄悄顶掉内置实现。

## 兼容但尚未生效的配置项

以下 key 为**兼容原版 `config.json`**而保留（缺失时按默认值补齐，不会被清掉），
但当前版本不读取，改了不会有任何效果：

| key | 原语义 | 为什么暂不接入 |
| --- | --- | --- |
| `app_fallback_font` | 界面候选字体（主字体缺失时回退） | gpui 主题只暴露单一 `font_family`，没有候选列表；要支持得自己探测已装字体后再写回 `app_font` |

（`backup_type` 已接入：见下；`app_font` / `app_font_size` 已接入：见「界面排版与字体」。）

## 界面排版与字体

应用自有文字统一走 `crates/oxidict-ui/src/tokens.rs` 的具名档位
（`Xs` 10 / `Sm` 11 / `Body` 12 / `Label` 13 / `Heading` 14 / `Logo` 16 px）：

- **默认零视觉变化**：`app_font_size = 16` 时缩放系数恰为 1，档位值就是改造前手写的像素值
  （12px 是「说明与数据」、13px 是「设置项标签」，这套分工原本就是自洽的，只是没有名字）。
- **`app_font_size` 真正生效**：所有档位按 `app_font_size / 16` 等比缩放，夹在 `[0.75, 1.6]`
  以防手改配置把布局撑爆；设置页「界面字号」提供 14 / 16 / 18 / 20 四档。
- **`app_font` 真正生效**：经 `ThemeConfig.font_family` 注入，组件库自带控件
  （按钮、输入框、下拉）一并跟随；留空即系统字体。
- 为什么注入 `ThemeConfig` 而不是直接改 `Theme.font_family` / `font_size`：后者会被
  下一次 `Theme::change` 用 light/dark 配置覆盖回去（见 gpui-component
  `theme/schema.rs::apply_config`）。
- 「跟随系统」主题在**窗口重新激活**时按当前系统外观重算——gpui 没有外观变化回调，
  这是不加轮询的可行做法（系统里切深浅色后切回应用即生效）。

## 备份后端

`backup_type` 可选两个落点，共用同一组 list/put/get 操作（`oxiddict-net::backup_backend`）：

- `webdav`（默认）：WebDAV 服务器，程序在远端建 `oxidict-app/` 目录存 zip。
- `local_folder`：本机目录（`backup_folder`），可指向 iCloud / OneDrive 等**由系统负责上云**
  的同步目录——这是原版 `backup_type` 里 local_folder 那一档的语义。

设计上对齐参考实现 clippi 的 `services/backends`（同样的「按 backend_type 选后端 +
统一操作面」），差异仅在于 clippi 的后端集合运行期可增（`Vec` + `dyn Trait`），
本工程的启用后端由单一 `backup_type` 决定、集合封闭，故用 enum 派发而非 `dyn`——
省掉 boxed future，`match` 还能保证穷尽性。

## 关于插件运行时

`.potext` 就是「zip + `main.js` + `info.json`」。`main.js` 按脚本 eval，
入口函数（`translate` / `recognize` / `tts` / `collection`）落在 globalThis 上由宿主直接调用，
与老版 Tauri 宿主的 `eval(main.js)` 语义一致。宿主能力面（HTTP 代发、
受限文件读写、受限子进程）以 `oxidict` 原生模块 / 全局对象 / legacy `utils` 三种方式提供，
全部限定在插件目录内。运行时基于 rquickjs，不依赖窗口与渲染循环，
CLI / HTTP / UI 三种上下文行为一致。
未实现：legacy 的 `utils.tauriFetch` / `utils.readBinaryFile` / `utils.CryptoJS`。
