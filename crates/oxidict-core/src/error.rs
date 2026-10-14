//! 统一错误类型。
//!
//! 全项目只有这一个错误枚举：服务层、平台层、插件层的错误都归并到这里，
//! 因此 `?` 可以跨层传递而不需要每层各写一个 From 转换。
//!
//! 分层约定：
//! - 库代码返回**有语义、可匹配**的变体（`Http` / `MissingConfig` / `Service` …），
//!   便于调用方按类型决定「提示用户」「重试」还是「上报」；
//! - 应用层（`app`）在边界补操作上下文与日志，并决定退出码；
//! - [`Error::is_retryable`] 统一回答「值不值得自动重试」，避免各处自己判断状态码。
//!
//! 依赖方向上有一点刻意：`wreq::Error` 的转换**不在本模块**，而在 `oxidict-net`
//! 的 `NetErr` trait。因为 core 不依赖 wreq，加一个 `From<wreq::Error>` 会把
//! HTTP 客户端拖进内核。

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("HTTP request error: status {status}, body: {body}")]
    Http { status: u16, body: String },

    #[error("Network error: {0}")]
    Network(String),

    #[error("Service error: {0}")]
    Service(String),

    #[error("Missing configuration: {0}")]
    MissingConfig(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Unsupported language: {0}")]
    UnsupportedLanguage(String),

    #[error("Platform capability error: {0}")]
    Platform(String),

    #[error("Plugin error: {0}")]
    Plugin(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("File watch error: {0}")]
    Watch(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Error::Database(e.to_string())
    }
}

impl Error {
    /// 判定是否值得自动重试（网络抖动、5xx）。
    pub fn is_retryable(&self) -> bool {
        match self {
            Error::Network(_) => true,
            Error::Http { status, .. } => *status >= 500,
            _ => false,
        }
    }
}
