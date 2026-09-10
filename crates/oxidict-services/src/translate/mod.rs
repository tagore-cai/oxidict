//! translate 服务实现。

pub mod alibaba;
pub mod baidu;
pub mod baidu_field;
pub mod bing;
pub mod bing_dict;
pub mod caiyun;
pub mod cambridge_dict;
pub mod chatglm;
pub mod chatglm_cloud;
pub mod claude_cloud;
pub mod deepl;
pub mod deepl_cloud;
pub mod deepseek_cloud;
pub mod ecdict;
pub mod gemini_cloud;
pub mod geminipro;
pub mod google;
pub mod lingva;
pub mod minimax_cloud;
pub mod moonshot_cloud;
pub mod niutrans;
pub mod ollama;
pub mod openai;
pub mod openai_cloud;
/// OpenAI 协议共享实现层（消息构造、URL 规范化、请求体组装），被
/// openai / ollama / geminipro 复用。它不是可注册的服务——不在
/// `register()` 里注册，也不应作为服务 id 出现在 config.json 中，
/// 故收紧为 crate 内可见。
pub(crate) mod openai_compatible;
pub mod tencent;
pub mod tongyi_cloud;
pub mod transmart;
pub mod volcengine;
pub mod yandex;
pub mod youdao;

/// 注册内置 translate 服务。
pub fn register(services: &crate::Services) {
    services.register_translator(std::sync::Arc::new(alibaba::Alibaba));
    services.register_translator(std::sync::Arc::new(baidu::Baidu));
    services.register_translator(std::sync::Arc::new(baidu_field::BaiduField));
    services.register_translator(std::sync::Arc::new(bing::Bing));
    services.register_translator(std::sync::Arc::new(bing_dict::BingDict));
    services.register_translator(std::sync::Arc::new(caiyun::Caiyun));
    services.register_translator(std::sync::Arc::new(cambridge_dict::CambridgeDict));
    services.register_translator(std::sync::Arc::new(chatglm::Chatglm));
    services.register_translator(std::sync::Arc::new(chatglm_cloud::ChatglmCloud));
    services.register_translator(std::sync::Arc::new(claude_cloud::ClaudeCloud));
    services.register_translator(std::sync::Arc::new(deepl::DeepL));
    services.register_translator(std::sync::Arc::new(deepl_cloud::DeeplCloud));
    services.register_translator(std::sync::Arc::new(deepseek_cloud::DeepseekCloud));
    services.register_translator(std::sync::Arc::new(ecdict::Ecdict));
    services.register_translator(std::sync::Arc::new(gemini_cloud::GeminiCloud));
    services.register_translator(std::sync::Arc::new(geminipro::Geminipro));
    services.register_translator(std::sync::Arc::new(google::Google));
    services.register_translator(std::sync::Arc::new(lingva::Lingva));
    services.register_translator(std::sync::Arc::new(minimax_cloud::MinimaxCloud));
    services.register_translator(std::sync::Arc::new(moonshot_cloud::MoonshotCloud));
    services.register_translator(std::sync::Arc::new(niutrans::Niutrans));
    services.register_translator(std::sync::Arc::new(ollama::Ollama));
    services.register_translator(std::sync::Arc::new(openai::Openai));
    services.register_translator(std::sync::Arc::new(openai_cloud::OpenaiCloud));
    services.register_translator(std::sync::Arc::new(tencent::Tencent));
    services.register_translator(std::sync::Arc::new(tongyi_cloud::TongyiCloud));
    services.register_translator(std::sync::Arc::new(transmart::Transmart));
    services.register_translator(std::sync::Arc::new(volcengine::Volcengine));
    services.register_translator(std::sync::Arc::new(yandex::Yandex));
    services.register_translator(std::sync::Arc::new(youdao::Youdao));
}
