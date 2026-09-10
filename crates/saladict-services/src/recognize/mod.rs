//! recognize 服务实现。

pub mod baidu;
pub mod baidu_accurate;
pub mod baidu_img;
pub mod iflytek;
pub mod iflytek_intsig;
pub mod iflytek_latex;
pub mod qrcode;
pub mod simple_latex;
pub mod system;
pub mod tencent;
pub mod tencent_accurate;
pub mod tencent_img;
pub mod tesseract;
pub mod volcengine;
pub mod volcengine_multi_lang;

/// 注册内置 recognize 服务。
pub fn register(services: &crate::Services) {
    services.register_recognizer(std::sync::Arc::new(baidu::Baidu));
    services.register_recognizer(std::sync::Arc::new(baidu_accurate::BaiduAccurate));
    services.register_recognizer(std::sync::Arc::new(baidu_img::BaiduImg));
    services.register_recognizer(std::sync::Arc::new(iflytek::Iflytek));
    services.register_recognizer(std::sync::Arc::new(iflytek_intsig::IflytekIntsig));
    services.register_recognizer(std::sync::Arc::new(iflytek_latex::IflytekLatex));
    services.register_recognizer(std::sync::Arc::new(qrcode::Qrcode));
    services.register_recognizer(std::sync::Arc::new(simple_latex::SimpleLatex));
    services.register_recognizer(std::sync::Arc::new(system::System));
    services.register_recognizer(std::sync::Arc::new(tencent::Tencent));
    services.register_recognizer(std::sync::Arc::new(tencent_accurate::TencentAccurate));
    services.register_recognizer(std::sync::Arc::new(tencent_img::TencentImg));
    services.register_recognizer(std::sync::Arc::new(tesseract::Tesseract));
    services.register_recognizer(std::sync::Arc::new(volcengine::Volcengine));
    services.register_recognizer(std::sync::Arc::new(
        volcengine_multi_lang::VolcengineMultiLang,
    ));
}
