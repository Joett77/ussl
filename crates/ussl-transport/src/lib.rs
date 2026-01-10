//! USSL Transport Layer
//!
//! Provides network transport for USSL:
//! - TCP: Raw TCP connections with USSP protocol
//! - WebSocket: Browser-compatible transport

pub mod tcp;
#[cfg(feature = "websocket")]
pub mod websocket;
pub mod handler;

pub use tcp::TcpServer;
#[cfg(feature = "websocket")]
pub use websocket::WebSocketServer;
pub use handler::ConnectionHandler;
