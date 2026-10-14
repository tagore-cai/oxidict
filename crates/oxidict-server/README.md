# oxidict-server

**本机外部调用接口。**

> 内部结构、逐文件职责、不变量清单与设计决策 → [ARCHITECTURE.md](./ARCHITECTURE.md)

与原实现（`src-tauri/src/server.rs`，tiny_http 监听 `127.0.0.1:60606`）
保持**相同的路由约定**，第三方软件可以用 curl 触发翻译。

## 依赖

```
oxidict-server  →  oxidict-core
被依赖：app（唯一）
```

外部依赖：`tiny_http`（阻塞式 HTTP 服务器）、`anyhow`、`log`。**故意很薄**——只有 190 行。

## 为什么是注入回调而不是直接依赖

`ServerContext` 用两个 `Arc<dyn Fn>` 把「取词」和「翻译」注入进来，而不是让本 crate 依赖
`oxidict-services`：

```rust
pub struct ServerContext {
    pub selected_text: Arc<dyn Fn() -> Result<String> + Send + Sync>,
    translate: Arc<dyn Fn(TranslateRequest) -> Result<TranslateResult> + Send + Sync>,
}
```

好处：本板条箱只做 HTTP 协议层（方法/路径/状态码/body），服务选择与历史写入留在 `app`，
且 `route()` 可以脱离网络单测——测试直接打表验证，不用真监听端口。

`app/src/main.rs` 注入的翻译回调就是 CLI 用的那个 `translate_once`：
**服务选择、历史写入全进程只有一份实现**。

## 路由表

| 方法 + 路径 | 行为 | 状态码 |
| --- | --- | --- |
| `GET /ping` | 健康检查，返回 `ok` | 200 |
| `POST /` 或 `POST /translate` | body 即待译文本（自动 `trim`，空则报错） | 200 / 500 |
| `GET /selection_translate` | 由宿主取词（`selected_text` 回调）后翻译 | 200 / 500 |
| `GET /config`、`/input_translate`、`/ocr_recognize`、`/ocr_translate` | 依赖图形界面的窗口端点 | **501** |
| 其它 | 未知路径 | 404 |

```bash
# 第三方软件的典型用法
curl -s -X POST http://127.0.0.1:60606/translate -d 'hello world'
curl -s http://127.0.0.1:60606/selection_translate    # 翻译当前选中文本
curl -s http://127.0.0.1:60606/ping
```

两个刻意的行为：

1. **窗口端点返回 501 而不是假装成功**。这些端点依赖图形界面（打开设置窗、OCR 窗等），
   在 UI 层就绪前明确告知「未实现」比返回一个空洞的 200 更好排查。
2. **空 body 直接报错**（映射为 500），而不是拿空字符串去跑一遍翻译。

## 启动

```rust
// app 启动流程：
let ctx = oxidict_server::ServerContext::new(
    Arc::new(oxidict_platform::selection::selected_text),
    Arc::new(|req| translate_once(req).map(|(_i, r)| r)),
);
if let Err(e) = oxidict_server::start_from_config(ctx) {
    // 端口被占用（例如老版本同时在跑）不应阻塞主程序。
    log::warn!("外部调用服务未启动: {e}");
}
```

- `start_from_config` 从配置读 `server_port`（默认 `60606`）。
- 服务跑在一个独立 `std::thread` 的阻塞循环里，**不占 GPUI 主线程**，也不占 tokio worker。
- **监听失败只记警告不退出**：端口可能被正在运行的老版本 Tauri 应用占用，那不该让新版本打不开。
- 翻译回调内部走 `translate_first_enabled_blocking`（`runtime::handle().block_on`），
  这是它必须跑在独立线程上的原因——tiny_http 的工作线程不是 tokio 上下文。

## 测试

```bash
cargo test -p oxidict-server
```

`route_dispatches_paths_and_status_codes` 覆盖全部路径 × 状态码分支（含 404、501、空 body、
回调失败冒泡）。注意测试里所有断言写在一个 `#[test]` 里：全局 `ConfigStore` 是 `OnceCell`，
只能初始化一次。
