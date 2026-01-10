//! SQLite storage backend

use crate::{Storage, StorageError, StorageStats};
use async_trait::async_trait;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::Mutex;
use ussl_core::{DocumentId, DocumentMeta};

/// SQLite storage backend
///
/// Embedded persistence suitable for edge deployments and single-node setups.
pub struct SqliteStorage {
    conn: Mutex<Connection>,
}

impl SqliteStorage {
    /// Create a new SQLite storage with the given path
    pub fn new(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let conn = Connection::open(path)
            .map_err(|e| StorageError::Database(e.to_string()))?;

        let storage = Self {
            conn: Mutex::new(conn),
        };

        storage.init_schema()?;
        Ok(storage)
    }

    /// Create an in-memory SQLite database (for testing)
    pub fn in_memory() -> Result<Self, StorageError> {
        let conn = Connection::open_in_memory()
            .map_err(|e| StorageError::Database(e.to_string()))?;

        let storage = Self {
            conn: Mutex::new(conn),
        };

        storage.init_schema()?;
        Ok(storage)
    }

    fn init_schema(&self) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();

        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS documents (
                id TEXT PRIMARY KEY,
                meta BLOB NOT NULL,
                data BLOB NOT NULL,
                created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now') * 1000),
                updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now') * 1000)
            );

            CREATE INDEX IF NOT EXISTS idx_documents_updated_at ON documents(updated_at);
            "#,
        )
        .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(())
    }
}

#[async_trait]
impl Storage for SqliteStorage {
    async fn store(
        &self,
        id: &DocumentId,
        meta: &DocumentMeta,
        data: &[u8],
    ) -> Result<(), StorageError> {
        let meta_bytes = serde_json::to_vec(meta)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;

        let conn = self.conn.lock().unwrap();

        conn.execute(
            r#"
            INSERT INTO documents (id, meta, data, updated_at)
            VALUES (?1, ?2, ?3, strftime('%s', 'now') * 1000)
            ON CONFLICT(id) DO UPDATE SET
                meta = excluded.meta,
                data = excluded.data,
                updated_at = excluded.updated_at
            "#,
            params![id.as_str(), meta_bytes, data],
        )
        .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(())
    }

    async fn load(
        &self,
        id: &DocumentId,
    ) -> Result<Option<(DocumentMeta, Vec<u8>)>, StorageError> {
        let conn = self.conn.lock().unwrap();

        let result: Option<(Vec<u8>, Vec<u8>)> = conn
            .query_row(
                "SELECT meta, data FROM documents WHERE id = ?1",
                params![id.as_str()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|e| StorageError::Database(e.to_string()))?;

        match result {
            Some((meta_bytes, data)) => {
                let meta: DocumentMeta = serde_json::from_slice(&meta_bytes)
                    .map_err(|e| StorageError::Serialization(e.to_string()))?;
                Ok(Some((meta, data)))
            }
            None => Ok(None),
        }
    }

    async fn delete(&self, id: &DocumentId) -> Result<bool, StorageError> {
        let conn = self.conn.lock().unwrap();

        let affected = conn
            .execute("DELETE FROM documents WHERE id = ?1", params![id.as_str()])
            .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(affected > 0)
    }

    async fn list(&self, pattern: Option<&str>) -> Result<Vec<DocumentId>, StorageError> {
        let conn = self.conn.lock().unwrap();

        let sql = match pattern {
            Some(p) if p.ends_with('*') => {
                let prefix = p.trim_end_matches('*');
                format!(
                    "SELECT id FROM documents WHERE id LIKE '{}%' ORDER BY updated_at DESC",
                    prefix.replace('\'', "''")
                )
            }
            Some(p) if p.starts_with('*') => {
                let suffix = p.trim_start_matches('*');
                format!(
                    "SELECT id FROM documents WHERE id LIKE '%{}' ORDER BY updated_at DESC",
                    suffix.replace('\'', "''")
                )
            }
            Some(p) => format!(
                "SELECT id FROM documents WHERE id = '{}' ORDER BY updated_at DESC",
                p.replace('\'', "''")
            ),
            None => "SELECT id FROM documents ORDER BY updated_at DESC".to_string(),
        };

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| StorageError::Database(e.to_string()))?;

        let ids: Vec<DocumentId> = stmt
            .query_map([], |row| {
                let id: String = row.get(0)?;
                Ok(id)
            })
            .map_err(|e| StorageError::Database(e.to_string()))?
            .filter_map(|r| r.ok())
            .filter_map(|id| DocumentId::new(id).ok())
            .collect();

        Ok(ids)
    }

    async fn exists(&self, id: &DocumentId) -> Result<bool, StorageError> {
        let conn = self.conn.lock().unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM documents WHERE id = ?1",
                params![id.as_str()],
                |row| row.get(0),
            )
            .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(count > 0)
    }

    async fn stats(&self) -> Result<StorageStats, StorageError> {
        let conn = self.conn.lock().unwrap();

        let document_count: usize = conn
            .query_row("SELECT COUNT(*) FROM documents", [], |row| row.get(0))
            .map_err(|e| StorageError::Database(e.to_string()))?;

        let total_size: usize = conn
            .query_row(
                "SELECT COALESCE(SUM(LENGTH(meta) + LENGTH(data)), 0) FROM documents",
                [],
                |row| row.get(0),
            )
            .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(StorageStats {
            document_count,
            total_size_bytes: total_size,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ussl_core::Strategy;

    #[tokio::test]
    async fn test_sqlite_crud() {
        let storage = SqliteStorage::in_memory().unwrap();
        let id = DocumentId::new("test:sqlite").unwrap();
        let meta = DocumentMeta::new(id.clone(), Strategy::Lww);
        let data = b"hello sqlite";

        // Store
        storage.store(&id, &meta, data).await.unwrap();

        // Load
        let (loaded_meta, loaded_data) = storage.load(&id).await.unwrap().unwrap();
        assert_eq!(loaded_meta.id.as_str(), "test:sqlite");
        assert_eq!(loaded_data, data);

        // Exists
        assert!(storage.exists(&id).await.unwrap());

        // Delete
        assert!(storage.delete(&id).await.unwrap());
        assert!(!storage.exists(&id).await.unwrap());
    }

    #[tokio::test]
    async fn test_sqlite_list_pattern() {
        let storage = SqliteStorage::in_memory().unwrap();

        // Create test documents
        for i in 0..5 {
            let id = DocumentId::new(format!("user:{}", i)).unwrap();
            let meta = DocumentMeta::new(id.clone(), Strategy::Lww);
            storage.store(&id, &meta, b"data").await.unwrap();
        }

        for i in 0..3 {
            let id = DocumentId::new(format!("cart:{}", i)).unwrap();
            let meta = DocumentMeta::new(id.clone(), Strategy::Lww);
            storage.store(&id, &meta, b"data").await.unwrap();
        }

        // List with pattern
        let users = storage.list(Some("user:*")).await.unwrap();
        assert_eq!(users.len(), 5);

        let carts = storage.list(Some("cart:*")).await.unwrap();
        assert_eq!(carts.len(), 3);

        let all = storage.list(None).await.unwrap();
        assert_eq!(all.len(), 8);
    }

    #[tokio::test]
    async fn test_sqlite_upsert() {
        let storage = SqliteStorage::in_memory().unwrap();
        let id = DocumentId::new("test:upsert").unwrap();
        let meta = DocumentMeta::new(id.clone(), Strategy::Lww);

        // Initial store
        storage.store(&id, &meta, b"version1").await.unwrap();

        // Update
        storage.store(&id, &meta, b"version2").await.unwrap();

        // Verify update
        let (_, data) = storage.load(&id).await.unwrap().unwrap();
        assert_eq!(data, b"version2");

        // Should still be one document
        let stats = storage.stats().await.unwrap();
        assert_eq!(stats.document_count, 1);
    }
}
