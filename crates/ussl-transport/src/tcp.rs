//! TCP transport for USSL

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};
use ussl_core::DocumentManager;
use ussl_protocol::Response;
use ussl_storage::Storage;

use crate::handler::ConnectionHandler;

/// TCP Server for USSL
pub struct TcpServer {
    manager: Arc<DocumentManager>,
    addr: SocketAddr,
    client_counter: AtomicU64,
    password: Option<String>,
    storage: Option<Arc<dyn Storage>>,
}

impl TcpServer {
    pub fn new(manager: Arc<DocumentManager>, addr: SocketAddr) -> Self {
        Self {
            manager,
            addr,
            client_counter: AtomicU64::new(0),
            password: None,
            storage: None,
        }
    }

    /// Create a server with authentication required
    pub fn with_password(manager: Arc<DocumentManager>, addr: SocketAddr, password: String) -> Self {
        Self {
            manager,
            addr,
            client_counter: AtomicU64::new(0),
            password: Some(password),
            storage: None,
        }
    }

    /// Set the storage backend for persistence
    pub fn with_storage(mut self, storage: Arc<dyn Storage>) -> Self {
        self.storage = Some(storage);
        self
    }

    /// Start the TCP server
    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let listener = TcpListener::bind(self.addr).await?;
        info!(addr = %self.addr, "USSL TCP server listening");

        loop {
            match listener.accept().await {
                Ok((stream, peer_addr)) => {
                    let client_id = format!(
                        "tcp:{}:{}",
                        peer_addr,
                        self.client_counter.fetch_add(1, Ordering::Relaxed)
                    );
                    let manager = self.manager.clone();
                    let password = self.password.clone();
                    let storage = self.storage.clone();

                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection(stream, client_id.clone(), manager, password, storage).await {
                            error!(client = %client_id, error = %e, "Connection error");
                        }
                    });
                }
                Err(e) => {
                    error!(error = %e, "Failed to accept connection");
                }
            }
        }
    }

    async fn handle_connection(
        mut stream: TcpStream,
        client_id: String,
        manager: Arc<DocumentManager>,
        password: Option<String>,
        storage: Option<Arc<dyn Storage>>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!(client = %client_id, "Client connected");

        let handler = match password {
            Some(pwd) => ConnectionHandler::with_auth(client_id.clone(), manager, pwd),
            None => ConnectionHandler::new(client_id.clone(), manager),
        };
        let mut handler = match storage {
            Some(s) => handler.with_storage(s),
            None => handler,
        };
        let mut buf = vec![0u8; 4096];
        let mut update_rx = handler.subscribe_updates();

        loop {
            tokio::select! {
                // Handle incoming data from client
                result = stream.read(&mut buf) => {
                    match result {
                        Ok(0) => {
                            info!(client = %client_id, "Client disconnected");
                            break;
                        }
                        Ok(n) => {
                            let responses = handler.process(&buf[..n]);
                            for response in responses {
                                let data = response.encode();
                                stream.write_all(&data).await?;

                                // Check for QUIT command
                                if matches!(response, Response::Ok(Some(ref msg)) if msg == "Goodbye") {
                                    handler.cleanup();
                                    return Ok(());
                                }
                            }
                        }
                        Err(e) => {
                            error!(client = %client_id, error = %e, "Read error");
                            break;
                        }
                    }
                }

                // Handle updates for subscriptions
                result = update_rx.recv() => {
                    match result {
                        Ok(delta) => {
                            if handler.matches_subscription(&delta) {
                                let response = Response::delta(delta.version, delta.data);
                                let data = response.encode();
                                if let Err(e) = stream.write_all(&data).await {
                                    error!(client = %client_id, error = %e, "Write error");
                                    break;
                                }
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            warn!(client = %client_id, missed = n, "Client lagged behind updates");
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            break;
                        }
                    }
                }
            }
        }

        handler.cleanup();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncBufReadExt, BufReader};

    #[tokio::test]
    async fn test_tcp_ping_pong() {
        let manager = Arc::new(DocumentManager::new());
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();

        let listener = TcpListener::bind(addr).await.unwrap();
        let bound_addr = listener.local_addr().unwrap();

        // Spawn server handler for one connection
        let manager_clone = manager.clone();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            TcpServer::handle_connection(stream, "test".into(), manager_clone, None, None).await.unwrap();
        });

        // Connect client
        let mut client = TcpStream::connect(bound_addr).await.unwrap();

        // Send PING
        client.write_all(b"PING\r\n").await.unwrap();

        // Read response
        let mut reader = BufReader::new(&mut client);
        let mut response = String::new();
        reader.read_line(&mut response).await.unwrap();

        assert_eq!(response.trim(), "+PONG");

        // Disconnect
        client.write_all(b"QUIT\r\n").await.unwrap();
        drop(client);

        server.await.unwrap();
    }
}
