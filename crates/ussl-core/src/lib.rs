//! USSL Core - CRDT Engine and Document Management
//!
//! This crate provides the core functionality for USSL:
//! - Document management with unique identifiers
//! - CRDT-based conflict resolution strategies
//! - Subscription and presence management

pub mod document;
pub mod crdt;
pub mod error;
pub mod manager;

pub use document::{Document, DocumentId, DocumentMeta};
pub use crdt::{Strategy, Value};
pub use error::{Error, Result};
pub use manager::DocumentManager;
