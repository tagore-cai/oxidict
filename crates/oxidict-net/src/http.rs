//! HTTP 客户端。
//!
//! 原实现走 `@tauri-apps/api/http` 的 `fetch`，由 WebView 代理到 Rust 网络层。
//! 现在直接在 Rust 内发请求，签名与解析都在同一进程完成，省掉一次跨进程往返。
//!
//! 底层是 `wreq`（reqwest 的硬分叉，HTTPS 走 BoringSSL）。选它的唯一理由是
//! **可以模拟浏览器指纹**：网页端点常被 WAF 按 TLS（JA3/JA4）与 HTTP/2 特征判定，
//! 默认的非浏览器 ClientHello 会被 403 或直接重置连接；`wreq` 还能保留 HTTP/1
//! header 的大小写（reqwest 会小写化，部分 WAF 因此拒绝）。成本与取舍见 README。

use futures::Stream;
use oxidict_core::{Error, Result, TlsProfile};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::sync::Arc;
use std::time::Duration;
use wreq::{Client, IntoUri, RequestBuilder, Response};

const DEFAULT_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

/// 已建好的客户端缓存：`(档位, client)`。
///
/// 只会有 3 个档位，用线性扫描的小 Vec 而不是 HashMap——规模上没必要，
/// 也避免为一个三元素集合引入哈希表。
static CLIENTS: parking_lot::RwLock<Vec<(TlsProfile, Arc<Client>)>> =
    parking_lot::RwLock::new(Vec::new());

/// 把档位映射到 `wreq-util` 的指纹档案。
fn emulation(profile: TlsProfile) -> wreq_util::Emulation {
    use wreq_util::{Emulation, Platform, Profile};
    // 平台对齐当前系统：UA 与 client hints 里的平台串要和真实环境一致，
    // 否则"macOS 上跑着却声称自己是 Windows"反而更容易被判定为异常。
    let platform = if cfg!(target_os = "macos") {
        Platform::MacOS
    } else if cfg!(target_os = "windows") {
        Platform::Windows
    } else {
        Platform::Linux
    };
    let archive = match profile {
        // 这两个变体是 wreq-util 里较新的主线版本，且长期维护。
        TlsProfile::Chrome => Profile::Chrome131,
        TlsProfile::Safari => Profile::Safari26,
        // 逻辑上不可达（调用方只在 `is_emulated()` 时才会走到这里）；
        // 真出现时给 Chrome 比给 panic 安全——指纹不匹配只是可能被拦，
        // panic 会让整个翻译流程崩掉。
        TlsProfile::Native => Profile::Chrome131,
    };
    Emulation::builder()
        .profile(archive)
        .platform(platform)
        .build()
}

fn build_client(profile: TlsProfile) -> Client {
    let mut builder = Client::builder()
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10))
        // Cookie 罐按**域名**隔离，多服务共享同一 client 不会跨域串 Cookie；
        // 而多步会话（先取会话再发主请求）正依赖它把 Cookie 带过去。
        .cookie_store(true);
    if profile.is_emulated() {
        // 模拟档连默认头（UA / Accept / client hints）一起交给 emulation，
        // 保证"连接特征 + 头部"是自洽的一套，而不是拼出来的四不像。
        builder = builder.emulation(emulation(profile));
    } else {
        // 原生档保持改造前的行为：显式给一个通用 UA。
        builder = builder.user_agent(DEFAULT_UA);
    }

    // 代理是**可选**设置：这里走 `store_opt()` 而不是 `config()`，
    // 未初始化配置时直连即可，不该 panic（测试与独立工具都会走到这里）。
    if let Some(store) = oxidict_core::config::store_opt()
        && let Some(proxy) = store.proxy()
        && let Ok(p) = wreq::Proxy::http(&proxy)
    {
        builder = builder.proxy(p);
    }
    // 构建失败极罕见（TLS 后端初始化问题），但静默回退会丢掉 UA/超时/代理
    // 等全部配置且无从排查，至少要把原因记下来。
    builder.build().unwrap_or_else(|e| {
        log::warn!("构建 HTTP client 失败（代理/指纹配置可能未生效），回退默认 client: {e}");
        Client::new()
    })
}

/// 取指定指纹档位的共享客户端，按需构建并缓存。
pub fn client_for(profile: TlsProfile) -> Arc<Client> {
    if let Some((_, c)) = CLIENTS.read().iter().find(|(p, _)| *p == profile) {
        return c.clone();
    }
    let mut write = CLIENTS.write();
    // Double-check：另一线程可能已经建好了。
    if let Some((_, c)) = write.iter().find(|(p, _)| *p == profile) {
        return c.clone();
    }
    let c = Arc::new(build_client(profile));
    write.push((profile, c.clone()));
    c
}

/// 进程内共享的原生客户端（多数服务用它）。代理配置变化后调用 [`rebuild_client`]。
pub fn client() -> Arc<Client> {
    client_for(TlsProfile::Native)
}

/// 代理配置变化后清空缓存，下次调用即按新配置重建。
///
/// 指纹档位也一并清掉：代理与指纹属于同一层配置，分开失效会让"改了代理但
/// 指纹档位还用旧 client"这种不一致难以排查。
pub fn rebuild_client() {
    CLIENTS.write().clear();
}

/// 把 `wreq::Error` 映射为 `Error::Network`。
///
/// core 不依赖 wreq（它只管领域模型），所以转换放在这一层，
/// 而不是给 `oxidict_core::Error` 加一个 `From<wreq::Error>`。
pub trait NetErr<T> {
    fn net_err(self) -> Result<T>;
}

impl<T> NetErr<T> for std::result::Result<T, wreq::Error> {
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

pub async fn get_json<T: DeserializeOwned>(url: impl IntoUri) -> Result<T> {
    let resp = client().get(url).send().await.net_err()?;
    let resp = check(resp).await?;
    resp.json().await.net_err()
}

/// 返回原始的 `serde_json::Value`，用于 Google 这类返回异构数组的接口。
pub async fn get_value(url: impl IntoUri) -> Result<serde_json::Value> {
    let resp = client().get(url).send().await.net_err()?;
    let resp = check(resp).await?;
    resp.json().await.net_err()
}

pub async fn post_json<B: Serialize, T: DeserializeOwned>(
    url: impl IntoUri,
    body: &B,
) -> Result<T> {
    let resp = client().post(url).json(body).send().await.net_err()?;
    let resp = check(resp).await?;
    resp.json().await.net_err()
}

pub async fn post_json_value<B: Serialize>(
    url: impl IntoUri,
    body: &B,
) -> Result<serde_json::Value> {
    let resp = client().post(url).json(body).send().await.net_err()?;
    let resp = check(resp).await?;
    resp.json().await.net_err()
}

pub async fn post_form<T: DeserializeOwned>(url: impl IntoUri, form: &[(&str, &str)]) -> Result<T> {
    let resp = client().post(url).form(form).send().await.net_err()?;
    let resp = check(resp).await?;
    resp.json().await.net_err()
}

pub async fn get_text(url: impl IntoUri) -> Result<String> {
    let resp = client().get(url).send().await.net_err()?;
    let resp = check(resp).await?;
    resp.text().await.net_err()
}

pub async fn get_bytes(url: impl IntoUri) -> Result<Vec<u8>> {
    let resp = client().get(url).send().await.net_err()?;
    let resp = check(resp).await?;
    Ok(resp.bytes().await.net_err()?.to_vec())
}

/// 便捷构造：带一组自定义头的 GET。
pub fn get_with_headers(url: impl IntoUri, headers: &[(&str, &str)]) -> RequestBuilder {
    with_headers(client().get(url), headers)
}

pub fn post_with_headers(url: impl IntoUri, headers: &[(&str, &str)]) -> RequestBuilder {
    with_headers(client().post(url), headers)
}

/// 带指定指纹档位的 GET/POST。
///
/// 网页端点型服务与声明式服务用它把实例配置里的 `tls` 落到具体连接上；
/// 其余调用点走 Native 版本，行为与改造前一致。
pub fn get_with_headers_for(
    profile: TlsProfile,
    url: impl IntoUri,
    headers: &[(&str, &str)],
) -> RequestBuilder {
    with_headers(client_for(profile).get(url), headers)
}

pub fn post_with_headers_for(
    profile: TlsProfile,
    url: impl IntoUri,
    headers: &[(&str, &str)],
) -> RequestBuilder {
    with_headers(client_for(profile).post(url), headers)
}

/// [`get_value`] 的指定档位版本。
pub async fn get_value_for(profile: TlsProfile, url: impl IntoUri) -> Result<serde_json::Value> {
    let resp = client_for(profile).get(url).send().await.net_err()?;
    let resp = check(resp).await?;
    resp.json().await.net_err()
}

/// [`post_json_value`] 的指定档位版本。
pub async fn post_json_value_for<B: Serialize>(
    profile: TlsProfile,
    url: impl IntoUri,
    body: &B,
) -> Result<serde_json::Value> {
    let resp = client_for(profile)
        .post(url)
        .json(body)
        .send()
        .await
        .net_err()?;
    let resp = check(resp).await?;
    resp.json().await.net_err()
}

/// [`get_text`] 的指定档位版本（cambridge 这类抓 HTML 的端点用）。
pub async fn get_text_for(profile: TlsProfile, url: impl IntoUri) -> Result<String> {
    let resp = client_for(profile).get(url).send().await.net_err()?;
    let resp = check(resp).await?;
    resp.text().await.net_err()
}

/// [`get_bytes`] 的指定档位版本。
pub async fn get_bytes_for(profile: TlsProfile, url: impl IntoUri) -> Result<Vec<u8>> {
    let resp = client_for(profile).get(url).send().await.net_err()?;
    let resp = check(resp).await?;
    Ok(resp.bytes().await.net_err()?.to_vec())
}

/// 便捷构造：自定义方法 + 自定义头的请求。
///
/// 用于插件宿主代发请求——不能把非 GET/POST 的方法静默降级（那会让插件的
/// PUT/DELETE 变成 GET，语义完全不同且服务端不会报错）。方法名非法时返回
/// `None`，由调用方决定如何呈现错误。
pub fn request_with_headers(
    method: &str,
    url: impl IntoUri,
    headers: &[(&str, &str)],
) -> Option<RequestBuilder> {
    request_with_headers_for(TlsProfile::Native, method, url, headers)
}

/// [`request_with_headers`] 的指定档位版本。
pub fn request_with_headers_for(
    profile: TlsProfile,
    method: &str,
    url: impl IntoUri,
    headers: &[(&str, &str)],
) -> Option<RequestBuilder> {
    let method = wreq::Method::from_bytes(method.as_bytes()).ok()?;
    Some(with_headers(
        client_for(profile).request(method, url),
        headers,
    ))
}

fn with_headers(mut rb: RequestBuilder, headers: &[(&str, &str)]) -> RequestBuilder {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `client_for` 必须按档位分别缓存且可复用同一实例。
    #[test]
    fn client_cache_is_per_profile_and_stable() {
        let native_a = client_for(TlsProfile::Native);
        let native_b = client_for(TlsProfile::Native);
        assert!(Arc::ptr_eq(&native_a, &native_b), "同一档位应复用实例");

        let chrome = client_for(TlsProfile::Chrome);
        assert!(
            !Arc::ptr_eq(&chrome, &native_a),
            "不同档位必须是不同 client（否则指纹配置会互相污染）"
        );
        assert!(Arc::ptr_eq(&chrome, &client_for(TlsProfile::Chrome)));

        // 重建后缓存清空，下次取到的是新实例。
        rebuild_client();
        assert!(!Arc::ptr_eq(&native_a, &client_for(TlsProfile::Native)));
    }
}
