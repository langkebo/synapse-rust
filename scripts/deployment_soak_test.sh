#!/usr/bin/env bash
# =============================================================================
# synapse-rust 部署浸泡测试 (deployment soak test)
# =============================================================================
# 用途: 长时间运行的多实例一致性/恢复验证，周期性执行 smoke check 并检测漂移
# 版本: v0.1 (2026-06-17)
# 对应文档: docs/synapse-rust/COMPREHENSIVE_AUDIT_REPORT_2026-06-03.md (P1-12)
#
# 用法:
#   SOAK_DURATION_SECONDS=3600 SOAK_INTERVAL_SECONDS=60 \
#     ADMIN_ENDPOINT=http://127.0.0.1:8008 \
#     ADMIN_AUTH_HEADER="Authorization: Bearer <token>" \
#     REPLICATION_SECRET="<secret>" \
#     bash scripts/deployment_soak_test.sh
#
# 环境变量:
#   SOAK_DURATION_SECONDS    总运行时长（秒），默认 3600（1 小时）
#   SOAK_INTERVAL_SECONDS    验证间隔（秒），默认 60
#   SOAK_DRIFT_TOLERANCE     连续漂移容忍次数，默认 3
#   ADMIN_ENDPOINT           管理端点，默认 http://127.0.0.1:8008
#   ADMIN_AUTH_HEADER        管理认证头
#   REPLICATION_SECRET       复制密钥
#   CLIENT_ENDPOINT          客户端端点
#   SYNC_ENDPOINT            同步端点
#   MEDIA_ENDPOINT           媒体端点
#   FEDERATION_ENDPOINT      联邦端点
#   REPLICATION_ENDPOINT     复制端点
#   SOAK_WORKER_ID           测试 worker ID 前缀
# =============================================================================

set -euo pipefail

# —— 配置 ——
SOAK_DURATION_SECONDS="${SOAK_DURATION_SECONDS:-3600}"
SOAK_INTERVAL_SECONDS="${SOAK_INTERVAL_SECONDS:-60}"
SOAK_DRIFT_TOLERANCE="${SOAK_DRIFT_TOLERANCE:-3}"
ADMIN_ENDPOINT="${ADMIN_ENDPOINT:-http://127.0.0.1:8008}"
CLIENT_ENDPOINT="${CLIENT_ENDPOINT:-http://127.0.0.1:8008}"
SYNC_ENDPOINT="${SYNC_ENDPOINT:-http://127.0.0.1:8008}"
MEDIA_ENDPOINT="${MEDIA_ENDPOINT:-http://127.0.0.1:8008}"
FEDERATION_ENDPOINT="${FEDERATION_ENDPOINT:-http://127.0.0.1:8008}"
REPLICATION_ENDPOINT="${REPLICATION_ENDPOINT:-http://127.0.0.1:8008}"
ADMIN_AUTH_HEADER="${ADMIN_AUTH_HEADER:-}"
REPLICATION_SECRET="${REPLICATION_SECRET:-}"
TIMEOUT="${SOAK_TIMEOUT:-10}"
SOAK_WORKER_PREFIX="${SOAK_WORKER_PREFIX:-soak-worker-$$-$(date +%s)}"

# —— 状态 ——
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

CYCLE=0
TOTAL_PASS=0
TOTAL_FAIL=0
TOTAL_WARN=0
DRIFT_COUNT=0
CONSECUTIVE_FAILS=0
START_TIME=$(date +%s)
LAST_TOPO_SNAPSHOT=""
LAST_WORKER_COUNT=""
ABORT_FLAG=0

# —— 工具函数 ——

cleanup() {
    echo ""
    echo -e "${CYAN}[soak]${NC} received signal, shutting down gracefully..."
    ABORT_FLAG=1
}

trap cleanup SIGTERM SIGINT

log_cycle_header() {
    local elapsed="$1"
    echo ""
    echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${CYAN}[soak]${NC} Cycle $CYCLE | Elapsed: ${elapsed}s | Pass: $TOTAL_PASS | Fail: $TOTAL_FAIL | Warn: $TOTAL_WARN"
    echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
}

pass_note() {
    echo -e "  ${GREEN}PASS${NC} $1"
    TOTAL_PASS=$((TOTAL_PASS + 1))
}

fail_note() {
    echo -e "  ${RED}FAIL${NC} $1"
    TOTAL_FAIL=$((TOTAL_FAIL + 1))
}

warn_note() {
    echo -e "  ${YELLOW}WARN${NC} $1"
    TOTAL_WARN=$((TOTAL_WARN + 1))
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

# —— 浸泡专项检查 ——

# 拓扑漂移检测：比较当前拓扑快照与上一个周期的快照
check_topology_drift() {
    local topo
    local worker_count

    if [ -n "$ADMIN_AUTH_HEADER" ]; then
        topo=$(curl -s --max-time "$TIMEOUT" -H "$ADMIN_AUTH_HEADER" \
            "$ADMIN_ENDPOINT/_synapse/worker/v1/topology" 2>/dev/null || echo "{}")
    else
        topo=$(curl -s --max-time "$TIMEOUT" \
            "$ADMIN_ENDPOINT/_synapse/worker/v1/topology" 2>/dev/null || echo "{}")
    fi

    if echo "$topo" | python3 -c "import sys,json; json.load(sys.stdin)" 2>/dev/null; then
        worker_count=$(echo "$topo" | python3 -c "import json,sys; print(len(json.load(sys.stdin)))" 2>/dev/null || echo "0")
    else
        fail_note "topology drift: unable to parse topology JSON"
        DRIFT_COUNT=$((DRIFT_COUNT + 1))
        return 1
    fi

    if [ -n "$LAST_TOPO_SNAPSHOT" ] && [ "$topo" != "$LAST_TOPO_SNAPSHOT" ]; then
        DRIFT_COUNT=$((DRIFT_COUNT + 1))
        if [ "$DRIFT_COUNT" -ge "$SOAK_DRIFT_TOLERANCE" ]; then
            fail_note "topology drift: snapshot changed for $DRIFT_COUNT consecutive cycles"
        else
            warn_note "topology drift detected (cycle $DRIFT_COUNT/$SOAK_DRIFT_TOLERANCE) — may be legitimate scaling"
        fi
    else
        DRIFT_COUNT=0
        pass_note "topology stable ($worker_count workers)"
    fi

    if [ -n "$LAST_WORKER_COUNT" ] && [ "$worker_count" != "$LAST_WORKER_COUNT" ]; then
        warn_note "worker count changed: $LAST_WORKER_COUNT → $worker_count"
    fi

    LAST_TOPO_SNAPSHOT="$topo"
    LAST_WORKER_COUNT="$worker_count"
}

# Worker 心跳持续检查（验证已注册 worker 仍在心跳）
check_worker_heartbeat_continuity() {
    if [ -z "$ADMIN_AUTH_HEADER" ]; then
        warn_note "heartbeat continuity: ADMIN_AUTH_HEADER not set, skip"
        return 0
    fi

    local workers_json
    workers_json=$(curl -s --max-time "$TIMEOUT" -H "$ADMIN_AUTH_HEADER" \
        "$ADMIN_ENDPOINT/_synapse/worker/v1/topology" 2>/dev/null || echo "{}")

    local stale_count
    stale_count=$(echo "$workers_json" | python3 -c "
import json, sys, time
data = json.load(sys.stdin)
now = int(time.time() * 1000)
stale = 0
for wid, info in data.items():
    last_hb = info.get('last_heartbeat_ts', 0)
    if last_hb and (now - last_hb) > 300_000:  # 5 minutes
        stale += 1
        print(f'STALE: {wid} last heartbeat {last_hb}', file=sys.stderr)
print(stale)
" 2>/dev/null || echo "0")

    if [ "$stale_count" -gt 0 ]; then
        fail_note "heartbeat continuity: $stale_count worker(s) with stale heartbeat (>5min)"
        return 1
    else
        pass_note "heartbeat continuity: all workers fresh"
        return 0
    fi
}

# Replication position 一致性检查
check_replication_position_consistency() {
    if [ -z "$REPLICATION_SECRET" ]; then
        warn_note "replication consistency: REPLICATION_SECRET not set, skip"
        return 0
    fi

    local rep_auth="x-synapse-worker-secret: $REPLICATION_SECRET"

    # 获取所有 worker 的 replication position
    local topo
    if [ -n "$ADMIN_AUTH_HEADER" ]; then
        topo=$(curl -s --max-time "$TIMEOUT" -H "$ADMIN_AUTH_HEADER" \
            "$ADMIN_ENDPOINT/_synapse/worker/v1/topology" 2>/dev/null || echo "{}")
    else
        warn_note "replication consistency: cannot fetch topology without auth"
        return 0
    fi

    local worker_ids
    worker_ids=$(echo "$topo" | python3 -c "import json,sys; print(' '.join(json.load(sys.stdin).keys()))" 2>/dev/null || echo "")

    local ok=1
    for wid in $worker_ids; do
        local pos_body
        pos_body=$(curl -s --max-time "$TIMEOUT" -H "$rep_auth" \
            "$REPLICATION_ENDPOINT/_synapse/worker/v1/replication/$wid/position?stream_name=events" 2>/dev/null || echo "{}")
        local pos
        pos=$(echo "$pos_body" | python3 -c "import json,sys; print(json.load(sys.stdin).get('data', {}).get('position', 'N/A'))" 2>/dev/null || echo "N/A")
        if [ "$pos" = "N/A" ] || [ "$pos" = "None" ]; then
            warn_note "replication consistency: worker $wid has no events position"
            ok=0
        fi
    done

    if [ "$ok" = "1" ]; then
        pass_note "replication consistency: all workers have events position"
    fi
}

# 运行单轮 smoke 验证
run_smoke_cycle() {
    local cycle_ok=1

    # 1. 基础可达性
    echo "  [reachability]"
    check "admin root"       "$ADMIN_ENDPOINT/_matrix/client/versions" 200 || cycle_ok=0
    check "admin health"     "$ADMIN_ENDPOINT/health"                  200 || true

    # 2. 拓扑检查
    echo "  [topology]"
    check_json "topology" "$ADMIN_ENDPOINT/_synapse/worker/v1/topology" 200 "$ADMIN_AUTH_HEADER" || cycle_ok=0
    check_topology_drift || cycle_ok=0

    # 3. Worker 心跳连续性
    echo "  [heartbeat]"
    check_worker_heartbeat_continuity || cycle_ok=0

    # 4. Replication position 一致性
    echo "  [replication]"
    check_replication_position_consistency || cycle_ok=0

    # 5. 路由可达性
    echo "  [routes]"
    check "client versions"         "$CLIENT_ENDPOINT/_matrix/client/versions"         200 || cycle_ok=0
    check "sync route probe"        "$SYNC_ENDPOINT/_matrix/client/v3/sync"            200 || true
    check "media route probe"       "$MEDIA_ENDPOINT/_matrix/media/v3/config"          200 || true
    check "federation route probe"  "$FEDERATION_ENDPOINT/_matrix/federation/v1/version" 200 || true

    # 6. Replication 安全边界
    echo "  [security]"
    local rep_status
    rep_status=$(curl -s -o /dev/null -w "%{http_code}" --max-time "$TIMEOUT" \
        "$CLIENT_ENDPOINT/_synapse/worker/v1/replication/events" 2>/dev/null || echo "000")
    if [ "$rep_status" = "403" ] || [ "$rep_status" = "404" ] || [ "$rep_status" = "000" ]; then
        pass_note "replication path blocked from client endpoint (HTTP $rep_status)"
    else
        fail_note "replication path accessible from client endpoint (HTTP $rep_status)"
        cycle_ok=0
    fi

    return "$cycle_ok"
}

# —— 主循环 ——

echo ""
echo -e "${CYAN}=== synapse-rust deployment soak test ===${NC}"
echo "Duration:      ${SOAK_DURATION_SECONDS}s"
echo "Interval:      ${SOAK_INTERVAL_SECONDS}s"
echo "Drift tolerance: ${SOAK_DRIFT_TOLERANCE}"
echo "Admin endpoint:  ${ADMIN_ENDPOINT}"
echo "Client endpoint: ${CLIENT_ENDPOINT}"
echo "Started at:      $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo ""

END_TIME=$((START_TIME + SOAK_DURATION_SECONDS))

while [ "$(date +%s)" -lt "$END_TIME" ] && [ "$ABORT_FLAG" = "0" ]; do
    CYCLE=$((CYCLE + 1))
    ELAPSED=$(($(date +%s) - START_TIME))
    log_cycle_header "$ELAPSED"

    run_smoke_cycle
    CYCLE_OK=$?

    if [ "$CYCLE_OK" = "0" ]; then
        CONSECUTIVE_FAILS=$((CONSECUTIVE_FAILS + 1))
        echo -e "  ${RED}Cycle $CYCLE FAILED${NC} (consecutive fails: $CONSECUTIVE_FAILS)"
    else
        CONSECUTIVE_FAILS=0
        echo -e "  ${GREEN}Cycle $CYCLE PASSED${NC}"
    fi

    # 连续失败超过容忍次数则提前退出
    if [ "$CONSECUTIVE_FAILS" -ge "$SOAK_DRIFT_TOLERANCE" ]; then
        echo ""
        echo -e "${RED}[soak] ABORT: $CONSECUTIVE_FAILS consecutive cycle failures, exceeds tolerance $SOAK_DRIFT_TOLERANCE${NC}"
        break
    fi

    # 等待下一轮
    if [ "$(date +%s)" -lt "$END_TIME" ] && [ "$ABORT_FLAG" = "0" ]; then
        sleep "$SOAK_INTERVAL_SECONDS" &
        SLEEP_PID=$!
        wait $SLEEP_PID 2>/dev/null || true
    fi
done

# —— 总结 ——
ELAPSED_TOTAL=$(($(date +%s) - START_TIME))
echo ""
echo -e "${CYAN}========================================${NC}"
echo -e "${CYAN}=== Soak Test Summary ===${NC}"
echo -e "${CYAN}========================================${NC}"
echo "Duration:     ${ELAPSED_TOTAL}s"
echo "Cycles:       $CYCLE"
echo "Interval:     ${SOAK_INTERVAL_SECONDS}s"
echo -e "  ${GREEN}PASS: $TOTAL_PASS${NC}"
echo -e "  ${YELLOW}WARN: $TOTAL_WARN${NC}"
echo -e "  ${RED}FAIL: $TOTAL_FAIL${NC}"
echo "Drift events: $DRIFT_COUNT"
echo "Finished at:  $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo ""

if [ "$ABORT_FLAG" = "1" ]; then
    echo "Soak test interrupted by signal."
    exit 130
elif [ "$TOTAL_FAIL" -gt 0 ]; then
    echo "Soak test FAILED with $TOTAL_FAIL failure(s)."
    exit 1
else
    echo "Soak test PASSED."
    exit 0
fi