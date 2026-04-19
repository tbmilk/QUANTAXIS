use thiserror::Error;

/// 市场数据网关错误类型
#[derive(Error, Debug)]
pub enum GatewayError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Market data conversion error: {0}")]
    ConversionError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("WebSocket error: {0}")]
    WebSocketError(String),

    #[error("Invalid instrument: {0}")]
    InvalidInstrument(String),

    #[error("Authentication error: {0}")]
    AuthError(String),

    #[error("Other error: {0}")]
    Other(String),
}

pub type GatewayResult<T> = Result<T, GatewayError>;
