//! 插件运行时：每个插件一个 QuickJS（rquickjs）VM。
//!
//! rquickjs 的 `AsyncRuntime` 在默认（非 parallel）配置下 `!Send`，
//! 因此所有 VM 固定在一个专职线程上执行；调用方通过 channel 提交请求、
//! oneshot 回传结果，超时在调用方一侧控制。目前请求在线程上顺序处理
//! ——与旧 gpui-shell 方案（所有插件 View 挤在 GPUI 主线程）相同的串行度，
//! 插件数量上来后可升级为 `LocalSet` + `spawn_local` 并行。
//!
//! 与旧 gpui-shell 方案的本质区别：不依赖窗口与渲染循环，
//! CLI / HTTP / UI 三种上下文下行为一致。
//!
//! 插件加载语义与原版 Tauri 宿主一致：`main.js` 作为脚本 eval，
//! 入口函数（`translate` / `recognize` / `tts` / `collection`）落在
//! globalThis 上，宿主直接调用。

use crate::host;
use crate::{Error, Result};
use rquickjs::{AsyncContext, AsyncRuntime, Function, Object, Value};
use serde_json::{Map, Value as Json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

/// 单次插件调用的超时，避免插件挂死拖住翻译面板。
const CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// 一次插件调用。
pub struct CallRequest {
    pub plugin_id: String,
    /// 入口函数名：translate / recognize / tts / collection。
    pub func: &'static str,
    /// 翻译/朗读/收藏的文本；OCR 时是 base64 图片。
    pub input: String,
    pub from: String,
    pub to: String,
    /// 服务实例配置。
    pub config: Map<String, Json>,
}

enum Request {
    Load {
        plugin_id: String,
        dir: PathBuf,
        reply: oneshot::Sender<Result<()>>,
    },
    Call {
        req: CallRequest,
        reply: oneshot::Sender<Result<Json>>,
    },
    Unload {
        plugin_id: String,
    },
}

struct PluginVm {
    /// rquickjs 要求 runtime 与 context 同时存活。
    _rt: AsyncRuntime,
    ctx: AsyncContext,
}

static HOST: once_cell::sync::Lazy<HostHandle> = once_cell::sync::Lazy::new(HostHandle::spawn);

#[derive(Clone)]
struct HostHandle {
    tx: mpsc::UnboundedSender<Request>,
}

impl HostHandle {
    fn spawn() -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel();
        std::thread::Builder::new()
            .name("oxidict-plugin".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("无法创建插件线程的 tokio runtime");
                rt.block_on(plugin_loop(&mut rx));
            })
            .expect("无法启动插件线程");
        Self { tx }
    }
}

async fn plugin_loop(rx: &mut mpsc::UnboundedReceiver<Request>) {
    let mut vms: HashMap<String, PluginVm> = HashMap::new();
    while let Some(req) = rx.recv().await {
        match req {
            Request::Load {
                plugin_id,
                dir,
                reply,
            } => {
                let _ = reply.send(load_plugin(&mut vms, plugin_id, dir).await);
            }
            Request::Call { req, reply } => {
                let _ = reply.send(call_plugin(&vms, req).await);
            }
            Request::Unload { plugin_id } => {
                vms.remove(&plugin_id);
            }
        }
    }
}

/// 加载插件（同步阻塞直到完成）。可在任意线程调用，包括 CLI 与 GPUI 主线程。
pub fn load(plugin_id: &str, dir: &Path) -> Result<()> {
    let (reply_tx, reply_rx) = oneshot::channel();
    send(Request::Load {
        plugin_id: plugin_id.to_string(),
        dir: dir.to_path_buf(),
        reply: reply_tx,
    })?;
    match oxidict_core::runtime::handle().block_on(reply_rx) {
        Ok(inner) => inner,
        Err(_) => Err(Error::Plugin("插件线程已退出".into())),
    }
}

/// 卸载插件，VM 随最后一个引用销毁。
pub fn unload(plugin_id: &str) {
    let _ = send(Request::Unload {
        plugin_id: plugin_id.to_string(),
    });
}

/// 提交一次插件调用并等待结果（带超时）。
pub async fn call(req: CallRequest) -> Result<Json> {
    let (reply_tx, reply_rx) = oneshot::channel();
    send(Request::Call {
        req,
        reply: reply_tx,
    })?;
    match tokio::time::timeout(CALL_TIMEOUT, reply_rx).await {
        Ok(Ok(outcome)) => outcome,
        Ok(Err(_)) => Err(Error::Plugin("插件线程已退出".into())),
        Err(_) => Err(Error::Plugin(format!(
            "插件 {} 秒内没有响应，已放弃本次调用",
            CALL_TIMEOUT.as_secs()
        ))),
    }
}

fn send(req: Request) -> Result<()> {
    HOST.tx
        .send(req)
        .map_err(|_| Error::Plugin("插件线程已退出".into()))
}

async fn load_plugin(
    vms: &mut HashMap<String, PluginVm>,
    plugin_id: String,
    dir: PathBuf,
) -> Result<()> {
    let source = std::fs::read_to_string(dir.join("main.js"))
        .map_err(|e| Error::Plugin(format!("读取 main.js 失败: {e}")))?;

    let rt =
        AsyncRuntime::new().map_err(|e| Error::Plugin(format!("创建 JS runtime 失败: {e}")))?;
    // 原生模块 `oxidict`。不挂 FileResolver——插件的 main.js 按脚本 eval，
    // 与老宿主一致，不支持模块相对导入。
    rt.set_loader(
        rquickjs::loader::BuiltinResolver::default().with_module("oxidict"),
        rquickjs::loader::ModuleLoader::default().with_module("oxidict", host::OxidictModule),
    )
    .await;

    let ctx = AsyncContext::full(&rt)
        .await
        .map_err(|e| Error::Plugin(format!("创建 JS context 失败: {e}")))?;

    ctx.with(|ctx| -> std::result::Result<(), Error> {
        host::inject_globals(&ctx).map_err(|e| plugin_js_error(&ctx, e))?;
        ctx.eval::<rquickjs::Value, _>(source.as_str())
            .map_err(|e| plugin_js_error(&ctx, e))?;
        Ok(())
    })
    .await?;

    log::info!("插件 {} VM 就绪", plugin_id);
    vms.insert(plugin_id, PluginVm { _rt: rt, ctx });
    Ok(())
}

async fn call_plugin(vms: &HashMap<String, PluginVm>, req: CallRequest) -> Result<Json> {
    let vm = vms
        .get(&req.plugin_id)
        .ok_or_else(|| Error::Plugin(format!("插件 {} 未加载", req.plugin_id)))?;
    let CallRequest {
        func,
        input,
        from,
        to,
        config,
        ..
    } = req;

    // rquickjs 0.13 起 `async_with!` 宏被标记废弃，直接调用
    // `AsyncContext::async_with`（宏本身只是它的展开，闭包接收 owned `Ctx`）。
    rquickjs::AsyncContext::async_with(&vm.ctx, async |ctx| {
        let options = Object::new(ctx.clone()).map_err(|e| plugin_js_error(&ctx, e))?;
        options
            .set(
                "config",
                host::json_to_js(&ctx, &Json::Object(config))
                    .map_err(|e| plugin_js_error(&ctx, e))?,
            )
            .map_err(|e| plugin_js_error(&ctx, e))?;
        options
            .set("detect", from.as_str())
            .map_err(|e| plugin_js_error(&ctx, e))?;
        options
            .set("setResult", Function::new(ctx.clone(), || ()))
            .map_err(|e| plugin_js_error(&ctx, e))?;
        options
            .set(
                "utils",
                host::build_utils(&ctx).map_err(|e| plugin_js_error(&ctx, e))?,
            )
            .map_err(|e| plugin_js_error(&ctx, e))?;

        let f: Function = ctx
            .globals()
            .get(func)
            .map_err(|_| Error::Plugin(format!("插件未定义入口函数 {func}")))?;

        let out: Value = f
            .call((input, from, to, options))
            .map_err(|e| plugin_js_error(&ctx, e))?;
        // 入口函数可能是 async（返回 Promise）也可能是同步函数。
        let out = if out.is_promise() {
            rquickjs::Promise::from_value(out)
                .map_err(|e| plugin_js_error(&ctx, e))?
                .into_future::<Value>()
                .await
                .map_err(|e| plugin_js_error(&ctx, e))?
        } else {
            out
        };
        host::js_to_json(&out).map_err(|e| plugin_js_error(&ctx, e))
    })
    .await
}

/// 把 rquickjs 错误转成带 JS 侧消息的插件错误。
/// `Error::Exception` 时用 `Ctx::catch` 取回被抛出的值。
fn plugin_js_error(ctx: &rquickjs::Ctx<'_>, err: rquickjs::Error) -> Error {
    if !err.is_exception() {
        return Error::Plugin(err.to_string());
    }
    let thrown = ctx.catch();
    let obj = thrown.as_object();
    let message: Option<String> = obj.and_then(|o| o.get("message").ok());
    let stack: Option<String> = obj.and_then(|o| o.get("stack").ok());
    let detail = match (message, stack) {
        (Some(m), Some(s)) => format!("{m}\n{s}"),
        (Some(m), None) => m,
        (None, Some(s)) => s,
        (None, None) => thrown
            .as_string()
            .and_then(|s| s.to_string().ok())
            .unwrap_or_else(|| err.to_string()),
    };
    Error::Plugin(detail)
}
