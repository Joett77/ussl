//! USSL Benchmark / Load Test
//!
//! This benchmark tests USSL performance under various loads.
//!
//! Run with: cargo run --example benchmark --release
//!
//! Make sure usld is running: cargo run --bin usld --release

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::Barrier;

/// Benchmark configuration
struct BenchConfig {
    /// Server address
    addr: SocketAddr,
    /// Number of concurrent clients
    clients: usize,
    /// Operations per client
    ops_per_client: usize,
    /// Password (if auth enabled)
    password: Option<String>,
}

/// Benchmark results
#[derive(Debug)]
struct BenchResults {
    name: String,
    total_ops: u64,
    duration: Duration,
    successful: u64,
    failed: u64,
    ops_per_sec: f64,
    avg_latency_us: f64,
}

impl BenchResults {
    fn print(&self) {
        println!("\n╔══════════════════════════════════════════════════════════╗");
        println!("║  {} ", self.name);
        println!("╠══════════════════════════════════════════════════════════╣");
        println!("║  Total operations:    {:>10}                         ║", self.total_ops);
        println!("║  Successful:          {:>10}                         ║", self.successful);
        println!("║  Failed:              {:>10}                         ║", self.failed);
        println!("║  Duration:            {:>10.2?}                       ║", self.duration);
        println!("║  Throughput:          {:>10.0} ops/sec                ║", self.ops_per_sec);
        println!("║  Avg latency:         {:>10.0} µs                     ║", self.avg_latency_us);
        println!("╚══════════════════════════════════════════════════════════╝");
    }
}

/// TCP client for benchmarking
struct BenchClient {
    reader: BufReader<tokio::net::tcp::OwnedReadHalf>,
    writer: tokio::net::tcp::OwnedWriteHalf,
}

impl BenchClient {
    async fn connect(addr: SocketAddr) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let stream = TcpStream::connect(addr).await?;
        let (reader, writer) = stream.into_split();
        Ok(Self {
            reader: BufReader::new(reader),
            writer,
        })
    }

    async fn send(&mut self, cmd: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        self.writer.write_all(format!("{}\r\n", cmd).as_bytes()).await?;
        let mut response = String::new();
        self.reader.read_line(&mut response).await?;
        Ok(response)
    }

    async fn auth(&mut self, password: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let resp = self.send(&format!("AUTH {}", password)).await?;
        if !resp.starts_with("+OK") {
            return Err(format!("Auth failed: {}", resp).into());
        }
        Ok(())
    }
}

/// Benchmark: SET operations
async fn bench_set(config: &BenchConfig) -> BenchResults {
    let barrier = Arc::new(Barrier::new(config.clients));
    let successful = Arc::new(AtomicU64::new(0));
    let failed = Arc::new(AtomicU64::new(0));
    let total_latency_ns = Arc::new(AtomicU64::new(0));

    let mut handles = vec![];

    for client_id in 0..config.clients {
        let addr = config.addr;
        let ops = config.ops_per_client;
        let barrier = barrier.clone();
        let successful = successful.clone();
        let failed = failed.clone();
        let total_latency = total_latency_ns.clone();
        let password = config.password.clone();

        handles.push(tokio::spawn(async move {
            let mut client = match BenchClient::connect(addr).await {
                Ok(c) => c,
                Err(_) => return,
            };

            // Auth if needed
            if let Some(ref pass) = password {
                if client.auth(pass).await.is_err() {
                    return;
                }
            }

            // Create document for this client
            let doc_id = format!("bench:set:{}", client_id);
            let _ = client.send(&format!("CREATE {} STRATEGY lww", doc_id)).await;

            // Wait for all clients to be ready
            barrier.wait().await;

            // Perform operations
            for i in 0..ops {
                let start = Instant::now();
                let result = client.send(&format!("SET {} key{} \"value{}\"", doc_id, i, i)).await;
                let elapsed = start.elapsed();

                match result {
                    Ok(resp) if resp.starts_with("+OK") => {
                        successful.fetch_add(1, Ordering::Relaxed);
                        total_latency.fetch_add(elapsed.as_nanos() as u64, Ordering::Relaxed);
                    }
                    _ => {
                        failed.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        }));
    }

    let start = Instant::now();

    // Wait for barrier release
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Wait for all clients to finish
    for handle in handles {
        let _ = handle.await;
    }

    let duration = start.elapsed();
    let total_ops = (config.clients * config.ops_per_client) as u64;
    let succ = successful.load(Ordering::Relaxed);
    let fail = failed.load(Ordering::Relaxed);
    let total_lat = total_latency_ns.load(Ordering::Relaxed);

    BenchResults {
        name: format!("SET Benchmark ({} clients × {} ops)", config.clients, config.ops_per_client),
        total_ops,
        duration,
        successful: succ,
        failed: fail,
        ops_per_sec: succ as f64 / duration.as_secs_f64(),
        avg_latency_us: if succ > 0 { (total_lat as f64 / succ as f64) / 1000.0 } else { 0.0 },
    }
}

/// Benchmark: GET operations
async fn bench_get(config: &BenchConfig) -> BenchResults {
    let barrier = Arc::new(Barrier::new(config.clients));
    let successful = Arc::new(AtomicU64::new(0));
    let failed = Arc::new(AtomicU64::new(0));
    let total_latency_ns = Arc::new(AtomicU64::new(0));

    let mut handles = vec![];

    for client_id in 0..config.clients {
        let addr = config.addr;
        let ops = config.ops_per_client;
        let barrier = barrier.clone();
        let successful = successful.clone();
        let failed = failed.clone();
        let total_latency = total_latency_ns.clone();
        let password = config.password.clone();

        handles.push(tokio::spawn(async move {
            let mut client = match BenchClient::connect(addr).await {
                Ok(c) => c,
                Err(_) => return,
            };

            // Auth if needed
            if let Some(ref pass) = password {
                if client.auth(pass).await.is_err() {
                    return;
                }
            }

            // Create and populate document
            let doc_id = format!("bench:get:{}", client_id);
            let _ = client.send(&format!("CREATE {} STRATEGY lww", doc_id)).await;
            let _ = client.send(&format!("SET {} data \"test-value\"", doc_id)).await;

            // Wait for all clients to be ready
            barrier.wait().await;

            // Perform operations
            for _ in 0..ops {
                let start = Instant::now();
                let result = client.send(&format!("GET {}", doc_id)).await;
                let elapsed = start.elapsed();

                match result {
                    Ok(resp) if !resp.starts_with("-ERR") => {
                        successful.fetch_add(1, Ordering::Relaxed);
                        total_latency.fetch_add(elapsed.as_nanos() as u64, Ordering::Relaxed);
                    }
                    _ => {
                        failed.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        }));
    }

    let start = Instant::now();
    tokio::time::sleep(Duration::from_millis(100)).await;

    for handle in handles {
        let _ = handle.await;
    }

    let duration = start.elapsed();
    let total_ops = (config.clients * config.ops_per_client) as u64;
    let succ = successful.load(Ordering::Relaxed);
    let fail = failed.load(Ordering::Relaxed);
    let total_lat = total_latency_ns.load(Ordering::Relaxed);

    BenchResults {
        name: format!("GET Benchmark ({} clients × {} ops)", config.clients, config.ops_per_client),
        total_ops,
        duration,
        successful: succ,
        failed: fail,
        ops_per_sec: succ as f64 / duration.as_secs_f64(),
        avg_latency_us: if succ > 0 { (total_lat as f64 / succ as f64) / 1000.0 } else { 0.0 },
    }
}

/// Benchmark: INC (counter) operations
async fn bench_inc(config: &BenchConfig) -> BenchResults {
    let barrier = Arc::new(Barrier::new(config.clients));
    let successful = Arc::new(AtomicU64::new(0));
    let failed = Arc::new(AtomicU64::new(0));
    let total_latency_ns = Arc::new(AtomicU64::new(0));

    let mut handles = vec![];

    for client_id in 0..config.clients {
        let addr = config.addr;
        let ops = config.ops_per_client;
        let barrier = barrier.clone();
        let successful = successful.clone();
        let failed = failed.clone();
        let total_latency = total_latency_ns.clone();
        let password = config.password.clone();

        handles.push(tokio::spawn(async move {
            let mut client = match BenchClient::connect(addr).await {
                Ok(c) => c,
                Err(_) => return,
            };

            // Auth if needed
            if let Some(ref pass) = password {
                if client.auth(pass).await.is_err() {
                    return;
                }
            }

            // Create counter document
            let doc_id = format!("bench:inc:{}", client_id);
            let _ = client.send(&format!("CREATE {} STRATEGY crdt-counter", doc_id)).await;

            // Wait for all clients to be ready
            barrier.wait().await;

            // Perform operations
            for _ in 0..ops {
                let start = Instant::now();
                let result = client.send(&format!("INC {} counter 1", doc_id)).await;
                let elapsed = start.elapsed();

                match result {
                    Ok(resp) if resp.starts_with(':') || resp.starts_with("+OK") => {
                        successful.fetch_add(1, Ordering::Relaxed);
                        total_latency.fetch_add(elapsed.as_nanos() as u64, Ordering::Relaxed);
                    }
                    _ => {
                        failed.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        }));
    }

    let start = Instant::now();
    tokio::time::sleep(Duration::from_millis(100)).await;

    for handle in handles {
        let _ = handle.await;
    }

    let duration = start.elapsed();
    let total_ops = (config.clients * config.ops_per_client) as u64;
    let succ = successful.load(Ordering::Relaxed);
    let fail = failed.load(Ordering::Relaxed);
    let total_lat = total_latency_ns.load(Ordering::Relaxed);

    BenchResults {
        name: format!("INC Benchmark ({} clients × {} ops)", config.clients, config.ops_per_client),
        total_ops,
        duration,
        successful: succ,
        failed: fail,
        ops_per_sec: succ as f64 / duration.as_secs_f64(),
        avg_latency_us: if succ > 0 { (total_lat as f64 / succ as f64) / 1000.0 } else { 0.0 },
    }
}

/// Benchmark: Mixed workload
async fn bench_mixed(config: &BenchConfig) -> BenchResults {
    let barrier = Arc::new(Barrier::new(config.clients));
    let successful = Arc::new(AtomicU64::new(0));
    let failed = Arc::new(AtomicU64::new(0));
    let total_latency_ns = Arc::new(AtomicU64::new(0));

    let mut handles = vec![];

    for client_id in 0..config.clients {
        let addr = config.addr;
        let ops = config.ops_per_client;
        let barrier = barrier.clone();
        let successful = successful.clone();
        let failed = failed.clone();
        let total_latency = total_latency_ns.clone();
        let password = config.password.clone();

        handles.push(tokio::spawn(async move {
            let mut client = match BenchClient::connect(addr).await {
                Ok(c) => c,
                Err(_) => return,
            };

            // Auth if needed
            if let Some(ref pass) = password {
                if client.auth(pass).await.is_err() {
                    return;
                }
            }

            // Create documents
            let lww_id = format!("bench:mixed:lww:{}", client_id);
            let counter_id = format!("bench:mixed:counter:{}", client_id);
            let _ = client.send(&format!("CREATE {} STRATEGY lww", lww_id)).await;
            let _ = client.send(&format!("CREATE {} STRATEGY crdt-counter", counter_id)).await;

            // Wait for all clients to be ready
            barrier.wait().await;

            // Perform mixed operations: 40% SET, 40% GET, 20% INC
            for i in 0..ops {
                let start = Instant::now();
                let result = match i % 5 {
                    0 | 1 => client.send(&format!("SET {} key{} \"value{}\"", lww_id, i, i)).await,
                    2 | 3 => client.send(&format!("GET {}", lww_id)).await,
                    _ => client.send(&format!("INC {} counter 1", counter_id)).await,
                };
                let elapsed = start.elapsed();

                match result {
                    Ok(resp) if !resp.starts_with("-ERR") => {
                        successful.fetch_add(1, Ordering::Relaxed);
                        total_latency.fetch_add(elapsed.as_nanos() as u64, Ordering::Relaxed);
                    }
                    _ => {
                        failed.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        }));
    }

    let start = Instant::now();
    tokio::time::sleep(Duration::from_millis(100)).await;

    for handle in handles {
        let _ = handle.await;
    }

    let duration = start.elapsed();
    let total_ops = (config.clients * config.ops_per_client) as u64;
    let succ = successful.load(Ordering::Relaxed);
    let fail = failed.load(Ordering::Relaxed);
    let total_lat = total_latency_ns.load(Ordering::Relaxed);

    BenchResults {
        name: format!("MIXED Benchmark ({} clients × {} ops)", config.clients, config.ops_per_client),
        total_ops,
        duration,
        successful: succ,
        failed: fail,
        ops_per_sec: succ as f64 / duration.as_secs_f64(),
        avg_latency_us: if succ > 0 { (total_lat as f64 / succ as f64) / 1000.0 } else { 0.0 },
    }
}

/// Test connection capacity
async fn bench_connections(addr: SocketAddr, max_clients: usize, password: Option<String>) -> BenchResults {
    let start = Instant::now();
    let mut handles = vec![];
    let successful = Arc::new(AtomicU64::new(0));
    let failed = Arc::new(AtomicU64::new(0));

    for _ in 0..max_clients {
        let addr = addr;
        let successful = successful.clone();
        let failed = failed.clone();
        let password = password.clone();

        handles.push(tokio::spawn(async move {
            match BenchClient::connect(addr).await {
                Ok(mut client) => {
                    // Auth if needed
                    if let Some(ref pass) = password {
                        if client.auth(pass).await.is_err() {
                            failed.fetch_add(1, Ordering::Relaxed);
                            return;
                        }
                    }
                    // Send PING
                    if let Ok(resp) = client.send("PING").await {
                        if resp.contains("PONG") {
                            successful.fetch_add(1, Ordering::Relaxed);
                            return;
                        }
                    }
                    failed.fetch_add(1, Ordering::Relaxed);
                }
                Err(_) => {
                    failed.fetch_add(1, Ordering::Relaxed);
                }
            }
        }));
    }

    for handle in handles {
        let _ = handle.await;
    }

    let duration = start.elapsed();
    let succ = successful.load(Ordering::Relaxed);
    let fail = failed.load(Ordering::Relaxed);

    BenchResults {
        name: format!("Connection Benchmark ({} clients)", max_clients),
        total_ops: max_clients as u64,
        duration,
        successful: succ,
        failed: fail,
        ops_per_sec: succ as f64 / duration.as_secs_f64(),
        avg_latency_us: (duration.as_micros() as f64) / max_clients as f64,
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!();
    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║           USSL BENCHMARK / LOAD TEST                     ║");
    println!("╚══════════════════════════════════════════════════════════╝");
    println!();

    // Parse command line args
    let args: Vec<String> = std::env::args().collect();
    let host = args.iter()
        .position(|a| a == "-H" || a == "--host")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str())
        .unwrap_or("127.0.0.1");

    let port: u16 = args.iter()
        .position(|a| a == "-p" || a == "--port")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(6380);

    let password = args.iter()
        .position(|a| a == "-a" || a == "--password")
        .and_then(|i| args.get(i + 1))
        .cloned();

    let clients: usize = args.iter()
        .position(|a| a == "-c" || a == "--clients")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(10);

    let ops: usize = args.iter()
        .position(|a| a == "-n" || a == "--ops")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(1000);

    let addr: SocketAddr = format!("{}:{}", host, port).parse()?;

    println!("Configuration:");
    println!("  Server:     {}:{}", host, port);
    println!("  Clients:    {}", clients);
    println!("  Ops/client: {}", ops);
    println!("  Auth:       {}", if password.is_some() { "enabled" } else { "disabled" });
    println!();

    // Check connectivity
    print!("Connecting to server... ");
    match BenchClient::connect(addr).await {
        Ok(mut client) => {
            if let Some(ref pass) = password {
                if let Err(e) = client.auth(pass).await {
                    println!("AUTH FAILED: {}", e);
                    return Ok(());
                }
            }
            match client.send("PING").await {
                Ok(resp) if resp.contains("PONG") => {
                    println!("OK");
                }
                Ok(_) => {
                    println!("ERROR: Unexpected response");
                    return Ok(());
                }
                Err(e) => {
                    println!("ERROR: {}", e);
                    return Ok(());
                }
            }
        }
        Err(e) => {
            println!("FAILED");
            println!("\nError: {}", e);
            println!("\nMake sure usld is running:");
            println!("  cargo run --bin usld --release");
            return Ok(());
        }
    }

    let config = BenchConfig {
        addr,
        clients,
        ops_per_client: ops,
        password: password.clone(),
    };

    // Run benchmarks
    println!("\nRunning benchmarks...");

    // 1. Connection benchmark
    bench_connections(addr, clients * 2, password.clone()).await.print();

    // 2. SET benchmark
    bench_set(&config).await.print();

    // 3. GET benchmark
    bench_get(&config).await.print();

    // 4. INC benchmark
    bench_inc(&config).await.print();

    // 5. Mixed workload
    bench_mixed(&config).await.print();

    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║                    BENCHMARK COMPLETE                    ║");
    println!("╚══════════════════════════════════════════════════════════╝");
    println!();

    // Cleanup: delete benchmark documents
    if let Ok(mut client) = BenchClient::connect(addr).await {
        if let Some(ref pass) = password {
            let _ = client.auth(pass).await;
        }
        // Best effort cleanup
        for i in 0..clients {
            let _ = client.send(&format!("DEL bench:set:{}", i)).await;
            let _ = client.send(&format!("DEL bench:get:{}", i)).await;
            let _ = client.send(&format!("DEL bench:inc:{}", i)).await;
            let _ = client.send(&format!("DEL bench:mixed:lww:{}", i)).await;
            let _ = client.send(&format!("DEL bench:mixed:counter:{}", i)).await;
        }
    }

    Ok(())
}
