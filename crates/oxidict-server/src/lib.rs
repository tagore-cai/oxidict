//! oxidict-server：本机外部调用接口。
//!
//! 与原实现（src-tauri/src/server.rs，tiny_http 监听 127.0.0.1:60606）保持
//! 相同的路由约定，第三方软件可以用 curl 触发翻译。

use oxidict_core::{Error, Result, TranslateRequest, TranslateResult};
use std::sync::Arc;
use tiny_http::{Method, Response, Server};

/// 外部调用所需的运行时回调。
#[derive(Clone)]
pub struct ServerContext {
    /// 取当前选中文本（划词翻译），由上层注入。
    pub selected_text: Arc<dyn Fn() -> Result<String> + Send + Sync>,
    /// 执行翻译（含服务选择与历史写入），由上层注入。
    ///
    /// 「读启用列表 -> 取首个实例 -> resolve -> translate」的解析逻辑统一在
    /// [`oxidict_services::translate_first_enabled`]，本层只负责按当前配置
    /// 组装请求，不重复实现一遍服务选择。
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
            Ok((200, ctx.translate(request_from_store(text))?.as_text()))
        }

        // 划词翻译：由宿主完成取词。
        (Method::Get, "/selection_translate") => {
            let text = (ctx.selected_text)()?;
            Ok((200, ctx.translate(request_from_store(&text))?.as_text()))
        }

        // 其余窗口类端点在 UI 层落地前先占位。
        (Method::Get, "/config")
        | (Method::Get, "/input_translate")
        | (Method::Get, "/ocr_recognize")
        | (Method::Get, "/ocr_translate") => {
            Ok((501, "该端点依赖图形界面，尚未在 gpui-kit UI 层就绪".into()))
        }

        _ => Ok((404, "not found".into())),
    }
}

/// 按当前配置组装一个翻译请求：语言取 config 里的源/目标语言。
///
/// 不在这里解析服务实例——那是注入回调的职责。
fn request_from_store(text: &str) -> TranslateRequest {
    let store = oxidict_core::config::config();
    TranslateRequest::new(text, store.source_language(), store.target_language())
}

/// 便捷封装：读取配置端口并启动服务。
pub fn start_from_config(ctx: ServerContext) -> Result<()> {
    start(oxidict_core::config::config().server_port(), ctx)
}

#[cfg(test)]
mod tests {
    // 测试中 unwrap 直观且失败即测试失败，豁免。
    #![allow(clippy::unwrap_used)]

    use super::*;
    use oxidict_core::config::ConfigStore;

    /// `route` 只依赖注入的回调，不需要真的监听端口，这里直接打表验证。
    ///
    /// 全部断言放在一个测试里：全局 ConfigStore 是 OnceCell，只能初始化一次。
    #[test]
    fn route_dispatches_paths_and_status_codes() {
        let dir = std::env::temp_dir().join(format!("oxidict-server-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("创建测试配置目录");
        ConfigStore::init_with_path(dir.join("config.json")).expect("初始化配置");

        let ok = ServerContext::new(
            Arc::new(|| Ok("selected text".into())),
            Arc::new(|req: TranslateRequest| {
                // 回调收到的是待译文本，回译结果由测试控制。
                Ok(TranslateResult::Plain(format!("译:{}", req.text)))
            }),
        );

        // 健康检查。
        assert_eq!(
            route(&ok, &Method::Get, "/ping", "").unwrap(),
            (200, "ok".into())
        );

        // POST 翻译：`/` 与 `/translate` 等价，body 即待译文本。
        assert_eq!(
            route(&ok, &Method::Post, "/translate", "hello").unwrap(),
            (200, "译:hello".into())
        );
        assert_eq!(
            route(&ok, &Method::Post, "/", "hello").unwrap(),
            (200, "译:hello".into())
        );

        // 空 body 直接报错（调用方映射为 500）。
        assert!(route(&ok, &Method::Post, "/translate", "   ").is_err());

        // 划词翻译：文本来自 selected_text 回调而非 body。
        assert_eq!(
            route(&ok, &Method::Get, "/selection_translate", "").unwrap(),
            (200, "译:selected text".into())
        );

        // 依赖 GUI 的窗口端点尚未落地，返回 501 而不是假装成功。
        for path in [
            "/config",
            "/input_translate",
            "/ocr_recognize",
            "/ocr_translate",
        ] {
            assert_eq!(route(&ok, &Method::Get, path, "").unwrap().0, 501, "{path}");
        }

        // 未知路径 404。
        assert_eq!(
            route(&ok, &Method::Get, "/nope", "").unwrap(),
            (404, "not found".into())
        );

        // 回调失败冒泡成 Err，由调用方统一转 500。
        let failing = ServerContext::new(
            Arc::new(|| Err(Error::Service("没有选中文本".into()))),
            Arc::new(|_| Err(Error::Service("翻译失败".into()))),
        );
        assert!(route(&failing, &Method::Post, "/translate", "x").is_err());
        assert!(route(&failing, &Method::Get, "/selection_translate", "").is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
