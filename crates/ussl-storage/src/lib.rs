//! USSL Storage Backends
//!
//! Provides pluggable storage for document persistence:
//! - Memory (default): Fast, volatile storage
//! - SQLite: Embedded persistence
//! - PostgreSQL: Scalable persistence

pub mod memory;
#[cfg(feature = "sqlite")]
pub mod sqlite;
#[cfg(feature = "postgres")]
pub mod postgres;

use async_trait::async_trait;
use ussl_core::{DocumentId, DocumentMeta};

/// Storage backend trait
#[async_trait]
pub trait Storage: Send + Sync {
    /// Store a document
    async fn store(&self, id: &DocumentId, meta: &DocumentMeta, data: &[u8]) -> Result<(), StorageError>;

    /// Load a document
    async fn load(&self, id: &DocumentId) -> Result<Option<(DocumentMeta, Vec<u8>)>, StorageError>;

    /// Delete a document
    async fn delete(&self, id: &DocumentId) -> Result<bool, StorageError>;

    /// List document IDs matching a pattern
    async fn list(&self, pattern: Option<&str>) -> Result<Vec<DocumentId>, StorageError>;

    /// Check if a document exists
    async fn exists(&self, id: &DocumentId) -> Result<bool, StorageError>;

    /// Get storage statistics
    async fn stats(&self) -> Result<StorageStats, StorageError>;
}

/// Storage error types
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("Document not found: {0}")]
    NotFound(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("IO error: {0}")]
    Io(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("Connection error: {0}")]
    Connection(String),
}

/// Storage statistics
#[derive(Debug, Clone, Default)]
pub struct StorageStats {
    pub document_count: usize,
    pub total_size_bytes: usize,
}

pub use memory::MemoryStorage;
#[cfg(feature = "sqlite")]
pub use sqlite::SqliteStorage;
#[cfg(feature = "postgres")]
pub use postgres::PostgresStorage;
