#!/bin/bash

# Load Test Script using wrk
# This script performs load testing on the synapse-rust API endpoints

set -e

SERVER_URL="${SERVER_URL:-http://localhost:8008}"
DURATION="${DURATION:-30s}"
THREADS="${THREADS:-4}"
CONNECTIONS="${CONNECTIONS:-100}"

echo "=========================================="
echo "API Load Test Suite"
echo "=========================================="
echo "Server URL: $SERVER_URL"
echo "Duration: $DURATION"
echo "Threads: $THREADS"
echo "Connections: $CONNECTIONS"
echo ""

# Check if wrk is installed
if ! command -v wrk &> /dev/null; then
    echo "wrk is not installed. Installing..."
    sudo apt-get update && sudo apt-get install -y wrk || {
        echo "Failed to install wrk. Using alternative method."
        USE_ALTERNATIVE=true
    }
fi

# Create test scripts
mkdir -p /tmp/synapse-load-test

# Login test
cat > /tmp/synapse-load-test/login.json << 'EOF'
{
    "identifier": {"type": "m.id.user", "user": "admin"},
    "password": "Admin@123",
    "type": "m.login.password"
}
EOF

# Sync test (with token placeholder)
cat > /tmp/synapse-load-test/sync.lua << 'EOF'
wrk.method = "GET"
wrk.headers["Authorization"] = "Bearer test_token"
wrk.headers["Content-Type"] = "application/json"
EOF

echo "Running Load Tests..."
echo ""

# Test 1: Health endpoint
echo "----------------------------------------"
echo "1. Health Endpoint Load Test"
echo "----------------------------------------"
if command -v wrk &> /dev/null; then
    wrk -t$THREADS -c$CONNECTIONS -d$DURATION "$SERVER_URL/health" || true
else
    echo "Simulating health endpoint load..."
    for i in {1..100}; do curl -s "$SERVER_URL/health" > /dev/null & done
    wait
fi

# Test 2: Version endpoint
echo ""
echo "----------------------------------------"
echo "2. Version Endpoint Load Test"
echo "----------------------------------------"
if command -v wrk &> /dev/null; then
    wrk -t$THREADS -c$CONNECTIONS -d$DURATION "$SERVER_URL/_matrix/client/versions" || true
else
    echo "Simulating version endpoint load..."
    for i in {1..100}; do curl -s "$SERVER_URL/_matrix/client/versions" > /dev/null & done
    wait
fi

# Test 3: Well-Known endpoints
echo ""
echo "----------------------------------------"
echo "3. Well-Known Client Endpoint Load Test"
echo "----------------------------------------"
if command -v wrk &> /dev/null; then
    wrk -t$THREADS -c$CONNECTIONS -d$DURATION "$SERVER_URL/.well-known/matrix/client" || true
else
    echo "Simulating well-known endpoint load..."
    for i in {1..100}; do curl -s "$SERVER_URL/.well-known/matrix/client" > /dev/null & done
    wait
fi

echo ""
echo "=========================================="
echo "Load Test Complete"
echo "=========================================="

# Cleanup
rm -rf /tmp/synapse-load-test
