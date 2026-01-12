//! WebSocket transport for USSL

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio_tungstenite::{accept_async, tungstenite::Message, WebSocketStream};
use tracing::{debug, error, info, warn};
use ussl_core::DocumentManager;
use ussl_protocol::Response;
use ussl_storage::Storage;

use crate::handler::ConnectionHandler;
use crate::rate_limit::RateLimitConfig;

#[cfg(feature = "tls")]
use crate::tls::TlsConfig;

/// WebSocket Server for USSL
pub struct WebSocketServer {
    manager: Arc<DocumentManager>,
    addr: SocketAddr,
    client_counter: AtomicU64,
    password: Option<String>,
    storage: Option<Arc<dyn Storage>>,
    rate_limit: Option<RateLimitConfig>,
    #[cfg(feature = "tls")]
    tls_config: Option<TlsConfig>,
}

impl WebSocketServer {
    pub fn new(manager: Arc<DocumentManager>, addr: SocketAddr) -> Self {
        Self {
            manager,
            addr,
            client_counter: AtomicU64::new(0),
            password: None,
            storage: None,
            rate_limit: None,
            #[cfg(feature = "tls")]
            tls_config: None,
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
            rate_limit: None,
            #[cfg(feature = "tls")]
            tls_config: None,
        }
    }

    /// Set the storage backend for persistence
    pub fn with_storage(mut self, storage: Arc<dyn Storage>) -> Self {
        self.storage = Some(storage);
        self
    }

    /// Set rate limiting for connections
    pub fn with_rate_limit(mut self, config: RateLimitConfig) -> Self {
        self.rate_limit = Some(config);
        self
    }

    /// Enable TLS with the given configuration (wss://)
    #[cfg(feature = "tls")]
    pub fn with_tls(mut self, tls_config: TlsConfig) -> Self {
        self.tls_config = Some(tls_config);
        self
    }

    /// Check if TLS is enabled
    #[cfg(feature = "tls")]
    pub fn is_tls_enabled(&self) -> bool {
        self.tls_config.is_some()
    }

    /// Start the WebSocket server
    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let listener = TcpListener::bind(self.addr).await?;

        #[cfg(feature = "tls")]
        let protocol = if self.tls_config.is_some() { "wss" } else { "ws" };
        #[cfg(not(feature = "tls"))]
        let protocol = "ws";

        info!(addr = %self.addr, protocol = protocol, "USSL WebSocket server listening");

        loop {
            match listener.accept().await {
                Ok((stream, peer_addr)) => {
                    let client_id = format!(
                        "{}:{}:{}",
                        protocol,
                        peer_addr,
                        self.client_counter.fetch_add(1, Ordering::Relaxed)
                    );
                    let manager = self.manager.clone();
                    let password = self.password.clone();
                    let storage = self.storage.clone();
                    let rate_limit = self.rate_limit.clone();

                    #[cfg(feature = "tls")]
                    let tls_config = self.tls_config.clone();

                    tokio::spawn(async move {
                        #[cfg(feature = "tls")]
                        {
                            if let Some(tls) = tls_config {
                                // TLS handshake first, then WebSocket upgrade
                                match tls.acceptor().accept(stream).await {
                                    Ok(tls_stream) => {
                                        match accept_async(tls_stream).await {
                                            Ok(ws_stream) => {
                                                if let Err(e) = handle_ws_connection(ws_stream, client_id.clone(), manager, password, storage, rate_limit).await {
                                                    error!(client = %client_id, error = %e, "WSS connection error");
                                                }
                                            }
                                            Err(e) => {
                                                error!(client = %client_id, error = %e, "WebSocket upgrade failed");
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        error!(client = %client_id, error = %e, "TLS handshake failed");
                                    }
                                }
                            } else {
                                match accept_async(stream).await {
                                    Ok(ws_stream) => {
                                        if let Err(e) = handle_ws_connection(ws_stream, client_id.clone(), manager, password, storage, rate_limit).await {
                                            error!(client = %client_id, error = %e, "WebSocket connection error");
                                        }
                                    }
                                    Err(e) => {
                                        error!(client = %client_id, error = %e, "WebSocket upgrade failed");
                                    }
                                }
                            }
                        }

                        #[cfg(not(feature = "tls"))]
                        {
                            match accept_async(stream).await {
                                Ok(ws_stream) => {
                                    if let Err(e) = handle_ws_connection(ws_stream, client_id.clone(), manager, password, storage, rate_limit).await {
                                        error!(client = %client_id, error = %e, "WebSocket connection error");
                                    }
                                }
                                Err(e) => {
                                    error!(client = %client_id, error = %e, "WebSocket upgrade failed");
                                }
                            }
                        }
                    });
                }
                Err(e) => {
                    error!(error = %e, "Failed to accept connection");
                }
            }
        }
    }
}

/// Handle a WebSocket connection with any underlying stream
async fn handle_ws_connection<S>(
    ws_stream: WebSocketStream<S>,
    client_id: String,
    manager: Arc<DocumentManager>,
    password: Option<String>,
    storage: Option<Arc<dyn Storage>>,
    rate_limit: Option<RateLimitConfig>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let (mut write, mut read) = ws_stream.split();

    info!(client = %client_id, "WebSocket client connected");

    let handler = match password {
        Some(pwd) => ConnectionHandler::with_auth(client_id.clone(), manager, pwd),
        None => ConnectionHandler::new(client_id.clone(), manager),
    };
    let handler = match storage {
        Some(s) => handler.with_storage(s),
        None => handler,
    };
    let mut handler = match rate_limit {
        Some(config) => handler.with_rate_limit(config),
        None => handler,
    };
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
