#!/bin/bash
# t1_sync.sh — T1 sync module API tests.
# Covers: /sync, /events, /joined_rooms, /my_rooms
set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

MODULE="sync"
load_creds || exit 1

# Re-login to get a fresh token — creds.env may contain a revoked token if
# prior tests (e.g. auth_compat logout/all) ran on the same account.
TOKEN1=$(login_user "e2etest1" "Test@1234")
if [ -z "$TOKEN1" ]; then
    echo "ERROR: failed to login e2etest1"
    exit 1
fi

echo ""
echo "=========================================="
echo "T1 Module: $MODULE"
echo "=========================================="

# ---------------------------------------------------------------------------
# GET /_matrix/client/v3/sync
# ---------------------------------------------------------------------------
# SYNC-001: Initial sync (no since parameter)
RESP=$(http GET "/_matrix/client/v3/sync?timeout=0" "$TOKEN1" "")
test_case "SYNC-001" "初始 sync 返回 200" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"
INITIAL_SINCE=$(echo "$RESP" | cut -f2- | python3 -c "import sys,json; print(json.load(sys.stdin).get('next_batch',''))" 2>/dev/null)

# SYNC-002: Incremental sync with valid since
RESP=$(http GET "/_matrix/client/v3/sync?since=$INITIAL_SINCE&timeout=0" "$TOKEN1" "")
test_case "SYNC-002" "增量 sync (with since) 返回 200" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# SYNC-003: Sync with timeout parameter
RESP=$(http GET "/_matrix/client/v3/sync?timeout=5000" "$TOKEN1" "")
test_case "SYNC-003" "sync with timeout=5000 返回 200" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# SYNC-004: Sync with filter parameter (basic filter)
RESP=$(http GET "/_matrix/client/v3/sync?filter=%7B%22room%22%3A%7B%22timeline%22%3A%7B%22limit%22%3A10%7D%7D%7D&timeout=0" "$TOKEN1" "")
test_case "SYNC-004" "sync with filter 参数返回 200" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# SYNC-005: Sync with full_state=true
RESP=$(http GET "/_matrix/client/v3/sync?full_state=true&timeout=0" "$TOKEN1" "")
test_case "SYNC-005" "sync with full_state=true 返回 200" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# SYNC-006: Sync without auth token (should be 401)
RESP=$(http GET "/_matrix/client/v3/sync?timeout=0" none "")
test_case "SYNC-006" "无 token sync 返回 401" "401" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# SYNC-007: Sync with invalid since — Matrix spec says 400 M_INVALID_PARAM;
# but the implementation silently treats invalid since as a fresh sync (200).
RESP=$(http GET "/_matrix/client/v3/sync?since=invalid_token&timeout=0" "$TOKEN1" "")
ACTUAL=$(echo "$RESP" | cut -f1)
if [ "$ACTUAL" = "400" ]; then
    test_case "SYNC-007" "无效 since 参数返回 400" "400" "$ACTUAL" "$(echo "$RESP" | cut -f2-)"
else
    # Server silently falls back to full sync instead of rejecting — record issue.
    test_case "SYNC-007" "无效 since 参数返回 4xx" "400" "$ACTUAL" "$(echo "$RESP" | cut -f2-)"
    record_issue "$MODULE" "Medium" "SYNC-007" \
        "GET /sync with invalid since token returns $ACTUAL (full sync). Matrix spec mandates 400 M_INVALID_PARAM for malformed since. Silent fallback risks clients missing state changes (treated as no new events when since is unknown)."
fi

# ---------------------------------------------------------------------------
# GET /_matrix/client/v3/events (deprecated endpoint)
# ---------------------------------------------------------------------------
# Matrix spec deprecates /events in favour of /sync. Server should return
# 410 Gone or 404. Actual implementation still serves it.
RESP=$(http GET "/_matrix/client/v3/events?timeout=0" "$TOKEN1" "")
ACTUAL=$(echo "$RESP" | cut -f1)
case "$ACTUAL" in
    4*)
        test_case "SYNC-008" "/events 已废弃返回 4xx (实际: $ACTUAL)" "$ACTUAL" "$ACTUAL" "$(echo "$RESP" | cut -f2-)"
        ;;
    *)
        test_case "SYNC-008" "/events 已废弃端点应返回 4xx" "4xx" "$ACTUAL" "$(echo "$RESP" | cut -f2-)"
        record_issue "$MODULE" "Low" "SYNC-008" \
            "GET /_matrix/client/v3/events is deprecated in Matrix spec but the implementation still serves events with HTTP $ACTUAL. Should return 410 Gone (or at minimum 404) to discourage client use."
        ;;
esac

# ---------------------------------------------------------------------------
# GET /_matrix/client/v3/joined_rooms
# ---------------------------------------------------------------------------
# SYNC-009: joined_rooms with valid token
RESP=$(http GET "/_matrix/client/v3/joined_rooms" "$TOKEN1" "")
test_case "SYNC-009" "joined_rooms 已登录用户返回 200" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# SYNC-010: joined_rooms without auth token
RESP=$(http GET "/_matrix/client/v3/joined_rooms" none "")
test_case "SYNC-010" "joined_rooms 无 token 返回 401" "401" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# ---------------------------------------------------------------------------
# GET /_matrix/client/v3/my_rooms
# ---------------------------------------------------------------------------
# SYNC-011: my_rooms with valid token
RESP=$(http GET "/_matrix/client/v3/my_rooms" "$TOKEN1" "")
test_case "SYNC-011" "my_rooms 已登录用户返回 200" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# SYNC-012: my_rooms without auth token
RESP=$(http GET "/_matrix/client/v3/my_rooms" none "")
test_case "SYNC-012" "my_rooms 无 token 返回 401" "401" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

emit_summary
