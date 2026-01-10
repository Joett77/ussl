//! Document types and operations

use crate::crdt::{Strategy, Value};
use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use parking_lot::RwLock;
use yrs::{Doc, Text, Transact, ReadTxn, GetString};
use yrs::updates::decoder::Decode;

/// Maximum document size in bytes (16MB default)
pub const MAX_DOCUMENT_SIZE: usize = 16 * 1024 * 1024;

/// Maximum nesting depth
pub const MAX_NESTING_DEPTH: usize = 32;

/// Document identifier - UTF-8 string, max 512 bytes
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DocumentId(String);

impl DocumentId {
    /// Create a new document ID, validating the format
    pub fn new(id: impl Into<String>) -> Result<Self> {
        let id = id.into();

        if id.is_empty() {
            return Err(Error::InvalidDocumentId("Document ID cannot be empty".into()));
        }

        if id.len() > 512 {
            return Err(Error::InvalidDocumentId("Document ID exceeds 512 bytes".into()));
        }

        // Validate pattern: [a-zA-Z0-9:_-]+
        if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == ':' || c == '_' || c == '-') {
            return Err(Error::InvalidDocumentId(
                "Document ID must match pattern [a-zA-Z0-9:_-]+".into()
            ));
        }

        Ok(Self(id))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for DocumentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Document metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentMeta {
    pub id: DocumentId,
    pub strategy: Strategy,
    pub created_at: u64,
    pub updated_at: u64,
    pub version: u64,
    pub ttl: Option<u64>,
}

impl DocumentMeta {
    pub fn new(id: DocumentId, strategy: Strategy) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        Self {
            id,
            strategy,
            created_at: now,
            updated_at: now,
            version: 1,
            ttl: None,
        }
    }
}

/// A synchronized document with CRDT support
pub struct Document {
    meta: RwLock<DocumentMeta>,
    /// Y.js document for CRDT operations
    ydoc: Doc,
    /// LWW fallback for simple key-value
    lww_data: RwLock<Value>,
}

impl Document {
    /// Create a new document with the given ID and strategy
    pub fn new(id: DocumentId, strategy: Strategy) -> Self {
        Self {
            meta: RwLock::new(DocumentMeta::new(id, strategy)),
            ydoc: Doc::new(),
            lww_data: RwLock::new(Value::Object(std::collections::HashMap::new())),
        }
    }

    /// Get the document ID
    pub fn id(&self) -> DocumentId {
        self.meta.read().id.clone()
    }

    /// Get the document strategy
    pub fn strategy(&self) -> Strategy {
        self.meta.read().strategy
    }

    /// Get document metadata
    pub fn meta(&self) -> DocumentMeta {
        self.meta.read().clone()
    }

    /// Get the current version
    pub fn version(&self) -> u64 {
        self.meta.read().version
    }

    /// Get a value at the given path
    pub fn get(&self, path: Option<&str>) -> Result<Value> {
        let strategy = self.strategy();

        match strategy {
            Strategy::Lww | Strategy::CrdtMap => {
                let data = self.lww_data.read();
                match path {
                    Some(p) => data.get_path(p)
                        .cloned()
                        .ok_or_else(|| Error::InvalidPath(p.to_string())),
                    None => Ok(data.clone()),
                }
            }
            Strategy::CrdtText => {
                let text = self.ydoc.get_or_insert_text("content");
                let txn = self.ydoc.transact();
                Ok(Value::String(text.get_string(&txn)))
            }
            Strategy::CrdtCounter => {
                let data = self.lww_data.read();
                Ok(data.clone())
            }
            Strategy::CrdtSet => {
                let data = self.lww_data.read();
                Ok(data.clone())
            }
        }
    }

    /// Set a value at the given path
    pub fn set(&self, path: &str, value: Value) -> Result<()> {
        let strategy = self.strategy();

        match strategy {
            Strategy::Lww | Strategy::CrdtMap => {
                let mut data = self.lww_data.write();
                data.set_path(path, value)?;
                self.update_version();
                Ok(())
            }
            Strategy::CrdtText => {
                if let Value::String(text_value) = value {
                    let ytext = self.ydoc.get_or_insert_text("content");
                    let mut txn = self.ydoc.transact_mut();
                    let current_len = ytext.get_string(&txn).len() as u32;
                    ytext.remove_range(&mut txn, 0, current_len);
                    ytext.insert(&mut txn, 0, &text_value);
                    self.update_version();
                    Ok(())
                } else {
                    Err(Error::Crdt("CrdtText strategy requires string values".into()))
                }
            }
            _ => {
                let mut data = self.lww_data.write();
                data.set_path(path, value)?;
                self.update_version();
                Ok(())
            }
        }
    }

    /// Delete a value at the given path (or the entire document content)
    pub fn delete(&self, path: Option<&str>) -> Result<()> {
        match path {
            Some(p) => {
                let mut data = self.lww_data.write();
                data.set_path(p, Value::Null)?;
                self.update_version();
                Ok(())
            }
            None => {
                let mut data = self.lww_data.write();
                *data = Value::Object(std::collections::HashMap::new());
                self.update_version();
                Ok(())
            }
        }
    }

    /// Push a value to an array at the given path
    pub fn push(&self, path: &str, value: Value) -> Result<()> {
        let mut data = self.lww_data.write();

        // Get or create array at path
        let arr = match data.get_path(path) {
            Some(Value::Array(_)) => {}
            Some(_) => return Err(Error::InvalidPath(format!("{} is not an array", path))),
            None => {
                data.set_path(path, Value::Array(vec![]))?;
            }
        };

        // Navigate to the array and push
        if let Some(Value::Array(arr)) = data.get_path(path).cloned() {
            let mut new_arr = arr;
            new_arr.push(value);
            data.set_path(path, Value::Array(new_arr))?;
        }

        self.update_version();
        Ok(())
    }

    /// Increment a counter at the given path
    pub fn increment(&self, path: &str, delta: i64) -> Result<i64> {
        let mut data = self.lww_data.write();

        let current = data.get_path(path)
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        let new_value = current + delta;
        data.set_path(path, Value::Number(crate::crdt::Number::Integer(new_value)))?;

        self.update_version();
        Ok(new_value)
    }

    /// Get the Y.js document state as bytes (for sync)
    pub fn encode_state(&self) -> Vec<u8> {
        let txn = self.ydoc.transact();
        txn.encode_state_as_update_v1(&yrs::StateVector::default())
    }

    /// Apply a Y.js update from another peer
    pub fn apply_update(&self, update: &[u8]) -> Result<()> {
        let mut txn = self.ydoc.transact_mut();
        let decoded = yrs::Update::decode_v1(update)
            .map_err(|e: yrs::encoding::read::Error| Error::Crdt(e.to_string()))?;
        txn.apply_update(decoded);
        self.update_version();
        Ok(())
    }

    fn update_version(&self) {
        let mut meta = self.meta.write();
        meta.version += 1;
        meta.updated_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
    }
}

impl std::fmt::Debug for Document {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Document")
            .field("meta", &self.meta)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_id_valid() {
        assert!(DocumentId::new("user:123").is_ok());
        assert!(DocumentId::new("cart_items-456").is_ok());
    }

    #[test]
    fn test_document_id_invalid() {
        assert!(DocumentId::new("").is_err());
        assert!(DocumentId::new("user/123").is_err()); // invalid char
        assert!(DocumentId::new("a".repeat(513)).is_err()); // too long
    }

    #[test]
    fn test_document_set_get() {
        let id = DocumentId::new("test:1").unwrap();
        let doc = Document::new(id, Strategy::Lww);

        doc.set("name", Value::String("Alice".into())).unwrap();
        let value = doc.get(Some("name")).unwrap();

        assert_eq!(value, Value::String("Alice".into()));
    }

    #[test]
    fn test_document_nested_path() {
        let id = DocumentId::new("test:2").unwrap();
        let doc = Document::new(id, Strategy::CrdtMap);

        doc.set("user.profile.name", Value::String("Bob".into())).unwrap();
        let value = doc.get(Some("user.profile.name")).unwrap();

        assert_eq!(value, Value::String("Bob".into()));
    }

    #[test]
    fn test_document_increment() {
        let id = DocumentId::new("test:3").unwrap();
        let doc = Document::new(id, Strategy::CrdtCounter);

        assert_eq!(doc.increment("count", 5).unwrap(), 5);
        assert_eq!(doc.increment("count", 3).unwrap(), 8);
        assert_eq!(doc.increment("count", -2).unwrap(), 6);
    }
}
