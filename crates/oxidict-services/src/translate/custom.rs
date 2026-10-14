//! 配置驱动的**自定义服务**。
//!
//! 目标是「加一个纯映射服务不用写代码」：用户往 config.json 的
//! `custom_services` 数组里写一条 spec，重启即生效。覆盖的情形是
//! **单次 HTTP 请求 + 从 JSON/HTML 响应里取字段**——这也是绝大多数翻译 API 的形态。
//!
//! 表达不了的情形（应当走别的路径，而不是把这里做大）：
//!
//! - 需要**任意编排**的多步流程（条件分支、循环、状态机）→ `.potext` 插件
//! - HMAC 之外的签名协议（如密钥参与 HMAC 的复杂算法）→ 插件
//!
//! 注意：单次"先取会话再发主请求"（`session` 字段）与 HTML 选择器提取
//! （`html_selector`）已经内置，不需要为此写插件。
//!
//! ## 配置示例
//!
//! ```json
//! {
//!   "custom_services": [
//!     {
//!       "id": "my_mt",
//!       "label": "我的翻译",
//!       "endpoint": "https://api.example.com/v1/translate",
//!       "method": "POST",
//!       "auth": { "kind": "bearer" },
//!       "headers": { "X-App": "oxidict" },
//!       "body": { "q": "{{text}}", "source": "{{fromCode}}", "target": "{{toCode}}" },
//!       "extract": { "text": "data.translations.0.text" },
//!       "fields": [
//!         { "kind": "secret", "key": "apiKey", "label": "API Key" }
//!       ]
//!     }
//!   ]
//! }
//! ```
//!
//! ## 占位符
//!
//! 可在 `endpoint` / `headers` / `body` / `auth` 里使用：
//! `{{text}}`、`{{from}}`/`{{to}}`（英文语言名）、`{{fromCode}}`/`{{toCode}}`
//! （内部语言码）、`{{apiKey}}`、`{{salt}}`（每次请求一个 UUID）、
//! `{{timestamp}}`（Unix 秒）、`{{token}}`（会话前置请求取回的值），以及
//! **任意配置字段名**（如 `{{model}}`）。
//!
//! 当某个字符串**整串就是一个占位符**、且该字段在配置里是数字/布尔时，会替换成
//! 对应的 JSON 类型而不是字符串——这样 `"temperature": "{{temperature}}"` 传出去
//! 仍是数字。
//!
//! ## 需要签名的接口
//!
//! 用 `auth.kind = "hash_query"`（明文拼接 + MD5/SHA256/HMAC，结果作为查询参数）：
//!
//! ```json
//! "auth": {
//!   "kind": "hash_query",
//!   "algo": "sha256",
//!   "template": "{{appKey}}{{text}}{{salt}}{{timestamp}}{{appSecret}}",
//!   "extra": { "appid": "{{appid}}", "salt": "{{salt}}", "curtime": "{{timestamp}}" }
//! }
//! ```
//!
//! `{{salt}}` / `{{timestamp}}` 在一次请求内**只生成一次**：签名明文与随请求发出的
//! 参数必须引用同一个值，各生成一次会让服务端算出的签名与收到的不一致。
//!
//! ## 已知能力边界
//!
//! - `{{fromCode}}` / `{{toCode}}` 是**内部码**（下划线形式，如 `zh_cn`），
//!   不是 BCP-47（`zh-CN`），也不带大小写规整。目标 API 若要求其它写法，
//!   可先在 `fields` 里声明一个带默认值的下拉，再用 `{{该字段名}}` 引用；
//!   需要按语言动态变换的，应当走插件。
//! - 没有语言映射的自定义能力（`{{from}}`/`{{to}}` 固定为英文语言名）。
//! - `hash_query` 覆盖"明文拼接 + 哈希"一族；密钥参与 HMAC 运算的复杂协议走插件。

use crate::Translator;
use async_trait::async_trait;
use futures::StreamExt;
use oxidict_core::HasConfig as _;
use oxidict_core::schema::ConfigField;
use oxidict_core::{Error, Language, Result, TlsProfile, TranslateRequest, TranslateResult};
use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

/// 配置里存放自定义服务的键。
pub const CONFIG_KEY: &str = oxidict_core::config::keys::CUSTOM_SERVICES;

/// 鉴权方式（具名变换器）。
#[derive(Debug, Clone, Default, Deserialize, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuthSpec {
    /// 不带鉴权信息。
    #[default]
    None,
    /// `Authorization: Bearer <apiKey>`。
    Bearer,
    /// 密钥放进指定请求头。
    Header { name: String },
    /// 密钥放进查询参数（会做 URL 编码）。
    Query { name: String },
    /// 「明文拼接 + 哈希」类签名，结果作为查询参数发出。
    ///
    /// 覆盖有道/百度这一族的签名协议：
    /// `template` 是签名明文（可用全部占位符，含 `{{salt}}` / `{{timestamp}}`），
    /// 结果放进名为 `name` 的参数；`extra` 里的键值一并作为查询参数发出。
    ///
    /// 例（有道 v3）：
    /// `algo = "sha256"`、
    /// `template = "{{appKey}}{{text}}{{salt}}{{timestamp}}{{appSecret}}"`、
    /// `extra = {"appid": "{{appid}}", "salt": "{{salt}}", "curtime": "{{timestamp}}"}`。
    HashQuery {
        /// `md5` / `sha256` / `hmac_sha256`。
        algo: String,
        /// 签名明文模板。
        template: String,
        /// 签名参数名，默认 `sign`。
        #[serde(default = "default_sign_name")]
        name: String,
        /// `hmac_sha256` 时取 HMAC 密钥的配置字段名（默认 `apiKey`）；
        /// 拼接类哈希忽略此字段。
        #[serde(default = "default_key_field")]
        key_field: String,
        /// 需要一并发出的其它查询参数（值支持占位符）。
        #[serde(default)]
        extra: std::collections::BTreeMap<String, String>,
    },
}

fn default_sign_name() -> String {
    "sign".to_string()
}

fn default_key_field() -> String {
    "apiKey".to_string()
}

/// 多步会话的前置请求：先打一发（通常为了拿会话 Cookie，或一个要注入
/// 主请求的 `{{token}}`）。
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct SessionSpec {
    pub url: String,
    #[serde(default = "default_method")]
    pub method: String,
    /// 从 JSON 响应里取 token 的路径；**留空表示只要 Cookie**。
    #[serde(default)]
    pub token_path: String,
}

/// 流式解析方式。
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamMode {
    /// 不解析流：只读完整响应体。
    #[default]
    None,
    /// SSE：逐行 `data: {...}`，增量取自 `stream_delta` 路径。
    SseJson,
}

/// 从响应里取译文的方式。
#[derive(Debug, Clone, Default, Deserialize, serde::Serialize)]
pub struct ExtractSpec {
    /// 译文路径（JSON 模式），如 `data.translations.0.text`。
    pub text: String,
    /// 流式增量路径；`stream_mode = "sse_json"` 时必填。
    #[serde(default)]
    pub stream_delta: String,
    #[serde(default)]
    pub stream_mode: StreamMode,
    /// **HTML 模式**：设置后响应按 HTML 解析，用 CSS 选择器取文本（多段用
    /// 换行拼接），`text` 字段被忽略。网页端点（词典类）走这里。
    #[serde(default)]
    pub html_selector: Option<String>,
}

/// 表单字段（渲染进设置面板，决定用户能填什么）。
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FieldSpec {
    Text {
        key: String,
        label: String,
        #[serde(default)]
        placeholder: String,
    },
    Secret {
        key: String,
        label: String,
    },
    Select {
        key: String,
        label: String,
        options: Vec<String>,
        #[serde(default)]
        default: String,
    },
    Switch {
        key: String,
        label: String,
        #[serde(default)]
        default: bool,
    },
    Number {
        key: String,
        label: String,
        #[serde(default)]
        default: f64,
        #[serde(default)]
        min: f64,
        #[serde(default)]
        max: f64,
    },
}

impl FieldSpec {
    fn key(&self) -> &str {
        match self {
            Self::Text { key, .. }
            | Self::Secret { key, .. }
            | Self::Select { key, .. }
            | Self::Switch { key, .. }
            | Self::Number { key, .. } => key,
        }
    }

    fn to_config_field(&self) -> ConfigField {
        match self {
            Self::Text {
                key,
                label,
                placeholder,
            } => ConfigField::Text {
                key: intern(key),
                label: intern(label),
                placeholder: intern(placeholder),
                secret: false,
                required: false,
            },
            Self::Secret { key, label } => ConfigField::secret(intern(key), intern(label)),
            Self::Select {
                key,
                label,
                options,
                default,
            } => ConfigField::Select {
                key: intern(key),
                label: intern(label),
                options: intern_options(options),
                default: intern(default),
            },
            Self::Switch {
                key,
                label,
                default,
            } => ConfigField::Switch {
                key: intern(key),
                label: intern(label),
                default: *default,
            },
            Self::Number {
                key,
                label,
                default,
                min,
                max,
            } => ConfigField::Number {
                key: intern(key),
                label: intern(label),
                default: *default,
                min: *min,
                max: *max,
            },
        }
    }
}

fn default_method() -> String {
    "POST".to_string()
}

fn default_tls() -> String {
    "native".to_string()
}

/// 一条自定义服务的完整定义。
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct CustomServiceSpec {
    /// 服务 id。会写进 config.json（实例列表用它），**不能含 `@`**——那是实例
    /// 与备注的分隔符（`id@备注`）。
    pub id: String,
    #[serde(default)]
    pub label: String,
    pub endpoint: String,
    #[serde(default = "default_method")]
    pub method: String,
    #[serde(default)]
    pub auth: AuthSpec,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    /// 请求体模板；`None` 表示不发送 body（GET 常见）。
    #[serde(default)]
    pub body: Option<Value>,
    pub extract: ExtractSpec,
    #[serde(default)]
    pub fields: Vec<FieldSpec>,
    /// 多步会话的前置请求（可选）。
    #[serde(default)]
    pub session: Option<SessionSpec>,
    /// 推荐的 HTTP 指纹档位（见 `oxidict_core::TlsProfile`）。
    ///
    /// 用在对端会被 WAF 按 TLS 指纹拦的服务上；实例配置里的 `tls` 可覆盖它
    /// （作者给推荐值，用户保留最终决定权）。
    #[serde(default = "default_tls")]
    pub tls: String,
}

impl CustomServiceSpec {
    /// 校验并返回**可定位**的错误信息（由调用方带上下标/内容记录日志）。
    ///
    /// 宁可在这里明确拒绝，也不要让一条坏 spec 变成运行时的神秘失败。
    pub fn validate(&self) -> std::result::Result<(), String> {
        let id = self.id.trim();
        if id.is_empty() {
            return Err("id 不能为空".into());
        }
        if self.id.contains('@') {
            return Err(format!(
                "id 不能包含 '@'：它是实例与备注的分隔符（如 `{}@家里`）",
                id
            ));
        }
        if self.id.chars().any(char::is_whitespace) {
            return Err(format!("id 不能含空白字符: {:?}", self.id));
        }
        if !(self.endpoint.starts_with("http://") || self.endpoint.starts_with("https://")) {
            return Err(format!(
                "endpoint 必须以 http:// 或 https:// 开头: {:?}",
                self.endpoint
            ));
        }
        if wreq::Method::from_bytes(self.method.as_bytes()).is_err() {
            return Err(format!("非法的 HTTP 方法: {:?}", self.method));
        }
        if self.extract.text.trim().is_empty() && self.extract.html_selector.is_none() {
            return Err("extract.text 不能为空（除非用 html_selector）".into());
        }
        if self.extract.stream_mode == StreamMode::SseJson
            && self.extract.stream_delta.trim().is_empty()
        {
            return Err("stream_mode 为 sse_json 时必须给出 extract.stream_delta".into());
        }
        if let Some(session) = &self.session
            && !(session.url.starts_with("http://") || session.url.starts_with("https://"))
        {
            return Err(format!(
                "session.url 必须以 http:// 或 https:// 开头: {:?}",
                session.url
            ));
        }
        if let AuthSpec::HashQuery {
            algo,
            template,
            name,
            ..
        } = &self.auth
        {
            if !matches!(algo.as_str(), "md5" | "sha256" | "hmac_sha256") {
                return Err(format!(
                    "auth.algo 只支持 md5 / sha256 / hmac_sha256，实际是 {algo:?}"
                ));
            }
            if template.trim().is_empty() {
                return Err("auth.template（签名明文）不能为空".into());
            }
            if name.trim().is_empty() {
                return Err("auth.name（签名参数名）不能为空".into());
            }
        }
        let mut seen = HashSet::new();
        for f in &self.fields {
            if !seen.insert(f.key()) {
                return Err(format!("fields 里 key 重复: {}", f.key()));
            }
        }
        Ok(())
    }
}

pub struct CustomTranslator {
    spec: Arc<CustomServiceSpec>,
    fields: Vec<ConfigField>,
}

impl CustomTranslator {
    /// 校验并构造。错误只带本 spec 的信息，由调用方补上下文。
    pub fn new(spec: CustomServiceSpec) -> std::result::Result<Self, String> {
        spec.validate()?;
        let fields = spec.fields.iter().map(FieldSpec::to_config_field).collect();
        Ok(Self {
            spec: Arc::new(spec),
            fields,
        })
    }

    /// 展示名（设置面板用；没写 label 就退回 id）。
    pub fn label(&self) -> &str {
        if self.spec.label.trim().is_empty() {
            &self.spec.id
        } else {
            &self.spec.label
        }
    }
}

#[async_trait]
impl Translator for CustomTranslator {
    fn id(&self) -> &str {
        &self.spec.id
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        self.fields.clone()
    }

    fn map_language(&self, lang: Language) -> String {
        super::lang_names::english_name(lang)
    }

    async fn translate(&self, req: TranslateRequest) -> Result<TranslateResult> {
        // 多步会话：先打前置请求拿 token / 会话 Cookie。失败即失败——
        // 没有 token，后面的请求注定 401，与其掩盖不如现在报。
        let token = match &self.spec.session {
            Some(session) => self.fetch_session_token(session, &req).await?,
            None => String::new(),
        };
        let ctx = Ctx::new(&req, &token);
        let api_key = req.str("apiKey").unwrap_or("").to_string();

        let mut url = substitute(&self.spec.endpoint, &ctx);
        let mut headers: Vec<(String, String)> = self
            .spec
            .headers
            .iter()
            .map(|(k, v)| (k.clone(), substitute(v, &ctx)))
            .collect();

        match &self.spec.auth {
            AuthSpec::None => {}
            AuthSpec::Bearer => {
                headers.push(("Authorization".into(), format!("Bearer {api_key}")));
            }
            AuthSpec::Header { name } => headers.push((name.clone(), api_key.clone())),
            AuthSpec::Query { name } => {
                append_query(&mut url, &[(name.clone(), api_key.clone())]);
            }
            AuthSpec::HashQuery {
                algo,
                template,
                name,
                key_field,
                extra,
            } => {
                let params = hash_query_params(algo, template, name, key_field, extra, &ctx);
                append_query(&mut url, &params);
            }
        }

        let method = self.spec.method.to_ascii_uppercase();
        let hdr_refs: Vec<(&str, &str)> = headers
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        // 实例配置的 `tls` 覆盖 spec 的推荐值：作者给建议，用户保留最终决定权。
        let tls = req
            .str("tls")
            .map(TlsProfile::from_code)
            .unwrap_or_else(|| TlsProfile::from_code(&self.spec.tls));

        let body_value = self.spec.body.as_ref().map(|t| render(t, &ctx));
        let build = || -> Result<wreq::RequestBuilder> {
            let rb = oxidict_net::request_with_headers_for(tls, &method, url.clone(), &hdr_refs)
                .ok_or_else(|| {
                    Error::Service(format!(
                        "{}: 非法的 HTTP 方法 {}",
                        self.spec.id, self.spec.method
                    ))
                })?;
            Ok(match &body_value {
                Some(b) => rb.json(b),
                None => rb,
            })
        };

        // 只重试 429 与连接错误，最多 MAX_ATTEMPTS 次，并尊重 `Retry-After`。
        //
        // 刻意**不重试 5xx**：POST 可能已被服务端处理，盲目重试有重复提交风险；
        // 429 是服务端明确说"没处理，稍后再来"，重试是安全的。
        let mut attempt = 0u32;
        let resp = loop {
            attempt += 1;
            match build()?.send().await {
                Ok(resp) if resp.status().as_u16() == 429 && attempt < MAX_ATTEMPTS => {
                    let wait = retry_after_secs(resp.headers().get("retry-after"))
                        .unwrap_or(DEFAULT_BACKOFF_SECS);
                    log::warn!(
                        "{}: 触发限流（429），{wait}s 后重试（第 {attempt}/{MAX_ATTEMPTS} 次）",
                        self.spec.id
                    );
                    tokio::time::sleep(Duration::from_secs(wait)).await;
                }
                Ok(resp) => break resp,
                Err(e) if attempt < MAX_ATTEMPTS => {
                    log::warn!(
                        "{}: 连接失败（{e}），{DEFAULT_BACKOFF_SECS}s 后重试",
                        self.spec.id
                    );
                    tokio::time::sleep(Duration::from_secs(DEFAULT_BACKOFF_SECS)).await;
                }
                Err(e) => return Err(Error::Network(e.to_string())),
            }
        };
        let resp = oxidict_net::check(resp).await?;

        // 流式：仅当 spec 声明了 SSE 且调用方要流式时才走。
        if self.spec.extract.stream_mode == StreamMode::SseJson
            && let Some(sink) = &req.on_stream
        {
            let mut full = String::new();
            let mut s = oxidict_net::sse_text(resp);
            while let Some(line) = s.next().await {
                let line = line?;
                if let Some(data) = oxidict_net::sse_payload(&line)
                    && let Ok(v) = serde_json::from_str::<Value>(data)
                    && let Some(delta) = extract_text(&v, &self.spec.extract.stream_delta)
                {
                    full.push_str(&delta);
                    sink(delta);
                }
            }
            return Ok(TranslateResult::Plain(full.trim().to_string()));
        }

        let text = resp
            .text()
            .await
            .map_err(|e| Error::Network(e.to_string()))?;

        // 提取分两种模式：HTML 选择器（词典类网页端点）与 JSON 路径。
        if let Some(selector) = &self.spec.extract.html_selector {
            let out = extract_html(&text, selector).ok_or_else(|| {
                Error::Service(format!(
                    "{}: HTML 里没有匹配 {} 的内容（响应前 200 字符: {}）",
                    self.spec.id,
                    selector,
                    first_chars(&text, 200)
                ))
            })?;
            return Ok(TranslateResult::Plain(out.trim().to_string()));
        }

        let v: Value = serde_json::from_str(&text)?;
        let out = extract_text(&v, &self.spec.extract.text).ok_or_else(|| {
            Error::Service(format!(
                "{}: 响应里找不到 {}（响应前 200 字符: {}）",
                self.spec.id,
                self.spec.extract.text,
                first_chars(&text, 200)
            ))
        })?;
        Ok(TranslateResult::Plain(out.trim().to_string()))
    }
}

impl CustomTranslator {
    /// 执行会话前置请求，返回 `{{token}}` 的值（未配置 `token_path` 则为空串，
    /// 此时它的意义只是拿回会话 Cookie——由共享的 cookie_store 接管）。
    async fn fetch_session_token(
        &self,
        session: &SessionSpec,
        req: &TranslateRequest,
    ) -> Result<String> {
        // token 此时尚不存在，用空串构造上下文即可（其余占位符照常可用）。
        let pre_ctx = Ctx::new(req, "");
        let url = substitute(&session.url, &pre_ctx);
        let method = session.method.to_ascii_uppercase();
        let rb = oxidict_net::request_with_headers_for(
            TlsProfile::from_code(&self.spec.tls),
            &method,
            url,
            &[],
        )
        .ok_or_else(|| Error::Service(format!("{}: 会话请求的 HTTP 方法非法", self.spec.id)))?;
        let resp = rb.send().await.map_err(|e| Error::Network(e.to_string()))?;
        let resp = oxidict_net::check(resp).await?;
        if session.token_path.trim().is_empty() {
            return Ok(String::new());
        }
        let text = resp
            .text()
            .await
            .map_err(|e| Error::Network(e.to_string()))?;
        let v: Value = serde_json::from_str(&text)
            .map_err(|e| Error::Service(format!("{}: 会话响应不是 JSON: {e}", self.spec.id)))?;
        extract_text(&v, &session.token_path).ok_or_else(|| {
            Error::Service(format!(
                "{}: 会话响应里找不到 {}",
                self.spec.id, session.token_path
            ))
        })
    }
}

/// 已注册的自定义服务 id。用于「改配置后立即生效」时把上一批摘掉。
static REGISTERED: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// 按当前配置重建自定义服务集合：**先摘掉上一批，再注册新的一批**。
///
/// 先摘是必须的：否则删除或改名一条 spec 后，旧实现会留在注册表里直到重启，
/// 表现成"删了还在"。
pub fn reload(services: &crate::Services, store: &oxidict_core::config::ConfigStore) {
    for id in std::mem::take(&mut *REGISTERED.lock()) {
        services.remove_translator(&id);
    }

    let Some(specs) = store.get::<Vec<CustomServiceSpec>>(CONFIG_KEY) else {
        return;
    };
    let mut registered = Vec::new();
    for (ix, spec) in specs.into_iter().enumerate() {
        let id = spec.id.clone();
        match CustomTranslator::new(spec) {
            Ok(t) => {
                // 与内置服务同名会静默覆盖内置实现，且用户看不出区别——宁可跳过。
                if services.translator(&id).is_some() {
                    log::warn!("custom_services[{ix}]: id `{id}` 与内置服务同名，已跳过");
                    continue;
                }
                log::info!("已注册自定义服务 `{id}`（{}）", t.label());
                services.register_translator(Arc::new(t));
                registered.push(id);
            }
            Err(e) => log::warn!("custom_services[{ix}]（id={id:?}）已跳过: {e}"),
        }
    }
    *REGISTERED.lock() = registered;
}

/// 读取配置里的自定义服务定义。
pub fn specs(store: &oxidict_core::config::ConfigStore) -> Vec<CustomServiceSpec> {
    store
        .get::<Vec<CustomServiceSpec>>(CONFIG_KEY)
        .unwrap_or_default()
}

/// 写回配置并**立即生效**（无需重启）。
///
/// 先校验全部条目再落盘：半途失败会让配置处于"已写入但未生效"的中间态，
/// 比直接拒绝更难排查。
pub fn apply(store: &oxidict_core::config::ConfigStore, specs: &[CustomServiceSpec]) -> Result<()> {
    for (ix, spec) in specs.iter().enumerate() {
        spec.validate()
            .map_err(|e| Error::Service(format!("第 {} 条（id={:?}）: {e}", ix + 1, spec.id)))?;
    }
    let value = serde_json::to_value(specs)?;
    store.set(CONFIG_KEY, &value)?;
    reload(crate::services(), store);
    Ok(())
}

/// 鉴权方式下拉的取值顺序：UI 下标 ↔ 配置取值的映射**只此一处**。
pub const AUTH_KINDS: [&str; 4] = ["none", "bearer", "header", "query"];

/// 下拉下标 → 鉴权取值；越界回退 `none`。
///
/// 回退到"无鉴权"而不是别的：宁可请求少带一个头（服务端会明确报鉴权失败），
/// 也不要把密钥塞进一个用户没选择的字段里。
pub fn auth_kind_at(ix: usize) -> &'static str {
    AUTH_KINDS.get(ix).copied().unwrap_or("none")
}

/// 设置面板表单的值（[`CustomServiceSpec`] 常用子集的一对一映射）。
///
/// 为什么不是全量：`headers` / `fields` / 会话 / 流式配置属进阶用法，塞进表单
/// 会让界面复杂度暴涨；它们仍可在 config.json 里手写。表单覆盖"纯映射服务"
/// 的典型形态：一个地址 + 一个密钥 + 一个取译文的路径。
#[derive(Debug, Clone, Default)]
pub struct FormValues {
    pub id: String,
    pub endpoint: String,
    /// `GET` / `POST`。
    pub method: String,
    /// `none` / `bearer` / `header` / `query`。
    pub auth_kind: String,
    /// `header` / `query` 时的参数名。
    pub auth_name: String,
    /// 请求体模板（JSON 文本，可空）。
    pub body: String,
    pub extract_text: String,
    /// 指纹档位（`native` / `chrome` / `safari`）。
    pub tls: String,
}

impl FormValues {
    /// 表单 → spec；错误信息面向用户（会直接显示在设置面板上）。
    pub fn to_spec(&self) -> std::result::Result<CustomServiceSpec, String> {
        let body = {
            let text = self.body.trim();
            if text.is_empty() {
                None
            } else {
                Some(
                    serde_json::from_str::<Value>(text)
                        .map_err(|e| format!("请求体不是合法 JSON: {e}"))?,
                )
            }
        };

        let auth = match self.auth_kind.trim() {
            "bearer" => AuthSpec::Bearer,
            kind @ ("header" | "query") => {
                let name = self.auth_name.trim();
                if name.is_empty() {
                    return Err("选了自定义头/查询参数鉴权，就必须填参数名".into());
                }
                if kind == "header" {
                    AuthSpec::Header {
                        name: name.to_string(),
                    }
                } else {
                    AuthSpec::Query {
                        name: name.to_string(),
                    }
                }
            }
            // 只有从既有 spec 回填时才可能出现（下拉里没有这一项）。
            // 明确报错而不是悄悄降级成无鉴权——后者会让签名参数消失，
            // 用户只会看到服务端返回鉴权失败，却不知道是编辑器把配置吃了。
            "hash_query" => {
                return Err(
                    "签名类鉴权（hash_query）暂不支持在表单里编辑，请在 config.json 中修改".into(),
                );
            }
            _ => AuthSpec::None,
        };

        // 只有需要密钥的鉴权才附上 apiKey 字段，否则会渲染一个没用的输入框。
        let fields = if matches!(auth, AuthSpec::None) {
            Vec::new()
        } else {
            vec![FieldSpec::Secret {
                key: "apiKey".into(),
                label: "API Key".into(),
            }]
        };

        let spec = CustomServiceSpec {
            id: self.id.trim().to_string(),
            label: String::new(),
            endpoint: self.endpoint.trim().to_string(),
            method: if self.method.trim().is_empty() {
                default_method()
            } else {
                self.method.trim().to_string()
            },
            auth,
            headers: BTreeMap::new(),
            body,
            extract: ExtractSpec {
                text: self.extract_text.trim().to_string(),
                stream_delta: String::new(),
                stream_mode: StreamMode::None,
                html_selector: None,
            },
            fields,
            session: None,
            tls: if self.tls.trim().is_empty() {
                default_tls()
            } else {
                self.tls.trim().to_string()
            },
        };
        spec.validate()?;
        Ok(spec)
    }

    /// spec → 表单，用于编辑既有条目时回填。
    pub fn from_spec(spec: &CustomServiceSpec) -> Self {
        let (auth_kind, auth_name) = match &spec.auth {
            AuthSpec::None => ("none", String::new()),
            AuthSpec::Bearer => ("bearer", String::new()),
            AuthSpec::Header { name } => ("header", name.clone()),
            AuthSpec::Query { name } => ("query", name.clone()),
            // 表单的下拉表达不了签名协议。这里保留 kind 字符串，让 `to_spec`
            // 能识别出「这条不能走表单」，而不是把签名配置静默丢掉。
            AuthSpec::HashQuery { name, .. } => ("hash_query", name.clone()),
        };
        Self {
            id: spec.id.clone(),
            endpoint: spec.endpoint.clone(),
            method: spec.method.clone(),
            auth_kind: auth_kind.to_string(),
            auth_name,
            body: spec
                .body
                .as_ref()
                .map(|b| serde_json::to_string(b).unwrap_or_default())
                .unwrap_or_default(),
            extract_text: spec.extract.text.clone(),
            tls: TlsProfile::from_code(&spec.tls).code().to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// 占位符替换
// ---------------------------------------------------------------------------

/// 占位符求值上下文。
struct Ctx<'a> {
    req: &'a TranslateRequest,
    api_key: &'a str,
    /// 每次求值生成**一次**：签名明文与随请求发出的参数必须引用同一个值，
    /// 各生成一次会让服务端算出的签名与收到的一致不了。
    salt: String,
    /// 同上：Unix 秒。签名明文与 `curtime` 之类的参数必须同值。
    timestamp: String,
    /// 会话前置请求取回的 token（未配置会话则为空串）。
    token: String,
}

impl<'a> Ctx<'a> {
    fn new(req: &'a TranslateRequest, token: &str) -> Self {
        Self {
            req,
            api_key: req.str("apiKey").unwrap_or(""),
            salt: oxidict_net::uuid_v4(),
            timestamp: oxidict_net::unix_seconds().to_string(),
            token: token.to_string(),
        }
    }

    /// 取占位符的 JSON 值：先认内置名，再当配置字段名查。
    fn json(&self, name: &str) -> Option<Value> {
        let req = self.req;
        match name {
            "text" => Some(Value::String(req.text.clone())),
            "from" => Some(Value::String(super::lang_names::english_name(req.from))),
            "to" => Some(Value::String(super::lang_names::english_name(req.to))),
            "fromCode" => Some(Value::String(req.from.code().to_string())),
            "toCode" => Some(Value::String(req.to.code().to_string())),
            "apiKey" => Some(Value::String(self.api_key.to_string())),
            "salt" => Some(Value::String(self.salt.clone())),
            "timestamp" => Some(Value::String(self.timestamp.clone())),
            "token" => Some(Value::String(self.token.clone())),
            other => self.req.config.get(other).cloned(),
        }
    }

    fn string(&self, name: &str) -> Option<String> {
        self.json(name).map(|v| match v {
            Value::String(s) => s,
            other => compact(&other),
        })
    }
}

/// 紧凑序列化：字符串取其本身，其它走 `to_string`（数字/布尔保持可读形式）。
fn compact(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// 把模板里的占位符渲染成实际值（递归处理对象与数组）。
fn render(template: &Value, ctx: &Ctx<'_>) -> Value {
    match template {
        Value::String(s) => {
            // 整串就是一个占位符且字段是数字/布尔时，保留 JSON 类型——
            // 否则 `"temperature": "{{temperature}}"` 会把 0.7 发成字符串。
            if let Some(name) = whole_placeholder(s)
                && let Some(v) = ctx.json(name)
            {
                return v;
            }
            Value::String(substitute(s, ctx))
        }
        Value::Array(items) => Value::Array(items.iter().map(|i| render(i, ctx)).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), render(v, ctx)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// `s` 若形如 `{{ name }}` 且只有这一个占位符，返回其中的名字。
fn whole_placeholder(s: &str) -> Option<&str> {
    let t = s.trim();
    let rest = t.strip_prefix("{{")?;
    let name = rest.strip_suffix("}}")?;
    let name = name.trim();
    // 内部不能再出现 `{{`，否则是「多段拼接」而非单一占位符。
    if name.is_empty() || name.contains("{{") {
        return None;
    }
    Some(name)
}

/// 替换 `s` 中所有 `{{name}}`；未知名字替换为空串（与"配方里没这个字段"一致）。
fn substitute(s: &str, ctx: &Ctx<'_>) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find("}}") {
            Some(end) => {
                let name = after[..end].trim();
                if let Some(v) = ctx.string(name) {
                    out.push_str(&v);
                }
                rest = &after[end + 2..];
            }
            // 没有闭合：原样保留，避免把用户写坏的内容悄悄吞掉。
            None => {
                out.push_str(&rest[start..]);
                return out;
            }
        }
    }
    out.push_str(rest);
    out
}

// ---------------------------------------------------------------------------
// JSON 路径 / HTML 提取
// ---------------------------------------------------------------------------

/// 把 `a.b[0].c` 这样的点/方括号路径转成 RFC 6901 JSON Pointer。
///
/// 复用 `serde_json::Value::pointer` 而不是自己写遍历：它已按规范处理
/// 转义与边界，少一处自造解析器就少一类 bug。
fn path_to_pointer(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.starts_with('/') {
        // 已经是 pointer 形式，原样使用。
        return trimmed.to_string();
    }
    let mut segs: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut chars = trimmed.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '.' => {
                if !cur.is_empty() {
                    segs.push(std::mem::take(&mut cur));
                }
            }
            '[' => {
                if !cur.is_empty() {
                    segs.push(std::mem::take(&mut cur));
                }
                let mut idx = String::new();
                for c2 in chars.by_ref() {
                    if c2 == ']' {
                        break;
                    }
                    idx.push(c2);
                }
                let idx = idx.trim().trim_matches('"').trim_matches('\'').to_string();
                if !idx.is_empty() {
                    segs.push(idx);
                }
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() {
        segs.push(cur);
    }
    let escaped: Vec<String> = segs
        .iter()
        .map(|s| s.replace('~', "~0").replace('/', "~1"))
        .collect();
    format!("/{}", escaped.join("/"))
}

/// 按路径取译文。
///
/// 支持：字符串直接返回；字符串数组用换行拼接（Google 这类分段返回）；
/// 数字/布尔按字面量返回。
fn extract_text(v: &Value, path: &str) -> Option<String> {
    let ptr = path_to_pointer(path);
    let target = v.pointer(&ptr)?;
    match target {
        Value::String(s) => Some(s.clone()),
        Value::Array(items) => {
            let parts: Vec<String> = items
                .iter()
                .filter_map(|i| i.as_str().map(str::to_string))
                .collect();
            if parts.is_empty() {
                None
            } else {
                Some(parts.join("\n"))
            }
        }
        other => Some(compact(other)),
    }
}

/// HTML 模式提取：CSS 选择器选中元素的文本，多段换行拼接。
///
/// 选择器解析失败视为配置错误（返回 None，由调用方给出带选择器的报错）。
fn extract_html(html: &str, selector: &str) -> Option<String> {
    let sel = scraper::Selector::parse(selector).ok()?;
    let doc = scraper::Html::parse_document(html);
    let parts: Vec<String> = doc
        .select(&sel)
        .map(|el| el.text().collect::<String>().trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

fn first_chars(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

// ---------------------------------------------------------------------------
// 签名与重试
// ---------------------------------------------------------------------------

/// 限流重试：最多尝试次数（含首次）。
const MAX_ATTEMPTS: u32 = 3;
/// 没带 `Retry-After` 时的退避秒数，也是连接错误的重试间隔。
const DEFAULT_BACKOFF_SECS: u64 = 2;
/// `Retry-After` 的上限：防止服务端给一个天文数字把客户端挂死。
const MAX_BACKOFF_SECS: u64 = 30;

/// 解析 `Retry-After`（只认秒数形式；HTTP-date 形式忽略，回落到默认退避）。
///
/// 单独抽出来是为了能对 `HeaderValue` 直接做单测。
fn retry_after_secs(raw: Option<&wreq::header::HeaderValue>) -> Option<u64> {
    let raw = raw?.to_str().ok()?;
    raw.trim()
        .parse::<u64>()
        .ok()
        .map(|secs| secs.clamp(1, MAX_BACKOFF_SECS))
}

/// 计算 `hash_query` 鉴权要发的查询参数（签名 + `extra`）。
///
/// 抽成纯函数是为了能测：`translate()` 里那部分要发网络请求，测不到。
fn hash_query_params(
    algo: &str,
    template: &str,
    name: &str,
    key_field: &str,
    extra: &std::collections::BTreeMap<String, String>,
    ctx: &Ctx<'_>,
) -> Vec<(String, String)> {
    let plain = substitute(template, ctx);
    let sign = if algo == "md5" {
        oxidict_net::md5_hex(&plain)
    } else if algo == "hmac_sha256" {
        let key = ctx.string(key_field).unwrap_or_default();
        if key.is_empty() {
            // HMAC 的密钥来自配置字段；缺了就算不出合法签名。这里记录下来，
            // 让"服务端返回鉴权失败"时能立刻定位是密钥没填。
            log::warn!("hmac_sha256 需要配置字段 {key_field:?} 作为密钥，但它是空的");
        }
        oxidict_net::hmac_sha256_hex(key.as_bytes(), &plain)
    } else {
        oxidict_net::sha256_hex(plain.as_bytes())
    };
    let mut params: Vec<(String, String)> = extra
        .iter()
        .map(|(k, v)| (k.clone(), substitute(v, ctx)))
        .collect();
    params.push((name.to_string(), sign));
    params
}

/// 把查询参数追加到 URL（值做百分号编码，键假定为安全字面量）。
fn append_query(url: &mut String, params: &[(String, String)]) {
    let mut sep = if url.contains('?') { '&' } else { '?' };
    for (k, v) in params {
        url.push(sep);
        url.push_str(k);
        url.push('=');
        url.push_str(&urlencoding::encode(v));
        sep = '&';
    }
}

// ---------------------------------------------------------------------------
// 字符串驻留
// ---------------------------------------------------------------------------

/// 把运行时字符串驻留成 `'static`：`ConfigField` 的键/标签要求 `&'static str`，
/// 而自定义服务的这些字符串来自配置文件（运行期才知道）。
///
/// **按值去重**，与 UI 层 `config_window/services.rs` 的 id 驻留同一套路：
/// 无去重的话，重复注册会不断累积永不释放的字符串。
fn intern(s: &str) -> &'static str {
    static INTERNED: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let set = INTERNED.get_or_init(|| Mutex::new(HashSet::new()));
    let mut set = set.lock();
    if let Some(existing) = set.get(s) {
        return existing;
    }
    let leaked: &'static str = Box::leak(s.to_string().into_boxed_str());
    set.insert(leaked);
    leaked
}

/// 下拉选项切片同样需要 `'static`。
///
/// 说明：这里按**每次注册**泄漏一个很小的 `Vec`（选项数量级）——注册在进程启动时
/// 执行一次，量级可忽略。若将来支持热重载配置，需要改为按 spec 缓存。
fn intern_options(options: &[String]) -> &'static [(&'static str, &'static str)] {
    Box::leak(
        options
            .iter()
            .map(|o| (intern(o), intern(o)))
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    )
}

#[cfg(test)]
mod tests {
    // 测试中 unwrap 直观且失败即测试失败，豁免。
    #![allow(clippy::unwrap_used)]

    /// hmac_sha256(key="key1", msg="hello")，由 Python hmac 独立算出。
    const HMAC_KEY1_HELLO: &str =
        "647c219afcd3510fdd46e0cb970c5f40d100f8bbc2d0ca82f4d8b026b711872c";

    use super::*;
    use oxidict_core::ServiceConfig;
    use serde_json::json;

    fn spec_from(v: Value) -> CustomServiceSpec {
        serde_json::from_value(v).unwrap()
    }

    fn ctx_for(text: &str, config: Value) -> TranslateRequest {
        let mut cfg = ServiceConfig::new();
        if let Value::Object(map) = config {
            for (k, v) in map {
                cfg.insert(k, v);
            }
        }
        TranslateRequest::new(text, Language::En, Language::ZhCn).with_config(cfg)
    }

    /// 占位符要能穿透嵌套 JSON，且保持结构。
    #[test]
    fn render_replaces_placeholders_in_nested_json() {
        let req = ctx_for("hello", json!({}));
        let ctx = Ctx::new(&req, "");
        let template = json!({
            "q": "{{text}}",
            "lang": { "from": "{{fromCode}}", "to": "{{toCode}}" },
            "list": ["{{to}}", "fixed"],
        });
        let out = render(&template, &ctx);
        assert_eq!(out["q"], json!("hello"));
        assert_eq!(out["lang"]["from"], json!("en"));
        // 注意：`code()` 是**内部码**（下划线形式，如 zh_cn），不是 BCP47。
        assert_eq!(out["lang"]["to"], json!("zh_cn"));
        assert_eq!(out["list"][0], json!("Simplified Chinese"));
        assert_eq!(out["list"][1], json!("fixed"));
    }

    /// 整串为占位符且配置里是数字时，必须保持数字类型。
    #[test]
    fn render_keeps_json_type_for_whole_placeholder() {
        let mut req = ctx_for("x", json!({}));
        req.config.insert("temperature".into(), json!(0.7));
        req.config.insert("stream".into(), json!(true));
        let ctx = Ctx::new(&req, "");

        let out = render(
            &json!({ "temperature": "{{temperature}}", "stream": "{{stream}}" }),
            &ctx,
        );
        assert_eq!(out["temperature"], json!(0.7), "数字不能被变成字符串");
        assert_eq!(out["stream"], json!(true));

        // 拼接形式仍是字符串。
        let out = render(&json!({ "m": "temp={{temperature}}" }), &ctx);
        assert_eq!(out["m"], json!("temp=0.7"));
    }

    /// 未闭合或未知的占位符不能被静默吞掉。
    #[test]
    fn substitute_handles_unknown_and_unclosed() {
        let req = ctx_for("hi", json!({}));
        let ctx = Ctx::new(&req, "");
        assert_eq!(substitute("a{{nope}}b", &ctx), "ab", "未知字段替换为空");
        assert_eq!(substitute("a{{oops", &ctx), "a{{oops", "未闭合应原样保留");
        assert_eq!(substitute("{{ text }}", &ctx), "hi", "允许占位符内有空格");
    }

    /// 点/方括号路径要转成正确的 JSON Pointer（含转义）。
    #[test]
    fn path_to_pointer_handles_dots_brackets_and_escapes() {
        assert_eq!(
            path_to_pointer("data.translations.0.text"),
            "/data/translations/0/text"
        );
        assert_eq!(
            path_to_pointer("choices[0].message.content"),
            "/choices/0/message/content"
        );
        assert_eq!(path_to_pointer("a[\"k\"].b"), "/a/k/b");
        assert_eq!(
            path_to_pointer("a~b/c"),
            "/a~0b~1c",
            "~ 与 / 需按 RFC 6901 转义"
        );
        assert_eq!(path_to_pointer("/already/pointer"), "/already/pointer");
    }

    /// 取译文：字符串、分段数组、缺字段。
    #[test]
    fn extract_text_handles_shapes() {
        let v = json!({
            "data": { "t": "你好", "parts": ["第一", "第二"] },
            "num": 42
        });
        assert_eq!(extract_text(&v, "data.t").as_deref(), Some("你好"));
        assert_eq!(
            extract_text(&v, "data.parts").as_deref(),
            Some("第一\n第二")
        );
        assert_eq!(extract_text(&v, "num").as_deref(), Some("42"));
        assert_eq!(extract_text(&v, "data.missing"), None);
    }

    /// 坏 spec 必须被**明确拒绝**，且错误信息能指到问题所在。
    #[test]
    fn validate_rejects_bad_specs() {
        let base = json!({
            "id": "ok",
            "endpoint": "https://example.com/t",
            "extract": { "text": "t" }
        });
        assert!(spec_from(base.clone()).validate().is_ok());

        // id 含 @ —— 会被当成实例分隔符，必须拦住。
        let mut bad = base.clone();
        bad["id"] = json!("a@b");
        assert!(spec_from(bad).validate().unwrap_err().contains('@'));

        let mut bad = base.clone();
        bad["id"] = json!("  ");
        assert!(spec_from(bad).validate().is_err());

        let mut bad = base.clone();
        bad["endpoint"] = json!("ftp://example.com");
        assert!(spec_from(bad).validate().unwrap_err().contains("http"));

        let mut bad = base.clone();
        bad["method"] = json!("NOT A METHOD");
        assert!(spec_from(bad).validate().unwrap_err().contains("HTTP 方法"));

        let mut bad = base.clone();
        bad["extract"] = json!({ "text": "  " });
        assert!(spec_from(bad).validate().is_err());

        // SSE 模式必须给 delta 路径，否则流式解析无从下手。
        let mut bad = base.clone();
        bad["extract"] = json!({ "text": "t", "stream_mode": "sse_json" });
        assert!(
            spec_from(bad)
                .validate()
                .unwrap_err()
                .contains("stream_delta")
        );

        // 字段 key 重复会让后者覆盖前者。
        let mut bad = base.clone();
        bad["fields"] = json!([
            { "kind": "text", "key": "a", "label": "A" },
            { "kind": "text", "key": "a", "label": "A2" }
        ]);
        assert!(spec_from(bad).validate().unwrap_err().contains("重复"));
    }

    /// 表单字段要正确映射成 `ConfigField`（UI 靠它渲染）。
    #[test]
    fn field_specs_map_to_config_fields() {
        let spec = spec_from(json!({
            "id": "c",
            "endpoint": "https://e.com",
            "extract": { "text": "t" },
            "fields": [
                { "kind": "secret", "key": "apiKey", "label": "API Key" },
                { "kind": "select", "key": "model", "label": "模型",
                  "options": ["a", "b"], "default": "b" },
                { "kind": "switch", "key": "stream", "label": "流式", "default": true }
            ]
        }));
        let t = CustomTranslator::new(spec).unwrap();
        let fields = t.config_schema();
        assert_eq!(fields.len(), 3);
        match &fields[0] {
            ConfigField::Text { key, secret, .. } => {
                assert_eq!(*key, "apiKey");
                assert!(*secret, "secret 字段必须是密码框");
            }
            other => panic!("期望 Text，实得 {other:?}"),
        }
        match &fields[1] {
            ConfigField::Select {
                options, default, ..
            } => {
                assert_eq!(options.len(), 2);
                assert_eq!(*default, "b");
            }
            other => panic!("期望 Select，实得 {other:?}"),
        }
        assert_eq!(t.label(), "c", "未写 label 时退回 id");
    }

    /// 鉴权变换器：bearer / 自定义头 / 查询参数。
    #[test]
    fn auth_specs_deserialize_and_validate() {
        let base = json!({
            "id": "c", "endpoint": "https://e.com", "extract": { "text": "t" }
        });
        for auth in [
            json!({ "kind": "none" }),
            json!({ "kind": "bearer" }),
            json!({ "kind": "header", "name": "X-Api-Key" }),
            json!({ "kind": "query", "name": "key" }),
        ] {
            let mut s = base.clone();
            s["auth"] = auth.clone();
            let spec = spec_from(s);
            assert!(spec.validate().is_ok(), "{auth} 应合法");
        }
        // 缺省 auth 视为 none。
        assert!(matches!(spec_from(base).auth, AuthSpec::None));
    }

    /// 表单 ↔ spec 往返：编辑既有条目时不能丢字段。
    #[test]
    fn form_round_trips_through_spec() {
        let form = FormValues {
            id: "my_mt".into(),
            endpoint: "https://api.example.com/t".into(),
            method: "POST".into(),
            auth_kind: "header".into(),
            auth_name: "X-Api-Key".into(),
            body: r#"{"q":"{{text}}"}"#.into(),
            extract_text: "data.0.text".into(),
            tls: "chrome".into(),
        };
        let spec = form.to_spec().unwrap();
        assert!(matches!(spec.auth, AuthSpec::Header { ref name } if name == "X-Api-Key"));
        assert_eq!(spec.body.as_ref().unwrap()["q"], json!("{{text}}"));
        assert_eq!(spec.tls, "chrome");
        assert_eq!(spec.fields.len(), 1, "需要密钥的鉴权应自动带上 apiKey 字段");

        let back = FormValues::from_spec(&spec);
        assert_eq!(back.id, "my_mt");
        assert_eq!(back.method, "POST");
        assert_eq!(back.auth_kind, "header");
        assert_eq!(back.auth_name, "X-Api-Key");
        assert_eq!(back.extract_text, "data.0.text");
        assert_eq!(back.tls, "chrome");
        assert_eq!(back.body, r#"{"q":"{{text}}"}"#);
    }

    /// 表单错误必须面向用户可读。
    #[test]
    fn form_rejects_bad_input() {
        let base = FormValues {
            id: "x".into(),
            endpoint: "https://e.com".into(),
            extract_text: "t".into(),
            ..Default::default()
        };
        let bad = FormValues {
            body: "{not json".into(),
            ..base.clone()
        };
        assert!(bad.to_spec().unwrap_err().contains("JSON"));

        let bad = FormValues {
            auth_kind: "header".into(),
            ..base.clone()
        };
        assert!(bad.to_spec().unwrap_err().contains("参数名"));

        // 缺 endpoint 由 validate 挡住。
        let bad = FormValues {
            endpoint: String::new(),
            ..base.clone()
        };
        assert!(bad.to_spec().is_err());

        // 无鉴权时不该生成没用的 apiKey 输入框。
        let ok = base.to_spec().unwrap();
        assert!(matches!(ok.auth, AuthSpec::None));
        assert!(ok.fields.is_empty());
        // 空 method 落到默认 POST。
        assert_eq!(ok.method, "POST");
    }

    /// 驻留必须**按值去重**：同一个字符串两次驻留要拿到同一个引用，
    /// 否则重复注册会不断累积永不释放的内存。
    #[test]
    fn intern_deduplicates_by_value() {
        let a = intern("oxidict-intern-probe");
        let b = intern("oxidict-intern-probe");
        assert!(std::ptr::eq(a, b), "同值应返回同一引用");
        assert_eq!(a, "oxidict-intern-probe");
    }

    /// 下拉下标映射：越界必须回退 `none`，且与 `FormValues` 的取值集合一致。
    #[test]
    fn auth_kind_index_maps_safely() {
        assert_eq!(auth_kind_at(0), "none");
        assert_eq!(auth_kind_at(1), "bearer");
        assert_eq!(auth_kind_at(2), "header");
        assert_eq!(auth_kind_at(3), "query");
        assert_eq!(auth_kind_at(4), "none", "越界应回退 none");
        assert_eq!(auth_kind_at(usize::MAX), "none");
    }

    /// 签名：明文拼接 + 哈希必须与**独立实现**算出的向量一致；
    /// `extra` 的值也要过占位符。
    ///
    /// 向量由 Python `hashlib` 独立算出，用来交叉验证 Rust 侧实现。
    #[test]
    fn hash_query_matches_independent_vectors() {
        let req = ctx_for("hello", json!({ "appid": "app1", "apiKey": "key1" }));
        let ctx = Ctx::new(&req, "");

        let params = hash_query_params(
            "md5",
            "{{appid}}{{text}}",
            "sign",
            "apiKey",
            &BTreeMap::new(),
            &ctx,
        );
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].0, "sign");
        assert_eq!(params[0].1, "241a0782aad90650e679246aad91426e");

        let params = hash_query_params(
            "sha256",
            "{{text}}",
            "sign",
            "apiKey",
            &BTreeMap::new(),
            &ctx,
        );
        assert_eq!(
            params[0].1,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );

        // extra 的键按 BTreeMap 有序，签名追加在最后。
        let mut extra = BTreeMap::new();
        extra.insert("q".to_string(), "{{text}}".to_string());
        extra.insert("appid".to_string(), "{{appid}}".to_string());
        let params = hash_query_params("md5", "{{text}}", "sign", "apiKey", &extra, &ctx);
        assert_eq!(params.len(), 3);
        assert_eq!(params[0], ("appid".to_string(), "app1".to_string()));
        assert_eq!(params[1], ("q".to_string(), "hello".to_string()));
        assert_eq!(params[2].0, "sign");
    }

    /// `{{salt}}` / `{{timestamp}}` 在同一次求值里只能有**一个**值：
    /// 明文里用一次、`extra` 里再用一次时，服务端才能算出同样的签名。
    #[test]
    fn salt_and_timestamp_are_stable_within_one_evaluation() {
        let req = ctx_for("hi", json!({}));
        let ctx = Ctx::new(&req, "");

        let mut extra = BTreeMap::new();
        extra.insert("nonce".to_string(), "{{salt}}".to_string());
        extra.insert("curtime".to_string(), "{{timestamp}}".to_string());
        let params = hash_query_params(
            "md5",
            "{{salt}}{{timestamp}}",
            "sign",
            "apiKey",
            &extra,
            &ctx,
        );

        // 按键查找而不是按下标：`extra` 是 BTreeMap，顺序由 key 决定
        // （`curtime` 排在 `nonce` 前面），按下标取会取错。
        let by_key = |key: &str| {
            params
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        };
        let nonce = by_key("nonce");
        let curtime = by_key("curtime");
        assert!(!nonce.is_empty());
        assert!(
            curtime.parse::<i64>().is_ok(),
            "时间戳应为整数秒: {curtime}"
        );
        // 反证：签名 == md5(salt + timestamp)，说明明文与 extra 用的是同一份。
        assert_eq!(
            params[2].1,
            oxidict_net::md5_hex(&format!("{nonce}{curtime}"))
        );

        // 不同请求必须换 salt，否则签名的随机性形同虚设。
        let other = Ctx::new(&req, "");
        let salt_b = &hash_query_params(
            "md5",
            "{{salt}}",
            "sign",
            "apiKey",
            &BTreeMap::new(),
            &other,
        )[0]
        .1;
        let salt_a =
            &hash_query_params("md5", "{{salt}}", "sign", "apiKey", &BTreeMap::new(), &ctx)[0].1;
        assert_ne!(salt_a, salt_b, "不同请求不应复用同一个 salt");
    }

    /// `hash_query` 的非法配置要被明确拒绝。
    #[test]
    fn hash_query_validation() {
        let base = json!({
            "id": "s", "endpoint": "https://e.com", "extract": { "text": "t" },
        });
        let check = |auth: Value| {
            let mut s = base.clone();
            s["auth"] = auth;
            spec_from(s).validate()
        };
        assert!(check(json!({"kind":"hash_query","algo":"md5","template":"{{text}}"})).is_ok());
        assert!(check(json!({"kind":"hash_query","algo":"sha256","template":"x"})).is_ok());
        assert!(
            check(json!({"kind":"hash_query","algo":"sha1","template":"x"}))
                .unwrap_err()
                .contains("algo")
        );
        assert!(
            check(json!({"kind":"hash_query","algo":"md5","template":"  "}))
                .unwrap_err()
                .contains("template")
        );
        assert!(
            check(json!({"kind":"hash_query","algo":"md5","template":"x","name":" "}))
                .unwrap_err()
                .contains("name")
        );
    }

    /// HMAC：密钥取自 `key_field` 指向的配置字段，向量独立算出。
    #[test]
    fn hash_query_hmac_uses_config_field_as_key() {
        let req = ctx_for("hello", json!({ "apiKey": "key1" }));
        let ctx = Ctx::new(&req, "");

        // hmac_sha256(key="key1", msg="hello")
        let params = hash_query_params(
            "hmac_sha256",
            "{{text}}",
            "sign",
            "apiKey",
            &BTreeMap::new(),
            &ctx,
        );
        assert_eq!(params[0].1, HMAC_KEY1_HELLO);
    }

    /// HTML 模式：选择器取文本、多段拼接；无匹配与非法选择器都返回 None
    /// （后者是配置错误，不能 panic）。
    #[test]
    fn extract_html_selects_text() {
        let html = r#"<html><body>
            <div class="def">definition one</div>
            <div class="def">definition two</div>
            <span>noise</span>
        </body></html>"#;
        assert_eq!(
            extract_html(html, "div.def").as_deref(),
            Some("definition one\ndefinition two")
        );
        assert_eq!(extract_html(html, "div.missing"), None);
        assert_eq!(extract_html(html, "divdef["), None);
    }

    /// 会话取回的 token 必须能进入主请求的占位符。
    #[test]
    fn session_token_is_available_as_placeholder() {
        let req = ctx_for("hi", json!({}));
        let ctx = Ctx::new(&req, "tok-123");
        let out = render(&json!({ "t": "{{token}}" }), &ctx);
        assert_eq!(out["t"], json!("tok-123"));
    }

    /// 会话地址同样必须是 http(s)。
    #[test]
    fn session_url_is_validated() {
        let base = json!({
            "id": "s", "endpoint": "https://e.com", "extract": { "text": "t" },
        });
        let mut bad = base.clone();
        bad["session"] = json!({ "url": "ftp://e.com" });
        assert!(
            spec_from(bad)
                .validate()
                .unwrap_err()
                .contains("session.url")
        );
        let mut ok = base.clone();
        ok["session"] = json!({ "url": "https://e.com/session", "token_path": "t" });
        assert!(spec_from(ok).validate().is_ok());
    }
}
