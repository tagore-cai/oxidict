# oxidict-core

**与 UI 框架无关的领域内核。**

> 内部结构、逐文件职责、不变量清单与设计决策 → [ARCHITECTURE.md](./ARCHITECTURE.md)

整条依赖链的叶子——它不依赖任何内部 crate，也不依赖 gpui-kit，
因此可以在没有窗口的环境里独立编译与测试。

它承载四样东西：**语言码表、配置、历史库、领域模型**。服务层、平台层、窗口层都从它取类型。

## 依赖

```
oxidict-core  ←  oxidict-net / oxidict-platform / oxidict-services / oxidict-plugin / oxidict-ui / app
```

外部依赖：`serde`/`serde_json`、`rusqlite`（bundled sqlite）、`fluent-templates`、`unic-langid`、
`notify`（配置文件监听）、`parking_lot`、`tokio`、`zip`/`walkdir`（备份）、`thiserror`/`anyhow`、
`dirs`、`chrono`、`urlencoding`。

## 模块

| 模块 | 内容 | 要点 |
| --- | --- | --- |
| `language.rs` | `Language` 枚举 + `map_language!` 宏 | 全项目**唯一**语言码来源。原实现里语言映射散落在 5 处（内部码、各服务 `info.ts` 的枚举、`recognize/system` 的三套平台码、`lang_detect.js` 的厂商码、Rust 侧 lingua 映射），这里收敛为一处，服务只声明「内部码 → 厂商码」的偏差项 |
| `config.rs` | `ConfigStore` + `keys` + `watch_config` | 沿用原版 `config.json`，键名结构完全一致。原子落盘、并发写串行化、损坏自愈（见下） |
| `model.rs` | `TranslateResult` / `DictResult` / 四类 Request / `HasConfig` | 翻译结果两种形态（纯文本 / 结构化词典），GPUI 据此决定渲染 `Textarea` 还是词典卡片 |
| `schema.rs` | `ConfigField` | 声明式表单描述。原版 30 份手写 `Config.jsx` 由它归一，服务只声明字段，设置面板统一渲染 |
| `history.rs` | `History` | sqlite，表结构 `history(id, text, source, target, service, result, timestamp)` 与原版逐列一致 |
| `i18n.rs` | Fluent 本地化 | 编译期嵌入 20 个 locale；`t(key)` / `t_args(key, args)` |
| `backup.rs` | `pack` / `unpack` / `export_to_file` / `import_from_file` | 把 `config.json` + `history.db` + `plugins/` 打成一个 zip |
| `runtime.rs` | 全局 tokio runtime | `handle()` / `spawn()`，GPUI 与 tokio 之间的桥 |
| `error.rs` | `Error` / `Result` | 全项目统一错误类型，`is_retryable()` 判定是否值得自动重试 |

## 公开 API 速览

```rust
// 配置
ConfigStore::init()?                       // 加载（或创建）默认路径下的 config.json
ConfigStore::init_with_path(path)?         // 测试/自定义路径；重复调用返回既有单例
store.get::<T>(key) / get_or(key, default) // 类型化读取
store.set(key, &value)? / remove(key)?     // 写并原子落盘
config::config()                           // 进程级单例（未初始化会 panic）
config::store_opt()                        // 可选版本，库代码读「可缺省」配置用这个
config::watch_config(store, on_change)?     // 外部改动时 reload + 回调

// 历史
History::init_default()? / History::global()
history.add(text, from, to, service, result) / list(limit, offset) / search(kw, limit)
history.delete(id) / clear() / count()

// i18n
i18n::init_from_config(); i18n::set_language("zh-CN"); i18n::t("translate-title")
```

## 三条容易踩的约定

**1. `config()` vs `store_opt()`**
`config()` 在未初始化时 panic——这是**应用入口的契约**（`main` 起服务前必须 `ConfigStore::init`）。
库代码里「读一下就有、没有也照样能跑」的场景（HTTP 客户端读代理配置、独立工具函数、测试）必须用
`store_opt()`。用错会在测试环境莫名 panic。

**2. `ConfigStore::init_with_path` 重复调用不会新建实例**
全局单例只初始化一次，第二次调用直接返回既有实例。否则会出现「本次返回值与 `config()` 指向两个
不同 store」的隐性分裂——配置的读与写落在互不可见的两份内存映射上，表现为「改了设置却不生效」。

**3. `watch_config` 只认 `config.json` 自身的事件**
应用目录里还住着 `logs/`（持续写入）、`history.db`（翻译即写）、`plugins/`、单实例锁。不过滤就会
出现「自己写日志 → 触发 reload → 回调重新应用设置 → 再写日志」的自激循环。过滤规则有专门的回归
测试 `only_config_json_triggers_watch_callback` 把守，改动前先读它。

## 数据安全相关的实现细节

| 场景 | 行为 |
| --- | --- |
| `config.json` 解析失败 | 原文改名为 `config.json.corrupt` 留档，再以默认配置启动。**不会** `unwrap_or_default()` 后直接落盘覆盖用户全部配置 |
| `reload()` 解析失败 | 返回错误并**保持现有内存数据不变**（监听线程可能在文件写到一半时触发） |
| 并发 `set` | 「取快照 → 写 `.tmp` → rename」整段在 `save_lock` 内串行。否则两个并发 `set` 各取快照、先后 rename，先取的旧快照最后落地会抹掉后写的键 |
| 补齐默认值 | `apply_defaults` 只做 `or_insert`，**老键不会被清掉** |
| 跨版本兼容 | `ui_language` / `aliyun_*` 等已废弃键保留在文件里但被忽略，不会被删 |

## i18n 约定

- 20 个 locale 目录在 `locales/<tag>/main.ftl`，`fluent_templates::static_loader` 编译期嵌入，无外部文件依赖。
- **只有 `zh-CN` 与 `en-US` 必须保持完整**，其余 18 个按块翻译、缺失部分运行时回退 `en-US`。
- 新增文案只补这两种即可；`i18n::tests` 会强制两者键集一致，并校验 UI 引用的键确实能解析。
- 界面语言键是 `app_language`（对齐原版）；`ui_language` 曾是重复设计，已删除。

## 测试

```bash
cargo test -p oxidict-core
```

已有测试集中在两处真实风险上：`config.rs` 的 watch 过滤规则（生产事故防线）、`model.rs` 的
`as_text()` 展平顺序与 `HasConfig` 类型化读取（缺失/类型不符按默认处理，不 panic）。
