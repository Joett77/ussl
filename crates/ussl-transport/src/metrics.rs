//! Prometheus metrics for USSL
//!
//! This module provides observability metrics for monitoring USSL servers.
//! Metrics are exposed in Prometheus text format via HTTP.

use prometheus::{
    Counter, CounterVec, Gauge, GaugeVec, Histogram, HistogramOpts, HistogramVec,
    IntCounter, IntCounterVec, IntGauge, IntGaugeVec, Opts, Registry, TextEncoder, Encoder,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tracing::{error, info};

/// USSL metrics collector
#[derive(Clone)]
pub struct Metrics {
    registry: Registry,

    // Connection metrics
    pub connections_total: IntCounterVec,
    pub connections_active: IntGaugeVec,

    // Command metrics
    pub commands_total: IntCounterVec,
    pub commands_errors: IntCounterVec,
    pub command_duration_seconds: HistogramVec,

    // Document metrics
    pub documents_total: IntGauge,
    pub documents_created: IntCounter,
    pub documents_deleted: IntCounter,

    // Data metrics
    pub bytes_received: IntCounter,
    pub bytes_sent: IntCounter,

    // Subscription metrics
    pub subscriptions_active: IntGauge,
    pub updates_published: IntCounter,

    // Rate limiting metrics
    pub rate_limited_requests: IntCounter,

    // Compaction metrics
    pub compactions_total: IntCounter,
    pub compaction_bytes_saved: IntCounter,

    // Backup/Restore metrics
    pub backups_total: IntCounter,
    pub restores_total: IntCounter,
}

impl Metrics {
    /// Create a new metrics collector
    pub fn new() -> Self {
        let registry = Registry::new();

        // Connection metrics
        let connections_total = IntCounterVec::new(
            Opts::new("ussl_connections_total", "Total number of connections"),
            &["transport"]
        ).unwrap();

        let connections_active = IntGaugeVec::new(
            Opts::new("ussl_connections_active", "Number of active connections"),
            &["transport"]
        ).unwrap();

        // Command metrics
        let commands_total = IntCounterVec::new(
            Opts::new("ussl_commands_total", "Total number of commands processed"),
            &["command"]
        ).unwrap();

        let commands_errors = IntCounterVec::new(
            Opts::new("ussl_commands_errors_total", "Total number of command errors"),
            &["command", "error_type"]
        ).unwrap();

        let command_duration_seconds = HistogramVec::new(
            HistogramOpts::new("ussl_command_duration_seconds", "Command processing duration")
                .buckets(vec![0.0001, 0.0005, 0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0]),
            &["command"]
        ).unwrap();

        // Document metrics
        let documents_total = IntGauge::new(
            "ussl_documents_total", "Total number of documents in memory"
        ).unwrap();

        let documents_created = IntCounter::new(
            "ussl_documents_created_total", "Total documents created"
        ).unwrap();

        let documents_deleted = IntCounter::new(
            "ussl_documents_deleted_total", "Total documents deleted"
        ).unwrap();

        // Data metrics
        let bytes_received = IntCounter::new(
            "ussl_bytes_received_total", "Total bytes received from clients"
        ).unwrap();

        let bytes_sent = IntCounter::new(
            "ussl_bytes_sent_total", "Total bytes sent to clients"
        ).unwrap();

        // Subscription metrics
        let subscriptions_active = IntGauge::new(
            "ussl_subscriptions_active", "Number of active subscriptions"
        ).unwrap();

        let updates_published = IntCounter::new(
            "ussl_updates_published_total", "Total updates published to subscribers"
        ).unwrap();

        // Rate limiting metrics
        let rate_limited_requests = IntCounter::new(
            "ussl_rate_limited_requests_total", "Total requests rejected due to rate limiting"
        ).unwrap();

        // Compaction metrics
        let compactions_total = IntCounter::new(
            "ussl_compactions_total", "Total document compactions performed"
        ).unwrap();

        let compaction_bytes_saved = IntCounter::new(
            "ussl_compaction_bytes_saved_total", "Total bytes saved by compaction"
        ).unwrap();

        // Backup/Restore metrics
        let backups_total = IntCounter::new(
            "ussl_backups_total", "Total backups performed"
        ).unwrap();

        let restores_total = IntCounter::new(
            "ussl_restores_total", "Total restores performed"
        ).unwrap();

        // Register all metrics
        registry.register(Box::new(connections_total.clone())).unwrap();
        registry.register(Box::new(connections_active.clone())).unwrap();
        registry.register(Box::new(commands_total.clone())).unwrap();
        registry.register(Box::new(commands_errors.clone())).unwrap();
        registry.register(Box::new(command_duration_seconds.clone())).unwrap();
        registry.register(Box::new(documents_total.clone())).unwrap();
        registry.register(Box::new(documents_created.clone())).unwrap();
        registry.register(Box::new(documents_deleted.clone())).unwrap();
        registry.register(Box::new(bytes_received.clone())).unwrap();
        registry.register(Box::new(bytes_sent.clone())).unwrap();
        registry.register(Box::new(subscriptions_active.clone())).unwrap();
        registry.register(Box::new(updates_published.clone())).unwrap();
        registry.register(Box::new(rate_limited_requests.clone())).unwrap();
        registry.register(Box::new(compactions_total.clone())).unwrap();
        registry.register(Box::new(compaction_bytes_saved.clone())).unwrap();
        registry.register(Box::new(backups_total.clone())).unwrap();
        registry.register(Box::new(restores_total.clone())).unwrap();

        Self {
            registry,
            connections_total,
            connections_active,
            commands_total,
            commands_errors,
            command_duration_seconds,
            documents_total,
            documents_created,
            documents_deleted,
            bytes_received,
            bytes_sent,
            subscriptions_active,
            updates_published,
            rate_limited_requests,
            compactions_total,
            compaction_bytes_saved,
            backups_total,
            restores_total,
        }
    }

    /// Record a new connection
    pub fn record_connection(&self, transport: &str) {
        self.connections_total.with_label_values(&[transport]).inc();
        self.connections_active.with_label_values(&[transport]).inc();
    }

    /// Record a connection closed
    pub fn record_disconnection(&self, transport: &str) {
        self.connections_active.with_label_values(&[transport]).dec();
    }

    /// Record a command execution
    pub fn record_command(&self, command: &str, duration_secs: f64) {
        self.commands_total.with_label_values(&[command]).inc();
        self.command_duration_seconds.with_label_values(&[command]).observe(duration_secs);
    }

    /// Record a command error
    pub fn record_error(&self, command: &str, error_type: &str) {
        self.commands_errors.with_label_values(&[command, error_type]).inc();
    }

    /// Update document count
    pub fn set_document_count(&self, count: i64) {
        self.documents_total.set(count);
    }

    /// Record bytes transferred
    pub fn record_bytes(&self, received: u64, sent: u64) {
        self.bytes_received.inc_by(received);
        self.bytes_sent.inc_by(sent);
    }

    /// Export metrics in Prometheus text format
    pub fn export(&self) -> String {
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer).unwrap();
        String::from_utf8(buffer).unwrap()
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

/// HTTP server for metrics endpoint
pub struct MetricsServer {
    metrics: Arc<Metrics>,
    addr: SocketAddr,
}

impl MetricsServer {
    pub fn new(metrics: Arc<Metrics>, addr: SocketAddr) -> Self {
        Self { metrics, addr }
    }

    /// Run the metrics HTTP server
    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let listener = TcpListener::bind(self.addr).await?;
        info!(addr = %self.addr, "Metrics server listening on http://{}/metrics", self.addr);

        loop {
            match listener.accept().await {
                Ok((mut stream, _)) => {
                    let metrics = self.metrics.clone();

                    tokio::spawn(async move {
                        let mut buf = [0u8; 1024];
                        if let Ok(n) = stream.read(&mut buf).await {
                            if n > 0 {
                                let request = String::from_utf8_lossy(&buf[..n]);

                                // Simple HTTP request parsing
                                if request.starts_with("GET /metrics") || request.starts_with("GET / ") {
                                    let body = metrics.export();
                                    let response = format!(
                                        "HTTP/1.1 200 OK\r\n\
                                         Content-Type: text/plain; version=0.0.4; charset=utf-8\r\n\
                                         Content-Length: {}\r\n\
                                         \r\n\
                                         {}",
                                        body.len(),
                                        body
                                    );
                                    let _ = stream.write_all(response.as_bytes()).await;
                                } else if request.starts_with("GET /health") {
                                    let response = "HTTP/1.1 200 OK\r\n\
                                                   Content-Type: text/plain\r\n\
                                                   Content-Length: 2\r\n\
                                                   \r\n\
                                                   OK";
                                    let _ = stream.write_all(response.as_bytes()).await;
                                } else {
                                    let response = "HTTP/1.1 404 Not Found\r\n\
                                                   Content-Length: 0\r\n\
                                                   \r\n";
                                    let _ = stream.write_all(response.as_bytes()).await;
                                }
                            }
                        }
                    });
                }
                Err(e) => {
                    error!(error = %e, "Failed to accept metrics connection");
                }
            }
        }
    }
}
