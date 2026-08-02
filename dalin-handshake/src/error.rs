//! AHP 错误类型

use thiserror::Error;

/// AHP protocol error
#[derive(Error, Debug, Clone)]
pub enum HandshakeError {
    #[error("Transport error: {0}")]
    Transport(String),

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Session error: {0}")]
    Session(String),

    #[error("Authentication failed: {0}")]
    Auth(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Already exists: {0}")]
    AlreadyExists(String),

    #[error("IO error: {0}")]
    Io(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl From<std::io::Error> for HandshakeError {
    fn from(e: std::io::Error) -> Self {
        HandshakeError::Io(e.to_string())
    }
}

impl From<serde_json::Error> for HandshakeError {
    fn from(e: serde_json::Error) -> Self {
        HandshakeError::Serialization(e.to_string())
    }
}

/// AHP result type
pub type Result<T> = std::result::Result<T, HandshakeError>;
