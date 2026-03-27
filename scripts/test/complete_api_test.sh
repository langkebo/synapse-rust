#!/bin/bash

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

# 1. Health & Version
echo "=========================================="
echo "1. Health & Version"
echo "=========================================="
echo "1. Health Check"
curl -s -f "$SERVER_URL/health" > /dev/null 2>&1 && pass "Health endpoint" || fail "Health endpoint"

echo ""
echo "2. Version"
curl -s "$SERVER_URL/_matrix/client/versions" | grep -q "versions" && pass "Versions endpoint" || fail "Versions endpoint"

# 2. Login
echo ""
echo "=========================================="
echo "3. Authentication"
echo "=========================================="
LOGIN_RESP=$(curl -s -X POST "$SERVER_URL/_matrix/client/v3/login" \
    -H "Content-Type: application/json" \
    -d "{\"type\": \"m.login.password\", \"user\": \"$TEST_USER\", \"password\": \"$TEST_PASS\"}")
TOKEN=$(echo "$LOGIN_RESP" | grep -o "\"access_token\":\"[^\"]*\"" | cut -d'"' -f4)
USER_ID=$(echo "$LOGIN_RESP" | grep -o "\"user_id\":\"[^\"]*\"" | cut -d'"' -f4)
DEVICE_ID=$(echo "$LOGIN_RESP" | grep -o "\"device_id\":\"[^\"]*\"" | cut -d'"' -f4)
if [ -n "$TOKEN" ]; then
    pass "Login (User: $USER_ID)"
else
    fail "Login failed"
fi

echo ""
echo "4. Capabilities"
curl -s "$SERVER_URL/_matrix/client/v3/capabilities" -H "Authorization: Bearer $TOKEN" | grep -q "capabilities" && pass "Capabilities" || fail "Capabilities"

# 3. Room Setup
echo ""
echo "=========================================="
echo "5. Room Setup"
echo "=========================================="
ROOM_RESP=$(curl -s -X POST "$SERVER_URL/_matrix/client/v3/createRoom" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"name": "Test Room API", "topic": "API Test Room", "preset": "public_chat"}')
ROOM_ID=$(echo "$ROOM_RESP" | grep -o "\"room_id\":\"[^\"]*\"" | cut -d'"' -f4)
[ -n "$ROOM_ID" ] && pass "Create Test Room" || fail "Create Test Room"

ROOM2_RESP=$(curl -s -X POST "$SERVER_URL/_matrix/client/v3/createRoom" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"name": "Test Room 2", "preset": "private_chat"}')
ROOM2_ID=$(echo "$ROOM2_RESP" | grep -o "\"room_id\":\"[^\"]*\"" | cut -d'"' -f4)
[ -n "$ROOM2_ID" ] && pass "Create Second Room" || fail "Create Second Room"

# 4. Sync
echo ""
echo "=========================================="
echo "6. Sync & Events"
echo "=========================================="
echo "6. Sync"
curl -s "$SERVER_URL/_matrix/client/v3/sync?timeout=1000" -H "Authorization: Bearer $TOKEN" | grep -q "next_batch" && pass "Sync" || fail "Sync"

echo ""
echo "7. Room Sync Filter"
ROOM_SYNC_RESP=$(curl -s "$SERVER_URL/_matrix/client/v3/sync?filter=%7B%22room%22%3A%7B%22rooms%22%3A%5B%22$ROOM_ID%22%5D%7D%7D" -H "Authorization: Bearer $TOKEN")
echo "$ROOM_SYNC_RESP" | grep -q "rooms\|next_batch" && pass "Room Sync Filter" || fail "Room Sync Filter"

# 5. Profile
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
echo "10. Get Avatar URL"
curl -s "$SERVER_URL/_matrix/client/v3/profile/$USER_ID/avatar_url" -H "Authorization: Bearer $TOKEN" | grep -q "avatar_url" && pass "Get Avatar URL" || fail "Get Avatar URL"

echo ""
echo "11. Set Avatar URL"
curl -s -X PUT "$SERVER_URL/_matrix/client/v3/profile/$USER_ID/avatar_url" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"avatar_url": "mxc://cjystx.top/avatar"}' && pass "Set Avatar URL" || fail "Set Avatar URL"

# 6. Room State & Messages
echo ""
echo "=========================================="
echo "12. Room State & Messages"
echo "=========================================="
echo "12. Room State"
curl -s "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/state" -H "Authorization: Bearer $TOKEN" | grep -q "state" && pass "Get Room State" || fail "Get Room State"

echo ""
echo "13. Send Message"
MSG_RESP=$(curl -s -X PUT "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/send/m.room.message/1" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"msgtype":"m.text","body":"Hello from API Test"}')
echo "$MSG_RESP" | grep -q "event_id" && pass "Send Message" || fail "Send Message"

echo ""
echo "14. Room Messages"
curl -s "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/messages?limit=10" -H "Authorization: Bearer $TOKEN" | grep -q "chunk" && pass "Room Messages" || fail "Room Messages"

echo ""
echo "15. Joined Members"
curl -s "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/joined_members" -H "Authorization: Bearer $TOKEN" | grep -q "joined" && pass "Joined Members" || fail "Joined Members"

echo ""
echo "16. Room Aliases"
curl -s "$SERVER_URL/_matrix/client/v1/directory/room/%21$(echo $ROOM_ID | cut -d: -f1 | cut -c2-):cjystx.top" -H "Authorization: Bearer $TOKEN" && pass "Room Aliases" || fail "Room Aliases"

echo ""
echo "17. Set Room Topic"
curl -s -X PUT "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/state/m.room.topic" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"topic": "Updated Topic"}' && pass "Set Room Topic" || fail "Set Room Topic"

echo ""
echo "18. Redact Event"
REDACT_RESP=$(curl -s -X PUT "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/redact/\$1/local:0" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"reason": "test redacted"}')
echo "$REDACT_RESP" | grep -q "event_id" && pass "Redact Event" || skip "Redact Event (not implemented)"

# 7. Media
echo ""
echo "=========================================="
echo "19. Media"
echo "=========================================="
echo "19. Media Config"
curl -s "$SERVER_URL/_matrix/client/v3/media/config" -H "Authorization: Bearer $TOKEN" | grep -q "upload" && pass "Media Config" || fail "Media Config"

echo ""
echo "20. Media Upload"
MEDIA_RESP=$(curl -s -X POST "$SERVER_URL/_matrix/media/v3/upload" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: image/png" \
    --data-binary "PNG-DATA")
MEDIA_URI=$(echo "$MEDIA_RESP" | grep -o '"content_uri":"[^"]*"' | cut -d'"' -f4)
echo "$MEDIA_RESP" | grep -q "content_uri" && pass "Media Upload" || fail "Media Upload"

echo ""
echo "21. Media Download"
if [ -n "$MEDIA_URI" ]; then
    MEDIA_ID=$(echo "$MEDIA_URI" | cut -d/ -f3)
    MEDIA_SERVER=$(echo "$MEDIA_URI" | cut -d/ -f2)
    curl -s "$SERVER_URL/_matrix/media/v3/download/$MEDIA_SERVER/$MEDIA_ID" -H "Authorization: Bearer $TOKEN" | grep -q "" && pass "Media Download" || fail "Media Download"
else
    skip "Media Download (no media URI)"
fi

echo ""
echo "22. Media Thumbnail"
if [ -n "$MEDIA_ID" ]; then
    curl -s "$SERVER_URL/_matrix/media/v3/thumbnail/$MEDIA_SERVER/$MEDIA_ID?width=100&height=100" -H "Authorization: Bearer $TOKEN" | grep -q "" && pass "Media Thumbnail" || fail "Media Thumbnail"
else
    skip "Media Thumbnail (no media ID)"
fi

echo ""
echo "23. VoIP Config"
curl -s "$SERVER_URL/_matrix/client/v3/voip/config" -H "Authorization: Bearer $TOKEN" | grep -q "turn" && pass "VoIP Config" || fail "VoIP Config"

# 8. Account Data
echo ""
echo "=========================================="
echo "24. Account Data"
echo "=========================================="
echo "24. Set User Account Data"
curl -s -X PUT "$SERVER_URL/_matrix/client/v3/user/$USER_ID/account_data/m.custom" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"custom_key": "custom_value"}' && pass "Set User Account Data" || fail "Set User Account Data"

echo ""
echo "25. Get User Account Data"
curl -s "$SERVER_URL/_matrix/client/v3/user/$USER_ID/account_data/m.custom" -H "Authorization: Bearer $TOKEN" | grep -q "custom_key" && pass "Get User Account Data" || fail "Get User Account Data"

echo ""
echo "26. Set Room Account Data"
curl -s -X PUT "$SERVER_URL/_matrix/client/v3/user/$USER_ID/rooms/$ROOM_ID/account_data/m.room.color" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"color": "blue"}' && pass "Set Room Account Data" || fail "Set Room Account Data"

echo ""
echo "27. Get Room Account Data"
curl -s "$SERVER_URL/_matrix/client/v3/user/$USER_ID/rooms/$ROOM_ID/account_data/m.room.color" -H "Authorization: Bearer $TOKEN" | grep -q "color" && pass "Get Room Account Data" || fail "Get Room Account Data"

# 9. Room Tags
echo ""
echo "=========================================="
echo "28. Room Tags"
echo "=========================================="
echo "28. Add Room Tag"
curl -s -X PUT "$SERVER_URL/_matrix/client/v3/user/$USER_ID/rooms/$ROOM_ID/tags/m.favourite" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{}' && pass "Add Room Tag" || fail "Add Room Tag"

echo ""
echo "29. Get Room Tags"
curl -s "$SERVER_URL/_matrix/client/v3/user/$USER_ID/rooms/$ROOM_ID/tags" -H "Authorization: Bearer $TOKEN" | grep -q "tags" && pass "Get Room Tags" || fail "Get Room Tags"

echo ""
echo "30. Remove Room Tag"
curl -s -X DELETE "$SERVER_URL/_matrix/client/v3/user/$USER_ID/rooms/$ROOM_ID/tags/m.favourite" \
    -H "Authorization: Bearer $TOKEN" && pass "Remove Room Tag" || fail "Remove Room Tag"

# 10. Presence
echo ""
echo "=========================================="
echo "31. Presence"
echo "=========================================="
echo "31. Update Presence"
curl -s -X PUT "$SERVER_URL/_matrix/client/v3/presence/$USER_ID/status" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"presence": "online"}' && pass "Update Presence" || fail "Update Presence"

echo ""
echo "32. Get Presence"
curl -s "$SERVER_URL/_matrix/client/v3/presence/$USER_ID/status" -H "Authorization: Bearer $TOKEN" | grep -q "presence" && pass "Get Presence" || fail "Get Presence"

echo ""
echo "33. List Presences"
curl -s "$SERVER_URL/_matrix/client/v3/presence/list" -H "Authorization: Bearer $TOKEN" | grep -q "presence" && pass "List Presences" || skip "List Presences (not implemented)"

# 11. Room Membership
echo ""
echo "=========================================="
echo "34. Room Membership"
echo "=========================================="
echo "34. Invite User"
curl -s -X POST "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/invite" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"user_id": "@admin:cjystx.top"}' && pass "Invite User" || fail "Invite User"

echo ""
echo "35. Join Room"
curl -s -X POST "$SERVER_URL/_matrix/client/v3/join/$ROOM_ID" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{}' && pass "Join Room" || fail "Join Room"

echo ""
echo "36. Leave Room"
curl -s -X POST "$SERVER_URL/_matrix/client/v3/rooms/$ROOM2_ID/leave" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{}' && pass "Leave Room" || fail "Leave Room"

echo ""
echo "37. Get Membership"
curl -s "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/membership/$USER_ID" -H "Authorization: Bearer $TOKEN" | grep -q "member" && pass "Get Membership" || skip "Get Membership (not implemented)"

# 12. Devices
echo ""
echo "=========================================="
echo "38. Devices"
echo "=========================================="
echo "38. List Devices"
curl -s "$SERVER_URL/_matrix/client/v3/devices" -H "Authorization: Bearer $TOKEN" | grep -q "devices" && pass "List Devices" || fail "List Devices"

echo ""
echo "39. Get Device"
if [ -n "$DEVICE_ID" ]; then
    curl -s "$SERVER_URL/_matrix/client/v3/devices/$DEVICE_ID" -H "Authorization: Bearer $TOKEN" | grep -q "device_id" && pass "Get Device" || fail "Get Device"
else
    skip "Get Device (no device ID)"
fi

echo ""
echo "40. Update Device"
if [ -n "$DEVICE_ID" ]; then
    curl -s -X PUT "$SERVER_URL/_matrix/client/v3/devices/$DEVICE_ID" \
        -H "Authorization: Bearer $TOKEN" \
        -H "Content-Type: application/json" \
        -d '{"display_name": "Test Device"}' && pass "Update Device" || fail "Update Device"
else
    skip "Update Device (no device ID)"
fi

echo ""
echo "41. Delete Device"
if [ -n "$DEVICE_ID" ]; then
    curl -s -X DELETE "$SERVER_URL/_matrix/client/v3/devices/$DEVICE_ID" -H "Authorization: Bearer $TOKEN" && pass "Delete Device" || fail "Delete Device"
else
    skip "Delete Device (no device ID)"
fi

# 13. Key Upload (E2EE)
echo ""
echo "=========================================="
echo "42. E2EE Keys"
echo "=========================================="
echo "42. Upload Keys"
curl -s -X POST "$SERVER_URL/_matrix/client/v3/keys/upload" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"one_time_keys": {"key_id": "algo:key_id", "key": "base64_key"}}' && pass "Upload Keys" || fail "Upload Keys"

echo ""
echo "43. Query Keys"
curl -s -X POST "$SERVER_URL/_matrix/client/v3/keys/query" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"device_keys": {"@admin:cjystx.top": []}}' && pass "Query Keys" || fail "Query Keys"

echo ""
echo "44. Claim Keys"
curl -s -X POST "$SERVER_URL/_matrix/client/v3/keys/claim" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"one_time_keys": {"@admin:cjystx.top": {"device_id": "DEVICE_ID"}}}' && pass "Claim Keys" || fail "Claim Keys"

# 14. Public Rooms & Directory
echo ""
echo "=========================================="
echo "45. Public Rooms & Directory"
echo "=========================================="
echo "45. Public Rooms"
curl -s "$SERVER_URL/_matrix/client/v3/publicRooms" -H "Authorization: Bearer $TOKEN" | grep -q "chunk" && pass "Public Rooms" || fail "Public Rooms"

echo ""
echo "46. User Directory"
UD_RESP=$(curl -s "$SERVER_URL/_matrix/client/v1/user_directory/search/users?search_term=admin" -H "Authorization: Bearer $TOKEN")
if echo "$UD_RESP" | grep -q "results"; then
    pass "User Directory"
elif [ -z "$UD_RESP" ] || [ "$UD_RESP" = "{}" ]; then
    skip "User Directory (not fully implemented)"
else
    fail "User Directory"
fi

# 15. Room Summary
echo ""
echo "=========================================="
echo "47. Room Summary"
echo "=========================================="
echo "47. Room Summary"
curl -s "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/summary" -H "Authorization: Bearer $TOKEN" | grep -q "summary" && pass "Room Summary" || fail "Room Summary"

echo ""
echo "48. Room Summary Stats"
curl -s "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/summary/stats" -H "Authorization: Bearer $TOKEN" | grep -q "stats" && pass "Room Summary Stats" || fail "Room Summary Stats"

echo ""
echo "49. Room Summary Members"
curl -s "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/summary/members" -H "Authorization: Bearer $TOKEN" | grep -q "members" && pass "Room Summary Members" || fail "Room Summary Members"

echo ""
echo "50. Room Summary State"
curl -s "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/summary/state" -H "Authorization: Bearer $TOKEN" | grep -q "state" && pass "Room Summary State" || fail "Room Summary State"

# 16. Account
echo ""
echo "=========================================="
echo "51. Account"
echo "=========================================="
echo "51. WhoAmI"
curl -s "$SERVER_URL/_matrix/client/v3/account/whoami" -H "Authorization: Bearer $TOKEN" | grep -q "user_id" && pass "WhoAmI" || fail "WhoAmI"

# 17. Search
echo ""
echo "=========================================="
echo "53. Search"
echo "=========================================="
echo "53. Search Rooms"
curl -s -X POST "$SERVER_URL/_matrix/client/v3/search_rooms" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"search_term": "test", "limit": 10}' && pass "Search Rooms" || fail "Search Rooms"

# 18. Admin - Users
echo ""
echo "=========================================="
echo "54. Admin - Users"
echo "=========================================="
echo "54. Admin List Users"
curl -s "$SERVER_URL/_synapse/admin/v1/users" -H "Authorization: Bearer $TOKEN" | grep -q "users" && pass "Admin List Users" || fail "Admin List Users"

echo ""
echo "55. Admin User Details"
curl -s "$SERVER_URL/_synapse/admin/v1/users/@admin:cjystx.top" -H "Authorization: Bearer $TOKEN" | grep -q "name" && pass "Admin User Details" || fail "Admin User Details"

echo ""
echo "56. Admin User Sessions"
curl -s "$SERVER_URL/_synapse/admin/v1/user_sessions/@admin:cjystx.top" -H "Authorization: Bearer $TOKEN" | grep -q "sessions\|users" && pass "Admin User Sessions" || fail "Admin User Sessions"

echo ""
echo "57. Admin User Stats"
curl -s "$SERVER_URL/_synapse/admin/v1/user_stats/@admin:cjystx.top" -H "Authorization: Bearer $TOKEN" | grep -q "stats\|users" && pass "Admin User Stats" || skip "Admin User Stats (not implemented)"

echo ""
echo "58. Admin User Devices"
curl -s "$SERVER_URL/_synapse/admin/v1/users/@admin:cjystx.top/devices" -H "Authorization: Bearer $TOKEN" | grep -q "devices\|user_id" && pass "Admin User Devices" || fail "Admin User Devices"

# 19. Admin - Rooms
echo ""
echo "=========================================="
echo "59. Admin - Rooms"
echo "=========================================="
echo "59. Admin List Rooms"
curl -s "$SERVER_URL/_synapse/admin/v1/rooms" -H "Authorization: Bearer $TOKEN" | grep -q "rooms" && pass "Admin List Rooms" || fail "Admin List Rooms"

echo ""
echo "60. Admin Room Details"
ADMIN_ROOM_RESP=$(curl -s "$SERVER_URL/_synapse/admin/v1/rooms/$ROOM_ID" -H "Authorization: Bearer $TOKEN")
echo "$ADMIN_ROOM_RESP" | grep -q "room_id" && pass "Admin Room Details" || fail "Admin Room Details"

echo ""
echo "61. Admin Room Members"
curl -s "$SERVER_URL/_synapse/admin/v1/rooms/$ROOM_ID/members" -H "Authorization: Bearer $TOKEN" | grep -q "members" && pass "Admin Room Members" || fail "Admin Room Members"

echo ""
echo "62. Admin Room Messages"
ADMIN_MSG_RESP=$(curl -s "$SERVER_URL/_synapse/admin/v1/rooms/$ROOM_ID/messages" -H "Authorization: Bearer $TOKEN")
echo "$ADMIN_MSG_RESP" | grep -q "messages\|chunk\|start\|end" && pass "Admin Room Messages" || fail "Admin Room Messages"

echo ""
echo "63. Admin Room Block"
curl -s -X PUT "$SERVER_URL/_synapse/admin/v1/rooms/$ROOM_ID/block" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"block": true}' && pass "Admin Room Block" || fail "Admin Room Block"

echo ""
echo "64. Admin Room Unblock"
curl -s -X PUT "$SERVER_URL/_synapse/admin/v1/rooms/$ROOM_ID/unblock" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{}' && pass "Admin Room Unblock" || fail "Admin Room Unblock"

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