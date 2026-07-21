#!/usr/bin/env bash
# =============================================================================
# synapse-rust 部署浸泡测试 (deployment soak test)
# =============================================================================
# 用途: 长时间运行的多实例一致性/恢复验证，周期性执行 smoke check 并检测漂移
# 版本: v0.2 (2026-06-18)
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
#   SOAK_OUTPUT_DIR          设置后输出 JSON + Markdown 报告到该目录
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
SOAK_OUTPUT_DIR="${SOAK_OUTPUT_DIR:-}"

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
START_TS_ISO=$(date -u +%Y-%m-%dT%H:%M:%SZ)
LAST_TOPO_SNAPSHOT=""
LAST_WORKER_COUNT=""
ABORT_FLAG=0

# JSON 报告跟踪（SOAK_OUTPUT_DIR 设置时启用）
JSON_CYCLES_FILE=""
CYCLE_WARNINGS_FILE=""
CYCLE_ERRORS_FILE=""
CUR_CATEGORY=""

# 每轮各检查类别的状态（每轮重置）
CYCLE_REACHABILITY="pass"
CYCLE_TOPOLOGY_DRIFT="pass"
CYCLE_HEARTBEAT_CONTINUITY="pass"
CYCLE_REPLICATION_POSITION="pass"
CYCLE_ROUTES="pass"
CYCLE_SECURITY="pass"

# —— 工具函数 ——

cleanup() {
    echo ""
    echo -e "${CYAN}[soak]${NC} received signal, shutting down gracefully..."
    ABORT_FLAG=1
}

trap cleanup SIGTERM SIGINT
trap 'json_finalize' EXIT

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

warn_note() {
    echo -e "  ${YELLOW}WARN${NC} $1"
    TOTAL_WARN=$((TOTAL_WARN + 1))
    update_category "warn"
    if [ -n "$CYCLE_WARNINGS_FILE" ]; then
        echo "$1" >> "$CYCLE_WARNINGS_FILE"
    fi
}

fail_note() {
    echo -e "  ${RED}FAIL${NC} $1"
    TOTAL_FAIL=$((TOTAL_FAIL + 1))
    update_category "fail"
    if [ -n "$CYCLE_ERRORS_FILE" ]; then
        echo "$1" >> "$CYCLE_ERRORS_FILE"
    fi
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
        "$ADMIN_ENDPOINT/_synapse/worker/v1/statistics" 2>/dev/null || echo "[]")

    local heartbeat_summary
    heartbeat_summary=$(echo "$workers_json" | python3 -c "
import json, sys, time
try:
    data = json.load(sys.stdin)
except Exception:
    print('PARSE_ERROR')
    raise SystemExit(0)
if not isinstance(data, list):
    print('PARSE_ERROR')
    raise SystemExit(0)
now = int(time.time() * 1000)
stale = 0
draining = 0
terminal = 0
for info in data:
    wid = info.get('worker_id', '<unknown>')
    status = (info.get('status') or '').lower()
    last_hb = info.get('last_heartbeat_ts') or 0
    if status in ('running', 'starting'):
        if last_hb and (now - int(last_hb)) > 300_000:
            stale += 1
            print(f'STALE: {wid} status={status} last_heartbeat_ts={last_hb}', file=sys.stderr)
    elif status == 'stopping':
        draining += 1
        print(f'DRAINING: {wid} status=stopping', file=sys.stderr)
    elif status in ('stopped', 'error'):
        terminal += 1
        print(f'TERMINAL: {wid} status={status}', file=sys.stderr)
print(f'{stale} {draining} {terminal}')
" 2>/dev/null || echo "PARSE_ERROR")

    if [ "$heartbeat_summary" = "PARSE_ERROR" ]; then
        fail_note "heartbeat continuity: unable to parse worker statistics JSON"
        return 1
    fi

    local stale_count draining_count terminal_count
    stale_count=$(printf '%s' "$heartbeat_summary" | awk '{print $1}')
    draining_count=$(printf '%s' "$heartbeat_summary" | awk '{print $2}')
    terminal_count=$(printf '%s' "$heartbeat_summary" | awk '{print $3}')

    if [ "${stale_count:-0}" -gt 0 ]; then
        fail_note "heartbeat continuity: $stale_count running/starting worker(s) with stale heartbeat (>5min)"
        return 1
    fi

    if [ "${terminal_count:-0}" -gt 0 ]; then
        fail_note "heartbeat continuity: $terminal_count worker(s) entered stopped/error during soak"
        return 1
    fi

    if [ "${draining_count:-0}" -gt 0 ]; then
        warn_note "heartbeat continuity: $draining_count worker(s) currently draining in stopping state"
        return 0
    fi

    pass_note "heartbeat continuity: all running workers fresh and no terminal worker states observed"
    return 0
}

# Replication position 一致性检查
check_replication_position_consistency() {
    if [ -z "$REPLICATION_SECRET" ]; then
        warn_note "replication consistency: REPLICATION_SECRET not set, skip"
        return 0
    fi

    local rep_auth="x-synapse-worker-secret: $REPLICATION_SECRET"

    # 获取当前运行中的 worker 列表，避免把 stopping/stopped/error 实例误算为 position 缺失
    local statistics
    if [ -n "$ADMIN_AUTH_HEADER" ]; then
        statistics=$(curl -s --max-time "$TIMEOUT" -H "$ADMIN_AUTH_HEADER" \
            "$ADMIN_ENDPOINT/_synapse/worker/v1/statistics" 2>/dev/null || echo "[]")
    else
        warn_note "replication consistency: cannot fetch worker statistics without auth"
        return 0
    fi

    local worker_ids
    worker_ids=$(echo "$statistics" | python3 -c "
import json, sys
try:
    data = json.load(sys.stdin)
except Exception:
    print('')
    raise SystemExit(0)
if not isinstance(data, list):
    print('')
    raise SystemExit(0)
print(' '.join(
    item.get('worker_id', '')
    for item in data
    if item.get('worker_id') and (item.get('status') or '').lower() in ('running', 'starting', 'stopping')
))
" 2>/dev/null || echo "")

    if [ -z "$worker_ids" ]; then
        warn_note "replication consistency: no running/starting/stopping workers reported by statistics"
        return 0
    fi

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

# —— JSON 报告函数 ——

json_init() {
    if [ -z "$SOAK_OUTPUT_DIR" ]; then
        return 0
    fi
    mkdir -p "$SOAK_OUTPUT_DIR"
    JSON_CYCLES_FILE=$(mktemp)
    CYCLE_WARNINGS_FILE=$(mktemp)
    CYCLE_ERRORS_FILE=$(mktemp)
    : > "$JSON_CYCLES_FILE"
    : > "$CYCLE_WARNINGS_FILE"
    : > "$CYCLE_ERRORS_FILE"
}

reset_cycle_tracking() {
    CYCLE_REACHABILITY="pass"
    CYCLE_TOPOLOGY_DRIFT="pass"
    CYCLE_HEARTBEAT_CONTINUITY="pass"
    CYCLE_REPLICATION_POSITION="pass"
    CYCLE_ROUTES="pass"
    CYCLE_SECURITY="pass"
    if [ -n "$CYCLE_WARNINGS_FILE" ]; then
        : > "$CYCLE_WARNINGS_FILE"
    fi
    if [ -n "$CYCLE_ERRORS_FILE" ]; then
        : > "$CYCLE_ERRORS_FILE"
    fi
}

update_category() {
    local status="$1"
    case "$CUR_CATEGORY" in
        reachability)
            if [ "$status" = "fail" ]; then
                CYCLE_REACHABILITY="fail"
            fi
            ;;
        topology)
            if [ "$status" = "fail" ]; then
                CYCLE_TOPOLOGY_DRIFT="fail"
            elif [ "$status" = "warn" ] && [ "$CYCLE_TOPOLOGY_DRIFT" != "fail" ]; then
                CYCLE_TOPOLOGY_DRIFT="warn"
            fi
            ;;
        heartbeat)
            if [ "$status" = "fail" ]; then
                CYCLE_HEARTBEAT_CONTINUITY="fail"
            elif [ "$status" = "warn" ] && [ "$CYCLE_HEARTBEAT_CONTINUITY" != "fail" ]; then
                CYCLE_HEARTBEAT_CONTINUITY="warn"
            fi
            ;;
        replication)
            if [ "$status" = "fail" ]; then
                CYCLE_REPLICATION_POSITION="fail"
            elif [ "$status" = "warn" ] && [ "$CYCLE_REPLICATION_POSITION" != "fail" ]; then
                CYCLE_REPLICATION_POSITION="warn"
            fi
            ;;
        routes)
            if [ "$status" = "fail" ]; then
                CYCLE_ROUTES="fail"
            fi
            ;;
        security)
            if [ "$status" = "fail" ]; then
                CYCLE_SECURITY="fail"
            fi
            ;;
    esac
}

compute_cycle_status() {
    local status="pass"
    for cat_val in "$CYCLE_REACHABILITY" "$CYCLE_TOPOLOGY_DRIFT" "$CYCLE_HEARTBEAT_CONTINUITY" \
                   "$CYCLE_REPLICATION_POSITION" "$CYCLE_ROUTES" "$CYCLE_SECURITY"; do
        if [ "$cat_val" = "fail" ]; then
            status="fail"
            break
        elif [ "$cat_val" = "warn" ] && [ "$status" = "pass" ]; then
            status="warn"
        fi
    done
    echo "$status"
}

json_array_from_lines() {
    # 从 stdin 读取行，输出 JSON 字符串数组
    if command -v jq >/dev/null 2>&1; then
        jq -R -s -c 'split("\n") | map(select(length > 0))'
    else
        python3 -c 'import json,sys; print(json.dumps([l.rstrip("\n") for l in sys.stdin if l.strip()]))'
    fi
}

json_record_cycle() {
    if [ -z "$SOAK_OUTPUT_DIR" ]; then
        return 0
    fi
    local cycle_num="$1"
    local cycle_status="$2"
    local timestamp
    timestamp=$(date -u +%Y-%m-%dT%H:%M:%SZ)

    local warnings_json errors_json
    warnings_json=$(json_array_from_lines < "$CYCLE_WARNINGS_FILE")
    errors_json=$(json_array_from_lines < "$CYCLE_ERRORS_FILE")

    cat >> "$JSON_CYCLES_FILE" <<EOF
{
  "cycle_num": $cycle_num,
  "timestamp": "$timestamp",
  "status": "$cycle_status",
  "checks": {
    "reachability": "$CYCLE_REACHABILITY",
    "topology_drift": "$CYCLE_TOPOLOGY_DRIFT",
    "heartbeat_continuity": "$CYCLE_HEARTBEAT_CONTINUITY",
    "replication_position": "$CYCLE_REPLICATION_POSITION",
    "routes": "$CYCLE_ROUTES",
    "security": "$CYCLE_SECURITY"
  },
  "warnings": $warnings_json,
  "errors": $errors_json
}
EOF
}

json_finalize() {
    if [ -z "$SOAK_OUTPUT_DIR" ]; then
        return 0
    fi
    if [ -z "$JSON_CYCLES_FILE" ]; then
        return 0
    fi

    local end_ts_iso
    end_ts_iso=$(date -u +%Y-%m-%dT%H:%M:%SZ)
    local duration
    duration=$(($(date +%s) - START_TIME))

    local aborted="false"
    local abort_reason="null"
    if [ "$ABORT_FLAG" = "1" ]; then
        aborted="true"
        abort_reason='"interrupted by signal (SIGTERM/SIGINT)"'
    elif [ "$CONSECUTIVE_FAILS" -ge "$SOAK_DRIFT_TOLERANCE" ] && [ "$CONSECUTIVE_FAILS" -gt 0 ]; then
        aborted="true"
        abort_reason='"consecutive cycle failures exceeded drift tolerance"'
    fi

    # 组装 cycles 数组
    local cycles_json="[]"
    if [ -s "$JSON_CYCLES_FILE" ]; then
        if command -v jq >/dev/null 2>&1; then
            cycles_json=$(jq -s -c '.' "$JSON_CYCLES_FILE" 2>/dev/null || echo "[]")
        else
            cycles_json=$(python3 -c 'import json,sys; print(json.dumps([json.loads(l) for l in sys.stdin if l.strip()]))' < "$JSON_CYCLES_FILE" 2>/dev/null || echo "[]")
        fi
    fi

    local ts_suffix
    ts_suffix=$(date +%Y%m%d_%H%M%S)
    local json_report="$SOAK_OUTPUT_DIR/soak_report_${ts_suffix}.json"
    local md_report="$SOAK_OUTPUT_DIR/soak_report_${ts_suffix}.md"

    # 写入 JSON 报告
    cat > "$json_report" <<EOF
{
  "test_run": {
    "start_ts": "$START_TS_ISO",
    "end_ts": "$end_ts_iso",
    "duration_seconds": $duration,
    "interval_seconds": $SOAK_INTERVAL_SECONDS,
    "target_duration_seconds": $SOAK_DURATION_SECONDS
  },
  "summary": {
    "total_cycles": $CYCLE,
    "pass_count": $TOTAL_PASS,
    "warn_count": $TOTAL_WARN,
    "fail_count": $TOTAL_FAIL,
    "drift_events": $DRIFT_COUNT,
    "consecutive_failures": $CONSECUTIVE_FAILS,
    "aborted": $aborted,
    "abort_reason": $abort_reason
  },
  "cycles": $cycles_json
}
EOF

    # 写入 Markdown 摘要
    python3 - "$json_report" "$md_report" <<'PY'
import json
import sys

with open(sys.argv[1]) as f:
    report = json.load(f)

tr = report["test_run"]
sm = report["summary"]
cycles = report.get("cycles", [])

lines = []
lines.append("# Soak Test Report")
lines.append("")
lines.append("## Test Run Metadata")
lines.append("")
lines.append("| Field | Value |")
lines.append("|-------|-------|")
lines.append(f"| Start time | {tr['start_ts']} |")
lines.append(f"| End time | {tr['end_ts']} |")
lines.append(f"| Duration (s) | {tr['duration_seconds']} |")
lines.append(f"| Target duration (s) | {tr['target_duration_seconds']} |")
lines.append(f"| Interval (s) | {tr['interval_seconds']} |")
abort_reason = sm.get("abort_reason") or "-"
lines.append(f"| Aborted | {str(sm['aborted']).lower()} |")
lines.append(f"| Abort reason | {abort_reason} |")
lines.append("")

lines.append("## Summary")
lines.append("")
lines.append("| Metric | Value |")
lines.append("|--------|-------|")
lines.append(f"| Total cycles | {sm['total_cycles']} |")
lines.append(f"| Pass count | {sm['pass_count']} |")
lines.append(f"| Warn count | {sm['warn_count']} |")
lines.append(f"| Fail count | {sm['fail_count']} |")
lines.append(f"| Drift events | {sm['drift_events']} |")
lines.append(f"| Consecutive failures | {sm['consecutive_failures']} |")
lines.append("")

lines.append("## Per-Cycle Status")
lines.append("")
lines.append("| Cycle | Timestamp | Status | Warnings | Errors |")
lines.append("|-------|-----------|--------|----------|--------|")
for c in cycles:
    lines.append(f"| {c['cycle_num']} | {c['timestamp']} | {c['status']} | {len(c.get('warnings', []))} | {len(c.get('errors', []))} |")
lines.append("")

# 漂移事件与 worker 状态异常
drift_lines = []
for c in cycles:
    checks = c.get("checks", {})
    cid = c["cycle_num"]
    if checks.get("topology_drift") in ("warn", "fail"):
        for w in c.get("warnings", []):
            if "drift" in w.lower():
                drift_lines.append(f"- Cycle {cid}: {w}")
    if checks.get("heartbeat_continuity") in ("warn", "fail"):
        for e in c.get("errors", []) + c.get("warnings", []):
            lowered = e.lower()
            if "heartbeat" in lowered or "stale" in lowered or "stopping" in lowered or "stopped" in lowered or "error" in lowered:
                drift_lines.append(f"- Cycle {cid}: {e}")

lines.append("## Drift Events & Worker State Anomalies")
lines.append("")
if drift_lines:
    for dl in drift_lines:
        lines.append(dl)
else:
    lines.append("No drift events, stale heartbeats, or unexpected worker terminal states observed.")
lines.append("")

with open(sys.argv[2], "w") as f:
    f.write("\n".join(lines) + "\n")
PY

    # 清理临时文件
    rm -f "$JSON_CYCLES_FILE" "$CYCLE_WARNINGS_FILE" "$CYCLE_ERRORS_FILE" 2>/dev/null || true

    return 0
}

# 运行单轮 smoke 验证
run_smoke_cycle() {
    local cycle_ok=1
    reset_cycle_tracking

    # 1. 基础可达性
    CUR_CATEGORY="reachability"
    echo "  [reachability]"
    check "admin root"       "$ADMIN_ENDPOINT/_matrix/client/versions" 200 || cycle_ok=0
    check "admin health"     "$ADMIN_ENDPOINT/health"                  200 || true

    # 2. 拓扑检查
    CUR_CATEGORY="topology"
    echo "  [topology]"
    check_json "topology" "$ADMIN_ENDPOINT/_synapse/worker/v1/topology" 200 "$ADMIN_AUTH_HEADER" || cycle_ok=0
    check_topology_drift || cycle_ok=0

    # 3. Worker 心跳连续性
    CUR_CATEGORY="heartbeat"
    echo "  [heartbeat]"
    check_worker_heartbeat_continuity || cycle_ok=0

    # 4. Replication position 一致性
    CUR_CATEGORY="replication"
    echo "  [replication]"
    check_replication_position_consistency || cycle_ok=0

    # 5. 路由可达性
    CUR_CATEGORY="routes"
    echo "  [routes]"
    check "client versions"         "$CLIENT_ENDPOINT/_matrix/client/versions"         200 || cycle_ok=0
    check "sync route probe"        "$SYNC_ENDPOINT/_matrix/client/v3/sync"            200 || true
    check "media route probe"       "$MEDIA_ENDPOINT/_matrix/media/v3/config"          200 || true
    check "federation route probe"  "$FEDERATION_ENDPOINT/_matrix/federation/v1/version" 200 || true

    # 6. Replication 安全边界
    CUR_CATEGORY="security"
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

    CUR_CATEGORY=""
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
if [ -n "$SOAK_OUTPUT_DIR" ]; then
    echo "Output dir:      $SOAK_OUTPUT_DIR"
fi
echo ""

json_init
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

    # 记录本轮结果到 JSON 报告
    CYCLE_STATUS=$(compute_cycle_status)
    json_record_cycle "$CYCLE" "$CYCLE_STATUS"

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
