#!/bin/bash

# Simplified re-test script for warning and failed APIs
# This script systematically tests all APIs that had warnings or failures

BASE_URL="http://localhost:8008"
TEST_USER_TOKEN="eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAdGVzdHVzZXI6bWF0cml4LmNqeXN0eC50b3AiLCJ1c2VyX2lkIjoiQHRlc3R1c2VyOm1hdHJpeC5janlzdHgudG9wIiwiYWRtaW4iOmZhbHNlLCJleHAiOjE3NzAwOTI0MjEsImlhdCI6MTc3MDAwNjAyMSwiZGV2aWNlX2lkIjoibGNTOExhYXcwMWZHL1UrRW9SOHdIUT09In0.IMBfyvStKRfYvMB3bNM2-9UX1iHk1_qdsF-w4o7Ivpc"
TEST_ADMIN_TOKEN="eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAdGVzdGFkbWluOm1hdHJpeC5janlzdHgudG9wIiwidXNlcl9pZCI6IkB0ZXN0YWRtaW46bWF0cml4LmNqeXN0eC50b3AiLCJhZG1pbiI6dHJ1ZSwiZXhwIjoxNzcwMDkyNDI2LCJpYXQiOjE3NzAwMDYwMjYsImRldmljZV9pZCI6IkQwaTlPUzNHcnpuN0FsczNNbldPVWc9PSJ9.hQJjLomObejQQBA7y0FCU6ArZz7K7-lF_SZXRzkUKaA"
ROOM_ID="!nARAVqgdxvwdneCiXy0KW5pj:matrix.cjystx.top"
USER_ID="@testuser:matrix.cjystx.top"
ADMIN_ID="@testadmin:matrix.cjystx.top"

OUTPUT_FILE="/tmp/api_retest_results_v2.txt"
echo "=== API Re-test Results (v2) ===" > "$OUTPUT_FILE"
echo "Test Time: $(date)" >> "$OUTPUT_FILE"
echo "" >> "$OUTPUT_FILE"

test_api() {
    local name="$1"
    local method="$2"
    local url="$3"
    shift 3
    local headers_and_body="$@"
    
    echo "Testing: $name" >> "$OUTPUT_FILE"
    echo "Method: $method" >> "$OUTPUT_FILE"
    echo "URL: $url" >> "$OUTPUT_FILE"
    
    local response
    response=$(curl -s -w "\n%{http_code}" -X "$method" "$url" $headers_and_body 2>&1)
    
    local http_code=$(echo "$response" | tail -n1)
    local response_body=$(echo "$response" | sed '$d')
    
    echo "HTTP Status: $http_code" >> "$OUTPUT_FILE"
    echo "Response: $response_body" >> "$OUTPUT_FILE"
    echo "----------------------------------------" >> "$OUTPUT_FILE"
    echo "" >> "$OUTPUT_FILE"
}

# ============================================================================
# 1. Authentication Warning APIs
# ============================================================================
echo "=== 1. Authentication Warning APIs ===" >> "$OUTPUT_FILE"

# 1.1 Login with correct password
test_api "Login with correct password" \
    "POST" \
    "$BASE_URL/_matrix/client/r0/login" \
    -H "Content-Type: application/json" \
    -d '{"user":"testuser","password":"TestPass123!"}'

# 1.2 Refresh token with valid token
test_api "Refresh token" \
    "POST" \
    "$BASE_URL/_matrix/client/r0/refresh" \
    -H "Content-Type: application/json" \
    -d '{"refresh_token":"6v4CEA6tTZqlEZV7ijUqrV38EX88S8a5Xq9lmJs88qk="}'

# ============================================================================
# 2. Device Management Warning APIs
# ============================================================================
echo "=== 2. Device Management Warning APIs ===" >> "$OUTPUT_FILE"

# 2.1 Update device (PUT)
test_api "Update device (PUT)" \
    "PUT" \
    "$BASE_URL/_matrix/client/r0/devices/lcS8Laaw01fG%2FU%2BEoR8wHQ%3D%3D" \
    -H "Authorization: Bearer $TEST_USER_TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"display_name":"Test Device Updated"}'

# 2.2 Delete single device (DELETE)
test_api "Delete single device (DELETE)" \
    "DELETE" \
    "$BASE_URL/_matrix/client/r0/devices/lcS8Laaw01fG%2FU%2BEoR8wHQ%3D%3D" \
    -H "Authorization: Bearer $TEST_USER_TOKEN"

# ============================================================================
# 3. Room Management Warning APIs
# ============================================================================
echo "=== 3. Room Management Warning APIs ===" >> "$OUTPUT_FILE"

# 3.1 Delete room with admin token
test_api "Delete room (with admin)" \
    "DELETE" \
    "$BASE_URL/_matrix/client/r0/directory/room/$ROOM_ID" \
    -H "Authorization: Bearer $TEST_ADMIN_TOKEN"

# 3.2 Get room members (first join room)
test_api "Join room before getting members" \
    "POST" \
    "$BASE_URL/_matrix/client/r0/rooms/$ROOM_ID/join" \
    -H "Authorization: Bearer $TEST_USER_TOKEN"

test_api "Get room members (after joining)" \
    "GET" \
    "$BASE_URL/_matrix/client/r0/rooms/$ROOM_ID/members" \
    -H "Authorization: Bearer $TEST_USER_TOKEN"

# ============================================================================
# 4. Messaging Warning API
# ============================================================================
echo "=== 4. Messaging Warning API ===" >> "$OUTPUT_FILE"

# 4.1 Send message - test if POST is supported
test_api "Send message (POST)" \
    "POST" \
    "$BASE_URL/_matrix/client/r0/rooms/$ROOM_ID/send/m.room.message/1770008605026992206" \
    -H "Authorization: Bearer $TEST_USER_TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"msgtype":"m.text","body":"Test message from re-test"}'

# ============================================================================
# 5. Friend System Warning APIs
# ============================================================================
echo "=== 5. Friend System Warning APIs ===" >> "$OUTPUT_FILE"

# 5.1 Send friend request first
test_api "Send friend request" \
    "POST" \
    "$BASE_URL/_synapse/enhanced/friend/request" \
    -H "Authorization: Bearer $TEST_USER_TOKEN" \
    -H "Content-Type: application/json" \
    -d "{\"user_id\":\"$ADMIN_ID\",\"message\":\"Test friendship\"}"

# 5.2 Accept friend request with numeric ID
test_api "Accept friend request (numeric ID)" \
    "POST" \
    "$BASE_URL/_synapse/enhanced/friend/request/1/accept" \
    -H "Authorization: Bearer $TEST_USER_TOKEN"

# 5.3 Decline friend request with numeric ID
test_api "Decline friend request (numeric ID)" \
    "POST" \
    "$BASE_URL/_synapse/enhanced/friend/request/2/decline" \
    -H "Authorization: Bearer $TEST_USER_TOKEN"

# 5.4 Block user with correct field name
test_api "Block user (correct field)" \
    "POST" \
    "$BASE_URL/_synapse/enhanced/friend/blocks/$USER_ID" \
    -H "Authorization: Bearer $TEST_USER_TOKEN" \
    -H "Content-Type: application/json" \
    -d "{\"user_id\":\"$ADMIN_ID\",\"reason\":\"Test block\"}"

# ============================================================================
# 6. Voice Message Warning API
# ============================================================================
echo "=== 6. Voice Message Warning API ===" >> "$OUTPUT_FILE"

# 6.1 Upload voice message with valid base64
test_api "Upload voice message (valid base64)" \
    "POST" \
    "$BASE_URL/_matrix/client/r0/voice/upload" \
    -H "Authorization: Bearer $TEST_USER_TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"content":"VGVzdCBhdWRpbyBjb250ZW50","content_type":"audio/ogg","duration_ms":5000,"room_id":"'$ROOM_ID'"}'

# ============================================================================
# 7. E2EE Warning API
# ============================================================================
echo "=== 7. E2EE Warning API ===" >> "$OUTPUT_FILE"

# 7.1 Upload keys - test if POST is supported
test_api "Upload keys (POST)" \
    "POST" \
    "$BASE_URL/_matrix/client/r0/keys/upload/lcS8Laaw01fG%2FU%2BEoR8wHQ%3D%3D" \
    -H "Authorization: Bearer $TEST_USER_TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"device_keys":{"user_id":"'$USER_ID'","device_id":"lcS8Laaw01fG/U+EoR8wHQ==","algorithms":["m.olm.v1.curve25519-aes-sha2"],"keys":{},"signatures":{}},"one_time_keys":{}}'

# ============================================================================
# 8. Media Upload Warning APIs
# ============================================================================
echo "=== 8. Media Upload Warning APIs ===" >> "$OUTPUT_FILE"

# 8.1 Upload media with multipart/form-data
echo "Creating test media file..." >> "$OUTPUT_FILE"
echo "Test media content" > /tmp/test_media.jpg

test_api "Upload media v3 (multipart)" \
    "POST" \
    "$BASE_URL/_matrix/media/v3/upload?filename=test.jpg" \
    -H "Authorization: Bearer $TEST_USER_TOKEN" \
    -F "file=@/tmp/test_media.jpg;type=image/jpeg"

test_api "Upload media v1 (multipart)" \
    "POST" \
    "$BASE_URL/_matrix/media/v1/upload?filename=test.jpg" \
    -H "Authorization: Bearer $TEST_USER_TOKEN" \
    -F "file=@/tmp/test_media.jpg;type=image/jpeg"

# ============================================================================
# 9. Private Chat Warning APIs
# ============================================================================
echo "=== 9. Private Chat Warning APIs ===" >> "$OUTPUT_FILE"

# 9.1 Get private chat room details
test_api "Get private chat room details" \
    "GET" \
    "$BASE_URL/_matrix/client/r0/rooms/$ROOM_ID/dm" \
    -H "Authorization: Bearer $TEST_USER_TOKEN"

# 9.2 Get session details
test_api "Get session details" \
    "GET" \
    "$BASE_URL/_synapse/enhanced/private/sessions/ps_cb983f758d62482087e2d30f82d5c254" \
    -H "Authorization: Bearer $TEST_USER_TOKEN"

# 9.3 Delete message with valid UUID format
test_api "Delete message (valid ID)" \
    "DELETE" \
    "$BASE_URL/_synapse/enhanced/private/messages/550e8400-e29b-41d4-a716-446655440000" \
    -H "Authorization: Bearer $TEST_USER_TOKEN"

# ============================================================================
# 10. Key Backup Warning APIs
# ============================================================================
echo "=== 10. Key Backup Warning APIs ===" >> "$OUTPUT_FILE"

# 10.1 Create backup version first
test_api "Create backup version" \
    "POST" \
    "$BASE_URL/_matrix/client/r0/room_keys/version" \
    -H "Authorization: Bearer $TEST_USER_TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"algorithm":"m.megolm_backup.v1","auth_data":{}}'

# 10.2 Get backup version with correct version
test_api "Get backup version (latest)" \
    "GET" \
    "$BASE_URL/_matrix/client/r0/room_keys/version/1770008605" \
    -H "Authorization: Bearer $TEST_USER_TOKEN"

# 10.3 Update backup version
test_api "Update backup version" \
    "PUT" \
    "$BASE_URL/_matrix/client/r0/room_keys/version/1770008605" \
    -H "Authorization: Bearer $TEST_USER_TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"algorithm":"m.megolm_backup.v1","auth_data":{}}'

# 10.4 Get room keys
test_api "Get room keys" \
    "GET" \
    "$BASE_URL/_matrix/client/r0/room_keys/1770008605" \
    -H "Authorization: Bearer $TEST_USER_TOKEN"

# 10.5 Upload room keys
test_api "Upload room keys" \
    "PUT" \
    "$BASE_URL/_matrix/client/r0/room_keys/1770008605" \
    -H "Authorization: Bearer $TEST_USER_TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"rooms":{}}'

# 10.6 Get room keys by room ID
test_api "Get room keys by room ID" \
    "GET" \
    "$BASE_URL/_matrix/client/r0/room_keys/1770008605/keys/$ROOM_ID" \
    -H "Authorization: Bearer $TEST_USER_TOKEN"

# 10.7 Get session key
test_api "Get session key" \
    "GET" \
    "$BASE_URL/_matrix/client/r0/room_keys/1770008605/keys/$ROOM_ID/session123" \
    -H "Authorization: Bearer $TEST_USER_TOKEN"

# ============================================================================
# 11. Failed APIs - Voice Message
# ============================================================================
echo "=== 11. Failed APIs - Voice Message ===" >> "$OUTPUT_FILE"

# 11.1 Get voice message
test_api "Get voice message" \
    "GET" \
    "$BASE_URL/_matrix/client/r0/voice/msg123" \
    ""

# 11.2 Get user voice messages
test_api "Get user voice messages" \
    "GET" \
    "$BASE_URL/_matrix/client/r0/voice/user/$USER_ID" \
    ""

# 11.3 Get room voice messages
test_api "Get room voice messages" \
    "GET" \
    "$BASE_URL/_matrix/client/r0/voice/room/$ROOM_ID" \
    ""

# ============================================================================
# 12. Failed APIs - E2EE
# ============================================================================
echo "=== 12. Failed APIs - E2EE ===" >> "$OUTPUT_FILE"

# 12.1 Query keys
test_api "Query keys" \
    "POST" \
    "$BASE_URL/_matrix/client/r0/keys/query" \
    -H "Authorization: Bearer $TEST_USER_TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"device_keys":{"'$USER_ID':[]}}'

# ============================================================================
# 13. Failed APIs - Private Chat
# ============================================================================
echo "=== 13. Failed APIs - Private Chat ===" >> "$OUTPUT_FILE"

# 13.1 Create session (will fail if not friends)
test_api "Create session" \
    "POST" \
    "$BASE_URL/_synapse/enhanced/private/sessions" \
    -H "Authorization: Bearer $TEST_USER_TOKEN" \
    -H "Content-Type: application/json" \
    -d "{\"user_id\":\"$ADMIN_ID\",\"title\":\"Private Chat\"}"

# 13.2 Send session message
test_api "Send session message" \
    "POST" \
    "$BASE_URL/_synapse/enhanced/private/sessions/ps_cb983f758d62482087e2d30f82d5c254/messages" \
    -H "Authorization: Bearer $TEST_USER_TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"content":"Test private message"}'

echo "=== Re-test Complete ===" >> "$OUTPUT_FILE"
echo "Results saved to: $OUTPUT_FILE"
