//! USSL Transport Layer
//!
//! Provides network transport for USSL:
//! - TCP: Raw TCP connections with USSP protocol
//! - WebSocket: Browser-compatible transport
//! - TLS: Secure connections (optional feature)

pub mod tcp;
#[cfg(feature = "websocket")]
pub mod websocket;
pub mod handler;
#[cfg(feature = "tls")]
pub mod tls;

pub use tcp::TcpServer;
#[cfg(feature = "websocket")]
pub use websocket::WebSocketServer;
pub use handler::ConnectionHandler;
#[cfg(feature = "tls")]
pub use tls::{TlsConfig, TlsError};
