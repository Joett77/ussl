//! USSL CLI Client
//!
//! Interactive command-line client for USSL servers.
//!
//! # Usage
//!
//! ```bash
//! # Connect to local server
//! ussl
//!
//! # Connect to remote server
//! ussl --host example.com --port 6380
//!
//! # Execute single command
//! ussl -c "GET user:123"
//! ```

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;

use anyhow::{Context, Result};
use clap::Parser;
use colored::*;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

/// USSL Command Line Interface
#[derive(Parser, Debug)]
#[command(name = "ussl")]
#[command(author, version, about = "USSL CLI - Universal State Sync Layer client")]
struct Args {
    /// Server hostname
    #[arg(short = 'H', long, default_value = "127.0.0.1", env = "USSL_HOST")]
    host: String,

    /// Server port
    #[arg(short, long, default_value = "6380", env = "USSL_PORT")]
    port: u16,

    /// Password for authentication
    #[arg(short = 'a', long, env = "USSL_PASSWORD")]
    password: Option<String>,

    /// Execute command and exit
    #[arg(short, long)]
    command: Option<String>,

    /// Quiet mode (no banner)
    #[arg(short, long)]
    quiet: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let addr = format!("{}:{}", args.host, args.port);

    // Connect
    let mut stream = TcpStream::connect(&addr)
        .with_context(|| format!("Failed to connect to {}", addr))?;

    stream.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;

    // Authenticate if password provided
    if let Some(ref password) = args.password {
        execute_command(&mut stream, &format!("AUTH {}", password))
            .with_context(|| "Authentication failed")?;
        if !args.quiet {
            println!("{}", "Authenticated.".green());
        }
    }

    if !args.quiet {
        let auth_status = if args.password.is_some() { " (authenticated)" } else { "" };
        println!(
            "{}",
            format!(
                r#"
  ╦ ╦╔═╗╔═╗╦    CLI
  ║ ║╚═╗╚═╗║    Connected to {}{}
  ╚═╝╚═╝╚═╝╩═╝  Type 'help' for commands, 'quit' to exit
"#,
                addr, auth_status
            )
            .cyan()
        );
    }

    // Single command mode
    if let Some(cmd) = args.command {
        return execute_command(&mut stream, &cmd);
    }

    // Interactive mode
    let mut rl = DefaultEditor::new()?;
    let history_path = dirs_next::home_dir()
        .map(|p| p.join(".ussl_history"))
        .unwrap_or_default();

    let _ = rl.load_history(&history_path);

    loop {
        let prompt = format!("{}> ", "ussl".green());
        match rl.readline(&prompt) {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                let _ = rl.add_history_entry(line);

                // Handle local commands
                match line.to_uppercase().as_str() {
                    "QUIT" | "EXIT" => {
                        let _ = execute_command(&mut stream, "QUIT");
                        break;
                    }
                    "HELP" => {
                        print_help();
                        continue;
                    }
                    "CLEAR" => {
                        print!("\x1B[2J\x1B[1;1H");
                        continue;
                    }
                    _ => {}
                }

                // Execute remote command
                if let Err(e) = execute_command(&mut stream, line) {
                    eprintln!("{} {}", "Error:".red(), e);

                    // Try to reconnect
                    if let Ok(new_stream) = TcpStream::connect(&addr) {
                        stream = new_stream;
                        stream.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;
                        println!("{}", "Reconnected.".yellow());
                    } else {
                        eprintln!("{}", "Connection lost.".red());
                        break;
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("^C");
                continue;
            }
            Err(ReadlineError::Eof) => {
                println!("^D");
                break;
            }
            Err(err) => {
                eprintln!("Error: {:?}", err);
                break;
            }
        }
    }

    let _ = rl.save_history(&history_path);
    Ok(())
}

fn execute_command(stream: &mut TcpStream, cmd: &str) -> Result<()> {
    // Send command
    writeln!(stream, "{}", cmd)?;
    stream.flush()?;

    // Read response
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut response = String::new();
    reader.read_line(&mut response)?;

    // Parse and display response
    let response = response.trim();

    if response.starts_with("+OK") {
        println!("{}", response.green());
    } else if response.starts_with("+PONG") {
        println!("{}", "PONG".green());
    } else if response.starts_with("-ERR") {
        println!("{}", response.red());
    } else if response.starts_with(':') {
        // Integer
        println!("{}", response[1..].yellow());
    } else if response.starts_with('$') {
        // Bulk string
        if response == "$-1" {
            println!("{}", "(nil)".dimmed());
        } else {
            // Read the data line
            let mut data = String::new();
            reader.read_line(&mut data)?;
            println!("{}", data.trim());
        }
    } else if response.starts_with('*') {
        // Array
        let count: usize = response[1..].parse().unwrap_or(0);
        for i in 0..count {
            let mut line = String::new();
            reader.read_line(&mut line)?;
            println!("{}) {}", i + 1, line.trim());
        }
    } else if response.starts_with('#') {
        // Delta
        println!("{}", format!("Delta: {}", response).blue());
    } else {
        println!("{}", response);
    }

    Ok(())
}

fn print_help() {
    println!(
        r#"
{}

{}
  AUTH <password>                        Authenticate with server

{}
  CREATE <id> [STRATEGY <s>] [TTL <ms>]  Create a new document
  GET <id> [PATH <path>]                 Get document or path value
  SET <id> <path> <value>                Set value at path
  DEL <id> [PATH <path>]                 Delete document or path
  KEYS [pattern]                         List document IDs

{}
  SUB <pattern>                          Subscribe to document changes
  UNSUB <pattern>                        Unsubscribe from changes

{}
  PUSH <id> <path> <value>               Append value to array
  INC <id> <path> [delta]                Increment counter (default: 1)

{}
  PRESENCE <id> [DATA <json>]            Get/set presence info

{}
  PING                                   Check connection
  INFO                                   Server information
  QUIT                                   Close connection

{}
  help                                   Show this help
  clear                                  Clear screen
  quit/exit                              Exit CLI

{}
  lww          Last-Writer-Wins (default)
  crdt-counter Convergent counter
  crdt-set     Add/Remove set
  crdt-map     Nested map with LWW per key
  crdt-text    Collaborative text editing
"#,
        "USSL Commands".cyan().bold(),
        "Authentication".yellow().bold(),
        "Documents".yellow().bold(),
        "Subscriptions".yellow().bold(),
        "Operations".yellow().bold(),
        "Presence".yellow().bold(),
        "Server".yellow().bold(),
        "Local".yellow().bold(),
        "Strategies".yellow().bold(),
    );
}

// Minimal dirs_next replacement for home directory
mod dirs_next {
    use std::path::PathBuf;

    pub fn home_dir() -> Option<PathBuf> {
        std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
    }
}
