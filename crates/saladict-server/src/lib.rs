//! saladict-server：本机外部调用接口。
//!
//! 与原实现（src-tauri/src/server.rs，tiny_http 监听 127.0.0.1:60606）保持
//! 相同的路由约定，第三方软件可以用 curl 触发翻译。

use saladict_core::{Error, Result, TranslateRequest, TranslateResult};
use saladict_services::Translator;
use std::io::Read;
use std::sync::Arc;
use tiny_http::{Method, Response, Server};

/// 外部调用所需的运行时回调。
#[derive(Clone)]
pub struct ServerContext {
    /// 取当前选中文本（划词翻译），由上层注入。
    pub selected_text: Arc<dyn Fn() -> Result<String> + Send + Sync>,
    /// 执行翻译（含服务选择与语言记忆），由上层注入。
    translate: Arc<dyn Fn(TranslateRequest) -> Result<TranslateResult> + Send + Sync>,
}

impl ServerContext {
    pub fn new(
        selected_text: Arc<dyn Fn() -> Result<String> + Send + Sync>,
        translate: Arc<dyn Fn(TranslateRequest) -> Result<TranslateResult> + Send + Sync>,
    ) -> Self {
        Self {
            selected_text,
            translate,
        }
    }

    /// 执行翻译。
    pub fn translate(&self, req: TranslateRequest) -> Result<TranslateResult> {
        (self.translate)(req)
    }
}

/// 启动本地 HTTP 服务，返回后服务已在后台线程运行。
pub fn start(port: u16, ctx: ServerContext) -> Result<()> {
    let server = Server::http(("127.0.0.1", port))
        .map_err(|e| Error::Other(anyhow::anyhow!("无法监听 127.0.0.1:{}: {}", port, e)))?;
    log::info!("外部调用接口已启动: http://127.0.0.1:{}", port);

    std::thread::spawn(move || {
        for mut request in server.incoming_requests() {
            let ctx = ctx.clone();
            let method = request.method().clone();
            let url = request.url().to_string();

            // 读取请求体（翻译接口的文本走 POST body）。
            let mut body = String::new();
            let _ = request.as_reader().read_to_string(&mut body);

            let path = url.split('?').next().unwrap_or("");
            let response = route(&ctx, &method, path, &body);
            let (status, text) = match response {
                Ok(ok) => ok,
                Err(e) => (500u16, e.to_string()),
            };
            let resp = Response::from_string(text).with_status_code(status);
            let _ = request.respond(resp);
        }
    });
    Ok(())
}

fn route(ctx: &ServerContext, method: &Method, path: &str, body: &str) -> Result<(u16, String)> {
    match (method, path) {
        // 健康检查，供外部脚本探测服务是否存活。
        (Method::Get, "/ping") => Ok((200, "ok".into())),

        // POST / 或 /translate：body 为待翻译文本。
        (Method::Post, "/") | (Method::Post, "/translate") => {
            let text = body.trim();
            if text.is_empty() {
                return Err(Error::Service("请求体为空，请把待翻译文本放进 body".into()));
            }
            let result = translate_text(ctx, text)?;
            Ok((200, result.as_text()))
        }

        // 划词翻译：由宿主完成取词。
        (Method::Get, "/selection_translate") => {
            let text = (ctx.selected_text)()?;
            let result = translate_text(ctx, &text)?;
            Ok((200, result.as_text()))
        }

        // 其余窗口类端点在 UI 层落地前先占位。
        (Method::Get, "/config")
        | (Method::Get, "/input_translate")
        | (Method::Get, "/ocr_recognize")
        | (Method::Get, "/ocr_translate") => Ok((
            501,
            "该端点依赖图形界面，尚未在 gpui-kit UI 层就绪".into(),
        )),

        _ => Ok((404, "not found".into())),
    }
}

fn translate_text(ctx: &ServerContext, text: &str) -> Result<TranslateResult> {
    let store = saladict_core::config::config();
    let list = store.service_list(saladict_core::config::keys::TRANSLATE_SERVICE_LIST);
    let instance = list
        .first()
        .ok_or_else(|| Error::Service("翻译服务列表为空".into()))?;

    let (svc, cfg) = saladict_services::services().resolve_translator(instance, &store)?;
    let req = TranslateRequest::new(text, store.source_language(), store.target_language())
        .with_config(cfg);
    ctx.translate(req)
}

/// 便捷封装：读取配置端口并启动服务。
pub fn start_from_config(ctx: ServerContext) -> Result<()> {
    start(saladict_core::config::config().server_port(), ctx)
}
