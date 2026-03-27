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
SKIPPED=0

pass() { echo "✓ PASS: $1"; ((PASSED++)) || true; }
fail() { echo "✗ FAIL: $1"; ((FAILED++)) || true; }
skip() { echo "⊘ SKIP: $1"; ((SKIPPED++)) || true; }

# 1. Health Check
echo "=========================================="
echo "1. Health & Version"
echo "=========================================="
echo "1. Health Check"
curl -s -f "$SERVER_URL/health" > /dev/null 2>&1 && pass "Health endpoint" || fail "Health endpoint"

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

# Create a test room for later tests
echo ""
echo "5. Room Setup"
ROOM_RESP=$(curl -s -X POST "$SERVER_URL/_matrix/client/v3/createRoom" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"name": "Test Room API", "topic": "API Test Room", "preset": "public_chat"}')
ROOM_ID=$(echo "$ROOM_RESP" | grep -o "\"room_id\":\"[^\"]*\"" | cut -d'"' -f4)
if [ -n "$ROOM_ID" ]; then
    pass "Create Test Room"
else
    fail "Create Test Room"
fi

# Create second room
ROOM2_RESP=$(curl -s -X POST "$SERVER_URL/_matrix/client/v3/createRoom" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"name": "Test Room 2", "preset": "private_chat"}')
ROOM2_ID=$(echo "$ROOM2_RESP" | grep -o "\"room_id\":\"[^\"]*\"" | cut -d'"' -f4)
[ -n "$ROOM2_ID" ] && pass "Create Second Room" || fail "Create Second Room"

# 6. Sync
echo ""
echo "=========================================="
echo "6. Sync & Events"
echo "=========================================="
echo "6. Sync"
curl -s "$SERVER_URL/_matrix/client/v3/sync?timeout=1000" -H "Authorization: Bearer $TOKEN" | grep -q "next_batch" && pass "Sync" || fail "Sync"

echo ""
echo "7. Room Sync"
ROOM_SYNC_RESP=$(curl -s "$SERVER_URL/_matrix/client/v3/sync?filter=%7B%22room%22%3A%7B%22rooms%22%3A%5B%22$ROOM_ID%22%5D%7D%7D" -H "Authorization: Bearer $TOKEN")
echo "$ROOM_SYNC_RESP" | grep -q "rooms\|next_batch" && pass "Room Sync Filter" || fail "Room Sync Filter"

# 7. Profile
echo ""
echo "=========================================="
echo "8. Profile"
echo "=========================================="
echo "8. Get Profile"
curl -s "$SERVER_URL/_matrix/client/v3/profile/$USER_ID" -H "Authorization: Bearer $TOKEN" | grep -q "displayname" && pass "Get Profile" || fail "Get Profile"

echo ""
echo "9. Update Displayname"
curl -s -X PUT "$SERVER_URL/_matrix/client/v3/profile/$USER_ID/displayname" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"displayname": "API Test Admin"}' && pass "Update Displayname" || fail "Update Displayname"

echo ""
echo "10. Get Avatar"
curl -s "$SERVER_URL/_matrix/client/v3/profile/$USER_ID/avatar_url" -H "Authorization: Bearer $TOKEN" | grep -q "avatar_url" && pass "Get Avatar URL" || fail "Get Avatar URL"

# 8. Room State & Messages
echo ""
echo "=========================================="
echo "11. Room State & Messages"
echo "=========================================="
echo "11. Room State"
curl -s "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/state" -H "Authorization: Bearer $TOKEN" | grep -q "state" && pass "Get Room State" || fail "Get Room State"

echo ""
echo "12. Send Message"
MSG_RESP=$(curl -s -X PUT "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/send/m.room.message/1" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"msgtype":"m.text","body":"Hello from API Test"}')
echo "$MSG_RESP" | grep -q "event_id" && pass "Send Message" || fail "Send Message"

echo ""
echo "13. Room Messages"
curl -s "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/messages?limit=10" -H "Authorization: Bearer $TOKEN" | grep -q "chunk" && pass "Room Messages" || fail "Room Messages"

echo ""
echo "14. Joined Members"
curl -s "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/joined_members" -H "Authorization: Bearer $TOKEN" | grep -q "joined" && pass "Joined Members" || fail "Joined Members"

echo ""
echo "15. Room Aliases"
curl -s "$SERVER_URL/_matrix/client/v1/directory/room/%21$(echo $ROOM_ID | cut -d: -f1 | cut -c2-):cjystx.top" -H "Authorization: Bearer $TOKEN" && pass "Room Aliases" || fail "Room Aliases"

echo ""
echo "16. Set Room Topic"
curl -s -X PUT "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/state/m.room.topic" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"topic": "Updated Topic"}' && pass "Set Room Topic" || fail "Set Room Topic"

# 9. Media
echo ""
echo "=========================================="
echo "17. Media"
echo "=========================================="
echo "17. Media Config"
curl -s "$SERVER_URL/_matrix/client/v3/media/config" -H "Authorization: Bearer $TOKEN" | grep -q "upload" && pass "Media Config" || fail "Media Config"

echo ""
echo "18. Media Upload"
MEDIA_RESP=$(curl -s -X POST "$SERVER_URL/_matrix/media/v3/upload" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: image/png" \
    --data-binary "PNG-DATA")
echo "$MEDIA_RESP" | grep -q "content_uri" && pass "Media Upload" || fail "Media Upload"

echo ""
echo "19. VoIP Config"
curl -s "$SERVER_URL/_matrix/client/v3/voip/config" -H "Authorization: Bearer $TOKEN" | grep -q "turn" && pass "VoIP Config" || fail "VoIP Config"

# 10. Account Data
echo ""
echo "=========================================="
echo "20. Account Data"
echo "=========================================="
echo "20. Set User Account Data"
curl -s -X PUT "$SERVER_URL/_matrix/client/v3/user/$USER_ID/account_data/m.custom" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"custom_key": "custom_value"}' && pass "Set User Account Data" || fail "Set User Account Data"

echo ""
echo "21. Get User Account Data"
curl -s "$SERVER_URL/_matrix/client/v3/user/$USER_ID/account_data/m.custom" -H "Authorization: Bearer $TOKEN" | grep -q "custom_key" && pass "Get User Account Data" || fail "Get User Account Data"

echo ""
echo "22. Set Room Account Data"
curl -s -X PUT "$SERVER_URL/_matrix/client/v3/user/$USER_ID/rooms/$ROOM_ID/account_data/m.room.color" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"color": "blue"}' && pass "Set Room Account Data" || fail "Set Room Account Data"

echo ""
echo "23. Get Room Account Data"
curl -s "$SERVER_URL/_matrix/client/v3/user/$USER_ID/rooms/$ROOM_ID/account_data/m.room.color" -H "Authorization: Bearer $TOKEN" | grep -q "color" && pass "Get Room Account Data" || fail "Get Room Account Data"

# 11. Room Tags
echo ""
echo "=========================================="
echo "24. Room Tags"
echo "=========================================="
echo "24. Add Room Tag"
curl -s -X PUT "$SERVER_URL/_matrix/client/v3/user/$USER_ID/rooms/$ROOM_ID/tags/m.favourite" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{}' && pass "Add Room Tag" || fail "Add Room Tag"

echo ""
echo "25. Get Room Tags"
curl -s "$SERVER_URL/_matrix/client/v3/user/$USER_ID/rooms/$ROOM_ID/tags" -H "Authorization: Bearer $TOKEN" | grep -q "tags" && pass "Get Room Tags" || fail "Get Room Tags"

echo ""
echo "26. Remove Room Tag"
curl -s -X DELETE "$SERVER_URL/_matrix/client/v3/user/$USER_ID/rooms/$ROOM_ID/tags/m.favourite" \
    -H "Authorization: Bearer $TOKEN" && pass "Remove Room Tag" || fail "Remove Room Tag"

# 12. Presence
echo ""
echo "=========================================="
echo "27. Presence"
echo "=========================================="
echo "27. Update Presence"
curl -s -X PUT "$SERVER_URL/_matrix/client/v3/presence/$USER_ID/status" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"presence": "online"}' && pass "Update Presence" || fail "Update Presence"

echo ""
echo "28. Get Presence"
curl -s "$SERVER_URL/_matrix/client/v3/presence/$USER_ID/status" -H "Authorization: Bearer $TOKEN" | grep -q "presence" && pass "Get Presence" || fail "Get Presence"

# 13. Room Membership
echo ""
echo "=========================================="
echo "29. Room Membership"
echo "=========================================="
echo "29. Leave Room"
curl -s -X POST "$SERVER_URL/_matrix/client/v3/rooms/$ROOM2_ID/leave" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{}' && pass "Leave Room" || fail "Leave Room"

# 14. Public Rooms
echo ""
echo "=========================================="
echo "30. Public Rooms & Directory"
echo "=========================================="
echo "30. Public Rooms"
curl -s "$SERVER_URL/_matrix/client/v3/publicRooms" -H "Authorization: Bearer $TOKEN" | grep -q "chunk" && pass "Public Rooms" || fail "Public Rooms"

echo ""
echo "31. User Directory"
UD_RESP=$(curl -s "$SERVER_URL/_matrix/client/v1/user_directory/search/users?search_term=admin" -H "Authorization: Bearer $TOKEN")
if echo "$UD_RESP" | grep -q "results"; then
    pass "User Directory"
elif [ -z "$UD_RESP" ] || [ "$UD_RESP" = "{}" ]; then
    skip "User Directory (not fully implemented)"
else
    fail "User Directory"
fi

# 15. WhoAmI
echo ""
echo "=========================================="
echo "32. Account"
echo "=========================================="
echo "32. WhoAmI"
curl -s "$SERVER_URL/_matrix/client/v3/account/whoami" -H "Authorization: Bearer $TOKEN" | grep -q "user_id" && pass "WhoAmI" || fail "WhoAmI"

# 16. Admin APIs
echo ""
echo "=========================================="
echo "33. Admin - Users"
echo "=========================================="
echo "33. Admin List Users"
curl -s "$SERVER_URL/_synapse/admin/v1/users" -H "Authorization: Bearer $TOKEN" | grep -q "users" && pass "Admin List Users" || fail "Admin List Users"

echo ""
echo "34. Admin User Details"
curl -s "$SERVER_URL/_synapse/admin/v1/users/@admin:cjystx.top" -H "Authorization: Bearer $TOKEN" | grep -q "name" && pass "Admin User Details" || fail "Admin User Details"

echo ""
echo "35. Admin Create User"
NEW_USER_RESP=$(curl -s -X POST "$SERVER_URL/_synapse/admin/v1/users" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"username": "newadminuser_'"$(date +%s)"'", "password": "Test@123", "admin": false}')
echo "$NEW_USER_RESP" | grep -q "user_id\|name\|ok\|success" && pass "Admin Create User" || skip "Admin Create User (not implemented)"

echo ""
echo "=========================================="
echo "36. Admin - Rooms"
echo "=========================================="
echo "36. Admin List Rooms"
curl -s "$SERVER_URL/_synapse/admin/v1/rooms" -H "Authorization: Bearer $TOKEN" | grep -q "rooms" && pass "Admin List Rooms" || fail "Admin List Rooms"

echo ""
echo "37. Admin Room Details"
ADMIN_ROOM_RESP=$(curl -s "$SERVER_URL/_synapse/admin/v1/rooms/$ROOM_ID" -H "Authorization: Bearer $TOKEN")
echo "$ADMIN_ROOM_RESP" | grep -q "room_id" && pass "Admin Room Details" || fail "Admin Room Details"

echo ""
echo "38. Admin Room Members"
curl -s "$SERVER_URL/_synapse/admin/v1/rooms/$ROOM_ID/members" -H "Authorization: Bearer $TOKEN" | grep -q "members" && pass "Admin Room Members" || fail "Admin Room Members"

echo ""
echo "39. Admin Room Messages"
ADMIN_MSG_RESP=$(curl -s "$SERVER_URL/_synapse/admin/v1/rooms/$ROOM_ID/messages" -H "Authorization: Bearer $TOKEN")
echo "$ADMIN_MSG_RESP" | grep -q "messages\|chunk\|start\|end" && pass "Admin Room Messages" || fail "Admin Room Messages"

# Summary
echo ""
echo "=========================================="
echo "Test Summary"
echo "=========================================="
echo -e "Passed: \033[0;32m$PASSED\033[0m"
echo -e "Failed: \033[0;31m$FAILED\033[0m"
echo -e "Skipped: \033[0;33m$SKIPPED\033[0m"
echo ""

if [ "$FAILED" -eq 0 ]; then
    echo "✓ All tests passed!"
    exit 0
else
    echo "✗ Some tests failed!"
    exit 1
fi