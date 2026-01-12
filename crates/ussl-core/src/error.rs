//! Error types for USSL Core

use thiserror::Error;

/// Core error types
#[derive(Error, Debug)]
pub enum Error {
    #[error("Document not found: {0}")]
    DocumentNotFound(String),

    #[error("Invalid document ID: {0}")]
    InvalidDocumentId(String),

    #[error("Invalid path: {0}")]
    InvalidPath(String),

    #[error("Strategy mismatch: expected {expected}, got {got}")]
    StrategyMismatch { expected: String, got: String },

    #[error("Document already exists: {0}")]
    DocumentExists(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("CRDT error: {0}")]
    Crdt(String),

    #[error("Document size exceeds limit: {size} > {limit}")]
    DocumentTooLarge { size: usize, limit: usize },

    #[error("Nesting depth exceeds limit: {depth} > {limit}")]
    NestingTooDeep { depth: usize, limit: usize },

    #[error("Invalid strategy: {0}")]
    InvalidStrategy(String),

    #[error("Failed to restore state: {0}")]
    RestoreError(String),
}

/// Result type alias for USSL Core operations
pub type Result<T> = std::result::Result<T, Error>;
