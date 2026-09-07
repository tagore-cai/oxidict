//! 宿主与脚本之间的作业桥。
//!
//! gpui-shell 没有「Rust 调用脚本函数」的公开接口：`HostValue` 只承载纯数据，
//! 插件入口又是 `ScriptView`。所以这里把控制权反转过来——
//! 脚本启动后进入 `await take_job(...)` 的循环，宿主把作业塞进队列；
//! 脚本算完再调 `submit_result(...)` 回传。因为 `take_job` 是 async_function，
//! 等待发生在 channel 上，不是轮询，不做忙等。

use gpui_shell::{HostArguments, HostError, HostModule, HostObject, HostValue};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

pub type JobId = u64;

/// 一次作业的结果。脚本侧只回传纯数据，宿主自行解释。
pub type JobOutcome = std::result::Result<HostValue, String>;

/// 宿主交给脚本的作业。
#[derive(Debug, Clone)]
pub struct Job {
    pub id: JobId,
    /// `translate` / `recognize` / `tts` / `collection`
    pub kind: &'static str,
    pub text: String,
    pub from: String,
    pub to: String,
    /// 服务实例配置。
    pub config: Vec<(String, HostValue)>,
    /// OCR 用的图片（base64），仅 recognize 作业有值。
    pub image: Option<String>,
}

struct Pending {
    job: Job,
    reply: oneshot::Sender<JobOutcome>,
}

/// 一个插件对应一座桥。
pub struct PluginBridge {
    jobs: mpsc::UnboundedSender<Pending>,
    queue: tokio::sync::Mutex<mpsc::UnboundedReceiver<Pending>>,
    replies: Mutex<HashMap<JobId, oneshot::Sender<JobOutcome>>>,
}

impl PluginBridge {
    fn new() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            jobs: tx,
            queue: tokio::sync::Mutex::new(rx),
            replies: Mutex::new(HashMap::new()),
        }
    }

    /// 提交作业并等待结果。`timeout` 到期即失败，避免插件挂死拖住整个翻译面板。
    pub async fn call(&self, job: Job, timeout: std::time::Duration) -> crate::Result<HostValue> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.jobs
            .send(Pending {
                job,
                reply: reply_tx,
            })
            .map_err(|_| {
                crate::Error::Plugin("插件作业队列已关闭，插件可能已卸载".into())
            })?;

        let outcome = match tokio::time::timeout(timeout, reply_rx).await {
            Ok(Ok(outcome)) => outcome,
            Ok(Err(_)) => {
                return Err(crate::Error::Plugin("插件在执行期间被卸载".into()));
            }
            Err(_) => {
                return Err(crate::Error::Plugin(format!(
                    "插件 {} 秒内没有响应，已放弃本次调用",
                    timeout.as_secs()
                )));
            }
        };
        outcome.map_err(crate::Error::Plugin)
    }
}

static NEXT_JOB_ID: AtomicU64 = AtomicU64::new(1);

pub fn next_job_id() -> JobId {
    NEXT_JOB_ID.fetch_add(1, Ordering::Relaxed)
}

static BRIDGES: once_cell::sync::Lazy<Mutex<HashMap<String, Arc<PluginBridge>>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(HashMap::new()));

pub fn bridge_for(plugin_id: &str) -> Arc<PluginBridge> {
    BRIDGES
        .lock()
        .entry(plugin_id.to_string())
        .or_insert_with(|| Arc::new(PluginBridge::new()))
        .clone()
}

pub fn drop_bridge(plugin_id: &str) {
    BRIDGES.lock().remove(plugin_id);
}

/// legacy `.potext` 插件的 `utils` 表面。
///
/// 老宿主（Tauri JS 侧）注入的 utils 里有 tauriFetch/http/readTextFile/
/// readBinaryFile/run/cacheDir/pluginDir/osType/CryptoJS。这里用 Rust 原生实现
/// 等价能力：HTTP 走 reqwest（所以插件脚本自身不需要网络 capability），
/// 文件读取限定在插件数据目录内由调用方保证。
fn build_utils_object() -> HostValue {
    use gpui_shell::HostObject;
    HostObject::new()
        .field("cacheDir", HostValue::Str(saladict_core::config::config().app_dir().to_string_lossy().to_string()))
        .field("pluginDir", HostValue::Str(saladict_core::config::config().plugin_dir().to_string_lossy().to_string()))
        .field("osType", HostValue::Str(std::env::consts::OS.to_string()))
        .into()
}

/// 注册宿主模块 `saladict`，插件通过 `import { take_job, utils } from "saladict"` 使用。
pub fn register_host_module() -> std::result::Result<(), HostError> {
    gpui_shell::export_module(
        HostModule::new("saladict")
            .async_function("take_job", |args: &HostArguments| {
                let plugin_id = args.string(0)?.to_owned();
                let bridge = bridge_for(&plugin_id);
                Ok(Box::pin(async move {
                    let pending = {
                        let mut queue = bridge.queue.lock().await;
                        queue.recv().await
                    };
                    let pending = match pending {
                        Some(p) => p,
                        None => return Ok(HostValue::Null),
                    };
                    bridge.replies.lock().insert(pending.job.id, pending.reply);
                    Ok(job_to_value(&pending.job))
                }) as std::pin::Pin<
                    Box<dyn std::future::Future<Output = gpui_shell::HostResult> + Send>,
                >)
            })
            .function("submit_result", |args: &HostArguments| {
                let plugin_id = args.string(0)?.to_owned();
                let id = args.number(1)? as JobId;
                let ok = args.boolean(2)?;
                let value = args.value(3)?.clone();
                let bridge = bridge_for(&plugin_id);
                let reply = bridge.replies.lock().remove(&id);
                match reply {
                    Some(tx) => {
                        let outcome = if ok {
                            Ok(value)
                        } else {
                            Err(match value {
                                HostValue::Str(s) => s,
                                other => format!("{:?}", other),
                            })
                        };
                        // 脚本可能已经放弃等待，发送失败不算错误。
                        let _ = tx.send(outcome);
                        Ok(HostValue::Bool(true))
                    }
                    None => Ok(HostValue::Bool(false)),
                }
            })
            .async_function("http_request", |args: &HostArguments| {
                // legacy utils.http / tauriFetch 的宿主代发实现。
                // 参数: (method, url, headers_obj, body_str)
                let method = args.string(0)?.to_owned();
                let url = args.string(1)?.to_owned();
                let headers: Vec<(String, String)> = match args.value(2) {
                    Ok(HostValue::Object(pairs)) => pairs
                        .iter()
                        .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                        .collect(),
                    _ => vec![],
                };
                let body = args.value(3).ok().and_then(|v| match v {
                    HostValue::Str(s) => Some(s.clone()),
                    HostValue::Null => None,
                    other => Some(crate::bridge::host_to_json(&other).to_string()),
                });

                Ok(Box::pin(async move {
                    use saladict_net::NetErr as _;
                    let mut rb = match method.to_ascii_uppercase().as_str() {
                        "POST" => saladict_net::post_with_headers(&url, &[]),
                        _ => saladict_net::get_with_headers(&url, &[]),
                    };
                    for (k, v) in &headers {
                        rb = rb.header(k.as_str(), v.as_str());
                    }
                    if let Some(body) = body {
                        rb = rb.header("Content-Type", "application/json").body(body);
                    }
                    // HostError 是外部类型，无法实现 From<core::Error>，显式映射。
                    let host_err = |e: saladict_core::Error| HostError::new(e.to_string());
                    let resp = rb.send().await.net_err().map_err(host_err)?;
                    let resp = saladict_net::check(resp).await.map_err(host_err)?;
                    let text = resp.text().await.net_err().map_err(host_err)?;
                    Ok(HostValue::Str(text))
                }) as std::pin::Pin<
                    Box<dyn std::future::Future<Output = gpui_shell::HostResult> + Send>,
                >)
            })
            .function("utils", |_| Ok(build_utils_object()))
            .function("read_text_file", |args: &HostArguments| {
                let path = args.string(0)?.to_owned();
                // 限定插件目录内，防止越权读取。
                let root = saladict_core::config::config().plugin_dir();
                let full = root.join(&path);
                if !full.starts_with(&root) {
                    return Err(HostError::new("read_text_file: 路径越出插件目录"));
                }
                match std::fs::read_to_string(&full) {
                    Ok(s) => Ok(HostValue::Str(s)),
                    Err(e) => Err(HostError::new(format!("read_text_file 失败: {e}"))),
                }
            })
            .function("run", |args: &HostArguments| {
                // legacy utils.run：在插件目录下执行外部二进制。
                let program = args.string(0)?.to_owned();
                let cmd_args: Vec<String> = (1..args.len())
                    .filter_map(|i| args.value(i).ok().and_then(|v| match v {
                        HostValue::Str(s) => Some(s.clone()),
                        _ => None,
                    }))
                    .collect();
                let root = saladict_core::config::config().plugin_dir();
                let full = root.join(&program);
                if !full.starts_with(&root) {
                    return Err(HostError::new("run: 可执行文件必须位于插件目录内"));
                }
                match std::process::Command::new(&full).args(&cmd_args).output() {
                    Ok(out) => Ok(HostValue::Str(String::from_utf8_lossy(&out.stdout).to_string())),
                    Err(e) => Err(HostError::new(format!("run 失败: {e}"))),
                }
            })
            .declarations(
                r#"
                export interface SaladictJob {
                    id: number;
                    kind: string;
                    text: string;
                    from: string;
                    to: string;
                    config: Record<string, unknown>;
                    image?: string;
                }
                export function take_job(pluginId: string): Promise<SaladictJob | null>;
                export function submit_result(
                    pluginId: string,
                    id: number,
                    ok: boolean,
                    value: unknown,
                ): boolean;
                export const utils: Record<string, unknown>;
                export function http_request(
                    method: string,
                    url: string,
                    headers: Record<string, string>,
                    body?: string | null,
                ): Promise<string>;
                export function read_text_file(path: string): string;
                export function run(program: string, ...args: string[]): string;
                "#,
            ),
    )
}

fn job_to_value(job: &Job) -> HostValue {
    let mut fields: Vec<(String, HostValue)> = vec![
        ("id".into(), HostValue::Number(job.id as f64)),
        ("kind".into(), HostValue::Str(job.kind.to_string())),
        ("text".into(), HostValue::Str(job.text.clone())),
        ("from".into(), HostValue::Str(job.from.clone())),
        ("to".into(), HostValue::Str(job.to.clone())),
        (
            "config".into(),
            HostValue::Object(job.config.clone()),
        ),
    ];
    if let Some(image) = &job.image {
        fields.push(("image".into(), HostValue::Str(image.clone())));
    }
    HostValue::Object(fields)
}

/// 把 `serde_json::Map` 转成可跨边界传递的纯数据。
pub fn config_to_host_value(
    config: &serde_json::Map<String, serde_json::Value>,
) -> Vec<(String, HostValue)> {
    config.iter().map(|(k, v)| (k.clone(), json_to_host(v))).collect()
}

pub fn json_to_host(v: &serde_json::Value) -> HostValue {
    match v {
        serde_json::Value::Null => HostValue::Null,
        serde_json::Value::Bool(b) => HostValue::Bool(*b),
        serde_json::Value::Number(n) => HostValue::Number(n.as_f64().unwrap_or(0.0)),
        serde_json::Value::String(s) => HostValue::Str(s.clone()),
        serde_json::Value::Array(a) => HostValue::Array(a.iter().map(json_to_host).collect()),
        serde_json::Value::Object(o) => HostValue::Object(
            o.iter()
                .map(|(k, v)| (k.clone(), json_to_host(v)))
                .collect(),
        ),
    }
}

/// 把脚本回传的纯数据转回 JSON。
pub fn host_to_json(v: &HostValue) -> serde_json::Value {
    match v {
        HostValue::Null => serde_json::Value::Null,
        HostValue::Bool(b) => serde_json::Value::Bool(*b),
        HostValue::Number(n) => serde_json::json!(n),
        HostValue::Str(s) => serde_json::Value::String(s.clone()),
        HostValue::Array(a) => serde_json::Value::Array(a.iter().map(host_to_json).collect()),
        HostValue::Object(o) => {
            let map: serde_json::Map<String, serde_json::Value> =
                o.iter().map(|(k, v)| (k.clone(), host_to_json(v))).collect();
            serde_json::Value::Object(map)
        }
    }
}
