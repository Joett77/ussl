//! USSL Transport Layer
//!
//! Provides network transport for USSL:
//! - TCP: Raw TCP connections with USSP protocol
//! - WebSocket: Browser-compatible transport
//! - TLS: Secure connections (optional feature)
//! - Metrics: Prometheus metrics (optional feature)

pub mod tcp;
#[cfg(feature = "websocket")]
pub mod websocket;
pub mod handler;
#[cfg(feature = "tls")]
pub mod tls;
pub mod rate_limit;
#[cfg(feature = "metrics")]
pub mod metrics;

pub use tcp::TcpServer;
#[cfg(feature = "websocket")]
pub use websocket::WebSocketServer;
pub use handler::ConnectionHandler;
#[cfg(feature = "tls")]
pub use tls::{TlsConfig, TlsError};
pub use rate_limit::{RateLimiter, RateLimitConfig};
#[cfg(feature = "metrics")]
pub use metrics::{Metrics, MetricsServer};
