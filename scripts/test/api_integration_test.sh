#!/bin/bash

# API Integration Test Script
# This script runs complete API integration tests against the synapse-rust server

set -e

SERVER_URL="${SERVER_URL:-http://localhost:8008}"
ADMIN_USER="${ADMIN_USER:-admin}"
ADMIN_PASS="${ADMIN_PASS:-Admin@123}"
TEST_USER="${TEST_USER:-testuser}"
TEST_PASS="${TEST_PASS:-Test@123}"

echo "=========================================="
echo "API Integration Test Suite"
echo "=========================================="
echo "Server URL: $SERVER_URL"
echo ""

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Test counter
PASSED=0
FAILED=0

# Helper functions
pass() {
    echo -e "${GREEN}✓ PASS${NC}: $1"
    ((PASSED++))
}

fail() {
    echo -e "${RED}✗ FAIL${NC}: $1"
    ((FAILED++))
}

info() {
    echo -e "${YELLOW}ℹ INFO${NC}: $1"
}

# Check server health
echo "----------------------------------------"
echo "1. Health Check"
echo "----------------------------------------"
if curl -s -f "$SERVER_URL/health" > /dev/null 2>&1; then
    pass "Server is healthy"
else
    fail "Server health check failed"
fi

# Test Well-Known endpoints
echo ""
echo "----------------------------------------"
echo "2. Well-Known Endpoints"
echo "----------------------------------------"

well_known_client=$(curl -s "$SERVER_URL/.well-known/matrix/client")
if echo "$well_known_client" | grep -q "m.homeserver"; then
    pass "Well-Known client endpoint"
else
    fail "Well-Known client endpoint"
fi

well_known_server=$(curl -s "$SERVER_URL/.well-known/matrix/server")
if echo "$well_known_server" | grep -q "m.server"; then
    pass "Well-Known server endpoint"
else
    fail "Well-Known server endpoint"
fi

# Test Version endpoint
echo ""
echo "----------------------------------------"
echo "3. Version Endpoints"
echo "----------------------------------------"

version=$(curl -s "$SERVER_URL/_matrix/client/versions")
if echo "$version" | grep -q "versions"; then
    pass "Client versions endpoint"
else
    fail "Client versions endpoint"
fi

# Test Login
echo ""
echo "----------------------------------------"
echo "4. Authentication"
echo "----------------------------------------"

LOGIN_RESPONSE=$(curl -s -X POST "$SERVER_URL/_matrix/client/v3/login" \
    -H "Content-Type: application/json" \
    -d "{
        \"identifier\": {\"type\": \"m.id.user\", \"user\": \"$ADMIN_USER\"},
        \"password\": \"$ADMIN_PASS\",
        \"type\": \"m.login.password\"
    }")

ACCESS_TOKEN=$(echo "$LOGIN_RESPONSE" | grep -o '"access_token":"[^"]*"' | cut -d'"' -f4)
USER_ID=$(echo "$LOGIN_RESPONSE" | grep -o '"user_id":"[^"]*"' | cut -d'"' -f4)

if [ -n "$ACCESS_TOKEN" ]; then
    pass "Login successful (User: $USER_ID)"
else
    fail "Login failed"
    echo "Response: $LOGIN_RESPONSE"
fi

# Test Capabilities
echo ""
echo "----------------------------------------"
echo "5. Capabilities"
echo "----------------------------------------"

if [ -n "$ACCESS_TOKEN" ]; then
    capabilities=$(curl -s "$SERVER_URL/_matrix/client/v3/capabilities" \
        -H "Authorization: Bearer $ACCESS_TOKEN")
    if echo "$capabilities" | grep -q "capabilities"; then
        pass "Get capabilities"
    else
        fail "Get capabilities"
    fi
fi

# Test Create Room
echo ""
echo "----------------------------------------"
echo "6. Room Management"
echo "----------------------------------------"

if [ -n "$ACCESS_TOKEN" ]; then
    ROOM_RESPONSE=$(curl -s -X POST "$SERVER_URL/_matrix/client/v3/createRoom" \
        -H "Authorization: Bearer $ACCESS_TOKEN" \
        -H "Content-Type: application/json" \
        -d '{
            "name": "Test Room",
            "topic": "Test Topic",
            "preset": "public_chat"
        }')

    ROOM_ID=$(echo "$ROOM_RESPONSE" | grep -o '"room_id":"[^"]*"' | cut -d'"' -f4)

    if [ -n "$ROOM_ID" ]; then
        pass "Create room (Room: $ROOM_ID)"
    else
        fail "Create room"
    fi
fi

# Test Sync
echo ""
echo "----------------------------------------"
echo "7. Sync"
echo "----------------------------------------"

if [ -n "$ACCESS_TOKEN" ]; then
    sync_response=$(curl -s "$SERVER_URL/_matrix/client/v3/sync?timeout=0" \
        -H "Authorization: Bearer $ACCESS_TOKEN")
    if echo "$sync_response" | grep -q "next_batch"; then
        pass "Sync endpoint"
    else
        fail "Sync endpoint"
    fi
fi

# Test Profile
echo ""
echo "----------------------------------------"
echo "8. Profile"
echo "----------------------------------------"

if [ -n "$ACCESS_TOKEN" ] && [ -n "$USER_ID" ]; then
    profile=$(curl -s "$SERVER_URL/_matrix/client/v3/profile/$USER_ID" \
        -H "Authorization: Bearer $ACCESS_TOKEN")
    if echo "$profile" | grep -q "displayname"; then
        pass "Get profile"
    else
        fail "Get profile"
    fi
fi

# Test Media Config
echo ""
echo "----------------------------------------"
echo "9. Media"
echo "----------------------------------------"

media_config=$(curl -s "$SERVER_URL/_matrix/client/v3/media/config" \
    -H "Authorization: Bearer $ACCESS_TOKEN")
if echo "$media_config" | grep -q "config"; then
    pass "Media config endpoint"
else
    fail "Media config endpoint"
fi

# Test VoIP Config
echo ""
echo "----------------------------------------"
echo "10. VoIP"
echo "----------------------------------------"

voip_config=$(curl -s "$SERVER_URL/_matrix/client/v3/voip/config" \
    -H "Authorization: Bearer $ACCESS_TOKEN")
if echo "$voip_config" | grep -q "config"; then
    pass "VoIP config endpoint"
else
    fail "VoIP config endpoint"
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
    echo -e "${RED}Some tests failed!${NC}"
    exit 1
fi
