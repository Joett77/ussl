# USSL - Universal State Synchronization Layer
# Multi-stage build for minimal final image

# Build stage
FROM rust:1.75-slim-bookworm AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests first for dependency caching
COPY Cargo.toml Cargo.lock* ./
COPY crates/ussl-core/Cargo.toml crates/ussl-core/
COPY crates/ussl-protocol/Cargo.toml crates/ussl-protocol/
COPY crates/ussl-storage/Cargo.toml crates/ussl-storage/
COPY crates/ussl-transport/Cargo.toml crates/ussl-transport/
COPY crates/usld/Cargo.toml crates/usld/

# Create dummy source files for dependency compilation
RUN mkdir -p crates/ussl-core/src && echo "pub fn dummy() {}" > crates/ussl-core/src/lib.rs && \
    mkdir -p crates/ussl-protocol/src && echo "pub fn dummy() {}" > crates/ussl-protocol/src/lib.rs && \
    mkdir -p crates/ussl-storage/src && echo "pub fn dummy() {}" > crates/ussl-storage/src/lib.rs && \
    mkdir -p crates/ussl-transport/src && echo "pub fn dummy() {}" > crates/ussl-transport/src/lib.rs && \
    mkdir -p crates/usld/src && echo "fn main() {}" > crates/usld/src/main.rs

# Build dependencies only (cached layer)
RUN cargo build --release 2>/dev/null || true

# Copy actual source code
COPY crates/ crates/
COPY examples/ examples/

# Touch files to invalidate cache and rebuild
RUN touch crates/*/src/*.rs

# Build the actual application
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the binary
COPY --from=builder /app/target/release/usld /usr/local/bin/usld

# Create non-root user
RUN useradd -m -u 1000 ussl
USER ussl

# Expose ports
EXPOSE 6380 6381

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s \
    CMD echo "PING" | nc -w 1 localhost 6380 | grep -q "PONG" || exit 1

# Run the daemon
ENTRYPOINT ["usld"]
CMD ["--bind", "0.0.0.0"]
