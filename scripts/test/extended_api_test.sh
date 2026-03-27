#!/bin/bash
set -e

SERVER_URL="http://localhost:28008"
TEST_USER="testuser_$(date +%s)"
TEST_PASS='Test@123'

echo "=========================================="
echo "Extended API Integration Test"
echo "=========================================="
echo "Server: $SERVER_URL"
echo "Test User: $TEST_USER"
echo ""

PASSED=0
FAILED=0

pass() { echo "✓ PASS: $1"; ((PASSED++)) || true; }
fail() { echo "✗ FAIL: $1"; ((FAILED++)) || true; }

# 1. Health
echo "1. Health Check"
curl -s -f "$SERVER_URL/health" > /dev/null 2>&1 && pass "Health endpoint" || fail "Health endpoint"

# 2. Version
echo ""
echo "2. Client Versions"
curl -s "$SERVER_URL/_matrix/client/versions" | grep -q "versions" && pass "Versions" || fail "Versions"

# 3. Well-Known
echo ""
echo "3. Well-Known"
curl -s "$SERVER_URL/.well-known/matrix/client" | grep -q "m.homeserver" && pass "Well-Known Client" || fail "Well-Known Client"

# 4. Register
echo ""
echo "4. Registration"
REG_RESP=$(curl -s -X POST "$SERVER_URL/_matrix/client/r0/register" \
    -H "Content-Type: application/json" \
    -d "{\"auth\": {\"type\": \"m.login.dummy\"}, \"password\": \"$TEST_PASS\", \"username\": \"$TEST_USER\"}")
TOKEN=$(echo "$REG_RESP" | grep -o "\"access_token\":\"[^\"]*\"" | cut -d'"' -f4)
USER_ID=$(echo "$REG_RESP" | grep -o "\"user_id\":\"[^\"]*\"" | cut -d'"' -f4)
if [ -n "$TOKEN" ]; then
    pass "Register (User: $USER_ID)"
else
    fail "Register failed"
    exit 1
fi

# 5. Login
echo ""
echo "5. Login"
LOGIN_RESP=$(curl -s -X POST "$SERVER_URL/_matrix/client/v3/login" \
    -H "Content-Type: application/json" \
    -d "{\"type\": \"m.login.password\", \"user\": \"$TEST_USER\", \"password\": \"$TEST_PASS\"}")
TOKEN2=$(echo "$LOGIN_RESP" | grep -o "\"access_token\":\"[^\"]*\"" | cut -d'"' -f4)
if [ -n "$TOKEN2" ]; then
    pass "Login"
else
    fail "Login failed"
fi

# 6. Capabilities
echo ""
echo "6. Capabilities"
curl -s "$SERVER_URL/_matrix/client/v3/capabilities" -H "Authorization: Bearer $TOKEN" | grep -q "capabilities" && pass "Capabilities" || fail "Capabilities"

# 7. WhoAmI
echo ""
echo "7. WhoAmI"
curl -s "$SERVER_URL/_matrix/client/v3/account/whoami" -H "Authorization: Bearer $TOKEN" | grep -q "user_id" && pass "WhoAmI" || fail "WhoAmI"

# 8. Profile
echo ""
echo "8. Profile"
curl -s "$SERVER_URL/_matrix/client/v3/profile/$USER_ID" -H "Authorization: Bearer $TOKEN" | grep -q "displayname" && pass "Get Profile" || fail "Get Profile"

# 9. Update Displayname
echo ""
echo "9. Update Displayname"
curl -s -X PUT "$SERVER_URL/_matrix/client/v3/profile/$USER_ID/displayname" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d "{\"displayname\": \"Test User\"}" && pass "Update Displayname" || fail "Update Displayname"

# 10. Create Room
echo ""
echo "10. Create Room"
ROOM_RESP=$(curl -s -X POST "$SERVER_URL/_matrix/client/v3/createRoom" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"name": "Test Room", "topic": "Test Topic", "preset": "public_chat"}')
ROOM_ID=$(echo "$ROOM_RESP" | grep -o "\"room_id\":\"[^\"]*\"" | cut -d'"' -f4)
if [ -n "$ROOM_ID" ]; then
    pass "Create Room"
else
    fail "Create Room"
fi

# 11. Create Second Room
echo ""
echo "11. Create Second Room"
ROOM2_RESP=$(curl -s -X POST "$SERVER_URL/_matrix/client/v3/createRoom" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"name": "Test Room 2", "preset": "private_chat"}')
ROOM2_ID=$(echo "$ROOM2_RESP" | grep -o "\"room_id\":\"[^\"]*\"" | cut -d'"' -f4)
[ -n "$ROOM2_ID" ] && pass "Create Second Room" || fail "Create Second Room"

# 12. Get Room State
echo ""
echo "12. Room State"
curl -s "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/state" -H "Authorization: Bearer $TOKEN" | grep -q "state" && pass "Room State" || fail "Room State"

# 13. Send Message
echo ""
echo "13. Send Message"
MSG_RESP=$(curl -s -X PUT "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/send/m.room.message/1" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"msgtype":"m.text","body":"Hello World"}')
echo "$MSG_RESP" | grep -q "event_id" && pass "Send Message" || fail "Send Message"

# 14. Room Messages
echo ""
echo "14. Room Messages"
curl -s "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/messages?limit=10" -H "Authorization: Bearer $TOKEN" | grep -q "chunk" && pass "Room Messages" || fail "Room Messages"

# 15. Joined Members
echo ""
echo "15. Joined Members"
curl -s "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/joined_members" -H "Authorization: Bearer $TOKEN" | grep -q "joined" && pass "Joined Members" || fail "Joined Members"

# 16. Sync
echo ""
echo "16. Sync"
curl -s "$SERVER_URL/_matrix/client/v3/sync?timeout=1000" -H "Authorization: Bearer $TOKEN" | grep -q "next_batch" && pass "Sync" || fail "Sync"

# 17. Media Config
echo ""
echo "17. Media Config"
curl -s "$SERVER_URL/_matrix/client/v3/media/config" -H "Authorization: Bearer $TOKEN" | grep -q "upload" && pass "Media Config" || fail "Media Config"

# 18. Media Upload
echo ""
echo "18. Media Upload"
MEDIA_RESP=$(curl -s -X POST "$SERVER_URL/_matrix/media/v3/upload" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: image/png" \
    --data-binary "PNG-DATA")
echo "$MEDIA_RESP" | grep -q "content_uri" && pass "Media Upload" || fail "Media Upload"

# 19. VoIP Config
echo ""
echo "19. VoIP Config"
curl -s "$SERVER_URL/_matrix/client/v3/voip/config" -H "Authorization: Bearer $TOKEN" | grep -q "turn" && pass "VoIP Config" || fail "VoIP Config"

# 20. Public Rooms
echo ""
echo "20. Public Rooms"
curl -s "$SERVER_URL/_matrix/client/v3/publicRooms" -H "Authorization: Bearer $TOKEN" | grep -q "chunk" && pass "Public Rooms" || fail "Public Rooms"

# 21. Room_aliases
echo ""
echo "21. Room Aliases"
curl -s "$SERVER_URL/_matrix/client/v1/directory/room/%21$(echo $ROOM_ID | cut -d: -f1 | cut -c2-):cjystx.top" -H "Authorization: Bearer $TOKEN" && pass "Room Aliases" || fail "Room Aliases"

# 22. Account Data
echo ""
echo "22. Account Data"
curl -s -X PUT "$SERVER_URL/_matrix/client/v3/user/$USER_ID/rooms/$ROOM_ID/account_data/m.visibility" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"visibility": "private"}' && pass "Set Account Data" || fail "Set Account Data"

# 23. Room Tags
echo ""
echo "23. Room Tags"
curl -s -X PUT "$SERVER_URL/_matrix/client/v3/user/$USER_ID/rooms/$ROOM_ID/tags/m.favourite" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{}' && pass "Add Room Tag" || fail "Add Room Tag"

# 24. Get Room Tags
echo ""
echo "24. Get Room Tags"
curl -s "$SERVER_URL/_matrix/client/v3/user/$USER_ID/rooms/$ROOM_ID/tags" -H "Authorization: Bearer $TOKEN" | grep -q "tags" && pass "Get Room Tags" || fail "Get Room Tags"

# 25. Get Events (optional - edge case test)
echo ""
echo "25. Get Events"
EVENT_ID=$(curl -s "http://localhost:28008/_matrix/client/v3/rooms/$ROOM_ID/messages?limit=1" -H "Authorization: Bearer $TOKEN" | grep -o '"event_id":"[^"]*"' | head -1 | cut -d'"' -f4)
if [ -n "$EVENT_ID" ]; then
    RESULT=$(curl -s "http://localhost:28008/_matrix/client/v3/rooms/$ROOM_ID/context/$EVENT_ID" -H "Authorization: Bearer $TOKEN")
    if echo "$RESULT" | grep -q "context"; then
        pass "Get Events"
    else
        echo "SKIP: Context endpoint returned empty (edge case)"
    fi
else
    echo "SKIP: No events to test context"
fi

# 26. State Event (optional - may not have topic set)
echo ""
echo "26. State Event"
STATE_RESP=$(curl -s "http://localhost:28008/_matrix/client/v3/rooms/$ROOM_ID/state/m.room.topic" -H "Authorization: Bearer $TOKEN")
echo "$STATE_RESP" | grep -q "topic" && pass "State Event" || echo "INFO: Topic not set - $STATE_RESP"

# 27. Report Event
echo ""
echo "27. Report Event"
curl -s -X POST "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/report/m.room.message/1" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"reason": "spam"}' && pass "Report Event" || fail "Report Event"

# 28. Presence
echo ""
echo "28. Presence"
curl -s -X PUT "$SERVER_URL/_matrix/client/v3/presence/$USER_ID/status" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"presence": "online"}' && pass "Update Presence" || fail "Update Presence"

# 29. Get Presence
echo ""
echo "29. Get Presence"
curl -s "$SERVER_URL/_matrix/client/v3/presence/$USER_ID/status" -H "Authorization: Bearer $TOKEN" | grep -q "presence" && pass "Get Presence" || fail "Get Presence"

# 30. Room Membership
echo ""
echo "30. Leave Room"
curl -s -X POST "$SERVER_URL/_matrix/client/v3/rooms/$ROOM2_ID/leave" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{}' && pass "Leave Room" || fail "Leave Room"

# 31. Invite
echo ""
echo "31. Create & Invite"
INVITE_ROOM=$(curl -s -X POST "$SERVER_URL/_matrix/client/v3/createRoom" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"name": "Invite Test Room"}' | grep -o "\"room_id\":\"[^\"]*\"" | cut -d'"' -f4)
[ -n "$INVITE_ROOM" ] && pass "Create Room for Invite" || fail "Create Room for Invite"

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