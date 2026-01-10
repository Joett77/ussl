//! In-memory storage backend

use crate::{Storage, StorageError, StorageStats};
use async_trait::async_trait;
use dashmap::DashMap;
use ussl_core::{DocumentId, DocumentMeta};
use std::sync::atomic::{AtomicUsize, Ordering};

/// In-memory storage backend
///
/// Fast, volatile storage suitable for development and caching.
/// Data is lost when the process exits.
pub struct MemoryStorage {
    /// Document data: id -> (meta_bytes, data_bytes)
    data: DashMap<String, (Vec<u8>, Vec<u8>)>,
    /// Total size tracking
    total_size: AtomicUsize,
}

impl MemoryStorage {
    pub fn new() -> Self {
        Self {
            data: DashMap::new(),
            total_size: AtomicUsize::new(0),
        }
    }
}

impl Default for MemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Storage for MemoryStorage {
    async fn store(&self, id: &DocumentId, meta: &DocumentMeta, data: &[u8]) -> Result<(), StorageError> {
        let meta_bytes = serde_json::to_vec(meta)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;

        let key = id.as_str().to_string();
        let new_size = meta_bytes.len() + data.len();

        // Update size tracking
        if let Some(existing) = self.data.get(&key) {
            let old_size = existing.0.len() + existing.1.len();
            self.total_size.fetch_sub(old_size, Ordering::Relaxed);
        }
        self.total_size.fetch_add(new_size, Ordering::Relaxed);

        self.data.insert(key, (meta_bytes, data.to_vec()));
        Ok(())
    }

    async fn load(&self, id: &DocumentId) -> Result<Option<(DocumentMeta, Vec<u8>)>, StorageError> {
        match self.data.get(id.as_str()) {
            Some(entry) => {
                let (meta_bytes, data) = entry.value();
                let meta: DocumentMeta = serde_json::from_slice(meta_bytes)
                    .map_err(|e| StorageError::Serialization(e.to_string()))?;
                Ok(Some((meta, data.clone())))
            }
            None => Ok(None),
        }
    }

    async fn delete(&self, id: &DocumentId) -> Result<bool, StorageError> {
        match self.data.remove(id.as_str()) {
            Some((_, (meta_bytes, data))) => {
                let size = meta_bytes.len() + data.len();
                self.total_size.fetch_sub(size, Ordering::Relaxed);
                Ok(true)
            }
            None => Ok(false),
        }
    }

    async fn list(&self, pattern: Option<&str>) -> Result<Vec<DocumentId>, StorageError> {
        let mut ids = Vec::new();
        for entry in self.data.iter() {
            let key = entry.key();
            let matches = pattern.map_or(true, |p| matches_pattern(key, p));
            if matches {
                if let Ok(id) = DocumentId::new(key.clone()) {
                    ids.push(id);
                }
            }
        }
        Ok(ids)
    }

    async fn exists(&self, id: &DocumentId) -> Result<bool, StorageError> {
        Ok(self.data.contains_key(id.as_str()))
    }

    async fn stats(&self) -> Result<StorageStats, StorageError> {
        Ok(StorageStats {
            document_count: self.data.len(),
            total_size_bytes: self.total_size.load(Ordering::Relaxed),
        })
    }
}

/// Simple glob pattern matching
fn matches_pattern(key: &str, pattern: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    if let Some(prefix) = pattern.strip_suffix('*') {
        return key.starts_with(prefix);
    }

    if let Some(suffix) = pattern.strip_prefix('*') {
        return key.ends_with(suffix);
    }

    key == pattern
}

#[cfg(test)]
mod tests {
    use super::*;
    use ussl_core::Strategy;

    #[tokio::test]
    async fn test_store_and_load() {
        let storage = MemoryStorage::new();
        let id = DocumentId::new("test:1").unwrap();
        let meta = DocumentMeta::new(id.clone(), Strategy::Lww);
        let data = b"hello world";

        storage.store(&id, &meta, data).await.unwrap();

        let (loaded_meta, loaded_data) = storage.load(&id).await.unwrap().unwrap();
        assert_eq!(loaded_meta.id.as_str(), "test:1");
        assert_eq!(loaded_data, data);
    }

    #[tokio::test]
    async fn test_delete() {
        let storage = MemoryStorage::new();
        let id = DocumentId::new("test:2").unwrap();
        let meta = DocumentMeta::new(id.clone(), Strategy::Lww);

        storage.store(&id, &meta, b"data").await.unwrap();
        assert!(storage.exists(&id).await.unwrap());

        assert!(storage.delete(&id).await.unwrap());
        assert!(!storage.exists(&id).await.unwrap());
    }

    #[tokio::test]
    async fn test_list_with_pattern() {
        let storage = MemoryStorage::new();

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

        let users = storage.list(Some("user:*")).await.unwrap();
        assert_eq!(users.len(), 5);

        let carts = storage.list(Some("cart:*")).await.unwrap();
        assert_eq!(carts.len(), 3);

        let all = storage.list(None).await.unwrap();
        assert_eq!(all.len(), 8);
    }

    #[tokio::test]
    async fn test_stats() {
        let storage = MemoryStorage::new();

        let id = DocumentId::new("test:stats").unwrap();
        let meta = DocumentMeta::new(id.clone(), Strategy::Lww);
        storage.store(&id, &meta, b"some data here").await.unwrap();

        let stats = storage.stats().await.unwrap();
        assert_eq!(stats.document_count, 1);
        assert!(stats.total_size_bytes > 0);
    }
}
