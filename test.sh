#!/bin/bash
# USSL Quick Test Script

HOST=${1:-localhost}
PORT=${2:-6380}

echo "Testing USSL at $HOST:$PORT"
echo "================================"

# Test commands
commands=(
    "PING"
    "CREATE user:alice STRATEGY lww"
    "SET user:alice name \"Alice\""
    "SET user:alice email \"alice@example.com\""
    "GET user:alice"
    "CREATE counter:views STRATEGY crdt-counter"
    "INC counter:views total 1"
    "INC counter:views total 5"
    "INC counter:views total 10"
    "GET counter:views"
    "PUSH cart:alice items {\"sku\":\"ITEM-001\",\"qty\":2}"
    "PUSH cart:alice items {\"sku\":\"ITEM-002\",\"qty\":1}"
    "GET cart:alice"
    "KEYS"
    "KEYS user:*"
    "INFO"
    "QUIT"
)

for cmd in "${commands[@]}"; do
    echo ""
    echo "> $cmd"
    echo -e "$cmd\r" | nc -q 1 $HOST $PORT 2>/dev/null || echo "(connection closed)"
done

echo ""
echo "================================"
echo "Test complete!"
