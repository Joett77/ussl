//! WebSocket transport for USSL

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio_tungstenite::{accept_async, tungstenite::Message};
use tracing::{debug, error, info, warn};
use ussl_core::DocumentManager;
use ussl_protocol::Response;

use crate::handler::ConnectionHandler;

/// WebSocket Server for USSL
pub struct WebSocketServer {
    manager: Arc<DocumentManager>,
    addr: SocketAddr,
    client_counter: AtomicU64,
}

impl WebSocketServer {
    pub fn new(manager: Arc<DocumentManager>, addr: SocketAddr) -> Self {
        Self {
            manager,
            addr,
            client_counter: AtomicU64::new(0),
        }
    }

    /// Start the WebSocket server
    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let listener = TcpListener::bind(self.addr).await?;
        info!(addr = %self.addr, "USSL WebSocket server listening");

        loop {
            match listener.accept().await {
                Ok((stream, peer_addr)) => {
                    let client_id = format!(
                        "ws:{}:{}",
                        peer_addr,
                        self.client_counter.fetch_add(1, Ordering::Relaxed)
                    );
                    let manager = self.manager.clone();

                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection(stream, client_id.clone(), manager).await {
                            error!(client = %client_id, error = %e, "WebSocket connection error");
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
        stream: TcpStream,
        client_id: String,
        manager: Arc<DocumentManager>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let ws_stream = accept_async(stream).await?;
        let (mut write, mut read) = ws_stream.split();

        info!(client = %client_id, "WebSocket client connected");

        let mut handler = ConnectionHandler::new(client_id.clone(), manager);
        let mut update_rx = handler.subscribe_updates();

        loop {
            tokio::select! {
                // Handle incoming WebSocket messages
                msg = read.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            let mut data = text.into_bytes();
                            // Ensure line ending for parser
                            if !data.ends_with(b"\n") {
                                data.extend_from_slice(b"\r\n");
                            }

                            let responses = handler.process(&data);
                            for response in responses {
                                let encoded = response.encode();
                                let text = String::from_utf8_lossy(&encoded).to_string();
                                write.send(Message::Text(text.into())).await?;

                                // Check for QUIT
                                if matches!(response, Response::Ok(Some(ref msg)) if msg == "Goodbye") {
                                    handler.cleanup();
                                    return Ok(());
                                }
                            }
                        }
                        Some(Ok(Message::Binary(data))) => {
                            // Handle binary USSP messages
                            let responses = handler.process(&data);
                            for response in responses {
                                let encoded = response.encode();
                                write.send(Message::Binary(encoded.to_vec().into())).await?;
                            }
                        }
                        Some(Ok(Message::Ping(data))) => {
                            write.send(Message::Pong(data)).await?;
                        }
                        Some(Ok(Message::Close(_))) | None => {
                            info!(client = %client_id, "WebSocket client disconnected");
                            break;
                        }
                        Some(Ok(_)) => {
                            // Ignore other message types
                        }
                        Some(Err(e)) => {
                            error!(client = %client_id, error = %e, "WebSocket read error");
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
                                let encoded = response.encode();
                                let text = String::from_utf8_lossy(&encoded).to_string();
                                if let Err(e) = write.send(Message::Text(text.into())).await {
                                    error!(client = %client_id, error = %e, "WebSocket write error");
                                    break;
                                }
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            warn!(client = %client_id, missed = n, "WebSocket client lagged behind updates");
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
