# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0] - 2025-01-12

### Added
- **TLS/SSL support** - Secure connections via `--tls-cert` and `--tls-key` flags
  - TCP connections with TLS encryption
  - WebSocket connections with WSS support
  - Uses rustls (no OpenSSL dependency)
- Comprehensive use case examples in README (games, docs, IoT, fleet, delivery)
- Integration guide with React, Node.js, Python, Go examples
- Limitations section in README documenting what USSL is/isn't suited for

### Changed
- Transport layer refactored to support both plain and TLS connections
- Environment variables: `USSL_TLS_CERT`, `USSL_TLS_KEY`

## [0.2.0] - 2025-01-12

### Added
- **SQLite persistence** - Documents survive server restarts with `--db` flag
- **Authentication** - Password protection via `AUTH` command and `--password` flag
- **APT repository** - Install via `apt-get install usld` on Debian/Ubuntu
- **Systemd integration** - `usld` runs as a system service
- **Benchmark tool** - Load testing with `cargo run --example benchmark`
- **CLI improvements** - Environment variables support (`USSL_HOST`, `USSL_PASSWORD`)
- **GitHub Actions** - Automated releases with .deb packages

### Changed
- CLI now supports `-a` flag for password authentication
- Improved error messages for authentication failures

### Fixed
- WebSocket connection handling improvements
- Parser edge cases with quoted strings

## [0.1.0] - 2025-01-10

### Added
- **Core engine** - Document management with CRDT support
- **CRDT strategies**:
  - `lww` - Last-Writer-Wins (default)
  - `crdt-counter` - Convergent counter
  - `crdt-set` - Add/Remove set
  - `crdt-map` - Nested map with LWW per key
  - `crdt-text` - Collaborative text editing (via Yrs)
- **Protocol (USSP)** - Redis-like text protocol
- **Commands**: `CREATE`, `GET`, `SET`, `DEL`, `KEYS`, `SUB`, `UNSUB`, `PUSH`, `INC`, `PRESENCE`, `PING`, `INFO`, `QUIT`
- **Transports**:
  - TCP server (default port 6380)
  - WebSocket server (default port 6381)
- **JavaScript SDK** - `@ussl/client` package
- **CLI client** - Interactive REPL with command history
- **Docker support** - Development and production images

### Known Limitations
- Single-node only (no clustering)
- In-memory storage by default
- No TLS support yet
- Simple password authentication (no ACL)

---

## Versioning

- **0.x.x** - Development versions, API may change
- **1.0.0** - First stable release (planned)
- **2.0.0** - Multi-node clustering (planned)

## Links

- [GitHub Repository](https://github.com/Joett77/ussl)
- [APT Repository](https://joett77.github.io/ussl/)
- [Releases](https://github.com/Joett77/ussl/releases)
