//! collection 服务实现。

pub mod anki;
pub mod eudic;

/// 注册内置 collection 服务。
pub fn register(services: &crate::Services) {
    services.register_collector(std::sync::Arc::new(anki::Anki));
    services.register_collector(std::sync::Arc::new(eudic::Eudic));
}
