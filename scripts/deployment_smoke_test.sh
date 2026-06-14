#!/usr/bin/env bash
# =============================================================================
# synapse-rust 部署烟雾测试 (deployment smoke test)
# =============================================================================
# 用途: 部署后验证 worker topology / route ownership / replication 边界
# 版本: v0.1 (2026-06-14)
# 对应文档: docs/synapse-rust/WORKER_TOPOLOGY_BASELINE_2026-06-14.md
#
# 用法:
#   ADMIN_ENDPOINT=http://127.0.0.1:8008 bash scripts/deployment_smoke_test.sh
#   或通过环境变量覆盖:
#   ADMIN_AUTH_HEADER="Authorization: Bearer <admin_access_token>"
#   REPLICATION_SECRET="<worker_replication_secret>"
#   CLIENT_ENDPOINT=http://127.0.0.1:8101
#   SYNC_ENDPOINT=http://127.0.0.1:8103
#   MEDIA_ENDPOINT=http://127.0.0.1:8104
#   FEDERATION_ENDPOINT=http://127.0.0.1:8449
#   REPLICATION_ENDPOINT=http://127.0.0.1:9101
# =============================================================================

set -euo pipefail

# —— 配置 ——
ADMIN_ENDPOINT="${ADMIN_ENDPOINT:-http://127.0.0.1:8008}"
CLIENT_ENDPOINT="${CLIENT_ENDPOINT:-http://127.0.0.1:8008}"
SYNC_ENDPOINT="${SYNC_ENDPOINT:-http://127.0.0.1:8008}"
MEDIA_ENDPOINT="${MEDIA_ENDPOINT:-http://127.0.0.1:8008}"
FEDERATION_ENDPOINT="${FEDERATION_ENDPOINT:-http://127.0.0.1:8008}"
REPLICATION_ENDPOINT="${REPLICATION_ENDPOINT:-http://127.0.0.1:8008}"
ADMIN_AUTH_HEADER="${ADMIN_AUTH_HEADER:-}"
REPLICATION_SECRET="${REPLICATION_SECRET:-}"
SMOKE_WORKER_ID="${SMOKE_WORKER_ID:-smoke-worker-$$-$(date +%s)}"
SMOKE_PEER_WORKER_ID="${SMOKE_PEER_WORKER_ID:-${SMOKE_WORKER_ID}-peer}"
SMOKE_WORKER_NAME="${SMOKE_WORKER_NAME:-Deployment Smoke Worker}"
SMOKE_PEER_WORKER_NAME="${SMOKE_PEER_WORKER_NAME:-Deployment Smoke Peer Worker}"
SMOKE_WORKER_HOST="${SMOKE_WORKER_HOST:-127.0.0.1}"
SMOKE_PEER_WORKER_HOST="${SMOKE_PEER_WORKER_HOST:-127.0.0.1}"
SMOKE_WORKER_PORT="${SMOKE_WORKER_PORT:-19001}"
SMOKE_PEER_WORKER_PORT="${SMOKE_PEER_WORKER_PORT:-19002}"
SMOKE_STREAM_NAME="${SMOKE_STREAM_NAME:-events}"
SMOKE_STREAM_POSITION="${SMOKE_STREAM_POSITION:-424242}"
SMOKE_TASK_TYPE="${SMOKE_TASK_TYPE:-smoke_test}"

# 可通过环境变量跳过某些检查
SKIP_TOPOLOGY="${SKIP_TOPOLOGY:-0}"
SKIP_VERSIONS="${SKIP_VERSIONS:-0}"
SKIP_CLIENT="${SKIP_CLIENT:-0}"
SKIP_REPLICATION="${SKIP_REPLICATION:-0}"
SKIP_WORKER_LIFECYCLE="${SKIP_WORKER_LIFECYCLE:-0}"
TIMEOUT="${SMOKE_TIMEOUT:-10}"

PASS=0
FAIL=0
WARN=0
REQUEST_STATUS=""
REQUEST_BODY=""

# —— 工具函数 ——

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

pass_note() {
    echo -e "  ${GREEN}PASS${NC} $1"
    PASS=$((PASS + 1))
}

fail_note() {
    echo -e "  ${RED}FAIL${NC} $1"
    FAIL=$((FAIL + 1))
}

warn_note() {
    echo -e "  ${YELLOW}WARN${NC} $1"
    WARN=$((WARN + 1))
}

request() {
    local method="$1"
    local url="$2"
    local header="${3:-}"
    local body="${4:-}"
    local body_file
    body_file=$(mktemp)

    if [ -n "$header" ] && [ -n "$body" ]; then
        REQUEST_STATUS=$(curl -s -o "$body_file" -w "%{http_code}" --max-time "$TIMEOUT" \
            -X "$method" -H "$header" -H "Content-Type: application/json" -d "$body" "$url" 2>/dev/null || echo "000")
    elif [ -n "$header" ]; then
        REQUEST_STATUS=$(curl -s -o "$body_file" -w "%{http_code}" --max-time "$TIMEOUT" \
            -X "$method" -H "$header" "$url" 2>/dev/null || echo "000")
    elif [ -n "$body" ]; then
        REQUEST_STATUS=$(curl -s -o "$body_file" -w "%{http_code}" --max-time "$TIMEOUT" \
            -X "$method" -H "Content-Type: application/json" -d "$body" "$url" 2>/dev/null || echo "000")
    else
        REQUEST_STATUS=$(curl -s -o "$body_file" -w "%{http_code}" --max-time "$TIMEOUT" \
            -X "$method" "$url" 2>/dev/null || echo "000")
    fi

    REQUEST_BODY=$(cat "$body_file" 2>/dev/null || echo "")
    rm -f "$body_file"
}

json_extract() {
    local body="$1"
    local expression="$2"
    printf '%s' "$body" | python3 -c "import json,sys; data=json.load(sys.stdin); value=$expression; print(value if value is not None else '')" 2>/dev/null
}

check() {
    local name="$1"
    local url="$2"
    local expected_status="${3:-200}"
    local header="${4:-}"

    local status
    if [ -n "$header" ]; then
        status=$(curl -s -o /dev/null -w "%{http_code}" --max-time "$TIMEOUT" -H "$header" "$url" 2>/dev/null || echo "000")
    else
        status=$(curl -s -o /dev/null -w "%{http_code}" --max-time "$TIMEOUT" "$url" 2>/dev/null || echo "000")
    fi

    if [ "$status" = "$expected_status" ]; then
        pass_note "$name (HTTP $status)"
        return 0
    else
        fail_note "$name (expected HTTP $expected_status, got $status)"
        return 1
    fi
}

check_json() {
    local name="$1"
    local url="$2"
    local expected_status="${3:-200}"
    local header="${4:-}"
    local body
    local status

    if [ -n "$header" ]; then
        body=$(curl -s --max-time "$TIMEOUT" -H "$header" "$url" 2>/dev/null || echo "{}")
        status=$(curl -s -o /dev/null -w "%{http_code}" --max-time "$TIMEOUT" -H "$header" "$url" 2>/dev/null || echo "000")
    else
        body=$(curl -s --max-time "$TIMEOUT" "$url" 2>/dev/null || echo "{}")
        status=$(curl -s -o /dev/null -w "%{http_code}" --max-time "$TIMEOUT" "$url" 2>/dev/null || echo "000")
    fi

    if [ "$status" != "$expected_status" ]; then
        fail_note "$name (expected HTTP $expected_status, got $status)"
        return 1
    fi

    if echo "$body" | python3 -c "import sys,json; json.load(sys.stdin)" 2>/dev/null; then
        pass_note "$name (valid JSON, HTTP $status)"
        return 0
    else
        warn_note "$name (HTTP $status but invalid JSON)"
        return 1
    fi
}

# —— 检查列表 ——

echo ""
echo "=== synapse-rust deployment smoke test ==="
echo "Admin endpoint:  $ADMIN_ENDPOINT"
echo "Client endpoint: $CLIENT_ENDPOINT"
echo ""

# 1. 基础可达性检查 (admin endpoint)
echo "[1] Basic reachability"
check "admin root"       "$ADMIN_ENDPOINT/_matrix/client/versions" 200
check "admin health"     "$ADMIN_ENDPOINT/health"                  200 || true

# 2. Versions API (公开能力面)
if [ "$SKIP_VERSIONS" = "0" ]; then
    echo ""
    echo "[2] Versions API"
    check_json "versions"                "$ADMIN_ENDPOINT/_matrix/client/versions"      200
    check_json "capabilities (public)"   "$ADMIN_ENDPOINT/_matrix/client/v3/capabilities" 200
fi

# 3. Worker topology API
if [ "$SKIP_TOPOLOGY" = "0" ]; then
    echo ""
    echo "[3] Worker topology"
    check_json "topology" "$ADMIN_ENDPOINT/_synapse/worker/v1/topology" 200 "$ADMIN_AUTH_HEADER"

    # 验证 topology 响应中包含预期的 worker 类型
    topo=""
    if [ -n "$ADMIN_AUTH_HEADER" ]; then
        topo=$(curl -s --max-time "$TIMEOUT" -H "$ADMIN_AUTH_HEADER" "$ADMIN_ENDPOINT/_synapse/worker/v1/topology" 2>/dev/null || echo "{}")
    else
        topo=$(curl -s --max-time "$TIMEOUT" "$ADMIN_ENDPOINT/_synapse/worker/v1/topology" 2>/dev/null || echo "{}")
    fi
    for worker_type in master frontend synchrotron event_persister federation_reader federation_sender media_repository background pusher; do
        if echo "$topo" | python3 -c "import sys,json; d=json.load(sys.stdin); assert any('$worker_type' in str(v).lower() for v in d.values())" 2>/dev/null; then
            pass_note "topology contains worker type: $worker_type"
        else
            warn_note "topology may not contain worker type: $worker_type"
        fi
    done
fi

# 4. Client API route ownership
if [ "$SKIP_CLIENT" = "0" ]; then
    echo ""
    echo "[4] Client API reachability"
    check_json "client versions"         "$CLIENT_ENDPOINT/_matrix/client/versions"         200
    check "client login (公开)"           "$CLIENT_ENDPOINT/_matrix/client/v3/login"         200 || \
        check "client login (405)"       "$CLIENT_ENDPOINT/_matrix/client/v3/login"         405
fi

# 5. Replication protection (security boundary)
if [ "$SKIP_REPLICATION" = "0" ]; then
    echo ""
    echo "[5] Replication security boundary"
    # Replication paths should not be accessible from external endpoints
    rep_status=""
    rep_status=$(curl -s -o /dev/null -w "%{http_code}" --max-time "$TIMEOUT" \
        "$CLIENT_ENDPOINT/_synapse/worker/v1/replication/events" 2>/dev/null || echo "000")
    if [ "$rep_status" = "403" ] || [ "$rep_status" = "404" ] || [ "$rep_status" = "000" ]; then
        echo -e "  ${GREEN}PASS${NC} replication path blocked from client endpoint (HTTP $rep_status)"
        PASS=$((PASS + 1))
    else
        echo -e "  ${RED}FAIL${NC} replication path accessible from client endpoint (HTTP $rep_status)"
        FAIL=$((FAIL + 1))
    fi
fi

# 6. Multi-worker lifecycle
if [ "$SKIP_WORKER_LIFECYCLE" = "0" ]; then
    echo ""
    echo "[6] Multi-worker lifecycle"

    if [ -z "$ADMIN_AUTH_HEADER" ]; then
        warn_note "skip worker lifecycle checks: ADMIN_AUTH_HEADER is not set"
    elif [ -z "$REPLICATION_SECRET" ]; then
        warn_note "skip worker lifecycle checks: REPLICATION_SECRET is not set"
    else
        REPLICATION_AUTH_HEADER="x-synapse-worker-secret: $REPLICATION_SECRET"
        worker_created=0
        peer_worker_created=0
        primary_unregistered=0
        task_id=""
        next_task_id=""
        failed_task_id=""
        backlog_task_one=""
        backlog_task_two=""
        backlog_task_three=""

        register_body=$(cat <<EOF
{"worker_id":"$SMOKE_WORKER_ID","worker_name":"$SMOKE_WORKER_NAME","worker_type":"background","host":"$SMOKE_WORKER_HOST","port":$SMOKE_WORKER_PORT,"config":{},"metadata":{"smoke_test":true},"version":"smoke-test"}
EOF
)
        request "POST" "$ADMIN_ENDPOINT/_synapse/worker/v1/register" "$ADMIN_AUTH_HEADER" "$register_body"
        if [ "$REQUEST_STATUS" = "201" ]; then
            worker_created=1
            registered_worker_id=$(json_extract "$REQUEST_BODY" "data.get('worker_id', '')")
            if [ "$registered_worker_id" = "$SMOKE_WORKER_ID" ]; then
                pass_note "registered smoke worker $SMOKE_WORKER_ID"
            else
                fail_note "register smoke worker returned unexpected worker_id '${registered_worker_id}'"
            fi
        else
            fail_note "register smoke worker failed (HTTP $REQUEST_STATUS)"
        fi

        peer_register_body=$(cat <<EOF
{"worker_id":"$SMOKE_PEER_WORKER_ID","worker_name":"$SMOKE_PEER_WORKER_NAME","worker_type":"background","host":"$SMOKE_PEER_WORKER_HOST","port":$SMOKE_PEER_WORKER_PORT,"config":{},"metadata":{"smoke_test":true,"peer":true},"version":"smoke-test"}
EOF
)
        request "POST" "$ADMIN_ENDPOINT/_synapse/worker/v1/register" "$ADMIN_AUTH_HEADER" "$peer_register_body"
        if [ "$REQUEST_STATUS" = "201" ]; then
            peer_worker_created=1
            registered_peer_worker_id=$(json_extract "$REQUEST_BODY" "data.get('worker_id', '')")
            if [ "$registered_peer_worker_id" = "$SMOKE_PEER_WORKER_ID" ]; then
                pass_note "registered peer smoke worker $SMOKE_PEER_WORKER_ID"
            else
                fail_note "register peer smoke worker returned unexpected worker_id '${registered_peer_worker_id}'"
            fi
        else
            fail_note "register peer smoke worker failed (HTTP $REQUEST_STATUS)"
        fi

        if [ "$worker_created" = "1" ]; then
            heartbeat_body='{"status":"running","load_stats":{"cpu_usage":0.1,"queue_depth":1}}'
            request "POST" "$REPLICATION_ENDPOINT/_synapse/worker/v1/workers/$SMOKE_WORKER_ID/heartbeat" "$REPLICATION_AUTH_HEADER" "$heartbeat_body"
            if [ "$REQUEST_STATUS" = "200" ]; then
                pass_note "heartbeat accepted for $SMOKE_WORKER_ID"
            else
                fail_note "heartbeat failed for $SMOKE_WORKER_ID (HTTP $REQUEST_STATUS)"
            fi

            request "GET" "$ADMIN_ENDPOINT/_synapse/worker/v1/workers/$SMOKE_WORKER_ID" "$ADMIN_AUTH_HEADER"
            if [ "$REQUEST_STATUS" = "200" ]; then
                worker_status=$(json_extract "$REQUEST_BODY" "data.get('status', '')")
                last_heartbeat_ts=$(json_extract "$REQUEST_BODY" "data.get('last_heartbeat_ts', '')")
                if [ "$worker_status" = "running" ] && [ -n "$last_heartbeat_ts" ] && [ "$last_heartbeat_ts" != "None" ]; then
                    pass_note "worker detail reflects heartbeat and running status"
                else
                    fail_note "worker detail missing running heartbeat state"
                fi
            else
                fail_note "get worker after heartbeat failed (HTTP $REQUEST_STATUS)"
            fi

            if [ "$peer_worker_created" = "1" ]; then
                request "POST" "$REPLICATION_ENDPOINT/_synapse/worker/v1/workers/$SMOKE_PEER_WORKER_ID/heartbeat" "$REPLICATION_AUTH_HEADER" "$heartbeat_body"
                if [ "$REQUEST_STATUS" = "200" ]; then
                    pass_note "heartbeat accepted for $SMOKE_PEER_WORKER_ID"
                else
                    fail_note "heartbeat failed for $SMOKE_PEER_WORKER_ID (HTTP $REQUEST_STATUS)"
                fi

                request "GET" "$ADMIN_ENDPOINT/_synapse/worker/v1/workers/$SMOKE_PEER_WORKER_ID" "$ADMIN_AUTH_HEADER"
                if [ "$REQUEST_STATUS" = "200" ]; then
                    peer_worker_status=$(json_extract "$REQUEST_BODY" "data.get('status', '')")
                    peer_last_heartbeat_ts=$(json_extract "$REQUEST_BODY" "data.get('last_heartbeat_ts', '')")
                    if [ "$peer_worker_status" = "running" ] && [ -n "$peer_last_heartbeat_ts" ] && [ "$peer_last_heartbeat_ts" != "None" ]; then
                        pass_note "peer worker detail reflects heartbeat and running status"
                    else
                        fail_note "peer worker detail missing running heartbeat state"
                    fi
                else
                    fail_note "get peer worker after heartbeat failed (HTTP $REQUEST_STATUS)"
                fi
            fi

            replication_body=$(cat <<EOF
{"stream_name":"$SMOKE_STREAM_NAME","position":$SMOKE_STREAM_POSITION}
EOF
)
            request "PUT" "$REPLICATION_ENDPOINT/_synapse/worker/v1/replication/$SMOKE_WORKER_ID/$SMOKE_STREAM_NAME" "$REPLICATION_AUTH_HEADER" "$replication_body"
            if [ "$REQUEST_STATUS" = "200" ]; then
                pass_note "replication position update accepted for $SMOKE_STREAM_NAME"
            else
                fail_note "replication position update failed (HTTP $REQUEST_STATUS)"
            fi

            request "GET" "$REPLICATION_ENDPOINT/_synapse/worker/v1/replication/$SMOKE_WORKER_ID/position?stream_name=$SMOKE_STREAM_NAME" "$REPLICATION_AUTH_HEADER"
            if [ "$REQUEST_STATUS" = "200" ]; then
                stream_position=$(json_extract "$REQUEST_BODY" "data.get('position', '')")
                if [ "$stream_position" = "$SMOKE_STREAM_POSITION" ]; then
                    pass_note "replication position readback matches $SMOKE_STREAM_POSITION"
                else
                    fail_note "replication position readback mismatch (got ${stream_position:-empty})"
                fi
            else
                fail_note "replication position readback failed (HTTP $REQUEST_STATUS)"
            fi

            assign_body=$(cat <<EOF
{"task_type":"$SMOKE_TASK_TYPE","task_data":{"smoke_test":true,"worker_id":"$SMOKE_WORKER_ID"},"priority":1,"preferred_worker_id":"$SMOKE_WORKER_ID"}
EOF
)
            request "POST" "$ADMIN_ENDPOINT/_synapse/worker/v1/tasks" "$ADMIN_AUTH_HEADER" "$assign_body"
            if [ "$REQUEST_STATUS" = "201" ]; then
                task_id=$(json_extract "$REQUEST_BODY" "data.get('task_id', '')")
                if [ -n "$task_id" ]; then
                    pass_note "assigned smoke task $task_id"
                else
                    fail_note "assign smoke task returned empty task_id"
                fi
            else
                fail_note "assign smoke task failed (HTTP $REQUEST_STATUS)"
            fi

            if [ -n "$task_id" ]; then
                request "GET" "$ADMIN_ENDPOINT/_synapse/worker/v1/tasks?limit=100" "$ADMIN_AUTH_HEADER"
                if [ "$REQUEST_STATUS" = "200" ]; then
                    task_is_pending=$(json_extract "$REQUEST_BODY" "any(item.get('task_id') == '$task_id' and item.get('status') == 'pending' for item in data)")
                    if [ "$task_is_pending" = "True" ]; then
                        pass_note "new task appears in pending task list before claim"
                    else
                        fail_note "new task missing from pending task list before claim"
                    fi
                else
                    fail_note "pending task list pre-claim fetch failed (HTTP $REQUEST_STATUS)"
                fi

                request "POST" "$ADMIN_ENDPOINT/_synapse/worker/v1/tasks/$task_id/claim/$SMOKE_WORKER_ID" "$ADMIN_AUTH_HEADER" ""
                if [ "$REQUEST_STATUS" = "200" ]; then
                    pass_note "task claim accepted for $task_id"
                else
                    fail_note "task claim failed for $task_id (HTTP $REQUEST_STATUS)"
                fi

                request "POST" "$ADMIN_ENDPOINT/_synapse/worker/v1/tasks/$task_id/claim/$SMOKE_PEER_WORKER_ID" "$ADMIN_AUTH_HEADER" ""
                if [ "$REQUEST_STATUS" = "409" ]; then
                    pass_note "second worker claim correctly rejected for already-claimed task"
                else
                    fail_note "second worker claim expected HTTP 409, got $REQUEST_STATUS"
                fi

                request "GET" "$ADMIN_ENDPOINT/_synapse/worker/v1/tasks?limit=100" "$ADMIN_AUTH_HEADER"
                if [ "$REQUEST_STATUS" = "200" ]; then
                    task_still_pending=$(json_extract "$REQUEST_BODY" "any(item.get('task_id') == '$task_id' for item in data)")
                    if [ "$task_still_pending" = "False" ]; then
                        pass_note "claimed task no longer appears in pending task list"
                    else
                        fail_note "claimed task still appears in pending task list"
                    fi
                else
                    fail_note "pending task list fetch failed (HTTP $REQUEST_STATUS)"
                fi

                complete_body='{"result":{"smoke_test":"ok"}}'
                request "POST" "$REPLICATION_ENDPOINT/_synapse/worker/v1/tasks/$task_id/complete" "$REPLICATION_AUTH_HEADER" "$complete_body"
                if [ "$REQUEST_STATUS" = "200" ]; then
                    pass_note "task completion accepted for $task_id"
                else
                    fail_note "task completion failed for $task_id (HTTP $REQUEST_STATUS)"
                fi

                next_assign_body=$(cat <<EOF
{"task_type":"$SMOKE_TASK_TYPE","task_data":{"smoke_test":"claim_next","worker_id":"$SMOKE_WORKER_ID"},"priority":1}
EOF
)
                request "POST" "$ADMIN_ENDPOINT/_synapse/worker/v1/tasks" "$ADMIN_AUTH_HEADER" "$next_assign_body"
                if [ "$REQUEST_STATUS" = "201" ]; then
                    next_task_id=$(json_extract "$REQUEST_BODY" "data.get('task_id', '')")
                    if [ -n "$next_task_id" ]; then
                        pass_note "assigned claim-next smoke task $next_task_id"
                    else
                        fail_note "assign claim-next smoke task returned empty task_id"
                    fi
                else
                    fail_note "assign claim-next smoke task failed (HTTP $REQUEST_STATUS)"
                fi

                if [ -n "$next_task_id" ]; then
                    request "POST" "$ADMIN_ENDPOINT/_synapse/worker/v1/tasks/claim/$SMOKE_WORKER_ID" "$ADMIN_AUTH_HEADER" ""
                    if [ "$REQUEST_STATUS" = "200" ]; then
                        claimed_next_task_id=$(json_extract "$REQUEST_BODY" "data.get('task_id', '')")
                        claimed_next_worker_id=$(json_extract "$REQUEST_BODY" "data.get('assigned_worker_id', '')")
                        if [ "$claimed_next_task_id" = "$next_task_id" ] && [ "$claimed_next_worker_id" = "$SMOKE_WORKER_ID" ]; then
                            pass_note "claim_next_task atomically assigned $next_task_id to $SMOKE_WORKER_ID"
                        else
                            fail_note "claim_next_task returned unexpected task/worker assignment"
                        fi
                    else
                        fail_note "claim_next_task failed for $SMOKE_WORKER_ID (HTTP $REQUEST_STATUS)"
                    fi

                    request "POST" "$ADMIN_ENDPOINT/_synapse/worker/v1/tasks/claim/$SMOKE_PEER_WORKER_ID" "$ADMIN_AUTH_HEADER" ""
                    if [ "$REQUEST_STATUS" = "404" ]; then
                        pass_note "second worker claim_next_task correctly reports no pending tasks"
                    else
                        fail_note "second worker claim_next_task expected HTTP 404, got $REQUEST_STATUS"
                    fi

                    request "POST" "$REPLICATION_ENDPOINT/_synapse/worker/v1/tasks/$next_task_id/complete" "$REPLICATION_AUTH_HEADER" "$complete_body"
                    if [ "$REQUEST_STATUS" = "200" ]; then
                        pass_note "claim-next task completion accepted for $next_task_id"
                    else
                        fail_note "claim-next task completion failed for $next_task_id (HTTP $REQUEST_STATUS)"
                    fi
                fi

                failed_assign_body=$(cat <<EOF
{"task_type":"$SMOKE_TASK_TYPE","task_data":{"smoke_test":"fail_path","worker_id":"$SMOKE_WORKER_ID"},"priority":1}
EOF
)
                request "POST" "$ADMIN_ENDPOINT/_synapse/worker/v1/tasks" "$ADMIN_AUTH_HEADER" "$failed_assign_body"
                if [ "$REQUEST_STATUS" = "201" ]; then
                    failed_task_id=$(json_extract "$REQUEST_BODY" "data.get('task_id', '')")
                    if [ -n "$failed_task_id" ]; then
                        pass_note "assigned fail-path smoke task $failed_task_id"
                    else
                        fail_note "assign fail-path smoke task returned empty task_id"
                    fi
                else
                    fail_note "assign fail-path smoke task failed (HTTP $REQUEST_STATUS)"
                fi

                if [ -n "$failed_task_id" ]; then
                    request "POST" "$ADMIN_ENDPOINT/_synapse/worker/v1/tasks/claim/$SMOKE_WORKER_ID" "$ADMIN_AUTH_HEADER" ""
                    if [ "$REQUEST_STATUS" = "200" ]; then
                        claimed_failed_task_id=$(json_extract "$REQUEST_BODY" "data.get('task_id', '')")
                        if [ "$claimed_failed_task_id" = "$failed_task_id" ]; then
                            pass_note "claim_next_task picked fail-path task $failed_task_id"
                        else
                            fail_note "claim_next_task returned unexpected fail-path task"
                        fi
                    else
                        fail_note "claim_next_task for fail-path task failed (HTTP $REQUEST_STATUS)"
                    fi

                    fail_body='{"error":"smoke fail path"}'
                    request "POST" "$REPLICATION_ENDPOINT/_synapse/worker/v1/tasks/$failed_task_id/fail" "$REPLICATION_AUTH_HEADER" "$fail_body"
                    if [ "$REQUEST_STATUS" = "200" ]; then
                        pass_note "fail-task accepted for $failed_task_id"
                    else
                        fail_note "fail-task failed for $failed_task_id (HTTP $REQUEST_STATUS)"
                    fi

                    request "GET" "$ADMIN_ENDPOINT/_synapse/worker/v1/tasks?limit=100" "$ADMIN_AUTH_HEADER"
                    if [ "$REQUEST_STATUS" = "200" ]; then
                        failed_task_pending=$(json_extract "$REQUEST_BODY" "any(item.get('task_id') == '$failed_task_id' for item in data)")
                        if [ "$failed_task_pending" = "False" ]; then
                            pass_note "failed task is absent from pending task list"
                        else
                            fail_note "failed task unexpectedly returned to pending task list"
                        fi
                    else
                        fail_note "pending task list fetch after fail-task failed (HTTP $REQUEST_STATUS)"
                    fi

                    request "POST" "$ADMIN_ENDPOINT/_synapse/worker/v1/tasks/claim/$SMOKE_PEER_WORKER_ID" "$ADMIN_AUTH_HEADER" ""
                    if [ "$REQUEST_STATUS" = "404" ]; then
                        pass_note "failed task is not re-queued for peer claim_next_task"
                    else
                        fail_note "peer claim_next_task after fail-task expected HTTP 404, got $REQUEST_STATUS"
                    fi

                    backlog_assign_one=$(cat <<EOF
{"task_type":"$SMOKE_TASK_TYPE","task_data":{"smoke_test":"backlog_one","worker_id":"$SMOKE_WORKER_ID"},"priority":100003}
EOF
)
                    backlog_assign_two=$(cat <<EOF
{"task_type":"$SMOKE_TASK_TYPE","task_data":{"smoke_test":"backlog_two","worker_id":"$SMOKE_PEER_WORKER_ID"},"priority":100002}
EOF
)
                    backlog_assign_three=$(cat <<EOF
{"task_type":"$SMOKE_TASK_TYPE","task_data":{"smoke_test":"backlog_three","worker_id":"$SMOKE_WORKER_ID"},"priority":100001}
EOF
)

                    request "POST" "$ADMIN_ENDPOINT/_synapse/worker/v1/tasks" "$ADMIN_AUTH_HEADER" "$backlog_assign_one"
                    if [ "$REQUEST_STATUS" = "201" ]; then
                        backlog_task_one=$(json_extract "$REQUEST_BODY" "data.get('task_id', '')")
                    fi
                    request "POST" "$ADMIN_ENDPOINT/_synapse/worker/v1/tasks" "$ADMIN_AUTH_HEADER" "$backlog_assign_two"
                    if [ "$REQUEST_STATUS" = "201" ]; then
                        backlog_task_two=$(json_extract "$REQUEST_BODY" "data.get('task_id', '')")
                    fi
                    request "POST" "$ADMIN_ENDPOINT/_synapse/worker/v1/tasks" "$ADMIN_AUTH_HEADER" "$backlog_assign_three"
                    if [ "$REQUEST_STATUS" = "201" ]; then
                        backlog_task_three=$(json_extract "$REQUEST_BODY" "data.get('task_id', '')")
                    fi

                    if [ -n "$backlog_task_one" ] && [ -n "$backlog_task_two" ] && [ -n "$backlog_task_three" ]; then
                        pass_note "assigned three backlog smoke tasks"

                        request "POST" "$ADMIN_ENDPOINT/_synapse/worker/v1/tasks/claim/$SMOKE_WORKER_ID" "$ADMIN_AUTH_HEADER" ""
                        backlog_claim_one=$(json_extract "$REQUEST_BODY" "data.get('task_id', '')")
                        request "POST" "$ADMIN_ENDPOINT/_synapse/worker/v1/tasks/claim/$SMOKE_PEER_WORKER_ID" "$ADMIN_AUTH_HEADER" ""
                        backlog_claim_two=$(json_extract "$REQUEST_BODY" "data.get('task_id', '')")
                        request "POST" "$ADMIN_ENDPOINT/_synapse/worker/v1/tasks/claim/$SMOKE_WORKER_ID" "$ADMIN_AUTH_HEADER" ""
                        backlog_claim_three=$(json_extract "$REQUEST_BODY" "data.get('task_id', '')")

                        if [ "$backlog_claim_one" = "$backlog_task_one" ] && [ "$backlog_claim_two" = "$backlog_task_two" ] && [ "$backlog_claim_three" = "$backlog_task_three" ]; then
                            pass_note "two workers drain backlog in priority order without duplicate claims"
                        else
                            fail_note "backlog drain returned unexpected claim order or duplicate tasks"
                        fi

                        request "POST" "$REPLICATION_ENDPOINT/_synapse/worker/v1/tasks/$backlog_task_one/complete" "$REPLICATION_AUTH_HEADER" "$complete_body"
                        if [ "$REQUEST_STATUS" = "200" ]; then
                            pass_note "completed backlog task $backlog_task_one"
                        else
                            fail_note "complete backlog task failed for $backlog_task_one (HTTP $REQUEST_STATUS)"
                        fi
                        request "POST" "$REPLICATION_ENDPOINT/_synapse/worker/v1/tasks/$backlog_task_two/complete" "$REPLICATION_AUTH_HEADER" "$complete_body"
                        if [ "$REQUEST_STATUS" = "200" ]; then
                            pass_note "completed backlog task $backlog_task_two"
                        else
                            fail_note "complete backlog task failed for $backlog_task_two (HTTP $REQUEST_STATUS)"
                        fi
                        request "POST" "$REPLICATION_ENDPOINT/_synapse/worker/v1/tasks/$backlog_task_three/complete" "$REPLICATION_AUTH_HEADER" "$complete_body"
                        if [ "$REQUEST_STATUS" = "200" ]; then
                            pass_note "completed backlog task $backlog_task_three"
                        else
                            fail_note "complete backlog task failed for $backlog_task_three (HTTP $REQUEST_STATUS)"
                        fi
                    else
                        fail_note "assign backlog smoke tasks did not return three task ids"
                    fi

                    request "DELETE" "$ADMIN_ENDPOINT/_synapse/worker/v1/workers/$SMOKE_WORKER_ID" "$ADMIN_AUTH_HEADER"
                    if [ "$REQUEST_STATUS" = "204" ]; then
                        primary_unregistered=1
                        pass_note "primary worker unregister succeeded before recovery test"
                    else
                        fail_note "primary worker unregister failed before recovery test (HTTP $REQUEST_STATUS)"
                    fi

                    recovery_assign_body=$(cat <<EOF
{"task_type":"$SMOKE_TASK_TYPE","task_data":{"smoke_test":"recovery_path","worker_id":"$SMOKE_PEER_WORKER_ID"},"priority":100000}
EOF
)
                    request "POST" "$ADMIN_ENDPOINT/_synapse/worker/v1/tasks" "$ADMIN_AUTH_HEADER" "$recovery_assign_body"
                    if [ "$REQUEST_STATUS" = "201" ]; then
                        recovery_task_id=$(json_extract "$REQUEST_BODY" "data.get('task_id', '')")
                        if [ -n "$recovery_task_id" ]; then
                            pass_note "assigned recovery smoke task $recovery_task_id"
                            request "POST" "$ADMIN_ENDPOINT/_synapse/worker/v1/tasks/claim/$SMOKE_PEER_WORKER_ID" "$ADMIN_AUTH_HEADER" ""
                            if [ "$REQUEST_STATUS" = "200" ]; then
                                claimed_recovery_task_id=$(json_extract "$REQUEST_BODY" "data.get('task_id', '')")
                                claimed_recovery_worker_id=$(json_extract "$REQUEST_BODY" "data.get('assigned_worker_id', '')")
                                if [ "$claimed_recovery_task_id" = "$recovery_task_id" ] && [ "$claimed_recovery_worker_id" = "$SMOKE_PEER_WORKER_ID" ]; then
                                    pass_note "peer worker claims recovery task after primary unregister"
                                else
                                    fail_note "peer worker recovery claim returned unexpected task or worker"
                                fi
                            else
                                fail_note "peer worker recovery claim failed (HTTP $REQUEST_STATUS)"
                            fi

                            request "POST" "$REPLICATION_ENDPOINT/_synapse/worker/v1/tasks/$recovery_task_id/complete" "$REPLICATION_AUTH_HEADER" "$complete_body"
                            if [ "$REQUEST_STATUS" = "200" ]; then
                                pass_note "peer worker completed recovery task $recovery_task_id"
                            else
                                fail_note "peer worker recovery completion failed for $recovery_task_id (HTTP $REQUEST_STATUS)"
                            fi
                        else
                            fail_note "assign recovery smoke task returned empty task_id"
                        fi
                    else
                        fail_note "assign recovery smoke task failed (HTTP $REQUEST_STATUS)"
                    fi
                fi
            fi

            if [ "$primary_unregistered" = "0" ]; then
                request "DELETE" "$ADMIN_ENDPOINT/_synapse/worker/v1/workers/$SMOKE_WORKER_ID" "$ADMIN_AUTH_HEADER"
                if [ "$REQUEST_STATUS" = "204" ]; then
                    pass_note "cleanup unregister succeeded for $SMOKE_WORKER_ID"
                else
                    warn_note "cleanup unregister returned HTTP $REQUEST_STATUS for $SMOKE_WORKER_ID"
                fi
            fi

            if [ "$peer_worker_created" = "1" ]; then
                request "DELETE" "$ADMIN_ENDPOINT/_synapse/worker/v1/workers/$SMOKE_PEER_WORKER_ID" "$ADMIN_AUTH_HEADER"
                if [ "$REQUEST_STATUS" = "204" ]; then
                    pass_note "cleanup unregister succeeded for $SMOKE_PEER_WORKER_ID"
                else
                    warn_note "cleanup unregister returned HTTP $REQUEST_STATUS for $SMOKE_PEER_WORKER_ID"
                fi
            fi
        fi
    fi
fi

# 7. 总结
echo ""
echo "=== Results ==="
echo -e "  ${GREEN}PASS: $PASS${NC}"
echo -e "  ${YELLOW}WARN: $WARN${NC}"
echo -e "  ${RED}FAIL: $FAIL${NC}"
echo ""

if [ "$FAIL" -gt 0 ]; then
    echo "Smoke test FAILED with $FAIL failure(s)."
    exit 1
else
    echo "Smoke test PASSED."
    exit 0
fi
