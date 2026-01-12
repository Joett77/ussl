//! Connection handler - processes commands and manages subscriptions

use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};
use ussl_core::{DocumentId, DocumentManager, Strategy, Value};
use ussl_protocol::{Command, CommandKind, Parser, Response};
use ussl_storage::Storage;

/// Handles a single client connection
pub struct ConnectionHandler {
    /// Unique client ID
    pub client_id: String,
    /// Document manager reference
    manager: Arc<DocumentManager>,
    /// Protocol parser
    parser: Parser,
    /// Active subscriptions (patterns)
    subscriptions: Vec<String>,
    /// Whether auth is required
    require_auth: bool,
    /// Whether client is authenticated
    authenticated: bool,
    /// Server password (if auth required)
    password: Option<String>,
    /// Optional persistent storage
    storage: Option<Arc<dyn Storage>>,
}

impl ConnectionHandler {
    pub fn new(client_id: String, manager: Arc<DocumentManager>) -> Self {
        Self {
            client_id,
            manager,
            parser: Parser::new(),
            subscriptions: Vec::new(),
            require_auth: false,
            authenticated: true, // No auth required by default
            password: None,
            storage: None,
        }
    }

    /// Create a new handler with authentication required
    pub fn with_auth(client_id: String, manager: Arc<DocumentManager>, password: String) -> Self {
        Self {
            client_id,
            manager,
            parser: Parser::new(),
            subscriptions: Vec::new(),
            require_auth: true,
            authenticated: false,
            password: Some(password),
            storage: None,
        }
    }

    /// Set the storage backend for persistence
    pub fn with_storage(mut self, storage: Arc<dyn Storage>) -> Self {
        self.storage = Some(storage);
        self
    }

    /// Process incoming data and return responses
    pub fn process(&mut self, data: &[u8]) -> Vec<Response> {
        let mut responses = Vec::new();

        if let Err(e) = self.parser.feed(data) {
            responses.push(Response::error("PARSE_ERROR", e.to_string()));
            return responses;
        }

        loop {
            match self.parser.parse() {
                Ok(Some(cmd)) => {
                    let response = self.handle_command(cmd);
                    responses.push(response);
                }
                Ok(None) => break, // Need more data
                Err(e) => {
                    responses.push(Response::error("PARSE_ERROR", e.to_string()));
                    break;
                }
            }
        }

        responses
    }

    /// Handle a single command
    fn handle_command(&mut self, cmd: Command) -> Response {
        debug!(client = %self.client_id, cmd = ?cmd.kind, "Processing command");

        // AUTH and PING are always allowed
        match &cmd.kind {
            CommandKind::Auth { password } => {
                return self.handle_auth(password.clone());
            }
            CommandKind::Ping => return Response::pong(),
            CommandKind::Quit => return Response::ok_with_message("Goodbye"),
            _ => {}
        }

        // Check authentication for all other commands
        if self.require_auth && !self.authenticated {
            return Response::error("NOAUTH", "Authentication required. Use AUTH <password>");
        }

        match cmd.kind {
            CommandKind::Auth { .. } => unreachable!(), // Handled above
            CommandKind::Create { strategy, ttl } => {
                self.handle_create(cmd.document_id, strategy, ttl)
            }
            CommandKind::Get { path } => {
                self.handle_get(cmd.document_id, path)
            }
            CommandKind::Set { path, value } => {
                self.handle_set(cmd.document_id, path, value)
            }
            CommandKind::Delete { path } => {
                self.handle_delete(cmd.document_id, path)
            }
            CommandKind::Subscribe { pattern, path } => {
                self.handle_subscribe(pattern, path)
            }
            CommandKind::Unsubscribe { pattern } => {
                self.handle_unsubscribe(pattern)
            }
            CommandKind::Push { path, value } => {
                self.handle_push(cmd.document_id, path, value)
            }
            CommandKind::Increment { path, delta } => {
                self.handle_increment(cmd.document_id, path, delta)
            }
            CommandKind::Presence { data } => {
                self.handle_presence(cmd.document_id, data)
            }
            CommandKind::Ping => unreachable!(), // Handled above
            CommandKind::Quit => unreachable!(), // Handled above
            CommandKind::Info => self.handle_info(),
            CommandKind::Keys { pattern } => self.handle_keys(pattern),
            CommandKind::Compact => self.handle_compact(cmd.document_id),
        }
    }

    fn handle_auth(&mut self, password: String) -> Response {
        match &self.password {
            Some(expected) if expected == &password => {
                self.authenticated = true;
                info!(client = %self.client_id, "Client authenticated");
                Response::ok()
            }
            Some(_) => {
                warn!(client = %self.client_id, "Authentication failed");
                Response::error("WRONGPASS", "Invalid password")
            }
            None => {
                // No password required
                Response::ok_with_message("No authentication required")
            }
        }
    }

    fn handle_create(
        &self,
        doc_id: Option<String>,
        strategy: Strategy,
        ttl: Option<u64>,
    ) -> Response {
        let id_str = match doc_id {
            Some(id) => id,
            None => return Response::error("MISSING_ARG", "Document ID required"),
        };

        let id = match DocumentId::new(&id_str) {
            Ok(id) => id,
            Err(e) => return Response::error("INVALID_ID", e.to_string()),
        };

        match self.manager.create(id, strategy, ttl) {
            Ok(_) => Response::ok(),
            Err(e) => Response::error("CREATE_ERROR", e.to_string()),
        }
    }

    fn handle_get(&self, doc_id: Option<String>, path: Option<String>) -> Response {
        let id_str = match doc_id {
            Some(id) => id,
            None => return Response::error("MISSING_ARG", "Document ID required"),
        };

        let id = match DocumentId::new(&id_str) {
            Ok(id) => id,
            Err(e) => return Response::error("INVALID_ID", e.to_string()),
        };

        match self.manager.get(&id) {
            Ok(doc) => {
                match doc.get(path.as_deref()) {
                    Ok(value) => Response::value(value),
                    Err(e) => Response::error("GET_ERROR", e.to_string()),
                }
            }
            Err(_) => Response::null(),
        }
    }

    fn handle_set(&self, doc_id: Option<String>, path: String, value: Value) -> Response {
        let id_str = match doc_id {
            Some(id) => id,
            None => return Response::error("MISSING_ARG", "Document ID required"),
        };

        let id = match DocumentId::new(&id_str) {
            Ok(id) => id,
            Err(e) => return Response::error("INVALID_ID", e.to_string()),
        };

        // Get or create document with default strategy
        let doc = self.manager.get_or_create(id.clone(), Strategy::default());

        match doc.set(&path, value) {
            Ok(_) => {
                // Check if auto-compaction is needed
                self.maybe_auto_compact(&id, &doc);

                // Persist to storage if available
                self.persist_document(&id, &doc);

                // Publish update to subscribers
                let delta = ussl_core::manager::Delta {
                    document_id: id,
                    version: doc.version(),
                    path: Some(path),
                    data: doc.encode_state(),
                };
                self.manager.publish_update(delta);
                Response::ok()
            }
            Err(e) => Response::error("SET_ERROR", e.to_string()),
        }
    }

    fn handle_delete(&self, doc_id: Option<String>, path: Option<String>) -> Response {
        let id_str = match doc_id {
            Some(id) => id,
            None => return Response::error("MISSING_ARG", "Document ID required"),
        };

        let id = match DocumentId::new(&id_str) {
            Ok(id) => id,
            Err(e) => return Response::error("INVALID_ID", e.to_string()),
        };

        match path {
            Some(p) => {
                // Delete path within document
                match self.manager.get(&id) {
                    Ok(doc) => {
                        match doc.delete(Some(&p)) {
                            Ok(_) => Response::ok(),
                            Err(e) => Response::error("DELETE_ERROR", e.to_string()),
                        }
                    }
                    Err(_) => Response::not_found(&id_str),
                }
            }
            None => {
                // Delete entire document
                match self.manager.delete(&id) {
                    Ok(_) => Response::ok(),
                    Err(e) => Response::error("DELETE_ERROR", e.to_string()),
                }
            }
        }
    }

    fn handle_subscribe(&mut self, pattern: String, _path: Option<String>) -> Response {
        if !self.subscriptions.contains(&pattern) {
            self.subscriptions.push(pattern.clone());
        }
        Response::ok_with_message(format!("Subscribed to {}", pattern))
    }

    fn handle_unsubscribe(&mut self, pattern: String) -> Response {
        self.subscriptions.retain(|p| p != &pattern);
        Response::ok_with_message(format!("Unsubscribed from {}", pattern))
    }

    fn handle_push(&self, doc_id: Option<String>, path: String, value: Value) -> Response {
        let id_str = match doc_id {
            Some(id) => id,
            None => return Response::error("MISSING_ARG", "Document ID required"),
        };

        let id = match DocumentId::new(&id_str) {
            Ok(id) => id,
            Err(e) => return Response::error("INVALID_ID", e.to_string()),
        };

        let doc = self.manager.get_or_create(id.clone(), Strategy::default());

        match doc.push(&path, value) {
            Ok(_) => {
                // Check if auto-compaction is needed
                self.maybe_auto_compact(&id, &doc);

                self.persist_document(&id, &doc);
                Response::ok()
            }
            Err(e) => Response::error("PUSH_ERROR", e.to_string()),
        }
    }

    fn handle_increment(&self, doc_id: Option<String>, path: String, delta: i64) -> Response {
        let id_str = match doc_id {
            Some(id) => id,
            None => return Response::error("MISSING_ARG", "Document ID required"),
        };

        let id = match DocumentId::new(&id_str) {
            Ok(id) => id,
            Err(e) => return Response::error("INVALID_ID", e.to_string()),
        };

        let doc = self.manager.get_or_create(id.clone(), Strategy::CrdtCounter);

        match doc.increment(&path, delta) {
            Ok(new_value) => {
                // Check if auto-compaction is needed
                self.maybe_auto_compact(&id, &doc);

                self.persist_document(&id, &doc);
                Response::integer(new_value)
            }
            Err(e) => Response::error("INC_ERROR", e.to_string()),
        }
    }

    fn handle_presence(&self, doc_id: Option<String>, data: Option<serde_json::Value>) -> Response {
        let id_str = match doc_id {
            Some(id) => id,
            None => return Response::error("MISSING_ARG", "Document ID required"),
        };

        let id = match DocumentId::new(&id_str) {
            Ok(id) => id,
            Err(e) => return Response::error("INVALID_ID", e.to_string()),
        };

        match data {
            Some(d) => {
                self.manager.set_presence(self.client_id.clone(), id, d);
                Response::ok()
            }
            None => {
                let presence = self.manager.get_presence(&id);
                let json: Vec<serde_json::Value> = presence
                    .into_iter()
                    .map(|p| serde_json::json!({
                        "client_id": p.client_id,
                        "data": p.data
                    }))
                    .collect();
                Response::bulk(serde_json::to_vec(&json).unwrap_or_default())
            }
        }
    }

    fn handle_info(&self) -> Response {
        let stats = self.manager.stats();
        let info = serde_json::json!({
            "version": env!("CARGO_PKG_VERSION"),
            "documents": stats.document_count,
            "subscribers": stats.subscriber_count,
            "client_id": self.client_id,
            "subscriptions": self.subscriptions,
        });
        Response::bulk(serde_json::to_vec(&info).unwrap_or_default())
    }

    fn handle_keys(&self, pattern: Option<String>) -> Response {
        let docs = self.manager.list(pattern.as_deref());
        let keys: Vec<Response> = docs
            .into_iter()
            .map(|meta| Response::bulk(meta.id.as_str().as_bytes().to_vec()))
            .collect();
        Response::array(keys)
    }

    fn handle_compact(&self, doc_id: Option<String>) -> Response {
        let id_str = match doc_id {
            Some(id) => id,
            None => return Response::error("MISSING_ARG", "Document ID required"),
        };

        let id = match DocumentId::new(&id_str) {
            Ok(id) => id,
            Err(e) => return Response::error("INVALID_ID", e.to_string()),
        };

        match self.manager.get(&id) {
            Ok(doc) => {
                match doc.compact() {
                    Ok(bytes_saved) => {
                        info!(
                            doc_id = %id,
                            bytes_saved = bytes_saved,
                            compaction_count = doc.compaction_count(),
                            "Document compacted"
                        );
                        Response::integer(bytes_saved as i64)
                    }
                    Err(e) => Response::error("COMPACT_ERROR", e.to_string()),
                }
            }
            Err(_) => Response::not_found(&id_str),
        }
    }

    /// Check if a document should be compacted and do so automatically
    fn maybe_auto_compact(&self, id: &DocumentId, doc: &ussl_core::Document) {
        if doc.should_compact() {
            debug!(doc_id = %id, updates = doc.update_count(), "Auto-compacting document");
            match doc.compact() {
                Ok(bytes_saved) => {
                    info!(
                        doc_id = %id,
                        bytes_saved = bytes_saved,
                        compaction_count = doc.compaction_count(),
                        "Auto-compaction completed"
                    );
                }
                Err(e) => {
                    warn!(doc_id = %id, error = %e, "Auto-compaction failed");
                }
            }
        }
    }

    /// Persist a document to storage (if available)
    fn persist_document(&self, id: &DocumentId, doc: &ussl_core::Document) {
        if let Some(ref storage) = self.storage {
            let meta = doc.meta();
            let data = doc.encode_state();
            let storage = storage.clone();
            let id = id.clone();

            // Spawn async task to persist
            tokio::spawn(async move {
                if let Err(e) = storage.store(&id, &meta, &data).await {
                    warn!(doc_id = %id, error = %e, "Failed to persist document");
                }
            });
        }
    }

    /// Clean up when connection closes
    pub fn cleanup(&self) {
        self.manager.remove_presence(&self.client_id);
    }

    /// Get subscription receiver for real-time updates
    pub fn subscribe_updates(&self) -> broadcast::Receiver<ussl_core::manager::Delta> {
        self.manager.subscribe()
    }

    /// Check if a delta matches any of this client's subscriptions
    pub fn matches_subscription(&self, delta: &ussl_core::manager::Delta) -> bool {
        let doc_id = delta.document_id.as_str();
        self.subscriptions.iter().any(|pattern| {
            if pattern == "*" {
                true
            } else if let Some(prefix) = pattern.strip_suffix('*') {
                doc_id.starts_with(prefix)
            } else {
                doc_id == pattern
            }
        })
    }
}
