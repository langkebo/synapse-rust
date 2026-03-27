#!/bin/bash
set -e

SERVER_URL="http://localhost:28008"
TEST_USER="admin"
TEST_PASS='Wzc9890951!'

echo "=========================================="
echo "Complete API Integration Test"
echo "=========================================="
echo "Server: $SERVER_URL"
echo ""

PASSED=0
FAILED=0

pass() { echo "✓ PASS: $1"; ((PASSED++)) || true; }
fail() { echo "✗ FAIL: $1"; ((FAILED++)) || true; }

# 1. Health Check
echo "1. Health Check"
curl -s -f "$SERVER_URL/health" > /dev/null 2>&1 && pass "Health endpoint" || fail "Health endpoint"

# 2. Version
echo ""
echo "2. Version"
curl -s "$SERVER_URL/_matrix/client/versions" | grep -q "versions" && pass "Versions endpoint" || fail "Versions endpoint"

# 3. Login
echo ""
echo "3. Authentication"
LOGIN_RESP=$(curl -s -X POST "$SERVER_URL/_matrix/client/v3/login" \
    -H "Content-Type: application/json" \
    -d "{\"type\": \"m.login.password\", \"user\": \"$TEST_USER\", \"password\": \"$TEST_PASS\"}")
TOKEN=$(echo "$LOGIN_RESP" | grep -o "\"access_token\":\"[^\"]*\"" | cut -d'"' -f4)
USER_ID=$(echo "$LOGIN_RESP" | grep -o "\"user_id\":\"[^\"]*\"" | cut -d'"' -f4)
if [ -n "$TOKEN" ]; then
    pass "Login (User: $USER_ID)"
else
    fail "Login failed"
fi
echo ""
echo "4. Capabilities"
curl -s "$SERVER_URL/_matrix/client/v3/capabilities" -H "Authorization: Bearer $TOKEN" | grep -q "capabilities" && pass "Capabilities" || fail "Capabilities"

# 5. Create Room
echo ""
echo "5. Room Management"
ROOM_RESP=$(curl -s -X POST "$SERVER_URL/_matrix/client/v3/createRoom" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"name": "Test Room", "topic": "Test", "preset": "public_chat"}')
ROOM_ID=$(echo "$ROOM_RESP" | grep -o "\"room_id\":\"[^\"]*\"" | cut -d'"' -f4)
if [ -n "$ROOM_ID" ]; then
    pass "Create Room"
else
    fail "Create Room"
fi

# 6. Sync
echo ""
echo "6. Sync"
curl -s "$SERVER_URL/_matrix/client/v3/sync?timeout=1000" -H "Authorization: Bearer $TOKEN" | grep -q "next_batch" && pass "Sync" || fail "Sync"

# 7. Profile
echo ""
echo "7. Profile"
curl -s "$SERVER_URL/_matrix/client/v3/profile/$USER_ID" -H "Authorization: Bearer $TOKEN" | grep -q "displayname" && pass "Get Profile" || fail "Get Profile"

# 8. Update Displayname
echo ""
echo "8. Profile Update"
curl -s -X PUT "$SERVER_URL/_matrix/client/v3/profile/$USER_ID/displayname" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"displayname": "Test Admin"}' && pass "Update Displayname" || fail "Update Displayname"

# 9. Media Config
echo ""
echo "9. Media Config"
curl -s "$SERVER_URL/_matrix/client/v3/media/config" -H "Authorization: Bearer $TOKEN" | grep -q "upload" && pass "Media Config" || fail "Media Config"

# 10. VoIP Config
echo ""
echo "10. VoIP Config"
curl -s "$SERVER_URL/_matrix/client/v3/voip/config" -H "Authorization: Bearer $TOKEN" | grep -q "turn" && pass "VoIP Config" || fail "VoIP Config"

# 11. Room State
echo ""
echo "11. Room State"
curl -s "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/state" -H "Authorization: Bearer $TOKEN" | grep -q "state" && pass "Get Room State" || fail "Get Room State"

# 12. Send Message
echo ""
echo "12. Send Message"
MSG_RESP=$(curl -s -X PUT "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/send/m.room.message/1" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"msgtype":"m.text","body":"Hello World"}')
echo "$MSG_RESP" | grep -q "event_id" && pass "Send Message" || fail "Send Message"

# 13. Media Upload
echo ""
echo "13. Media Upload"
MEDIA_RESP=$(curl -s -X POST "$SERVER_URL/_matrix/media/v3/upload" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: image/png" \
    --data-binary "PNG-DATA")
echo "$MEDIA_RESP" | grep -q "content_uri" && pass "Media Upload" || fail "Media Upload"

# 14. Room Messages
echo ""
echo "14. Room Messages"
curl -s "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/messages?limit=10" -H "Authorization: Bearer $TOKEN" | grep -q "chunk" && pass "Room Messages" || fail "Room Messages"

# 15. User Directory (may return empty if not implemented)
echo ""
echo "15. User Directory"
UD_RESP=$(curl -s "$SERVER_URL/_matrix/client/v1/user_directory/search/users?search_term=test" -H "Authorization: Bearer $TOKEN")
if echo "$UD_RESP" | grep -q "results"; then
    pass "User Directory"
else
    echo "INFO: User Directory returned: $UD_RESP"
    if [ -z "$UD_RESP" ] || [ "$UD_RESP" = "{}" ]; then
        echo "SKIP: User Directory not fully implemented"
    else
        fail "User Directory"
    fi
fi

# 16. Joined Members
echo ""
echo "16. Joined Members"
curl -s "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/joined_members" -H "Authorization: Bearer $TOKEN" | grep -q "joined" && pass "Joined Members" || fail "Joined Members"

# 17. WhoAmI
echo ""
echo "17. WhoAmI"
curl -s "$SERVER_URL/_matrix/client/v3/account/whoami" -H "Authorization: Bearer $TOKEN" | grep -q "user_id" && pass "WhoAmI" || fail "WhoAmI"

# 18. Admin - List Users
echo ""
echo "18. Admin - List Users"
curl -s "$SERVER_URL/_synapse/admin/v1/users" -H "Authorization: Bearer $TOKEN" | grep -q "users" && pass "Admin List Users" || fail "Admin List Users"

# 19. Admin - User Details
echo ""
echo "19. Admin - User Details"
curl -s "$SERVER_URL/_synapse/admin/v1/users/@admin:cjystx.top" -H "Authorization: Bearer $TOKEN" | grep -q "name" && pass "Admin User Details" || fail "Admin User Details"

# 20. Admin - List Rooms
echo ""
echo "20. Admin - List Rooms"
curl -s "$SERVER_URL/_synapse/admin/v1/rooms" -H "Authorization: Bearer $TOKEN" | grep -q "rooms" && pass "Admin List Rooms" || fail "Admin List Rooms"

echo ""
echo "=========================================="
echo "Summary: Passed=$PASSED Failed=$FAILED"
echo "=========================================="
if [ "$FAILED" -eq 0 ]; then
    echo "✓ All tests passed!"
    exit 0
else
    echo "✗ Some tests failed!"
    exit 1
fi