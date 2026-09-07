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
