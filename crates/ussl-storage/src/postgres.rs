//! PostgreSQL storage backend

use crate::{Storage, StorageError, StorageStats};
use async_trait::async_trait;
use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::Row;
use ussl_core::{DocumentId, DocumentMeta};

/// PostgreSQL storage backend
///
/// Scalable persistence with NOTIFY/LISTEN support for real-time sync.
pub struct PostgresStorage {
    pool: PgPool,
}

impl PostgresStorage {
    /// Create a new PostgreSQL storage
    pub async fn new(database_url: &str) -> Result<Self, StorageError> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(database_url)
            .await
            .map_err(|e| StorageError::Connection(e.to_string()))?;

        let storage = Self { pool };
        storage.init_schema().await?;

        Ok(storage)
    }

    /// Create with an existing connection pool
    pub fn with_pool(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn init_schema(&self) -> Result<(), StorageError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS documents (
                id TEXT PRIMARY KEY,
                meta BYTEA NOT NULL,
                data BYTEA NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            );

            CREATE INDEX IF NOT EXISTS idx_documents_updated_at ON documents(updated_at DESC);
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        // Create trigger for update notifications
        sqlx::query(
            r#"
            CREATE OR REPLACE FUNCTION notify_document_change()
            RETURNS TRIGGER AS $$
            BEGIN
                PERFORM pg_notify('document_changes', NEW.id);
                RETURN NEW;
            END;
            $$ LANGUAGE plpgsql;

            DROP TRIGGER IF EXISTS document_change_trigger ON documents;
            CREATE TRIGGER document_change_trigger
                AFTER INSERT OR UPDATE ON documents
                FOR EACH ROW EXECUTE FUNCTION notify_document_change();
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(())
    }

    /// Subscribe to document changes via LISTEN/NOTIFY
    pub async fn subscribe_changes(&self) -> Result<sqlx::postgres::PgListener, StorageError> {
        let mut listener = sqlx::postgres::PgListener::connect_with(&self.pool)
            .await
            .map_err(|e| StorageError::Connection(e.to_string()))?;

        listener
            .listen("document_changes")
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(listener)
    }
}

#[async_trait]
impl Storage for PostgresStorage {
    async fn store(
        &self,
        id: &DocumentId,
        meta: &DocumentMeta,
        data: &[u8],
    ) -> Result<(), StorageError> {
        let meta_bytes = serde_json::to_vec(meta)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;

        sqlx::query(
            r#"
            INSERT INTO documents (id, meta, data, updated_at)
            VALUES ($1, $2, $3, NOW())
            ON CONFLICT (id) DO UPDATE SET
                meta = EXCLUDED.meta,
                data = EXCLUDED.data,
                updated_at = NOW()
            "#,
        )
        .bind(id.as_str())
        .bind(&meta_bytes)
        .bind(data)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(())
    }

    async fn load(
        &self,
        id: &DocumentId,
    ) -> Result<Option<(DocumentMeta, Vec<u8>)>, StorageError> {
        let row = sqlx::query("SELECT meta, data FROM documents WHERE id = $1")
            .bind(id.as_str())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;

        match row {
            Some(row) => {
                let meta_bytes: Vec<u8> = row.get("meta");
                let data: Vec<u8> = row.get("data");

                let meta: DocumentMeta = serde_json::from_slice(&meta_bytes)
                    .map_err(|e| StorageError::Serialization(e.to_string()))?;

                Ok(Some((meta, data)))
            }
            None => Ok(None),
        }
    }

    async fn delete(&self, id: &DocumentId) -> Result<bool, StorageError> {
        let result = sqlx::query("DELETE FROM documents WHERE id = $1")
            .bind(id.as_str())
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(result.rows_affected() > 0)
    }

    async fn list(&self, pattern: Option<&str>) -> Result<Vec<DocumentId>, StorageError> {
        let rows = match pattern {
            Some(p) if p.ends_with('*') => {
                let prefix = p.trim_end_matches('*');
                sqlx::query("SELECT id FROM documents WHERE id LIKE $1 ORDER BY updated_at DESC")
                    .bind(format!("{}%", prefix))
                    .fetch_all(&self.pool)
                    .await
            }
            Some(p) if p.starts_with('*') => {
                let suffix = p.trim_start_matches('*');
                sqlx::query("SELECT id FROM documents WHERE id LIKE $1 ORDER BY updated_at DESC")
                    .bind(format!("%{}", suffix))
                    .fetch_all(&self.pool)
                    .await
            }
            Some(p) => {
                sqlx::query("SELECT id FROM documents WHERE id = $1")
                    .bind(p)
                    .fetch_all(&self.pool)
                    .await
            }
            None => {
                sqlx::query("SELECT id FROM documents ORDER BY updated_at DESC")
                    .fetch_all(&self.pool)
                    .await
            }
        }
        .map_err(|e| StorageError::Database(e.to_string()))?;

        let ids: Vec<DocumentId> = rows
            .iter()
            .filter_map(|row| {
                let id: String = row.get("id");
                DocumentId::new(id).ok()
            })
            .collect();

        Ok(ids)
    }

    async fn exists(&self, id: &DocumentId) -> Result<bool, StorageError> {
        let row = sqlx::query("SELECT 1 FROM documents WHERE id = $1")
            .bind(id.as_str())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(row.is_some())
    }

    async fn stats(&self) -> Result<StorageStats, StorageError> {
        let count_row = sqlx::query("SELECT COUNT(*) as count FROM documents")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;

        let document_count: i64 = count_row.get("count");

        let size_row = sqlx::query(
            "SELECT COALESCE(SUM(LENGTH(meta) + LENGTH(data)), 0) as size FROM documents",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        let total_size: i64 = size_row.get("size");

        Ok(StorageStats {
            document_count: document_count as usize,
            total_size_bytes: total_size as usize,
        })
    }
}

#[cfg(test)]
mod tests {
    // Integration tests require a running PostgreSQL instance
    // Run with: cargo test --features postgres -- --ignored

    use super::*;

    #[tokio::test]
    #[ignore]
    async fn test_postgres_crud() {
        let storage = PostgresStorage::new("postgres://ussl:ussl@localhost/ussl")
            .await
            .unwrap();

        let id = DocumentId::new("test:postgres").unwrap();
        let meta = DocumentMeta::new(id.clone(), ussl_core::Strategy::Lww);
        let data = b"hello postgres";

        // Store
        storage.store(&id, &meta, data).await.unwrap();

        // Load
        let (loaded_meta, loaded_data) = storage.load(&id).await.unwrap().unwrap();
        assert_eq!(loaded_meta.id.as_str(), "test:postgres");
        assert_eq!(loaded_data, data);

        // Delete
        assert!(storage.delete(&id).await.unwrap());
    }
}
