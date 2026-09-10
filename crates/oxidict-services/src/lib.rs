//! oxidict-services：四大类服务的 trait 与注册表。
//!
//! 原实现中服务是一组 JS 模块，靠 `index.jsx` 的命名空间聚合；
//! 这里改为 trait + 注册表，内置服务与 gpui-shell 插件注册到同一张表里，
//! 调用方（UI、本地 HTTP 服务、外部调用）不区分二者。

pub mod collection;
pub mod recognize;
pub mod translate;
pub mod tts;

use async_trait::async_trait;
use once_cell::sync::OnceCell;
use oxidict_core::config::ConfigStore;
use oxidict_core::schema::ConfigField;
use oxidict_core::{
    CollectionRequest, Error, Language, RecognizeRequest, Result, ServiceConfig, TranslateRequest,
    TranslateResult, TtsRequest,
};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// 实例名在配置对象里的键，与原 `INSTANCE_NAME_CONFIG_KEY` 对齐。
pub const INSTANCE_NAME_KEY: &str = "instance_name";

// ---------------------------------------------------------------------------
// Traits
// ---------------------------------------------------------------------------

/// 翻译服务。
#[async_trait]
pub trait Translator: Send + Sync {
    fn id(&self) -> &str;

    fn name(&self) -> &str {
        self.id()
    }

    /// 设置面板需要渲染的字段。空表示免配置即可使用。
    fn config_schema(&self) -> Vec<ConfigField> {
        vec![]
    }

    /// 内部语言码 -> 厂商语言码。默认直接透传内部码。
    fn map_language(&self, lang: Language) -> String {
        lang.code().to_string()
    }

    async fn translate(&self, req: TranslateRequest) -> Result<TranslateResult>;
}

/// 文字识别（OCR）服务。返回识别出的文本。
#[async_trait]
pub trait Recognizer: Send + Sync {
    fn id(&self) -> &str;

    fn name(&self) -> &str {
        self.id()
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        vec![]
    }

    fn map_language(&self, lang: Language) -> String {
        lang.code().to_string()
    }

    async fn recognize(&self, req: RecognizeRequest) -> Result<String>;
}

/// 语音合成服务。返回音频字节（mp3）。
#[async_trait]
pub trait Tts: Send + Sync {
    fn id(&self) -> &str;

    fn name(&self) -> &str {
        self.id()
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        vec![]
    }

    fn map_language(&self, lang: Language) -> String {
        lang.code().to_string()
    }

    async fn tts(&self, req: TtsRequest) -> Result<Vec<u8>>;
}

/// 生词本服务。
#[async_trait]
pub trait Collector: Send + Sync {
    fn id(&self) -> &str;

    fn name(&self) -> &str {
        self.id()
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        vec![]
    }

    async fn collect(&self, req: CollectionRequest) -> Result<()>;
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

/// 全局服务注册表。内置服务在启动时注册，插件在安装后动态注册。
pub struct Services {
    translators: RwLock<HashMap<String, Arc<dyn Translator>>>,
    recognizers: RwLock<HashMap<String, Arc<dyn Recognizer>>>,
    tts: RwLock<HashMap<String, Arc<dyn Tts>>>,
    collectors: RwLock<HashMap<String, Arc<dyn Collector>>>,
}

impl Services {
    fn new() -> Self {
        Self {
            translators: RwLock::new(HashMap::new()),
            recognizers: RwLock::new(HashMap::new()),
            tts: RwLock::new(HashMap::new()),
            collectors: RwLock::new(HashMap::new()),
        }
    }

    pub fn register_translator(&self, t: Arc<dyn Translator>) {
        self.translators.write().insert(t.id().to_string(), t);
    }

    pub fn register_recognizer(&self, r: Arc<dyn Recognizer>) {
        self.recognizers.write().insert(r.id().to_string(), r);
    }

    pub fn register_tts(&self, t: Arc<dyn Tts>) {
        self.tts.write().insert(t.id().to_string(), t);
    }

    pub fn register_collector(&self, c: Arc<dyn Collector>) {
        self.collectors.write().insert(c.id().to_string(), c);
    }

    pub fn translator(&self, id: &str) -> Option<Arc<dyn Translator>> {
        self.translators.read().get(id).cloned()
    }

    pub fn recognizer(&self, id: &str) -> Option<Arc<dyn Recognizer>> {
        self.recognizers.read().get(id).cloned()
    }

    pub fn tts(&self, id: &str) -> Option<Arc<dyn Tts>> {
        self.tts.read().get(id).cloned()
    }

    pub fn collector(&self, id: &str) -> Option<Arc<dyn Collector>> {
        self.collectors.read().get(id).cloned()
    }

    pub fn translators(&self) -> Vec<Arc<dyn Translator>> {
        self.translators.read().values().cloned().collect()
    }

    pub fn recognizers(&self) -> Vec<Arc<dyn Recognizer>> {
        self.recognizers.read().values().cloned().collect()
    }

    pub fn tts_list(&self) -> Vec<Arc<dyn Tts>> {
        self.tts.read().values().cloned().collect()
    }

    pub fn collectors(&self) -> Vec<Arc<dyn Collector>> {
        self.collectors.read().values().cloned().collect()
    }

    /// 实例 key（`alibaba@r4nd0m`）-> 服务实现 + 该实例的配置。
    pub fn resolve_translator(
        &self,
        instance: &str,
        store: &ConfigStore,
    ) -> Result<(Arc<dyn Translator>, ServiceConfig)> {
        let name = oxidict_core::config::parse_instance(instance).0;
        let svc = self
            .translator(name)
            .ok_or_else(|| Error::Service(format!("未注册的翻译服务: {}", name)))?;
        Ok((svc, store.instance_config(instance)))
    }

    /// 与 [`Services::resolve_translator`] 同构，只是换成识别服务。
    pub fn resolve_recognizer(
        &self,
        instance: &str,
        store: &ConfigStore,
    ) -> Result<(Arc<dyn Recognizer>, ServiceConfig)> {
        let name = oxidict_core::config::parse_instance(instance).0;
        let svc = self
            .recognizer(name)
            .ok_or_else(|| Error::Service(format!("未注册的识别服务: {}", name)))?;
        Ok((svc, store.instance_config(instance)))
    }

    /// 与 [`Services::resolve_translator`] 同构，只是换成语音合成服务。
    pub fn resolve_tts(
        &self,
        instance: &str,
        store: &ConfigStore,
    ) -> Result<(Arc<dyn Tts>, ServiceConfig)> {
        let name = oxidict_core::config::parse_instance(instance).0;
        let svc = self
            .tts(name)
            .ok_or_else(|| Error::Service(format!("未注册的 TTS 服务: {}", name)))?;
        Ok((svc, store.instance_config(instance)))
    }
}

static SERVICES: OnceCell<Services> = OnceCell::new();

pub fn services() -> &'static Services {
    SERVICES.get_or_init(Services::new)
}

/// 注册全部内置服务。
pub fn init_builtin_services() {
    translate::register(services());
    recognize::register(services());
    tts::register(services());
    collection::register(services());
}

/// 一个实例在设置面板里的展示名：优先读 `instance_name`，否则用服务 id。
pub fn instance_display_name(instance: &str, store: &ConfigStore) -> String {
    let cfg = store.instance_config(instance);
    cfg.get(INSTANCE_NAME_KEY)
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            let (name, _) = oxidict_core::config::parse_instance(instance);
            name.to_string()
        })
}

// ---------------------------------------------------------------------------
// 面向 UI 的异步入口
// ---------------------------------------------------------------------------

/// 按实例 key 执行翻译。UI 层通过 [`spawn_translate`] 调度到 tokio 线程池。
pub async fn translate_instance(
    instance: &str,
    req: oxidict_core::TranslateRequest,
) -> oxidict_core::Result<oxidict_core::TranslateResult> {
    let store = oxidict_core::config::config();
    let (svc, cfg) = services().resolve_translator(instance, &store)?;
    svc.translate(req.with_config(cfg)).await
}

/// 按实例 key 执行识别。与 [`translate_instance`] 同构。
pub async fn recognize_instance(
    instance: &str,
    req: oxidict_core::RecognizeRequest,
) -> oxidict_core::Result<String> {
    let store = oxidict_core::config::config();
    let (svc, cfg) = services().resolve_recognizer(instance, &store)?;
    svc.recognize(oxidict_core::RecognizeRequest { config: cfg, ..req })
        .await
}

/// 按实例 key 执行语音合成。与 [`translate_instance`] 同构。
pub async fn tts_instance(
    instance: &str,
    req: oxidict_core::TtsRequest,
) -> oxidict_core::Result<Vec<u8>> {
    let store = oxidict_core::config::config();
    let (svc, cfg) = services().resolve_tts(instance, &store)?;
    svc.tts(oxidict_core::TtsRequest { config: cfg, ..req })
        .await
}

/// 取「翻译服务启用列表」的首个实例并执行翻译。
///
/// CLI（`--translate`）、本地 HTTP 服务、托盘三条入口过去各自实现了一遍
/// 「读列表 -> 取首个 -> resolve -> translate」，任何一处改动都要同步三处。
/// 这里收敛为唯一实现，返回 `(实例 key, 结果)` 便于调用方写历史库。
///
/// 列表为空时返回 [`Error::Service`]，调用方据此提示用户先启用一个服务。
pub async fn translate_first_enabled(
    store: &ConfigStore,
    req: oxidict_core::TranslateRequest,
) -> oxidict_core::Result<(String, oxidict_core::TranslateResult)> {
    let list = store.service_list(oxidict_core::config::keys::TRANSLATE_SERVICE_LIST);
    let instance = list
        .first()
        .ok_or_else(|| Error::Service("翻译服务列表为空".into()))?
        .clone();
    let (svc, cfg) = services().resolve_translator(&instance, store)?;
    let result = svc.translate(req.with_config(cfg)).await?;
    Ok((instance, result))
}

/// [`translate_first_enabled`] 的阻塞版。
///
/// 供 CLI 与 tiny_http 工作线程这类**非 async 上下文**使用。
///
/// # Panics
///
/// 调用线程若在全局 tokio runtime 上下文内（runtime 工作线程或
/// `block_on` 内），`block_on` 会 panic。三条调用点都在独立
/// `std::thread` 上，不受影响。
pub fn translate_first_enabled_blocking(
    req: oxidict_core::TranslateRequest,
) -> oxidict_core::Result<(String, oxidict_core::TranslateResult)> {
    let store = oxidict_core::config::config();
    oxidict_core::runtime::handle().block_on(translate_first_enabled(&store, req))
}

/// 把翻译任务调度到全局 tokio runtime。
///
/// 返回的 JoinHandle 可以在任意执行器（包括 GPUI 的）上 await，
/// 这是 GPUI 与 tokio 服务层之间的标准桥。
pub fn spawn_translate(
    instance: &str,
    req: oxidict_core::TranslateRequest,
) -> tokio::task::JoinHandle<oxidict_core::Result<oxidict_core::TranslateResult>> {
    let instance = instance.to_string();
    oxidict_core::runtime::spawn(async move { translate_instance(&instance, req).await })
}

/// 把识别任务调度到全局 tokio runtime。
pub fn spawn_recognize(
    instance: &str,
    req: oxidict_core::RecognizeRequest,
) -> tokio::task::JoinHandle<oxidict_core::Result<String>> {
    let instance = instance.to_string();
    oxidict_core::runtime::spawn(async move { recognize_instance(&instance, req).await })
}

/// 把语音合成任务调度到全局 tokio runtime。
pub fn spawn_tts(
    instance: &str,
    req: oxidict_core::TtsRequest,
) -> tokio::task::JoinHandle<oxidict_core::Result<Vec<u8>>> {
    let instance = instance.to_string();
    oxidict_core::runtime::spawn(async move { tts_instance(&instance, req).await })
}
