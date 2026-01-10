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
# Build and run
cargo run --bin usld

# Or with custom ports
cargo run --bin usld -- --tcp-port 7000 --ws-port 7001
```

### Connect with TCP (Redis-like protocol)

```bash
# Using netcat or telnet
nc localhost 6380

# Commands
PING
CREATE user:123 STRATEGY lww
SET user:123 name "Alice"
GET user:123
INC counter:views count 1
SUB user:*
QUIT
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
| `CREATE` | `CREATE <id> [STRATEGY <s>] [TTL <ms>]` | Create document |
| `GET` | `GET <id> [PATH <path>]` | Get document/path |
| `SET` | `SET <id> <path> <value>` | Set value |
| `DEL` | `DEL <id> [PATH <path>]` | Delete document/path |
| `SUB` | `SUB <pattern>` | Subscribe to changes |
| `UNSUB` | `UNSUB <pattern>` | Unsubscribe |
| `PUSH` | `PUSH <id> <path> <value>` | Append to array |
| `INC` | `INC <id> <path> <delta>` | Increment counter |
| `PRESENCE` | `PRESENCE <id> [DATA <json>]` | Set/get presence |
| `PING` | `PING` | Health check |
| `KEYS` | `KEYS [pattern]` | List documents |
| `INFO` | `INFO` | Server info |

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

## Building

### With Docker (Recommended)

```bash
# Start the server
docker compose up ussl-dev

# Or build production image
docker compose build ussl
docker compose up ussl
```

### With Rust (Local)

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

### Command Line

```bash
usld --tcp-port 7000 --ws-port 7001 --bind 127.0.0.1 --log-level debug
```

## Roadmap

- [x] v0.1 - Core engine, LWW strategy, memory storage, TCP, WebSocket
- [x] v0.1 - CRDT strategies (LWW, Counter, Set, Map, Text)
- [x] v0.1 - JavaScript/TypeScript SDK
- [x] v0.1 - CLI client tool
- [x] v0.1 - Docker support
- [ ] v0.5 - SQLite persistence, Python SDK
- [ ] v1.0 - Production-ready, PostgreSQL, WASM
- [ ] v1.1 - S3 storage, Swift SDK
- [ ] v2.0 - Multi-node clustering

## License

Licensed under either of:
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.
