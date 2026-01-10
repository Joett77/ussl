//! Protocol error types

use thiserror::Error;

/// Protocol-specific errors
#[derive(Error, Debug)]
pub enum ProtocolError {
    #[error("Invalid command: {0}")]
    InvalidCommand(String),

    #[error("Missing argument: {0}")]
    MissingArgument(String),

    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Incomplete message")]
    Incomplete,

    #[error("Message too large: {size} > {max}")]
    MessageTooLarge { size: usize, max: usize },

    #[error("Invalid JSON: {0}")]
    InvalidJson(String),

    #[error("Core error: {0}")]
    Core(#[from] ussl_core::Error),
}

/// Result type for protocol operations
pub type ProtocolResult<T> = Result<T, ProtocolError>;
