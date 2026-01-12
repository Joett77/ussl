//! Document Manager - handles document lifecycle and subscriptions

use crate::crdt::Strategy;
use crate::document::{Document, DocumentId, DocumentMeta};
use crate::error::{Error, Result};
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::broadcast;

/// Delta update sent to subscribers
#[derive(Debug, Clone)]
pub struct Delta {
    pub document_id: DocumentId,
    pub version: u64,
    pub path: Option<String>,
    pub data: Vec<u8>,
}

/// Presence information for a client
#[derive(Debug, Clone)]
pub struct Presence {
    pub client_id: String,
    pub document_id: DocumentId,
    pub data: serde_json::Value,
}

/// Document manager handles all documents and subscriptions
pub struct DocumentManager {
    /// All documents indexed by ID
    documents: DashMap<String, Arc<Document>>,
    /// Broadcast channel for document updates
    update_sender: broadcast::Sender<Delta>,
    /// Presence information per document
    presence: DashMap<String, Vec<Presence>>,
}

impl DocumentManager {
    /// Create a new document manager
    pub fn new() -> Self {
        let (update_sender, _) = broadcast::channel(10000);

        Self {
            documents: DashMap::new(),
            update_sender,
            presence: DashMap::new(),
        }
    }

    /// Create a new document
    pub fn create(
        &self,
        id: DocumentId,
        strategy: Strategy,
        ttl: Option<u64>,
    ) -> Result<Arc<Document>> {
        let key = id.as_str().to_string();

        if self.documents.contains_key(&key) {
            return Err(Error::DocumentExists(key));
        }

        let doc = Arc::new(match ttl {
            Some(ttl_ms) => Document::with_ttl(id, strategy, ttl_ms),
            None => Document::new(id, strategy),
        });
        self.documents.insert(key, doc.clone());

        Ok(doc)
    }

    /// Get an existing document
    pub fn get(&self, id: &DocumentId) -> Result<Arc<Document>> {
        self.documents
            .get(id.as_str())
            .map(|r| r.value().clone())
            .ok_or_else(|| Error::DocumentNotFound(id.to_string()))
    }

    /// Get or create a document
    pub fn get_or_create(
        &self,
        id: DocumentId,
        strategy: Strategy,
    ) -> Arc<Document> {
        let key = id.as_str().to_string();

        self.documents
            .entry(key)
            .or_insert_with(|| Arc::new(Document::new(id, strategy)))
            .value()
            .clone()
    }

    /// Delete a document
    pub fn delete(&self, id: &DocumentId) -> Result<()> {
        self.documents
            .remove(id.as_str())
            .map(|_| ())
            .ok_or_else(|| Error::DocumentNotFound(id.to_string()))
    }

    /// List all documents matching a pattern (glob syntax)
    pub fn list(&self, pattern: Option<&str>) -> Vec<DocumentMeta> {
        self.documents
            .iter()
            .filter(|entry| {
                pattern.map_or(true, |p| Self::matches_pattern(entry.key(), p))
            })
            .map(|entry| entry.value().meta())
            .collect()
    }

    /// Subscribe to document updates
    pub fn subscribe(&self) -> broadcast::Receiver<Delta> {
        self.update_sender.subscribe()
    }

    /// Publish an update to all subscribers
    pub fn publish_update(&self, delta: Delta) {
        let _ = self.update_sender.send(delta);
    }

    /// Set presence for a client
    pub fn set_presence(&self, client_id: String, document_id: DocumentId, data: serde_json::Value) {
        let presence = Presence {
            client_id: client_id.clone(),
            document_id: document_id.clone(),
            data,
        };

        let key = document_id.as_str().to_string();
        self.presence
            .entry(key)
            .or_insert_with(Vec::new)
            .retain(|p| p.client_id != client_id);

        self.presence
            .get_mut(document_id.as_str())
            .unwrap()
            .push(presence);
    }

    /// Get presence for a document
    pub fn get_presence(&self, document_id: &DocumentId) -> Vec<Presence> {
        self.presence
            .get(document_id.as_str())
            .map(|r| r.value().clone())
            .unwrap_or_default()
    }

    /// Remove presence for a client
    pub fn remove_presence(&self, client_id: &str) {
        for mut entry in self.presence.iter_mut() {
            entry.value_mut().retain(|p| p.client_id != client_id);
        }
    }

    /// Set TTL for an existing document
    pub fn set_expire(&self, id: &DocumentId, ttl_ms: Option<u64>) -> Result<()> {
        let doc = self.get(id)?;
        doc.set_ttl(ttl_ms);
        Ok(())
    }

    /// Get TTL remaining for a document (in ms)
    pub fn ttl(&self, id: &DocumentId) -> Result<Option<i64>> {
        let doc = self.get(id)?;
        Ok(doc.ttl_remaining())
    }

    /// Run garbage collection - removes expired documents
    /// Returns the number of documents removed
    pub fn gc(&self) -> usize {
        let mut to_remove = Vec::new();

        for entry in self.documents.iter() {
            if entry.value().is_expired() {
                to_remove.push(entry.key().clone());
            }
        }

        let count = to_remove.len();
        for key in to_remove {
            self.documents.remove(&key);
            // Also clean up any presence data
            self.presence.remove(&key);
        }

        count
    }

    /// Get count of expired documents (without removing them)
    pub fn expired_count(&self) -> usize {
        self.documents
            .iter()
            .filter(|entry| entry.value().is_expired())
            .count()
    }

    /// Get statistics
    pub fn stats(&self) -> ManagerStats {
        ManagerStats {
            document_count: self.documents.len(),
            subscriber_count: self.update_sender.receiver_count(),
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
}

impl Default for DocumentManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Manager statistics
#[derive(Debug, Clone)]
pub struct ManagerStats {
    pub document_count: usize,
    pub subscriber_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crdt::Value;

    #[test]
    fn test_create_and_get() {
        let manager = DocumentManager::new();
        let id = DocumentId::new("test:1").unwrap();

        let doc = manager.create(id.clone(), Strategy::Lww, None).unwrap();
        doc.set("key", Value::String("value".into())).unwrap();

        let retrieved = manager.get(&id).unwrap();
        assert_eq!(retrieved.get(Some("key")).unwrap(), Value::String("value".into()));
    }

    #[test]
    fn test_duplicate_create_fails() {
        let manager = DocumentManager::new();
        let id = DocumentId::new("test:2").unwrap();

        manager.create(id.clone(), Strategy::Lww, None).unwrap();
        assert!(manager.create(id, Strategy::Lww, None).is_err());
    }

    #[test]
    fn test_pattern_matching() {
        assert!(DocumentManager::matches_pattern("user:123", "user:*"));
        assert!(DocumentManager::matches_pattern("user:123", "*:123"));
        assert!(DocumentManager::matches_pattern("anything", "*"));
        assert!(!DocumentManager::matches_pattern("cart:456", "user:*"));
    }

    #[test]
    fn test_list_with_pattern() {
        let manager = DocumentManager::new();

        manager.create(DocumentId::new("user:1").unwrap(), Strategy::Lww, None).unwrap();
        manager.create(DocumentId::new("user:2").unwrap(), Strategy::Lww, None).unwrap();
        manager.create(DocumentId::new("cart:1").unwrap(), Strategy::Lww, None).unwrap();

        let users = manager.list(Some("user:*"));
        assert_eq!(users.len(), 2);

        let all = manager.list(None);
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_create_with_ttl() {
        let manager = DocumentManager::new();
        let id = DocumentId::new("ttl:1").unwrap();

        // Create with 1 hour TTL
        let doc = manager.create(id.clone(), Strategy::Lww, Some(3600_000)).unwrap();

        // Document should not be expired yet
        assert!(!doc.is_expired());

        // TTL should be approximately 1 hour (in ms)
        let ttl = doc.ttl_remaining().unwrap();
        assert!(ttl > 3599_000 && ttl <= 3600_000);
    }

    #[test]
    fn test_set_expire() {
        let manager = DocumentManager::new();
        let id = DocumentId::new("expire:1").unwrap();

        // Create without TTL
        manager.create(id.clone(), Strategy::Lww, None).unwrap();

        // Initially no TTL
        assert!(manager.ttl(&id).unwrap().is_none());

        // Set TTL
        manager.set_expire(&id, Some(5000)).unwrap();

        // Now has TTL
        let ttl = manager.ttl(&id).unwrap().unwrap();
        assert!(ttl > 0 && ttl <= 5000);

        // Remove TTL
        manager.set_expire(&id, None).unwrap();
        assert!(manager.ttl(&id).unwrap().is_none());
    }

    #[test]
    fn test_gc_removes_expired() {
        let manager = DocumentManager::new();

        // Create document with very short TTL (1ms)
        let id = DocumentId::new("gc:1").unwrap();
        manager.create(id.clone(), Strategy::Lww, Some(1)).unwrap();

        // Create document without TTL
        let id2 = DocumentId::new("gc:2").unwrap();
        manager.create(id2.clone(), Strategy::Lww, None).unwrap();

        // Wait for expiration
        std::thread::sleep(std::time::Duration::from_millis(10));

        // Should have 1 expired document
        assert_eq!(manager.expired_count(), 1);

        // GC should remove 1 document
        let removed = manager.gc();
        assert_eq!(removed, 1);

        // Only the non-expired document should remain
        assert_eq!(manager.stats().document_count, 1);
        assert!(manager.get(&id).is_err()); // Expired doc removed
        assert!(manager.get(&id2).is_ok()); // Non-expired doc remains
    }
}
