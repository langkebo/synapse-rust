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

# 可通过环境变量跳过某些检查
SKIP_TOPOLOGY="${SKIP_TOPOLOGY:-0}"
SKIP_VERSIONS="${SKIP_VERSIONS:-0}"
SKIP_CLIENT="${SKIP_CLIENT:-0}"
SKIP_REPLICATION="${SKIP_REPLICATION:-0}"
TIMEOUT="${SMOKE_TIMEOUT:-10}"

PASS=0
FAIL=0
WARN=0

# —— 工具函数 ——

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

check() {
    local name="$1"
    local url="$2"
    local expected_status="${3:-200}"
    local extra_curl_args="${4:-}"

    # shellcheck disable=SC2086
    local status
    status=$(curl -s -o /dev/null -w "%{http_code}" --max-time "$TIMEOUT" $extra_curl_args "$url" 2>/dev/null || echo "000")

    if [ "$status" = "$expected_status" ]; then
        echo -e "  ${GREEN}PASS${NC} $name (HTTP $status)"
        PASS=$((PASS + 1))
        return 0
    else
        echo -e "  ${RED}FAIL${NC} $name (expected HTTP $expected_status, got $status)"
        FAIL=$((FAIL + 1))
        return 1
    fi
}

check_json() {
    local name="$1"
    local url="$2"
    local expected_status="${3:-200}"
    local extra_curl_args="${4:-}"

    # shellcheck disable=SC2086
    local body
    body=$(curl -s --max-time "$TIMEOUT" $extra_curl_args "$url" 2>/dev/null || echo "{}")

    # shellcheck disable=SC2086
    local status
    status=$(curl -s -o /dev/null -w "%{http_code}" --max-time "$TIMEOUT" $extra_curl_args "$url" 2>/dev/null || echo "000")

    if [ "$status" != "$expected_status" ]; then
        echo -e "  ${RED}FAIL${NC} $name (expected HTTP $expected_status, got $status)"
        FAIL=$((FAIL + 1))
        return 1
    fi

    if echo "$body" | python3 -c "import sys,json; json.load(sys.stdin)" 2>/dev/null; then
        echo -e "  ${GREEN}PASS${NC} $name (valid JSON, HTTP $status)"
        PASS=$((PASS + 1))
        return 0
    else
        echo -e "  ${YELLOW}WARN${NC} $name (HTTP $status but invalid JSON)"
        WARN=$((WARN + 1))
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
    check_json "topology" "$ADMIN_ENDPOINT/_synapse/worker/v1/topology" 200

    # 验证 topology 响应中包含预期的 worker 类型
    topo=""
    topo=$(curl -s --max-time "$TIMEOUT" "$ADMIN_ENDPOINT/_synapse/worker/v1/topology" 2>/dev/null || echo "{}")
    for worker_type in master frontend synchrotron event_persister federation_reader federation_sender media_repository background pusher; do
        if echo "$topo" | python3 -c "import sys,json; d=json.load(sys.stdin); assert any('$worker_type' in str(v).lower() for v in d.values())" 2>/dev/null; then
            echo -e "  ${GREEN}PASS${NC} topology contains worker type: $worker_type"
            PASS=$((PASS + 1))
        else
            echo -e "  ${YELLOW}WARN${NC} topology may not contain worker type: $worker_type"
            WARN=$((WARN + 1))
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

# 6. 总结
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
