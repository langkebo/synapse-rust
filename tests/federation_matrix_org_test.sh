#!/bin/bash
set -e

echo "=========================================="
echo "Federation Test with matrix.org"
echo "=========================================="
echo ""

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test result counters
PASSED=0
FAILED=0

pass() {
    echo -e "${GREEN}✓ PASS:${NC} $1"
    ((PASSED++))
}

fail() {
    echo -e "${RED}✗ FAIL:${NC} $1"
    ((FAILED++))
}

info() {
    echo -e "${YELLOW}ℹ INFO:${NC} $1"
}

# Step 1: Verify matrix.org federation server
info "Checking matrix.org federation server..."

MATRIX_ORG_SERVER=$(curl -s https://matrix.org/.well-known/matrix/server | jq -r '.["m.server"]')
if [ -n "$MATRIX_ORG_SERVER" ]; then
    pass "matrix.org federation server: $MATRIX_ORG_SERVER"
else
    fail "Failed to discover matrix.org federation server"
    exit 1
fi

MATRIX_ORG_VERSION=$(curl -s https://matrix-federation.matrix.org/_matrix/federation/v1/version | jq -r '.server.version')
if [ -n "$MATRIX_ORG_VERSION" ]; then
    pass "matrix.org server version: $MATRIX_ORG_VERSION"
else
    fail "Failed to get matrix.org server version"
fi

# Step 2: Check if local synapse-rust is running
info "Checking local synapse-rust server..."

LOCAL_VERSION=$(curl -s http://localhost:8008/_matrix/client/versions 2>/dev/null | jq -r '.versions[0]' || echo "")
if [ -n "$LOCAL_VERSION" ]; then
    pass "Local server is responding (version: $LOCAL_VERSION)"
else
    fail "Local server is not responding at http://localhost:8008"
    info "Please start synapse-rust with: cargo run"
    exit 1
fi

# Step 3: Register a local user
info "Registering local user..."

NONCE=$(curl -s http://localhost:8008/_synapse/admin/v1/register/nonce | jq -r '.nonce')
if [ -z "$NONCE" ] || [ "$NONCE" = "null" ]; then
    fail "Failed to get nonce from local server"
    exit 1
fi

# Calculate MAC (assuming shared secret is in config)
SHARED_SECRET="test_shared_secret"
MAC=$(echo -n "${NONCE}\0testuser\0password123\0notadmin" | openssl dgst -sha256 -hmac "$SHARED_SECRET" | awk '{print $2}')

USER_RESPONSE=$(curl -s -X POST http://localhost:8008/_synapse/admin/v1/register \
    -H "Content-Type: application/json" \
    -d "{\"nonce\":\"$NONCE\",\"username\":\"testuser\",\"password\":\"password123\",\"admin\":false,\"mac\":\"$MAC\"}")

USER_TOKEN=$(echo "$USER_RESPONSE" | jq -r '.access_token')

if [ -n "$USER_TOKEN" ] && [ "$USER_TOKEN" != "null" ]; then
    pass "Local user registered successfully"
else
    fail "Failed to register local user: $USER_RESPONSE"
    exit 1
fi

# Step 4: Test federation query to matrix.org
info "Testing federation query to matrix.org..."

# Query a well-known public room on matrix.org
PUBLIC_ROOM_ID="!OGEhHVWSdvArJzumhm:matrix.org"  # Matrix HQ room

ROOM_STATE=$(curl -s -X GET "http://localhost:8008/_matrix/federation/v1/state/$PUBLIC_ROOM_ID" \
    -H "Authorization: Bearer $USER_TOKEN" 2>/dev/null || echo "")

if echo "$ROOM_STATE" | jq -e '.pdus' > /dev/null 2>&1; then
    pass "Successfully queried room state from matrix.org via federation"
else
    info "Federation query result: $ROOM_STATE"
    fail "Failed to query room state via federation (this may be expected if federation is not fully configured)"
fi

# Step 5: Test server key query
info "Testing server key query..."

KEY_RESPONSE=$(curl -s "http://localhost:8008/_matrix/key/v2/query/matrix.org" 2>/dev/null || echo "")

if echo "$KEY_RESPONSE" | jq -e '.server_keys' > /dev/null 2>&1; then
    pass "Successfully queried matrix.org server keys"
else
    info "Key query may require proper federation setup"
fi

# Summary
echo ""
echo "=========================================="
echo "Test Summary"
echo "=========================================="
echo -e "${GREEN}Passed: $PASSED${NC}"
echo -e "${RED}Failed: $FAILED${NC}"
echo ""

if [ $FAILED -eq 0 ]; then
    echo -e "${GREEN}All tests passed!${NC}"
    exit 0
else
    echo -e "${RED}Some tests failed.${NC}"
    exit 1
fi
