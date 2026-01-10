#!/bin/bash
# USSL Installation Script for Ubuntu/Debian
# Usage: curl -sSL https://raw.githubusercontent.com/yourusername/ussl/main/install.sh | bash

set -e

USSL_VERSION="${USSL_VERSION:-0.1.0}"
INSTALL_DIR="/usr/local/bin"
CONFIG_DIR="/etc/ussl"
DATA_DIR="/var/lib/ussl"
LOG_DIR="/var/log/ussl"
USER="ussl"

echo "╦ ╦╔═╗╔═╗╦    Installer"
echo "║ ║╚═╗╚═╗║    v${USSL_VERSION}"
echo "╚═╝╚═╝╚═╝╩═╝"
echo ""

# Check if running as root
if [ "$EUID" -ne 0 ]; then
    echo "Please run as root: sudo bash install.sh"
    exit 1
fi

echo "→ Installing dependencies..."
apt-get update -qq
apt-get install -y -qq curl build-essential pkg-config libssl-dev

# Check if Rust is installed
if ! command -v cargo &> /dev/null; then
    echo "→ Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi

echo "→ Creating USSL user..."
id -u $USER &>/dev/null || useradd -r -s /bin/false $USER

echo "→ Creating directories..."
mkdir -p $CONFIG_DIR $DATA_DIR $LOG_DIR
chown $USER:$USER $DATA_DIR $LOG_DIR

echo "→ Downloading and building USSL..."
TEMP_DIR=$(mktemp -d)
cd $TEMP_DIR

# Clone or download
if command -v git &> /dev/null; then
    git clone --depth 1 https://github.com/yourusername/ussl.git .
else
    curl -sSL https://github.com/yourusername/ussl/archive/refs/heads/main.tar.gz | tar xz --strip-components=1
fi

# Build
cargo build --release --bin usld

echo "→ Installing binaries..."
cp target/release/usld $INSTALL_DIR/
chmod +x $INSTALL_DIR/usld

# Create config file
echo "→ Creating configuration..."
cat > $CONFIG_DIR/ussl.toml << 'EOF'
# USSL Configuration

[server]
tcp_port = 6380
ws_port = 6381
bind = "0.0.0.0"

[storage]
type = "memory"
# type = "sqlite"
# path = "/var/lib/ussl/data.db"

[logging]
level = "info"
EOF

# Create systemd service
echo "→ Creating systemd service..."
cat > /etc/systemd/system/ussl.service << EOF
[Unit]
Description=USSL - Universal State Synchronization Layer
After=network.target

[Service]
Type=simple
User=$USER
Group=$USER
ExecStart=$INSTALL_DIR/usld --config $CONFIG_DIR/ussl.toml
Restart=always
RestartSec=5
LimitNOFILE=65535

# Logging
StandardOutput=append:$LOG_DIR/ussl.log
StandardError=append:$LOG_DIR/ussl-error.log

[Install]
WantedBy=multi-user.target
EOF

echo "→ Enabling service..."
systemctl daemon-reload
systemctl enable ussl

# Cleanup
cd /
rm -rf $TEMP_DIR

echo ""
echo "════════════════════════════════════════════"
echo "✓ USSL installed successfully!"
echo ""
echo "Commands:"
echo "  sudo systemctl start ussl    # Start server"
echo "  sudo systemctl stop ussl     # Stop server"
echo "  sudo systemctl status ussl   # Check status"
echo "  sudo journalctl -u ussl -f   # View logs"
echo ""
echo "Test connection:"
echo "  nc localhost 6380"
echo "  > PING"
echo ""
echo "Config: $CONFIG_DIR/ussl.toml"
echo "Data:   $DATA_DIR"
echo "Logs:   $LOG_DIR"
echo "════════════════════════════════════════════"
