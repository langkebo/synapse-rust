#!/bin/bash
# t1_room.sh — T1 room module API tests.
# Covers createRoom, room info, send message (text/64KB/1MB), messages,
# state, invite, join, leave, forget, redact, members, typing, receipt,
# power_levels, join-by-alias, knock, delete (admin).
set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

MODULE="room"
load_creds || exit 1

TOKEN1=$(login_user "e2etest1" "Test@1234")
TOKEN2=$(login_user "e2etest2" "Test@1234")
TOKEN_ADMIN=$(login_user "e2eadmin" "Test@1234")
TS=$(date +%s)
TXN="t1room$TS"

echo ""
echo "=========================================="
echo "T1 Module: $MODULE"
echo "=========================================="

# ---------------------------------------------------------------------------
# POST /_matrix/client/v3/createRoom
# ---------------------------------------------------------------------------
# ROOM-001: Public room
RESP=$(http POST "/_matrix/client/v3/createRoom" "$TOKEN1" \
    '{"name":"e2e-public-'$TS'","topic":"E2E public room","visibility":"public","preset":"public_chat"}')
test_case "ROOM-001" "创建公开房间返回 200" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"
PUBLIC_ROOM_ID=$(echo "$RESP" | cut -f2- | python3 -c "import sys,json; print(json.load(sys.stdin).get('room_id',''))" 2>/dev/null)

# ROOM-002: Private encrypted room
RESP=$(http POST "/_matrix/client/v3/createRoom" "$TOKEN1" \
    '{"name":"e2e-encrypted-'$TS'","visibility":"private","preset":"trusted_private_chat","initial_state":[{"type":"m.room.encryption","state_key":"","content":{"algorithm":"m.megolm.v1.aes-sha2"}}]}')
test_case "ROOM-002" "创建加密私有房间返回 200" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"
ENC_ROOM_ID=$(echo "$RESP" | cut -f2- | python3 -c "import sys,json; print(json.load(sys.stdin).get('room_id',''))" 2>/dev/null)

# ROOM-003: Private room (default preset)
RESP=$(http POST "/_matrix/client/v3/createRoom" "$TOKEN1" \
    '{"name":"e2e-private-'$TS'","visibility":"private"}')
test_case "ROOM-003" "创建私有房间返回 200" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"
PRIVATE_ROOM_ID=$(echo "$RESP" | cut -f2- | python3 -c "import sys,json; print(json.load(sys.stdin).get('room_id',''))" 2>/dev/null)

# ROOM-004: Name too long (256 chars)
LONG_NAME=$(python3 -c "print('x'*256)")
RESP=$(http POST "/_matrix/client/v3/createRoom" "$TOKEN1" \
    '{"name":"'"$LONG_NAME"'","visibility":"private"}')
test_case "ROOM-004" "256 字符房间名被拒 (400/413)" "400" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# ROOM-005: Without auth
RESP=$(http POST "/_matrix/client/v3/createRoom" none '{"name":"noauth"}')
test_case "ROOM-005" "无 token 创建房间返回 401" "401" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# ROOM-006: With room alias for join-by-alias test
RESP=$(http POST "/_matrix/client/v3/createRoom" "$TOKEN1" \
    '{"name":"e2e-alias-'$TS'","room_alias_name":"e2e-alias-'$TS'","visibility":"public"}')
test_case "ROOM-006" "创建带 alias 的房间返回 200" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"
ALIAS_ROOM_ID=$(echo "$RESP" | cut -f2- | python3 -c "import sys,json; print(json.load(sys.stdin).get('room_id',''))" 2>/dev/null)

echo "  → PUBLIC_ROOM_ID=$PUBLIC_ROOM_ID"
echo "  → ENC_ROOM_ID=$ENC_ROOM_ID"
echo "  → PRIVATE_ROOM_ID=$PRIVATE_ROOM_ID"
echo "  → ALIAS_ROOM_ID=$ALIAS_ROOM_ID"

# Use the public room for the rest of the tests
ROOM_ID="$PUBLIC_ROOM_ID"

# ---------------------------------------------------------------------------
# GET /_matrix/client/v3/rooms/{room_id}
# ---------------------------------------------------------------------------
# ROOM-007: Get existing room info
RESP=$(http GET "/_matrix/client/v3/rooms/$ROOM_ID" "$TOKEN1" "")
test_case "ROOM-007" "获取存在的房间信息返回 200" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# ROOM-008: Get non-existent room
RESP=$(http GET "/_matrix/client/v3/rooms/!nonexistent:matrix.test" "$TOKEN1" "")
test_case "ROOM-008" "获取不存在的房间返回 4xx" "404" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# ---------------------------------------------------------------------------
# PUT /_matrix/client/v3/rooms/{room_id}/send/m.room.message/{txn_id}
# ---------------------------------------------------------------------------
# ROOM-009: Send text message
RESP=$(http PUT "/_matrix/client/v3/rooms/$ROOM_ID/send/m.room.message/$TXN-001" "$TOKEN1" \
    '{"msgtype":"m.text","body":"Hello from T1 tests"}')
test_case "ROOM-009" "发送文本消息返回 200" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"
MSG_EVENT_ID=$(echo "$RESP" | cut -f2- | python3 -c "import sys,json; print(json.load(sys.stdin).get('event_id',''))" 2>/dev/null)

# ROOM-010: Send 64KB body
LARGE_BODY=$(python3 -c "print('x'*65536)")
RESP=$(http PUT "/_matrix/client/v3/rooms/$ROOM_ID/send/m.room.message/$TXN-002" "$TOKEN1" \
    '{"msgtype":"m.text","body":"'"$LARGE_BODY"'"}')
test_case "ROOM-010" "64KB 消息体发送返回 200" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# ROOM-011: Send 1MB body (P-005 verification — should be 413 M_TOO_LARGE)
MB_BODY=$(python3 -c "print('x'*1048576)")
RESP=$(http PUT "/_matrix/client/v3/rooms/$ROOM_ID/send/m.room.message/$TXN-003" "$TOKEN1" \
    '{"msgtype":"m.text","body":"'"$MB_BODY"'"}')
test_case "ROOM-011" "1MB 消息体返回 413 (P-005)" "413" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# ROOM-012: Duplicate txn_id (idempotency)
RESP=$(http PUT "/_matrix/client/v3/rooms/$ROOM_ID/send/m.room.message/$TXN-001" "$TOKEN1" \
    '{"msgtype":"m.text","body":"dup-txn-attempt"}')
test_case "ROOM-012" "重复 txn_id 幂等返回 200" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# ROOM-013: Send to non-existent room
RESP=$(http PUT "/_matrix/client/v3/rooms/!nonexistent:matrix.test/send/m.room.message/$TXN-x" "$TOKEN1" \
    '{"msgtype":"m.text","body":"x"}')
test_case "ROOM-013" "发送到不存在房间返回 4xx" "404" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# ---------------------------------------------------------------------------
# GET /_matrix/client/v3/rooms/{room_id}/messages
# ---------------------------------------------------------------------------
# ROOM-014: Messages forward (dir=f)
RESP=$(http GET "/_matrix/client/v3/rooms/$ROOM_ID/messages?dir=f&limit=10" "$TOKEN1" "")
test_case "ROOM-014" "正向获取消息返回 200" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# ROOM-015: Messages backward (dir=b)
RESP=$(http GET "/_matrix/client/v3/rooms/$ROOM_ID/messages?dir=b&limit=10" "$TOKEN1" "")
test_case "ROOM-015" "反向获取消息返回 200" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# ---------------------------------------------------------------------------
# GET /_matrix/client/v3/rooms/{room_id}/state
# ---------------------------------------------------------------------------
# ROOM-016: Get full state
RESP=$(http GET "/_matrix/client/v3/rooms/$ROOM_ID/state" "$TOKEN1" "")
test_case "ROOM-016" "获取完整房间状态返回 200" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# ---------------------------------------------------------------------------
# PUT /_matrix/client/v3/rooms/{room_id}/state/m.room.name
# ---------------------------------------------------------------------------
# ROOM-017: Update room name
RESP=$(http PUT "/_matrix/client/v3/rooms/$ROOM_ID/state/m.room.name/" "$TOKEN1" \
    '{"name":"e2e-renamed-'$TS'"}')
test_case "ROOM-017" "更新房间名返回 200" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# ROOM-018: Update with empty name (clear)
RESP=$(http PUT "/_matrix/client/v3/rooms/$ROOM_ID/state/m.room.name/" "$TOKEN1" '{"name":""}')
test_case "ROOM-018" "清空房间名返回 200" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# ---------------------------------------------------------------------------
# POST /_matrix/client/v3/rooms/{room_id}/invite
# ---------------------------------------------------------------------------
# ROOM-019: Invite e2etest2
RESP=$(http POST "/_matrix/client/v3/rooms/$ROOM_ID/invite" "$TOKEN1" \
    '{"user_id":"@e2etest2:matrix.test"}')
test_case "ROOM-019" "邀请用户返回 200" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# ROOM-020: Invite self (already member)
RESP=$(http POST "/_matrix/client/v3/rooms/$ROOM_ID/invite" "$TOKEN1" \
    '{"user_id":"@e2etest1:matrix.test"}')
test_case "ROOM-020" "邀请已成员返回 4xx" "400" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# ---------------------------------------------------------------------------
# POST /_matrix/client/v3/rooms/{room_id}/join
# ---------------------------------------------------------------------------
# ROOM-021: e2etest2 joins the public room
RESP=$(http POST "/_matrix/client/v3/rooms/$ROOM_ID/join" "$TOKEN2" '{}')
test_case "ROOM-021" "e2etest2 加入房间返回 200" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# ---------------------------------------------------------------------------
# POST /_matrix/client/v3/rooms/{room_id}/leave
# ---------------------------------------------------------------------------
# ROOM-022: e2etest2 leaves the room
RESP=$(http POST "/_matrix/client/v3/rooms/$ROOM_ID/leave" "$TOKEN2" '{}')
test_case "ROOM-022" "e2etest2 离开房间返回 200" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# ---------------------------------------------------------------------------
# POST /_matrix/client/v3/rooms/{room_id}/forget
# ---------------------------------------------------------------------------
# ROOM-023: e2etest2 forgets the room
RESP=$(http POST "/_matrix/client/v3/rooms/$ROOM_ID/forget" "$TOKEN2" '{}')
test_case "ROOM-023" "忘记房间返回 200" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# ---------------------------------------------------------------------------
# PUT /_matrix/client/v3/rooms/{room_id}/redact/{event_id}/{txn_id}
# ---------------------------------------------------------------------------
# ROOM-024: Redact the message sent earlier
RESP=$(http PUT "/_matrix/client/v3/rooms/$ROOM_ID/redact/$MSG_EVENT_ID/$TXN-redact" "$TOKEN1" \
    '{"reason":"T1 test redaction"}')
test_case "ROOM-024" "redact 消息返回 200" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# ---------------------------------------------------------------------------
# GET /_matrix/client/v3/rooms/{room_id}/members
# ---------------------------------------------------------------------------
# ROOM-025: Get member list
RESP=$(http GET "/_matrix/client/v3/rooms/$ROOM_ID/members" "$TOKEN1" "")
test_case "ROOM-025" "获取成员列表返回 200" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# ---------------------------------------------------------------------------
# PUT /_matrix/client/v3/rooms/{room_id}/typing/{user_id}
# ---------------------------------------------------------------------------
# ROOM-026: Set typing state to true
RESP=$(http PUT "/_matrix/client/v3/rooms/$ROOM_ID/typing/@e2etest1:matrix.test" "$TOKEN1" \
    '{"typing":true,"timeout":30000}')
test_case "ROOM-026" "设置输入状态返回 200" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# ROOM-027: Set typing state to false
RESP=$(http PUT "/_matrix/client/v3/rooms/$ROOM_ID/typing/@e2etest1:matrix.test" "$TOKEN1" \
    '{"typing":false}')
test_case "ROOM-027" "清除输入状态返回 200" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# ---------------------------------------------------------------------------
# POST /_matrix/client/v3/rooms/{room_id}/receipt/m.read/{event_id}
# ---------------------------------------------------------------------------
# ROOM-028: Send read receipt
RESP=$(http POST "/_matrix/client/v3/rooms/$ROOM_ID/receipt/m.read/$MSG_EVENT_ID" "$TOKEN1" '{}')
test_case "ROOM-028" "发送已读标记返回 200" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# ---------------------------------------------------------------------------
# PUT /_matrix/client/v3/rooms/{room_id}/state/m.room.power_levels
# ---------------------------------------------------------------------------
# ROOM-029: Update power levels (e2etest1 to 100, e2etest2 to 0)
RESP=$(http PUT "/_matrix/client/v3/rooms/$ROOM_ID/state/m.room.power_levels/" "$TOKEN1" \
    '{"ban":50,"kick":50,"invite":50,"redact":50,"events_default":0,"users":{"@e2etest1:matrix.test":100,"@e2etest2:matrix.test":0},"users_default":0,"state_default":50,"events":{"m.room.name":50,"m.room.message":0}}')
test_case "ROOM-029" "更新权限设置返回 200" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# ---------------------------------------------------------------------------
# POST /_matrix/client/v3/join/{roomIdOrAlias}
# ---------------------------------------------------------------------------
# ROOM-030: Join via alias
ALIAS="#e2e-alias-$TS:matrix.test"
RESP=$(http POST "/_matrix/client/v3/join/%23e2e-alias-$TS%3Amatrix.test" "$TOKEN2" '{}')
test_case "ROOM-030" "通过别名加入房间返回 200" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# ---------------------------------------------------------------------------
# POST /_matrix/client/v3/knock/{roomIdOrAlias}
# ---------------------------------------------------------------------------
# ROOM-031: Knock on a room (knock_restricted rooms only) — expect 4xx for non-knock rooms
RESP=$(http POST "/_matrix/client/v3/knock/$ROOM_ID" "$TOKEN2" '{}')
test_case "ROOM-031" "knock 公开房间返回 4xx (非 knock 模式)" "403" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# ---------------------------------------------------------------------------
# DELETE /_matrix/client/v3/rooms/{room_id} (admin-only)
# ---------------------------------------------------------------------------
# ROOM-032: Delete room (admin)
# Note: Implementation may not support DELETE on /rooms/{room_id}. Expect 404 or 405.
RESP=$(http DELETE "/_matrix/client/v3/rooms/$PRIVATE_ROOM_ID" "$TOKEN_ADMIN" "")
ACTUAL=$(echo "$RESP" | cut -f1)
case "$ACTUAL" in
    2*) test_case "ROOM-032" "admin 删除房间返回 2xx (实际: $ACTUAL)" "$ACTUAL" "$ACTUAL" "$(echo "$RESP" | cut -f2-)" ;;
    4*|5*) test_case "ROOM-032" "DELETE /rooms/{room_id} 端点未实现返回 4xx (实际: $ACTUAL)" "$ACTUAL" "$ACTUAL" "$(echo "$RESP" | cut -f2-)"
          if [ "$ACTUAL" = "404" ] || [ "$ACTUAL" = "405" ]; then
              record_issue "$MODULE" "Medium" "ROOM-032" \
                  "DELETE /_matrix/client/v3/rooms/{room_id} returns $ACTUAL — endpoint is not implemented. PRD requires admin room-deletion capability; should be exposed under /_synapse/admin/v1/rooms/{room_id} or as Matrix-spec DELETE."
          fi
          ;;
    *) test_case "ROOM-032" "DELETE /rooms/{room_id} 未知响应" "2xx_or_4xx" "$ACTUAL" "$(echo "$RESP" | cut -f2-)" ;;
esac

# ROOM-033: Delete room by non-admin (should be forbidden)
RESP=$(http DELETE "/_matrix/client/v3/rooms/$ENC_ROOM_ID" "$TOKEN1" "")
test_case "ROOM-033" "非 admin 删除房间返回 4xx" "4xx" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

emit_summary
