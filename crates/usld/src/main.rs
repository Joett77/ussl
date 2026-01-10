//! USSL Daemon (usld)
//!
//! The main server process for USSL - Universal State Synchronization Layer.
//!
//! # Usage
//!
//! ```bash
//! # Start with defaults (TCP on 6380, WebSocket on 6381)
//! usld
//!
//! # Custom ports
//! usld --tcp-port 7000 --ws-port 7001
//!
//! # With configuration file
//! usld --config /etc/ussl/config.toml
//! ```

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

use ussl_core::DocumentManager;
use ussl_transport::{TcpServer, WebSocketServer};

/// USSL Daemon - Universal State Synchronization Layer
#[derive(Parser, Debug)]
#[command(name = "usld")]
#[command(author, version, about, long_about = None)]
struct Args {
    /// TCP port to listen on
    #[arg(long, env = "USSL_TCP_PORT", default_value = "6380")]
    tcp_port: u16,

    /// WebSocket port to listen on
    #[arg(long, env = "USSL_WS_PORT", default_value = "6381")]
    ws_port: u16,

    /// Bind address
    #[arg(long, env = "USSL_BIND", default_value = "0.0.0.0")]
    bind: String,

    /// Configuration file path
    #[arg(short, long, env = "USSL_CONFIG")]
    config: Option<PathBuf>,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, env = "USSL_LOG_LEVEL", default_value = "info")]
    log_level: String,

    /// Disable TCP server
    #[arg(long)]
    no_tcp: bool,

    /// Disable WebSocket server
    #[arg(long)]
    no_ws: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging
    let level = match args.log_level.to_lowercase().as_str() {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "info" => Level::INFO,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    };

    let subscriber = FmtSubscriber::builder()
        .with_max_level(level)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .init();

    // Print banner
    print_banner();

    // Create shared document manager
    let manager = Arc::new(DocumentManager::new());

    info!(
        tcp_port = args.tcp_port,
        ws_port = args.ws_port,
        bind = %args.bind,
        "Starting USSL daemon"
    );

    // Start servers
    let mut handles = Vec::new();

    if !args.no_tcp {
        let tcp_addr: SocketAddr = format!("{}:{}", args.bind, args.tcp_port).parse()?;
        let tcp_server = TcpServer::new(manager.clone(), tcp_addr);
        handles.push(tokio::spawn(async move {
            if let Err(e) = tcp_server.run().await {
                tracing::error!(error = %e, "TCP server error");
            }
        }));
    }

    if !args.no_ws {
        let ws_addr: SocketAddr = format!("{}:{}", args.bind, args.ws_port).parse()?;
        let ws_server = WebSocketServer::new(manager.clone(), ws_addr);
        handles.push(tokio::spawn(async move {
            if let Err(e) = ws_server.run().await {
                tracing::error!(error = %e, "WebSocket server error");
            }
        }));
    }

    if handles.is_empty() {
        anyhow::bail!("At least one transport must be enabled");
    }

    // Wait for shutdown signal
    tokio::signal::ctrl_c().await?;
    info!("Shutting down...");

    Ok(())
}

fn print_banner() {
    println!(
        r#"
  ╦ ╦╔═╗╔═╗╦
  ║ ║╚═╗╚═╗║
  ╚═╝╚═╝╚═╝╩═╝
  Universal State Synchronization Layer
  Version {}
"#,
        env!("CARGO_PKG_VERSION")
    );
}
