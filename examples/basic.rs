//! Basic USSL Example
//!
//! This example demonstrates basic document operations using the USSL client.
//!
//! Run with: cargo run --example basic

use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

use ussl_core::{DocumentId, DocumentManager, Strategy};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("USSL Basic Example\n");

    // Example 1: Direct API usage (embedded mode)
    println!("=== Embedded Mode ===\n");
    embedded_example().await?;

    // Example 2: TCP client usage
    println!("\n=== TCP Client Mode ===");
    println!("(Start usld first with: cargo run --bin usld)\n");

    // Uncomment to test with running server:
    // tcp_client_example().await?;

    Ok(())
}

async fn embedded_example() -> Result<(), Box<dyn std::error::Error>> {
    let manager = Arc::new(DocumentManager::new());

    // Create a document
    let id = DocumentId::new("user:alice")?;
    let doc = manager.create(id.clone(), Strategy::Lww, None)?;

    // Set values
    doc.set("name", "Alice".into())?;
    doc.set("age", 30i64.into())?;
    doc.set("settings.theme", "dark".into())?;

    // Read values
    println!("Name: {:?}", doc.get(Some("name"))?);
    println!("Age: {:?}", doc.get(Some("age"))?);
    println!("Theme: {:?}", doc.get(Some("settings.theme"))?);
    println!("Full doc: {:?}", doc.get(None)?);

    // Increment counter
    let id2 = DocumentId::new("counter:views")?;
    let counter = manager.create(id2, Strategy::CrdtCounter, None)?;

    counter.increment("total", 1)?;
    counter.increment("total", 5)?;
    println!("\nCounter value: {:?}", counter.get(Some("total"))?);

    // List documents
    println!("\nDocuments:");
    for meta in manager.list(None) {
        println!("  - {} (strategy: {})", meta.id, meta.strategy);
    }

    Ok(())
}

async fn tcp_client_example() -> Result<(), Box<dyn std::error::Error>> {
    let addr: SocketAddr = "127.0.0.1:6380".parse()?;
    let mut stream = TcpStream::connect(addr).await?;
    let (reader, mut writer) = stream.split();
    let mut reader = BufReader::new(reader);

    // Helper to send command and read response
    async fn send_cmd(
        writer: &mut tokio::net::tcp::WriteHalf<'_>,
        reader: &mut BufReader<tokio::net::tcp::ReadHalf<'_>>,
        cmd: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        println!("> {}", cmd);
        writer.write_all(format!("{}\r\n", cmd).as_bytes()).await?;

        let mut response = String::new();
        reader.read_line(&mut response).await?;
        println!("< {}", response.trim());
        Ok(response)
    }

    // PING
    send_cmd(&mut writer, &mut reader, "PING").await?;

    // CREATE document
    send_cmd(&mut writer, &mut reader, "CREATE user:bob STRATEGY lww").await?;

    // SET values
    send_cmd(&mut writer, &mut reader, "SET user:bob name \"Bob\"").await?;
    send_cmd(&mut writer, &mut reader, "SET user:bob email \"bob@example.com\"").await?;

    // GET values
    send_cmd(&mut writer, &mut reader, "GET user:bob").await?;
    send_cmd(&mut writer, &mut reader, "GET user:bob PATH name").await?;

    // Counter
    send_cmd(&mut writer, &mut reader, "INC visits:home count 1").await?;
    send_cmd(&mut writer, &mut reader, "INC visits:home count 1").await?;
    send_cmd(&mut writer, &mut reader, "INC visits:home count 1").await?;

    // List keys
    send_cmd(&mut writer, &mut reader, "KEYS").await?;

    // INFO
    send_cmd(&mut writer, &mut reader, "INFO").await?;

    // QUIT
    send_cmd(&mut writer, &mut reader, "QUIT").await?;

    Ok(())
}
