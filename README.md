# USSL - Universal State Synchronization Layer

> An open-source infrastructure primitive for state synchronization across distributed systems.

[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE)

## Overview

USSL solves one of the most pervasive problems in modern distributed systems: **keeping state synchronized** across services, clients, and devices with automatic conflict resolution, offline support, and configurable consistency guarantees.

Just as Redis became the universal solution for in-memory caching, USSL aims to become the universal solution for **state synchronization**.

## Features

- 🔄 **CRDT-based sync** - Automatic conflict resolution with multiple strategies
- 📡 **Real-time updates** - Subscribe to changes with delta updates
- 📴 **Offline support** - Queue changes locally, sync when reconnected
- 🔌 **Pluggable storage** - Memory, SQLite, PostgreSQL
- 🌐 **Multiple transports** - TCP and WebSocket
- 🪶 **Lightweight** - Single binary, zero-config start
- 🦀 **Built in Rust** - Fast, safe, and WASM-compatible

## Quick Start

### Start the Server

```bash
# Build and run (in-memory only)
cargo run --bin usld

# With SQLite persistence
cargo run --bin usld -- --db /var/lib/ussl/data.db

# With authentication
cargo run --bin usld -- --password mysecretpassword

# With both persistence and auth
cargo run --bin usld -- --db ./data.db --password secret123

# Custom ports
cargo run --bin usld -- --tcp-port 7000 --ws-port 7001
```

### Connect with TCP (Redis-like protocol)

```bash
# Using netcat or telnet
nc localhost 6380

# If auth is enabled, authenticate first
AUTH mysecretpassword

# Commands
PING
CREATE user:123 STRATEGY lww
SET user:123 name "Alice"
GET user:123
INC counter:views count 1
SUB user:*
QUIT
```

### Connect with ussl CLI

```bash
# Install
cargo install --path crates/ussl-cli

# Connect to local server
ussl

# Connect with authentication
ussl -a mysecretpassword

# Connect to remote server
ussl -H example.com -p 6380 -a secret

# Execute single command
ussl -c "PING"
ussl -a secret -c "GET user:123"

# Environment variables also work
USSL_PASSWORD=secret ussl -c "KEYS *"
```

**Interactive session:**
```
ussl> PING
PONG
ussl> SET user:1 name "Alice"
+OK
ussl> GET user:1
{"name": "Alice"}
ussl> INC counter:views count 1
1
ussl> KEYS *
1) user:1
2) counter:views
ussl> quit
```

### Connect with JavaScript

```typescript
import { USSL } from '@ussl/client';

const client = await USSL.connect('ws://localhost:6381');

// Get or create a document
const doc = client.doc('user:123', { strategy: 'lww' });

// Set values
await doc.set('name', 'Alice');
await doc.set('preferences.theme', 'dark');

// Subscribe to changes
doc.subscribe((value) => {
  console.log('Document updated:', value);
});

// Increment counters
const views = await doc.increment('views', 1);

// Presence
client.presence.set('doc:123', { cursor: { x: 100, y: 200 } });
```

## Protocol (USSP)

USSL uses a simple text-based protocol inspired by Redis:

| Command | Syntax | Description |
|---------|--------|-------------|
| `AUTH` | `AUTH <password>` | Authenticate (required if server has --password) |
| `CREATE` | `CREATE <id> [STRATEGY <s>] [TTL <ms>]` | Create document |
| `GET` | `GET <id> [PATH <path>]` | Get document/path |
| `SET` | `SET <id> <path> <value>` | Set value |
| `DEL` | `DEL <id> [PATH <path>]` | Delete document/path |
| `SUB` | `SUB <pattern>` | Subscribe to changes |
| `UNSUB` | `UNSUB <pattern>` | Unsubscribe |
| `PUSH` | `PUSH <id> <path> <value>` | Append to array |
| `INC` | `INC <id> <path> <delta>` | Increment counter |
| `PRESENCE` | `PRESENCE <id> [DATA <json>]` | Set/get presence |
| `PING` | `PING` | Health check (always allowed) |
| `KEYS` | `KEYS [pattern]` | List documents |
| `INFO` | `INFO` | Server info |
| `QUIT` | `QUIT` | Close connection (always allowed) |

### Conflict Resolution Strategies

| Strategy | Description | Use Case |
|----------|-------------|----------|
| `lww` | Last-Writer-Wins | Simple key-value data |
| `crdt-counter` | Convergent counter | Metrics, inventory |
| `crdt-set` | Add/Remove set | Tags, memberships |
| `crdt-map` | Nested map with LWW | User preferences |
| `crdt-text` | Collaborative text | Documents, notes |

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        USSL Core                            │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │ CRDT Engine │  │  Protocol   │  │  Storage Adapters   │  │
│  │   (yrs)     │  │   (USSP)    │  │ memory/sqlite/pg    │  │
│  └──────┬──────┘  └──────┬──────┘  └──────────┬──────────┘  │
│         └────────────────┼────────────────────┘             │
│                          │                                  │
│         ┌────────────────┴────────────────┐                 │
│         │       Document Manager          │                 │
│         │ (subscriptions, presence, sync) │                 │
│         └────────────────┬────────────────┘                 │
├──────────────────────────┼──────────────────────────────────┤
│  Transport Layer:    TCP │ WebSocket │ QUIC (planned)       │
└──────────────────────────────────────────────────────────────┘
```

## Project Structure

```
ussl/
├── crates/
│   ├── ussl-core/       # CRDT engine, document management
│   ├── ussl-protocol/   # USSP parser and serialization
│   ├── ussl-storage/    # Storage backends
│   ├── ussl-transport/  # TCP and WebSocket servers
│   └── usld/            # Server daemon binary
├── sdks/
│   └── js/              # JavaScript/TypeScript SDK
└── examples/
    ├── basic.rs         # Basic Rust usage
    └── collaborative.html  # Browser demo
```

## Installation

### Debian/Ubuntu (APT)

**Quick install (one-liner):**
```bash
curl -fsSL https://joett77.github.io/ussl/install.sh | bash
```

**Manual APT repository setup:**
```bash
# Add GPG key
curl -fsSL https://joett77.github.io/ussl/KEY.gpg | sudo gpg --dearmor -o /usr/share/keyrings/ussl-archive-keyring.gpg

# Add repository
echo "deb [signed-by=/usr/share/keyrings/ussl-archive-keyring.gpg] https://joett77.github.io/ussl stable main" | sudo tee /etc/apt/sources.list.d/ussl.list

# Install
sudo apt-get update
sudo apt-get install usld ussl-cli

# Start the service
sudo systemctl enable usld
sudo systemctl start usld
```

**Alternative: Download .deb directly**
```bash
# From GitHub Releases
wget https://github.com/Joett77/ussl/releases/latest/download/usld_0.2.0_amd64.deb
sudo dpkg -i usld_0.2.0_amd64.deb
```

**Configuration file:** `/etc/ussl/ussl.toml`

**Systemd commands:**
```bash
sudo systemctl start usld      # Start server
sudo systemctl stop usld       # Stop server
sudo systemctl restart usld    # Restart server
sudo systemctl status usld     # Check status
sudo journalctl -u usld -f     # View logs
```

**Default paths:**
| Path | Description |
|------|-------------|
| `/usr/bin/usld` | Server binary |
| `/usr/bin/ussl` | CLI client binary |
| `/etc/ussl/ussl.toml` | Configuration file |
| `/var/lib/ussl/` | Data directory (SQLite) |
| `/lib/systemd/system/usld.service` | Systemd unit |

### With Docker

```bash
# Start the server
docker compose up ussl-dev

# Or build production image
docker compose build ussl
docker compose up ussl
```

### From Source (Rust)

```bash
# Install Rust (if needed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Build all crates
cargo build --release

# Run tests (30 tests)
cargo test

# Start server
cargo run --release --bin usld
```

### JavaScript SDK

```bash
cd sdks/js
pnpm install
pnpm build
```

## Configuration

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `USSL_TCP_PORT` | 6380 | TCP port |
| `USSL_WS_PORT` | 6381 | WebSocket port |
| `USSL_BIND` | 0.0.0.0 | Bind address |
| `USSL_LOG_LEVEL` | info | Log level |
| `USSL_DB` | (none) | SQLite database path for persistence |
| `USSL_PASSWORD` | (none) | Password for authentication |

### Command Line

```bash
usld [OPTIONS]

Options:
  --tcp-port <PORT>      TCP port [default: 6380] [env: USSL_TCP_PORT]
  --ws-port <PORT>       WebSocket port [default: 6381] [env: USSL_WS_PORT]
  --bind <ADDR>          Bind address [default: 0.0.0.0] [env: USSL_BIND]
  --log-level <LEVEL>    Log level [default: info] [env: USSL_LOG_LEVEL]
  --db <PATH>            SQLite database path [env: USSL_DB]
  --password <PASS>      Require authentication [env: USSL_PASSWORD]
  --no-tcp               Disable TCP server
  --no-ws                Disable WebSocket server
  -c, --config <FILE>    Configuration file path [env: USSL_CONFIG]
  -h, --help             Print help
  -V, --version          Print version
```

### Examples

```bash
# Development (in-memory, no auth)
usld

# Production (persistence + auth)
usld --db /var/lib/ussl/data.db --password $USSL_PASSWORD

# WebSocket only (for browser clients)
usld --no-tcp --ws-port 8080

# With environment variables
USSL_DB=/data/ussl.db USSL_PASSWORD=secret usld
```

## Authentication

When started with `--password`, the server requires authentication:

1. **Without auth**: All commands work immediately
2. **With auth**: Only `PING`, `AUTH`, and `QUIT` work before authentication
3. **After AUTH**: All commands are available

```bash
# Server
usld --password mysecret

# Client
nc localhost 6380
> GET user:1
< -ERR NOAUTH Authentication required. Use AUTH <password>
> AUTH mysecret
< +OK
> GET user:1
< $4
< null
```

### Authentication in JavaScript

```typescript
const client = await USSL.connect('ws://localhost:6381');
await client.auth('mysecret');  // Authenticate first
const doc = client.doc('user:123');
```

## Persistence

By default, USSL runs in-memory only. Enable SQLite persistence with `--db`:

```bash
usld --db /var/lib/ussl/data.db
```

**How it works:**
- Documents are saved to SQLite after every write operation (`SET`, `PUSH`, `INC`)
- On restart, documents are loaded from the database
- The database file is created automatically if it doesn't exist

**Storage backends:**
- `memory` - Fast, volatile (default)
- `sqlite` - Embedded persistence (single file)
- `postgres` - Scalable persistence (planned for v1.0)

## Benchmarking

USSL includes a built-in benchmark tool to test performance under load.

### Running Benchmarks

```bash
# Start the server (in release mode for accurate results)
cargo run --bin usld --release

# Run benchmark with default settings (10 clients, 1000 ops each)
cargo run --example benchmark --release

# Custom configuration
cargo run --example benchmark --release -- -c 50 -n 5000

# With authentication
cargo run --example benchmark --release -- -a mysecret -c 20 -n 2000
```

### Benchmark Options

| Option | Description | Default |
|--------|-------------|---------|
| `-H, --host` | Server hostname | 127.0.0.1 |
| `-p, --port` | Server port | 6380 |
| `-a, --password` | Authentication password | (none) |
| `-c, --clients` | Number of concurrent clients | 10 |
| `-n, --ops` | Operations per client | 1000 |

### What It Tests

1. **Connection Benchmark** - Tests how many concurrent connections can be established
2. **SET Benchmark** - Write throughput (ops/sec)
3. **GET Benchmark** - Read throughput (ops/sec)
4. **INC Benchmark** - Counter increment throughput
5. **Mixed Benchmark** - 40% SET + 40% GET + 20% INC workload

### Benchmark Results

> **Note:** Results vary based on hardware, OS, and virtualization. Tests below were run on Docker
> (rust:1.75-slim-bookworm) on Windows/WSL2, which adds overhead. Native Linux typically achieves
> 30-50% higher throughput and lower latency.

**Test environment:**
- Docker container: `rust:1.75-slim-bookworm`
- Host: Windows 10 + WSL2 + Docker Desktop
- CPU: Shared with host system
- Network: Docker virtual bridge (adds latency)

#### Light Load (10 clients × 1,000 ops = 10,000 operations)

| Operation | Throughput | Avg Latency | Success Rate |
|-----------|------------|-------------|--------------|
| **Connection** | 1,796 conn/sec | 557 µs | 100% |
| **SET** | 9,417 ops/sec | 1,042 µs | 100% |
| **GET** | 88,973 ops/sec | 76 µs | 100% |
| **INC** | 10,655 ops/sec | 925 µs | 100% |
| **MIXED** | 15,265 ops/sec | 566 µs | 100% |

#### Heavy Load (50 clients × 2,000 ops = 100,000 operations)

| Operation | Throughput | Avg Latency | Success Rate |
|-----------|------------|-------------|--------------|
| **Connection** | 2,764 conn/sec | 362 µs | 100% |
| **SET** | 12,710 ops/sec | 3.9 ms | 100% |
| **GET** | 225,791 ops/sec | 189 µs | 100% |
| **INC** | 25,255 ops/sec | 1.9 ms | 100% |
| **MIXED** | 10,090 ops/sec | 4 ms | 100% |

**Key observations:**
- **GET operations** are extremely fast (225K+ ops/sec) due to in-memory storage
- **SET/INC operations** maintain 10-25K ops/sec with CRDT overhead
- **Zero failures** under heavy concurrent load
- System scales well with increased client count

### Example Output

```
╔══════════════════════════════════════════════════════════╗
║  GET Benchmark (50 clients × 2000 ops)
╠══════════════════════════════════════════════════════════╣
║  Total operations:        100000
║  Successful:              100000
║  Failed:                       0
║  Duration:              442.89ms
║  Throughput:            225791 ops/sec
║  Avg latency:              189 µs
╚══════════════════════════════════════════════════════════╝
```

## APT Repository Setup (Maintainers)

To enable `apt-get install usld`, the repository needs:

### 1. Create GPG Key
```bash
gpg --full-generate-key   # RSA 4096, no expiration
gpg --armor --export-secret-keys YOUR_KEY_ID > private.key
```

### 2. Add GitHub Secret
Go to Settings > Secrets > Actions and add:
- Name: `GPG_PRIVATE_KEY`
- Value: contents of `private.key`

### 3. Enable GitHub Pages
Go to Settings > Pages:
- Source: **GitHub Actions**

### 4. Trigger Workflow
The APT repository is built automatically on each release, or manually via Actions > APT Repository > Run workflow.

After setup, users can install with:
```bash
curl -fsSL https://joett77.github.io/ussl/install.sh | bash
```

## Use Cases

### 1. Multiplayer Games

When players interact in real-time, USSL handles state synchronization automatically:

```
Player A builds wall  -->  USSL  -->  Player B sees wall instantly
Player B places door  -->  USSL  -->  Player A sees door instantly
Both edit same spot   -->  USSL  -->  Conflict resolved automatically
```

**Benefits:** No custom sync code, offline play works, automatic conflict resolution.

### 2. Collaborative Documents

Multiple users editing the same document simultaneously:

```
Alice types at home    --\
Bob types at office    ---+--> USSL --> Everyone sees all changes
Carol types on phone   --/
```

**Benefits:** Real-time collaboration, works offline (syncs when back online).

### 3. Shared Shopping Lists

Family members can add items from different devices:

```
Mom adds: milk     --\
Dad adds: bread    ---+--> USSL --> Complete list: milk, bread, eggs
Kid adds: eggs     --/
```

**Benefits:** No lost items, works without internet, syncs automatically.

## Why USSL?

### Comparison with Alternatives

| Solution | Sync | CRDT | Offline | Self-hosted | Complexity |
|----------|------|------|---------|-------------|------------|
| **USSL** | Yes | Yes | Yes | Yes | Low |
| Firebase | Yes | No | Partial | No | Low |
| Redis | No | No | No | Yes | Low |
| CouchDB | Yes | No | Yes | Yes | High |
| Yjs (library) | Yes | Yes | Yes | N/A | Medium |

### What Makes USSL Different

USSL combines:
- **Speed of Redis** - Simple protocol, single binary
- **CRDTs of Yjs** - Automatic conflict resolution
- **Offline-first of CouchDB** - Without the complexity
- **Open source** - No vendor lock-in

## Commercial Use

USSL is MIT/Apache-2.0 licensed. You can use it freely in commercial projects.

### Business Models

| Model | Description |
|-------|-------------|
| **Cloud Hosting** | Offer managed USSL instances (like Redis Labs) |
| **Enterprise License** | Premium features + support |
| **Consulting** | Integration and training services |

### Example Pricing (for cloud service)

| Tier | Price | Features |
|------|-------|----------|
| Free | $0 | 1 document, 1K ops/day |
| Pro | $29/mo | 1000 docs, 1M ops |
| Business | $199/mo | Unlimited, SLA 99.9% |
| Enterprise | Custom | Multi-node, SSO, support |

## Roadmap

- [x] v0.1 - Core engine, LWW strategy, memory storage, TCP, WebSocket
- [x] v0.1 - CRDT strategies (LWW, Counter, Set, Map, Text)
- [x] v0.1 - JavaScript/TypeScript SDK
- [x] v0.1 - CLI client tool
- [x] v0.1 - Docker support
- [x] v0.2 - SQLite persistence
- [x] v0.2 - Authentication (AUTH command)
- [x] v0.2 - APT/DEB packaging with systemd
- [x] v0.2 - Load testing benchmark tool
- [x] v0.2 - CLI improvements (env vars, auth flag)
- [ ] v0.3 - APT repository on GitHub Pages
- [ ] v0.5 - Python SDK, config file support
- [ ] v1.0 - Production-ready, PostgreSQL, WASM
- [ ] v1.1 - S3 storage, Swift SDK
- [ ] v2.0 - Multi-node clustering

## License

Licensed under either of:
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.
