//! HTTP 客户端。
//!
//! 原实现走 `@tauri-apps/api/http` 的 `fetch`，由 WebView 代理到 Rust 网络层。
//! 现在直接在 Rust 内发请求，签名与解析都在同一进程完成，省掉一次跨进程往返。

use futures::Stream;
use oxidict_core::{Error, Result};
use reqwest::{Client, IntoUrl, RequestBuilder, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::sync::Arc;
use std::time::Duration;

const DEFAULT_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

// 与项目其余部分一致用 parking_lot：无 poisoning，读写失败不会 panic，
// 也就不需要 `unwrap()`。
static CLIENT: parking_lot::RwLock<Option<Arc<Client>>> = parking_lot::RwLock::new(None);

fn build_client() -> Client {
    let mut builder = Client::builder()
        .user_agent(DEFAULT_UA)
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10));

    if let Some(proxy) = oxidict_core::config::config().proxy()
        && let Ok(p) = reqwest::Proxy::http(&proxy)
    {
        builder = builder.proxy(p);
    }
    // 构建失败极罕见（TLS 后端初始化问题），但静默回退会丢掉 UA/超时/代理
    // 等全部配置且无从排查，至少要把原因记下来。
    builder.build().unwrap_or_else(|e| {
        log::warn!("构建 HTTP client 失败（代理配置可能未生效），回退默认 client: {e}");
        Client::new()
    })
}

/// 进程内共享的 HTTP 客户端。代理配置变化后调用 [`rebuild_client`] 热更新。
pub fn client() -> Arc<Client> {
    if let Some(c) = CLIENT.read().as_ref() {
        return c.clone();
    }
    let mut write = CLIENT.write();
    // Double-check：另一线程可能已经创建了。
    if let Some(c) = write.as_ref() {
        return c.clone();
    }
    let c = Arc::new(build_client());
    *write = Some(c.clone());
    c
}

/// 代理配置变化后重建客户端，下次调用 [`client`] 即拿到新实例。
pub fn rebuild_client() {
    let mut write = CLIENT.write();
    *write = Some(Arc::new(build_client()));
}

/// 把 `reqwest::Error` 映射为 `Error::Network`。
///
/// core 不依赖 reqwest（它只管领域模型），所以转换放在这一层，
/// 而不是给 `oxidict_core::Error` 加一个 `From<reqwest::Error>`。
pub trait NetErr<T> {
    fn net_err(self) -> Result<T>;
}

impl<T> NetErr<T> for std::result::Result<T, reqwest::Error> {
    fn net_err(self) -> Result<T> {
        self.map_err(|e| Error::Network(e.to_string()))
    }
}

/// 统一的响应校验：非 2xx 时转成带状态码与响应体的错误。
pub async fn check(resp: Response) -> Result<Response> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    let body = resp.text().await.unwrap_or_default();
    Err(Error::Http {
        status: status.as_u16(),
        body: truncate(&body, 512),
    })
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max).collect();
        format!("{}…", cut)
    }
}

pub async fn get_json<T: DeserializeOwned>(url: impl IntoUrl) -> Result<T> {
    let resp = client().get(url).send().await.net_err()?;
    let resp = check(resp).await?;
    resp.json().await.net_err()
}

/// 返回原始的 `serde_json::Value`，用于 Google 这类返回异构数组的接口。
pub async fn get_value(url: impl IntoUrl) -> Result<serde_json::Value> {
    let resp = client().get(url).send().await.net_err()?;
    let resp = check(resp).await?;
    resp.json().await.net_err()
}

pub async fn post_json<B: Serialize, T: DeserializeOwned>(
    url: impl IntoUrl,
    body: &B,
) -> Result<T> {
    let resp = client().post(url).json(body).send().await.net_err()?;
    let resp = check(resp).await?;
    resp.json().await.net_err()
}

pub async fn post_json_value<B: Serialize>(
    url: impl IntoUrl,
    body: &B,
) -> Result<serde_json::Value> {
    let resp = client().post(url).json(body).send().await.net_err()?;
    let resp = check(resp).await?;
    resp.json().await.net_err()
}

pub async fn post_form<T: DeserializeOwned>(url: impl IntoUrl, form: &[(&str, &str)]) -> Result<T> {
    let resp = client().post(url).form(form).send().await.net_err()?;
    let resp = check(resp).await?;
    resp.json().await.net_err()
}

pub async fn get_text(url: impl IntoUrl) -> Result<String> {
    let resp = client().get(url).send().await.net_err()?;
    let resp = check(resp).await?;
    resp.text().await.net_err()
}

pub async fn get_bytes(url: impl IntoUrl) -> Result<Vec<u8>> {
    let resp = client().get(url).send().await.net_err()?;
    let resp = check(resp).await?;
    Ok(resp.bytes().await.net_err()?.to_vec())
}

/// 便捷构造：带一组自定义头的 GET。
pub fn get_with_headers(url: impl IntoUrl, headers: &[(&str, &str)]) -> RequestBuilder {
    let mut rb = client().get(url);
    for (k, v) in headers {
        rb = rb.header(*k, *v);
    }
    rb
}

pub fn post_with_headers(url: impl IntoUrl, headers: &[(&str, &str)]) -> RequestBuilder {
    let mut rb = client().post(url);
    for (k, v) in headers {
        rb = rb.header(*k, *v);
    }
    rb
}

/// 解析 SSE 流（`data: {...}` 行），供 LLM 类服务增量输出。
pub fn sse_text(resp: Response) -> impl Stream<Item = Result<String>> {
    use futures::StreamExt;
    resp.bytes_stream()
        .scan(String::new(), |buf, chunk| {
            let out = match chunk {
                Ok(bytes) => {
                    buf.push_str(&String::from_utf8_lossy(&bytes));
                    let mut lines = Vec::new();
                    while let Some(idx) = buf.find('\n') {
                        let line = buf.drain(..=idx).collect::<String>();
                        lines.push(Ok(line.trim_end().to_string()));
                    }
                    lines
                }
                Err(e) => vec![Err(Error::Network(e.to_string()))],
            };
            futures::future::ready(Some(out))
        })
        .flat_map(futures::stream::iter)
}

/// 从一行 SSE 数据中提取内容。`[DONE]` 表示流结束。
pub fn sse_payload(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("data:")?;
    let rest = rest.trim();
    if rest == "[DONE]" {
        return None;
    }
    Some(rest)
}
