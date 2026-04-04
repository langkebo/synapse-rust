#!/bin/bash
set -e

echo "=========================================="
echo "Federation Interoperability Test"
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

# Cleanup function
cleanup() {
    info "Cleaning up..."
    docker-compose -f docker-compose.federation-test.yml down -v
}

# Set trap to cleanup on exit
trap cleanup EXIT

# Step 1: Start both homeservers
info "Starting homeservers..."
docker-compose -f docker-compose.federation-test.yml up -d

info "Waiting for homeservers to be healthy..."
sleep 20

# Check if homeservers are running
if ! docker-compose -f docker-compose.federation-test.yml ps | grep -q "homeserver1.*Up"; then
    fail "Homeserver1 failed to start"
    exit 1
fi

if ! docker-compose -f docker-compose.federation-test.yml ps | grep -q "homeserver2.*Up"; then
    fail "Homeserver2 failed to start"
    exit 1
fi

pass "Both homeservers started successfully"

# Step 2: Check server versions
info "Checking server versions..."

SERVER1_VERSION=$(curl -s http://localhost:8008/_matrix/client/versions | jq -r '.versions[0]' 2>/dev/null || echo "")
SERVER2_VERSION=$(curl -s http://localhost:8009/_matrix/client/versions | jq -r '.versions[0]' 2>/dev/null || echo "")

if [ -n "$SERVER1_VERSION" ]; then
    pass "Server1 is responding (version: $SERVER1_VERSION)"
else
    fail "Server1 is not responding"
fi

if [ -n "$SERVER2_VERSION" ]; then
    pass "Server2 is responding (version: $SERVER2_VERSION)"
else
    fail "Server2 is not responding"
fi

# Step 3: Register users on both servers
info "Registering users..."

# Get nonce for server1
NONCE1=$(curl -s http://localhost:8008/_synapse/admin/v1/register/nonce | jq -r '.nonce')
if [ -z "$NONCE1" ] || [ "$NONCE1" = "null" ]; then
    fail "Failed to get nonce from server1"
    exit 1
fi

# Calculate MAC for server1
MAC1=$(echo -n "${NONCE1}\0user1\0password123\0notadmin" | openssl dgst -sha256 -hmac "test_shared_secret_1" | awk '{print $2}')

# Register user1 on server1
USER1_RESPONSE=$(curl -s -X POST http://localhost:8008/_synapse/admin/v1/register \
    -H "Content-Type: application/json" \
    -d "{\"nonce\":\"$NONCE1\",\"username\":\"user1\",\"password\":\"password123\",\"admin\":false,\"mac\":\"$MAC1\"}")

USER1_TOKEN=$(echo "$USER1_RESPONSE" | jq -r '.access_token')

if [ -n "$USER1_TOKEN" ] && [ "$USER1_TOKEN" != "null" ]; then
    pass "User1 registered on server1"
else
    fail "Failed to register user1 on server1"
    echo "Response: $USER1_RESPONSE"
fi

# Get nonce for server2
NONCE2=$(curl -s http://localhost:8009/_synapse/admin/v1/register/nonce | jq -r '.nonce')
if [ -z "$NONCE2" ] || [ "$NONCE2" = "null" ]; then
    fail "Failed to get nonce from server2"
    exit 1
fi

# Calculate MAC for server2
MAC2=$(echo -n "${NONCE2}\0user2\0password123\0notadmin" | openssl dgst -sha256 -hmac "test_shared_secret_2" | awk '{print $2}')

# Register user2 on server2
USER2_RESPONSE=$(curl -s -X POST http://localhost:8009/_synapse/admin/v1/register \
    -H "Content-Type: application/json" \
    -d "{\"nonce\":\"$NONCE2\",\"username\":\"user2\",\"password\":\"password123\",\"admin\":false,\"mac\":\"$MAC2\"}")

USER2_TOKEN=$(echo "$USER2_RESPONSE" | jq -r '.access_token')

if [ -n "$USER2_TOKEN" ] && [ "$USER2_TOKEN" != "null" ]; then
    pass "User2 registered on server2"
else
    fail "Failed to register user2 on server2"
    echo "Response: $USER2_RESPONSE"
fi

# Step 4: User1 creates a room
info "Creating room on server1..."

ROOM_RESPONSE=$(curl -s -X POST http://localhost:8008/_matrix/client/v3/createRoom \
    -H "Authorization: Bearer $USER1_TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"name":"Federation Test Room","preset":"public_chat"}')

ROOM_ID=$(echo "$ROOM_RESPONSE" | jq -r '.room_id')

if [ -n "$ROOM_ID" ] && [ "$ROOM_ID" != "null" ]; then
    pass "Room created: $ROOM_ID"
else
    fail "Failed to create room"
    echo "Response: $ROOM_RESPONSE"
    exit 1
fi

# Step 5: User1 invites User2 (cross-server invite)
info "Sending cross-server invite..."

INVITE_RESPONSE=$(curl -s -X POST "http://localhost:8008/_matrix/client/v3/rooms/$ROOM_ID/invite" \
    -H "Authorization: Bearer $USER1_TOKEN" \
    -H "Content-Type: application/json" \
    -d "{\"user_id\":\"@user2:server2.test\"}")

INVITE_STATUS=$(echo "$INVITE_RESPONSE" | jq -r '.errcode // "success"')

if [ "$INVITE_STATUS" = "success" ]; then
    pass "Cross-server invite sent"
else
    fail "Failed to send cross-server invite"
    echo "Response: $INVITE_RESPONSE"
fi

# Step 6: User2 accepts invite
info "User2 accepting invite..."

sleep 2  # Give federation time to propagate

JOIN_RESPONSE=$(curl -s -X POST "http://localhost:8009/_matrix/client/v3/rooms/$ROOM_ID/join" \
    -H "Authorization: Bearer $USER2_TOKEN" \
    -H "Content-Type: application/json")

JOIN_STATUS=$(echo "$JOIN_RESPONSE" | jq -r '.errcode // "success"')

if [ "$JOIN_STATUS" = "success" ]; then
    pass "User2 joined room via federation"
else
    fail "User2 failed to join room"
    echo "Response: $JOIN_RESPONSE"
fi

# Step 7: User1 sends a message
info "Sending message from server1..."

MESSAGE_RESPONSE=$(curl -s -X PUT "http://localhost:8008/_matrix/client/v3/rooms/$ROOM_ID/send/m.room.message/txn1" \
    -H "Authorization: Bearer $USER1_TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"msgtype":"m.text","body":"Hello from server1"}')

MESSAGE_EVENT_ID=$(echo "$MESSAGE_RESPONSE" | jq -r '.event_id')

if [ -n "$MESSAGE_EVENT_ID" ] && [ "$MESSAGE_EVENT_ID" != "null" ]; then
    pass "Message sent: $MESSAGE_EVENT_ID"
else
    fail "Failed to send message"
    echo "Response: $MESSAGE_RESPONSE"
fi

# Step 8: User2 syncs and verifies message received
info "Verifying message received on server2..."

sleep 3  # Give federation time to propagate

SYNC_RESPONSE=$(curl -s -X GET "http://localhost:8009/_matrix/client/v3/sync" \
    -H "Authorization: Bearer $USER2_TOKEN")

# Check if room exists in sync response
if echo "$SYNC_RESPONSE" | jq -e ".rooms.join[\"$ROOM_ID\"]" > /dev/null 2>&1; then
    pass "User2 received room state from server1"

    # Check if message was received
    if echo "$SYNC_RESPONSE" | jq -e ".rooms.join[\"$ROOM_ID\"].timeline.events[] | select(.content.body == \"Hello from server1\")" > /dev/null 2>&1; then
        pass "User2 received message from server1"
    else
        fail "User2 did not receive message from server1"
    fi
else
    fail "User2 did not receive room state"
fi

# Step 9: Test bidirectional messaging
info "Testing bidirectional messaging..."

MESSAGE2_RESPONSE=$(curl -s -X PUT "http://localhost:8009/_matrix/client/v3/rooms/$ROOM_ID/send/m.room.message/txn2" \
    -H "Authorization: Bearer $USER2_TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"msgtype":"m.text","body":"Hello from server2"}')

MESSAGE2_EVENT_ID=$(echo "$MESSAGE2_RESPONSE" | jq -r '.event_id')

if [ -n "$MESSAGE2_EVENT_ID" ] && [ "$MESSAGE2_EVENT_ID" != "null" ]; then
    pass "Message sent from server2"
else
    fail "Failed to send message from server2"
fi

sleep 3

SYNC2_RESPONSE=$(curl -s -X GET "http://localhost:8008/_matrix/client/v3/sync" \
    -H "Authorization: Bearer $USER1_TOKEN")

if echo "$SYNC2_RESPONSE" | jq -e ".rooms.join[\"$ROOM_ID\"].timeline.events[] | select(.content.body == \"Hello from server2\")" > /dev/null 2>&1; then
    pass "User1 received message from server2"
else
    fail "User1 did not receive message from server2"
fi

# Print summary
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
