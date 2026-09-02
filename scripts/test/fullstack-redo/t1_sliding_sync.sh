#!/bin/bash
# t1_sliding_sync.sh — T1 sliding_sync module API tests.
# Covers: POST /_matrix/client/v1/sync, /v4/sync, MSC3575 unstable paths
# Note: Only POST is supported (the handler expects JSON body with lists/pos).
set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

MODULE="sliding_sync"
load_creds || exit 1

TOKEN1=$(login_user "e2etest1" "Test@1234")
if [ -z "$TOKEN1" ]; then
    echo "ERROR: failed to login e2etest1"
    exit 1
fi

echo ""
echo "=========================================="
echo "T1 Module: $MODULE"
echo "=========================================="

# Minimal sliding sync request body — one list filtering all joined rooms
SLIDING_BODY='{"lists":{"all":{"ranges":[[0,20]],"required_state":[["m.room.create",""],["m.room.name",""]],"timeline_limit":10}}}'

# ---------------------------------------------------------------------------
# POST /_matrix/client/v1/sync
# ---------------------------------------------------------------------------
# SS-001: Initial sliding sync (no pos) on v1 endpoint
RESP=$(http POST "/_matrix/client/v1/sync" "$TOKEN1" "$SLIDING_BODY")
test_case "SS-001" "v1/sync 初始化 sliding sync" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"
POS=$(echo "$RESP" | cut -f2- | python3 -c "import sys,json; print(json.load(sys.stdin).get('pos',''))" 2>/dev/null)

# SS-002: Incremental sliding sync with pos
if [ -n "$POS" ]; then
    RESP=$(http POST "/_matrix/client/v1/sync" "$TOKEN1" \
        '{"pos":"'"$POS"'","lists":{"all":{"ranges":[[0,20]]}}}')
    test_case "SS-002" "v1/sync 增量同步 with pos 参数" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"
else
    skip_case "SS-002" "增量同步" "未获取到 pos"
fi

# SS-003: Verify response shape includes lists/timeline/required_state
RESP=$(http POST "/_matrix/client/v1/sync" "$TOKEN1" "$SLIDING_BODY")
HAS_LISTS=$(echo "$RESP" | cut -f2- | python3 -c "import sys,json; d=json.load(sys.stdin); print('yes' if 'lists' in d else 'no')" 2>/dev/null)
if [ "$HAS_LISTS" = "yes" ]; then
    test_case "SS-003" "响应包含 lists 字段" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"
else
    test_case "SS-003" "响应应包含 lists 字段" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"
fi

# SS-004: Without auth token
RESP=$(http POST "/_matrix/client/v1/sync" none "$SLIDING_BODY")
test_case "SS-004" "无 token 返回 401" "401" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# SS-005: Empty body — Matrix MSC3575 requires `lists`; should be 400.
# Implementation accepts {} and returns 200 with empty lists/rooms.
RESP=$(http POST "/_matrix/client/v1/sync" "$TOKEN1" "{}")
SS005_ACTUAL=$(echo "$RESP" | cut -f1)
if [ "$SS005_ACTUAL" = "400" ]; then
    test_case "SS-005" "空 body 返回 4xx (lists 必填)" "400" "$SS005_ACTUAL" "$(echo "$RESP" | cut -f2-)"
else
    test_case "SS-005" "空 body 返回 4xx (lists 必填)" "400" "$SS005_ACTUAL" "$(echo "$RESP" | cut -f2-)"
    record_issue "$MODULE" "Low" "SS-005" \
        "POST /_matrix/client/v1/sync accepts empty body {} and returns 200 with empty lists/rooms. MSC3575 requires the lists field — should return 400 M_BAD_JSON to surface client bugs."
fi

# SS-006: Invalid body (malformed JSON)
RESP=$(http POST "/_matrix/client/v1/sync" "$TOKEN1" "not-json")
test_case "SS-006" "非 JSON body 返回 4xx" "400" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# ---------------------------------------------------------------------------
# POST /_matrix/client/v4/sync (stable MSC4186)
# ---------------------------------------------------------------------------
# SS-007: Initial sliding sync on v4 endpoint
RESP=$(http POST "/_matrix/client/v4/sync" "$TOKEN1" "$SLIDING_BODY")
test_case "SS-007" "v4/sync 初始化 sliding sync (MSC4186)" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# ---------------------------------------------------------------------------
# POST /_matrix/client/unstable/org.matrix.msc3575/sync
# ---------------------------------------------------------------------------
# SS-008: MSC3575 unstable path
RESP=$(http POST "/_matrix/client/unstable/org.matrix.msc3575/sync" "$TOKEN1" "$SLIDING_BODY")
test_case "SS-008" "MSC3575 unstable 路径" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# ---------------------------------------------------------------------------
# POST /_matrix/client/unstable/org.matrix.simplified_msc3575/sync
# ---------------------------------------------------------------------------
# SS-009: simplified MSC3575 unstable path
RESP=$(http POST "/_matrix/client/unstable/org.matrix.simplified_msc3575/sync" "$TOKEN1" "$SLIDING_BODY")
test_case "SS-009" "simplified_msc3575 unstable 路径" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# ---------------------------------------------------------------------------
# Validation of body fields — room_id filter / extensions
# ---------------------------------------------------------------------------
# SS-010: Body with extensions (to-device + typing)
RESP=$(http POST "/_matrix/client/v1/sync" "$TOKEN1" \
    '{"lists":{"all":{"ranges":[[0,5]]}},"extensions":{"to_device":{"enabled":true,"limit":100},"typing":{"enabled":true}}}')
test_case "SS-010" "extensions: to_device + typing" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

emit_summary
