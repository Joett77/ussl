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
//! # With persistence
//! usld --db /var/lib/ussl/data.db
//!
//! # With authentication
//! usld --password mysecret
//!
//! # With TLS
//! usld --tls-cert /path/to/cert.pem --tls-key /path/to/key.pem
//!
//! # With configuration file
//! usld --config /etc/ussl/config.toml
//! ```

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use tracing::{info, warn, Level};
use tracing_subscriber::FmtSubscriber;

use ussl_core::DocumentManager;
use ussl_storage::SqliteStorage;
use ussl_transport::{RateLimitConfig, TcpServer, TlsConfig, WebSocketServer};

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

    /// SQLite database path for persistence (default: in-memory only)
    #[arg(long, env = "USSL_DB")]
    db: Option<PathBuf>,

    /// Require authentication with this password
    #[arg(long, env = "USSL_PASSWORD")]
    password: Option<String>,

    /// Path to TLS certificate file (PEM format)
    #[arg(long, env = "USSL_TLS_CERT", requires = "tls_key")]
    tls_cert: Option<PathBuf>,

    /// Path to TLS private key file (PEM format)
    #[arg(long, env = "USSL_TLS_KEY", requires = "tls_cert")]
    tls_key: Option<PathBuf>,

    /// Rate limit: max requests per second per client (0 = disabled)
    #[arg(long, env = "USSL_RATE_LIMIT", default_value = "0")]
    rate_limit: u32,

    /// Rate limit burst size (default: 2x rate limit)
    #[arg(long, env = "USSL_RATE_BURST")]
    rate_burst: Option<u32>,
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

    // Initialize SQLite storage if path provided
    let storage = if let Some(db_path) = &args.db {
        info!(path = %db_path.display(), "Initializing SQLite persistence");
        match SqliteStorage::new(db_path) {
            Ok(storage) => {
                let storage = Arc::new(storage);
                info!("SQLite persistence enabled");
                Some(storage)
            }
            Err(e) => {
                warn!(error = %e, "Failed to initialize SQLite, running in-memory only");
                None
            }
        }
    } else {
        info!("Running in-memory only (no --db specified)");
        None
    };

    // Initialize TLS if certificate and key provided
    let tls_config = match (&args.tls_cert, &args.tls_key) {
        (Some(cert_path), Some(key_path)) => {
            info!(cert = %cert_path.display(), key = %key_path.display(), "Loading TLS certificates");
            match TlsConfig::from_pem(cert_path, key_path) {
                Ok(config) => {
                    info!("TLS enabled");
                    Some(config)
                }
                Err(e) => {
                    anyhow::bail!("Failed to load TLS certificates: {}", e);
                }
            }
        }
        _ => None,
    };

    // Initialize rate limiting if configured
    let rate_limit_config = if args.rate_limit > 0 {
        let burst = args.rate_burst.unwrap_or(args.rate_limit * 2);
        info!(rate = args.rate_limit, burst = burst, "Rate limiting enabled");
        Some(RateLimitConfig::new(args.rate_limit, burst))
    } else {
        None
    };

    info!(
        tcp_port = args.tcp_port,
        ws_port = args.ws_port,
        bind = %args.bind,
        tls = tls_config.is_some(),
        rate_limit = args.rate_limit,
        "Starting USSL daemon"
    );

    // Log auth status
    if args.password.is_some() {
        info!("Authentication enabled");
    }

    // Start servers
    let mut handles = Vec::new();

    if !args.no_tcp {
        let tcp_addr: SocketAddr = format!("{}:{}", args.bind, args.tcp_port).parse()?;
        let mut tcp_server = match &args.password {
            Some(pwd) => TcpServer::with_password(manager.clone(), tcp_addr, pwd.clone()),
            None => TcpServer::new(manager.clone(), tcp_addr),
        };
        if let Some(ref s) = storage {
            tcp_server = tcp_server.with_storage(s.clone());
        }
        if let Some(ref tls) = tls_config {
            tcp_server = tcp_server.with_tls(tls.clone());
        }
        if let Some(ref rl) = rate_limit_config {
            tcp_server = tcp_server.with_rate_limit(rl.clone());
        }
        handles.push(tokio::spawn(async move {
            if let Err(e) = tcp_server.run().await {
                tracing::error!(error = %e, "TCP server error");
            }
        }));
    }

    if !args.no_ws {
        let ws_addr: SocketAddr = format!("{}:{}", args.bind, args.ws_port).parse()?;
        let mut ws_server = match &args.password {
            Some(pwd) => WebSocketServer::with_password(manager.clone(), ws_addr, pwd.clone()),
            None => WebSocketServer::new(manager.clone(), ws_addr),
        };
        if let Some(ref s) = storage {
            ws_server = ws_server.with_storage(s.clone());
        }
        if let Some(ref tls) = tls_config {
            ws_server = ws_server.with_tls(tls.clone());
        }
        if let Some(ref rl) = rate_limit_config {
            ws_server = ws_server.with_rate_limit(rl.clone());
        }
        handles.push(tokio::spawn(async move {
            if let Err(e) = ws_server.run().await {
                tracing::error!(error = %e, "WebSocket server error");
            }
        }));
    }

    if handles.is_empty() {
        anyhow::bail!("At least one transport must be enabled");
    }

    // Start background GC task
    let gc_manager = manager.clone();
    handles.push(tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            let removed = gc_manager.gc();
            if removed > 0 {
                tracing::info!(removed = removed, "GC: removed expired documents");
            }
        }
    }));

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
