//! tts 服务实现。

pub mod baidu;
pub mod google;
pub mod lingva;

/// 注册内置 tts 服务。
pub fn register(services: &crate::Services) {
    services.register_tts(std::sync::Arc::new(baidu::Baidu));
    services.register_tts(std::sync::Arc::new(google::Google));
    services.register_tts(std::sync::Arc::new(lingva::Lingva));
}
