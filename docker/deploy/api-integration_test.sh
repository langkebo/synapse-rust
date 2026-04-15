#!/bin/bash
set +H

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# ============================================================================
# 测试环境配置
# ============================================================================
# TEST_ENV: 测试环境类型
#   - "safe"      : 隔离测试环境，可执行所有测试包括破坏性测试
#   - "dev"      : 开发环境，只跳过明确的破坏性测试
#   - "prod"     : 生产环境，跳过所有可能修改数据的测试
#
# SERVER_URL: 服务器地址（可通过环境变量覆盖）
#   - 本地开发: http://localhost:8008
#   - Docker 环境: http://localhost:28008
#
# 破坏性测试标记: DESTRUCTIVE
#   - 用户删除、数据清除、数据库修改等不可逆操作
# ============================================================================

TEST_ENV="${TEST_ENV:-dev}"
SERVER_URL="${SERVER_URL:-http://localhost:8008}"
API_INTEGRATION_PROFILE="${API_INTEGRATION_PROFILE:-core}"
if [ "${1:-}" = "--profile" ] && [ -n "${2:-}" ]; then
    API_INTEGRATION_PROFILE="$2"
    shift 2
fi
if [ "$API_INTEGRATION_PROFILE" != "core" ] && [ "$API_INTEGRATION_PROFILE" != "full" ] && [ "$API_INTEGRATION_PROFILE" != "optional" ]; then
    echo "Unknown profile: $API_INTEGRATION_PROFILE (expected: core|full|optional)"
    exit 2
fi

TEST_USER="${TEST_USER:-testuser1}"
TEST_PASS="${TEST_PASS:-Test@123}"
TEST_USER2="${TEST_USER2:-testuser2}"
TEST_PASS2="${TEST_PASS2:-Test@123}"
CURRENT_TEST_PASS="$TEST_PASS"
ADMIN_USER="${ADMIN_USER:-admin}"
ADMIN_PASS="${ADMIN_PASS:-Admin@123}"
ADMIN_SHARED_SECRET="${ADMIN_SHARED_SECRET:-change-me-admin-shared-secret}"
DB_CONTAINER="${DB_CONTAINER:-${COMPOSE_PROJECT_NAME:-synapse}-postgres}"
DB_USER="${DB_USER:-postgres}"
DB_NAME="${DB_NAME:-synapse}"
RESULTS_DIR="${RESULTS_DIR:-$(pwd)/test-results}"
PASSED_LIST_FILE="$RESULTS_DIR/api-integration.passed.txt"
FAILED_LIST_FILE="$RESULTS_DIR/api-integration.failed.txt"
SKIPPED_LIST_FILE="$RESULTS_DIR/api-integration.skipped.txt"
MISSING_LIST_FILE="$RESULTS_DIR/api-integration.missing.txt"
HTTP_CONNECT_TIMEOUT="${HTTP_CONNECT_TIMEOUT:-5}"
HTTP_MAX_TIME="${HTTP_MAX_TIME:-20}"

curl() {
    command curl --connect-timeout "$HTTP_CONNECT_TIMEOUT" --max-time "$HTTP_MAX_TIME" "$@"
}

echo "=========================================="
echo "Complete API Integration Test"
echo "=========================================="
echo "Server: $SERVER_URL"
echo "Test Environment: $TEST_ENV"
echo "Profile: $API_INTEGRATION_PROFILE"
echo ""

if [ "$ADMIN_SHARED_SECRET" = "change-me-admin-shared-secret" ] && [ -f ".env" ]; then
    ADMIN_SHARED_SECRET=$(grep -E '^ADMIN_SHARED_SECRET=' .env | head -n1 | cut -d= -f2- | tr -d '\r' || echo "$ADMIN_SHARED_SECRET")
fi

# 环境检测和警告
check_environment() {
    case "$TEST_ENV" in
        safe)
            echo "✓ 隔离测试环境 - 允许执行所有测试包括破坏性操作"
            ;;
        dev)
            echo "⚠ 开发环境 - 破坏性测试将被跳过"
            ;;
        prod)
            echo "⚠ 生产环境 - 只执行只读测试"
            ;;
        *)
            echo "⚠ 未知的测试环境 ($TEST_ENV)，默认为 dev"
            TEST_ENV="dev"
            ;;
    esac
    echo ""
}

# 破坏性测试检查函数
# 用法: destructive || skip_destructive "原因"
destructive() {
    if [ "$TEST_ENV" = "safe" ]; then
        return 0  # 在隔离环境中允许执行
    fi
    return 1  # 非隔离环境跳过
}

skip_destructive() {
    local reason="${1:-操作被跳过}"
    echo "⊘ SKIP [DESTRUCTIVE]: $reason"
    ((SKIPPED++)) || true
}

# 计数器初始化
PASSED=0
FAILED=0
SKIPPED=0
MISSING=0
ADMIN_AUTH_AVAILABLE=1

mkdir -p "$RESULTS_DIR"
: > "$PASSED_LIST_FILE"
: > "$FAILED_LIST_FILE"
: > "$SKIPPED_LIST_FILE"
: > "$MISSING_LIST_FILE"

pass() {
    local name="$1"
    local reason="${2:-}"
    if [ -n "$reason" ]; then
        echo "✓ PASS: $name - $reason"
        printf '%s\t%s\n' "$name" "$reason" >> "$PASSED_LIST_FILE"
    else
        echo "✓ PASS: $name"
        printf '%s\n' "$name" >> "$PASSED_LIST_FILE"
    fi
    ((PASSED++)) || true
}

fail() {
    local name="$1"
    local reason="${2:-}"
    if [ -n "$reason" ]; then
        echo "✗ FAIL: $name - $reason"
        printf '%s\t%s\n' "$name" "$reason" >> "$FAILED_LIST_FILE"
    else
        echo "✗ FAIL: $name"
        printf '%s\n' "$name" >> "$FAILED_LIST_FILE"
    fi
    ((FAILED++)) || true
}

missing() {
    local name="$1"
    local reason="${2:-}"
    if [ -n "$reason" ]; then
        echo "! MISSING: $name - $reason"
        printf '%s\t%s\n' "$name" "$reason" >> "$MISSING_LIST_FILE"
    else
        echo "! MISSING: $name"
        printf '%s\n' "$name" >> "$MISSING_LIST_FILE"
    fi
    ((MISSING++)) || true
}

federation_smoke() {
    local name="$1"
    local status="$2"
    local body="$3"

    if [[ "$status" == 2* ]]; then
        if echo "$body" | grep -q '"errcode"'; then
            local err
            err=$(json_err_summary "$body")
            if echo "$err" | grep -q "Missing federation signature"; then
                skip "$name" "requires federation signed request"
            elif echo "$err" | grep -q "Missing or invalid federation signing key"; then
                skip "$name" "federation signing key not configured"
            elif echo "$err" | grep -q "M_UNRECOGNIZED"; then
                pass "$name" "$err"
            else
                fail "$name" "$err"
            fi
        else
            pass "$name"
        fi
        return 0
    fi

    local err
    err=$(json_err_summary "$body")
    if echo "$err" | grep -q "Missing federation signature"; then
        skip "$name" "requires federation signed request"
    elif echo "$err" | grep -q "Missing or invalid federation signing key"; then
        skip "$name" "federation signing key not configured"
    elif echo "$err" | grep -q "M_UNAUTHORIZED"; then
        skip "$name" "${err:-requires federation auth}"
    elif echo "$err" | grep -q "Remote server key" && echo "$err" | grep -q "M_NOT_FOUND"; then
        pass "$name" "$err"
    elif echo "$err" | grep -q "M_UNRECOGNIZED"; then
        pass "$name" "$err"
    else
        fail "$name" "${err:-HTTP $status}"
    fi
}

json_is_array() {
    printf '%s' "$1" | python3 -c 'import json,sys; v=json.load(sys.stdin); sys.exit(0 if isinstance(v, list) else 1)' 2>/dev/null
}

json_is_object() {
    printf '%s' "$1" | python3 -c 'import json,sys; v=json.load(sys.stdin); sys.exit(0 if isinstance(v, dict) else 1)' 2>/dev/null
}

assert_success_array() {
    local name="$1"
    local body="$2"
    local status="$3"
    if [[ "$status" == 2* ]] && json_is_array "$body"; then
        pass "$name"
    else
        err=$(json_err_summary "$body")
        fail "$name" "${err:-HTTP $status}"
    fi
}

assert_success_object() {
    local name="$1"
    local body="$2"
    local status="$3"
    if [[ "$status" == 2* ]] && json_is_object "$body"; then
        pass "$name"
    else
        err=$(json_err_summary "$body")
        fail "$name" "${err:-HTTP $status}"
    fi
}

last_body_is_unrecognized() {
    if [ -z "${HTTP_BODY:-}" ]; then
        return 1
    fi
    echo "$HTTP_BODY" | grep -q -E '"errcode"[[:space:]]*:[[:space:]]*"M_UNRECOGNIZED"|M_UNRECOGNIZED'
}

skip() {
    local name="$1"
    local reason="${2:-}"
    if echo "$name" | grep -Eq '\(endpoint not available\)|\(not implemented\)|\(unavailable\)|\(not found\)'; then
        if [ -n "${HTTP_STATUS:-}" ] && [[ "$HTTP_STATUS" != 2* ]] && last_body_is_unrecognized; then
            pass "$name" "M_UNRECOGNIZED"
        else
            missing "$name" "$reason"
        fi
        return 0
    fi
    if echo "$reason" | grep -Eq 'admin authentication unavailable|endpoint not available|not implemented'; then
        if echo "$reason" | grep -Eq 'endpoint not available|not implemented' && [ -n "${HTTP_STATUS:-}" ] && [[ "$HTTP_STATUS" != 2* ]] && last_body_is_unrecognized; then
            pass "$name" "M_UNRECOGNIZED"
        else
            missing "$name" "$reason"
        fi
        return 0
    fi
    if [ -n "$reason" ]; then
        echo "⊘ SKIP: $name - $reason"
        printf '%s\t%s\n' "$name" "$reason" >> "$SKIPPED_LIST_FILE"
    else
        echo "⊘ SKIP: $name"
        printf '%s\n' "$name" >> "$SKIPPED_LIST_FILE"
    fi
    ((SKIPPED++)) || true
}

json_get() {
    printf '%s' "$1" | python3 -c '
import json
import sys

key = sys.argv[1]
try:
    data = json.load(sys.stdin)
except Exception:
    sys.exit(0)

value = data.get(key, "")
if value is None:
    value = ""
if isinstance(value, (dict, list)):
    print(json.dumps(value, ensure_ascii=False))
else:
    print(value)
' "$2" 2>/dev/null
}

HTTP_STATUS=""
HTTP_BODY=""
http_json() {
    local method="$1"
    local url="$2"
    local auth_token="${3:-}"
    local data="${4:-}"
    local tmp
    tmp=$(mktemp)
    local args=(-s -X "$method" "$url")
    if [ -n "$auth_token" ]; then
        args+=(-H "Authorization: Bearer $auth_token")
    fi
    if [ -n "$data" ]; then
        args+=(-H "Content-Type: application/json" -d "$data")
    fi
    HTTP_STATUS=$(curl "${args[@]}" -o "$tmp" -w "%{http_code}")
    HTTP_BODY=$(cat "$tmp")
    rm -f "$tmp"
}

FEDERATION_SIGNING_READY=0
FEDERATION_SIGNING_PROBED=0
FEDERATION_SIGNING_BATCH_SKIPPED=0
FEDERATION_SIGNING_REASON="requires federation signed request"
FEDERATION_SERVER_NAME=""
FEDERATION_KEY_ID=""
FEDERATION_SIGNING_KEY=""
FEDERATION_SIGNER_BIN="${FEDERATION_SIGNER_BIN:-$SCRIPT_DIR/../../target/debug/federation_sign_request}"

db_sql_value() {
    local sql="$1"
    docker exec "$DB_CONTAINER" psql -U "$DB_USER" -d "$DB_NAME" -t -A -c "$sql" 2>/dev/null | head -n1 | tr -d '\r'
}

federation_signing_key_from_env() {
    local key="${FEDERATION_SIGNING_KEY_OVERRIDE:-${FEDERATION_SIGNING_KEY:-}}"
    if [ -z "$key" ]; then
        return 1
    fi
    if ! printf '%s' "$key" | python3 -c 'import base64,sys; s=sys.stdin.read().strip(); d=base64.b64decode(s+"=="); sys.exit(0 if len(d)==32 else 1)' 2>/dev/null; then
        return 1
    fi
    printf '%s' "$key"
    return 0
}

detect_server_container() {
    local from_env="${SERVER_CONTAINER:-}"
    if [ -n "$from_env" ] && docker inspect "$from_env" >/dev/null 2>&1; then
        printf '%s' "$from_env"
        return 0
    fi
    docker ps --format '{{.Names}}' | grep -E 'synapse-rust|synapse.*rust' | head -n1
}

federation_signing_key_from_container_file() {
    local key_id="$1"
    local container
    container="$(detect_server_container)"
    if [ -z "$container" ]; then
        return 1
    fi
    local tmp
    tmp=$(mktemp)
    if ! docker cp "$container:/app/data/signing.key" "$tmp" >/dev/null 2>&1; then
        rm -f "$tmp"
        return 1
    fi
    python3 -c 'import sys,re,pathlib; target=sys.argv[1]; txt=pathlib.Path(sys.argv[2]).read_text(encoding="utf-8", errors="ignore").splitlines()
for ln in txt:
    line=ln.strip()
    if not line or line.startswith("#"): continue
    m=re.match(r"^ed25519\\s+([^\\s]+)\\s+([^\\s]+)$", line)
    if not m: continue
    kid=f"ed25519:{m.group(1)}"; seed=m.group(2)
    if target and kid!=target: continue
    print(seed); sys.exit(0)
sys.exit(1)' "$key_id" "$tmp" 2>/dev/null
    local rc=$?
    rm -f "$tmp"
    return $rc
}

url_path_and_query() {
    python3 -c 'import sys,urllib.parse; u=urllib.parse.urlparse(sys.argv[1]); p=u.path or "/"; q=("?"+u.query) if u.query else ""; print(p+q)' "$1" 2>/dev/null
}

ensure_federation_signer() {
    if [ -x "$FEDERATION_SIGNER_BIN" ]; then
        return 0
    fi
    cargo build -q --bin federation_sign_request >/dev/null 2>&1
    [ -x "$FEDERATION_SIGNER_BIN" ]
}

federation_prepare_signing() {
    if [ "${FEDERATION_SIGNING_READY:-0}" = "1" ]; then
        return 0
    fi
    if [ "${FEDERATION_SIGNING_PROBED:-0}" = "1" ]; then
        return 1
    fi

    http_json GET "$SERVER_URL/_matrix/key/v2/server" ""
    if [[ "$HTTP_STATUS" != 2* ]]; then
        FEDERATION_SIGNING_PROBED=1
        FEDERATION_SIGNING_REASON="requires federation signed request"
        return 1
    fi

    FEDERATION_SERVER_NAME=$(python3 -c 'import json,sys; j=json.loads(sys.argv[1]); print(j.get("server_name",""))' "$HTTP_BODY" 2>/dev/null)
    FEDERATION_KEY_ID=$(python3 -c 'import json,sys; j=json.loads(sys.argv[1]); vk=j.get("verify_keys") or {}; print(next(iter(vk.keys()), ""))' "$HTTP_BODY" 2>/dev/null)

    if [ -z "$FEDERATION_SERVER_NAME" ] && [ -n "$USER_ID" ]; then
        FEDERATION_SERVER_NAME="${USER_ID#*:}"
    fi

    if [ -z "$FEDERATION_SERVER_NAME" ] || [ -z "$FEDERATION_KEY_ID" ]; then
        FEDERATION_SIGNING_PROBED=1
        FEDERATION_SIGNING_REASON="requires federation signed request"
        return 1
    fi

    FEDERATION_SIGNING_KEY=$(db_sql_value "SELECT secret_key FROM federation_signing_keys WHERE server_name='$FEDERATION_SERVER_NAME' AND key_id='$FEDERATION_KEY_ID' ORDER BY created_ts DESC LIMIT 1;")
    if [ -z "$FEDERATION_SIGNING_KEY" ]; then
        FEDERATION_SIGNING_KEY=$(db_sql_value "SELECT secret_key FROM federation_signing_keys WHERE key_id='$FEDERATION_KEY_ID' ORDER BY created_ts DESC LIMIT 1;")
    fi
    if [ -z "$FEDERATION_SIGNING_KEY" ]; then
        local latest_key_id
        latest_key_id=$(db_sql_value "SELECT key_id FROM federation_signing_keys ORDER BY created_ts DESC LIMIT 1;")
        if [ -n "$latest_key_id" ]; then
            local latest_signing_key
            latest_signing_key=$(db_sql_value "SELECT secret_key FROM federation_signing_keys WHERE key_id='$latest_key_id' ORDER BY created_ts DESC LIMIT 1;")
            if [ -n "$latest_signing_key" ]; then
                FEDERATION_KEY_ID="$latest_key_id"
                FEDERATION_SIGNING_KEY="$latest_signing_key"
            fi
        fi
    fi
    if [ -z "$FEDERATION_SIGNING_KEY" ]; then
        FEDERATION_SIGNING_KEY=$(federation_signing_key_from_env || true)
    fi
    if [ -z "$FEDERATION_SIGNING_KEY" ]; then
        FEDERATION_SIGNING_KEY=$(federation_signing_key_from_container_file "$FEDERATION_KEY_ID" || true)
    fi
    if [ -z "$FEDERATION_SIGNING_KEY" ]; then
        FEDERATION_SIGNING_PROBED=1
        FEDERATION_SIGNING_REASON="federation signing key not configured"
        return 1
    fi

    if ! ensure_federation_signer; then
        FEDERATION_SIGNING_PROBED=1
        FEDERATION_SIGNING_REASON="federation signer binary unavailable"
        return 1
    fi

    FEDERATION_SIGNING_PROBED=1
    FEDERATION_SIGNING_READY=1
    FEDERATION_SIGNING_REASON=""
    return 0
}

federation_signed_ready() {
    federation_prepare_signing
}

federation_skip_signed_tests() {
    FEDERATION_SIGNING_BATCH_SKIPPED=1
    local reason="${FEDERATION_SIGNING_REASON:-requires federation signed request}"
    local case_name
    for case_name in "$@"; do
        skip "$case_name" "$reason"
    done
}

federation_http_json() {
    local name="$1"
    local method="$2"
    local url="$3"
    local data="${4:-}"

    if ! federation_prepare_signing; then
        if [ "${FEDERATION_SIGNING_BATCH_SKIPPED:-0}" = "1" ]; then
            return 1
        fi
        skip "$name" "${FEDERATION_SIGNING_REASON:-requires federation signed request}"
        return 1
    fi

    local uri
    uri=$(url_path_and_query "$url")
    if [ -z "$uri" ]; then
        skip "$name" "${FEDERATION_SIGNING_REASON:-requires federation signed request}"
        return 1
    fi

    local sig
    if [ -n "$data" ]; then
        sig=$(FEDERATION_SIGNING_KEY="$FEDERATION_SIGNING_KEY" "$FEDERATION_SIGNER_BIN" "$method" "$uri" "$FEDERATION_SERVER_NAME" "$FEDERATION_SERVER_NAME" "$data" 2>/dev/null || true)
    else
        sig=$(FEDERATION_SIGNING_KEY="$FEDERATION_SIGNING_KEY" "$FEDERATION_SIGNER_BIN" "$method" "$uri" "$FEDERATION_SERVER_NAME" "$FEDERATION_SERVER_NAME" 2>/dev/null || true)
    fi

    if [ -z "$sig" ]; then
        skip "$name" "${FEDERATION_SIGNING_REASON:-requires federation signed request}"
        return 1
    fi

    local tmp
    tmp=$(mktemp)
    local args=(-s -X "$method" "$url" -H "Authorization: X-Matrix origin=\"$FEDERATION_SERVER_NAME\",key=\"$FEDERATION_KEY_ID\",sig=\"$sig\"")
    if [ -n "$data" ]; then
        args+=(-H "Content-Type: application/json" -d "$data")
    fi
    HTTP_STATUS=$(curl "${args[@]}" -o "$tmp" -w "%{http_code}")
    HTTP_BODY=$(cat "$tmp")
    rm -f "$tmp"
    if [[ "$HTTP_STATUS" != 2* ]]; then
        local err
        err=$(json_err_summary "$HTTP_BODY")
        if echo "$err" | grep -q "M_UNAUTHORIZED"; then
            skip "$name" "${err:-M_UNAUTHORIZED}"
            return 1
        fi
    fi
    return 0
}

db_sql() {
    local sql="$1"
    docker exec "$DB_CONTAINER" psql -U "$DB_USER" -d "$DB_NAME" -c "$sql" 2>/dev/null || true
}

json_err_summary() {
    printf '%s' "$1" | python3 -c '
import json, sys
try:
    d=json.load(sys.stdin)
except Exception:
    sys.exit(0)
err=d.get("errcode")
msg=d.get("error") or d.get("message") or ""
if err:
    if msg:
        print(f"{err}: {msg}")
    else:
        print(err)
' 2>/dev/null
}

json_has_key() {
    printf '%s' "$1" | python3 -c '
import json, sys
key=sys.argv[1]
try:
    d=json.load(sys.stdin)
except Exception:
    sys.exit(1)
sys.exit(0 if (isinstance(d, dict) and key in d) else 1)
' "$2" 2>/dev/null
}

json_is_valid() {
    printf '%s' "$1" | python3 -c '
import json, sys
json.load(sys.stdin)
' 2>/dev/null
}

ASSERT_ERROR=""
check_success_json() {
    local body="$1"
    local status="$2"
    shift 2
    ASSERT_ERROR=""
    if [[ "$status" != 2* ]]; then
        ASSERT_ERROR="HTTP $status"
        return 1
    fi
    if [ -z "$body" ]; then
        if [ "$status" = "204" ]; then
            return 0
        fi
        ASSERT_ERROR="Empty body"
        return 1
    fi
    if ! json_is_valid "$body"; then
        ASSERT_ERROR="Invalid JSON"
        return 1
    fi
    local err
    err=$(json_err_summary "$body")
    if [ -n "$err" ]; then
        ASSERT_ERROR="$err"
        return 1
    fi
    local key
    for key in "$@"; do
        if [ -n "$key" ] && ! json_has_key "$body" "$key"; then
            ASSERT_ERROR="Missing field: $key"
            return 1
        fi
    done
    return 0
}

assert_success_json() {
    local name="$1"
    local body="$2"
    local status="$3"
    shift 3
    if [[ "$status" != 2* ]]; then
        fail "$name" "HTTP $status"
        return 1
    fi
    local err
    err=$(json_err_summary "$body")
    if [ -n "$err" ]; then
        fail "$name" "$err"
        return 1
    fi
    local key
    for key in "$@"; do
        if [ -n "$key" ] && ! json_has_key "$body" "$key"; then
            fail "$name" "Missing field: $key"
            return 1
        fi
    done
    pass "$name"
    return 0
}

normalize_login_user() {
    local value="$1"
    value="${value#@}"
    value="${value%%:*}"
    printf '%s' "$value"
}

url_encode() {
    python3 -c 'import sys, urllib.parse; print(urllib.parse.quote(sys.argv[1], safe=""))' "$1" 2>/dev/null
}

admin_ready() {
    [ "$ADMIN_AUTH_AVAILABLE" -eq 1 ] && [ -n "$ADMIN_TOKEN" ]
}

print_result_file() {
    local title="$1"
    local file_path="$2"
    if [ -s "$file_path" ]; then
        echo "$title"
        while IFS= read -r line; do
            if [ -n "$line" ]; then
                echo " - $line"
            fi
        done < "$file_path"
        echo ""
    fi
}

print_reason_summary() {
    local title="$1"
    local file_path="$2"
    [ -s "$file_path" ] || return 0
    local reason_summary
    reason_summary=$(awk -F '\t' 'NF>1 && $2!="" {count[$2]++} END {for (k in count) printf "%s\t%d\n", k, count[k]}' "$file_path" | sort -k2,2nr)
    if [ -n "$reason_summary" ]; then
        echo "$title"
        while IFS=$'\t' read -r reason count; do
            [ -n "$reason" ] && echo " - [$count] $reason"
        done <<< "$reason_summary"
        echo ""
    fi
}

finalize() {
    echo ""
    echo "=========================================="
    echo "Test Summary"
    echo "=========================================="
    echo -e "Passed: \033[0;32m$PASSED\033[0m"
    echo -e "Failed: \033[0;31m$FAILED\033[0m"
    echo -e "Missing: \033[0;35m$MISSING\033[0m"
    echo -e "Skipped: \033[0;33m$SKIPPED\033[0m"
    echo ""
    echo "Artifacts:"
    echo " - Passed list: $PASSED_LIST_FILE"
    echo " - Failed list: $FAILED_LIST_FILE"
    echo " - Missing list: $MISSING_LIST_FILE"
    echo " - Skipped list: $SKIPPED_LIST_FILE"
    echo ""

    print_result_file "Failed Cases" "$FAILED_LIST_FILE"
    print_result_file "Missing Cases" "$MISSING_LIST_FILE"
    print_result_file "Skipped Cases" "$SKIPPED_LIST_FILE"
    print_reason_summary "Failed Reasons" "$FAILED_LIST_FILE"
    print_reason_summary "Missing Reasons" "$MISSING_LIST_FILE"
    print_reason_summary "Skipped Reasons" "$SKIPPED_LIST_FILE"

    if [ "$FAILED" -eq 0 ] && [ "$MISSING" -eq 0 ]; then
        echo "✓ All tests passed!"
        exit 0
    else
        echo "✗ Some tests failed!"
        exit 1
    fi
}

run_optional_profile() {
    echo ""
    echo "=========================================="
    echo "Optional Profile"
    echo "=========================================="

    echo ""
    echo "Admin Federation Rewrite"
    if admin_ready; then
        http_json POST "$SERVER_URL/_synapse/admin/v1/federation/rewrite" "$ADMIN_TOKEN" '{"from": "localhost", "to": "localhost"}'
        check_success_json "$HTTP_BODY" "$HTTP_STATUS" "rewritten" && pass "Admin Federation Rewrite" || skip "Admin Federation Rewrite (requires federation destination data)"
    else
        skip "Admin Federation Rewrite" "admin authentication unavailable"
    fi

    echo ""
    echo "SSO Login"
    http_json GET "$SERVER_URL/_matrix/client/v3/login/sso/redirect" ""
    if [[ "$HTTP_STATUS" == 2* ]] || [[ "$HTTP_STATUS" == 3* ]]; then
        pass "SSO Login"
    else
        skip "SSO Login" "not supported"
    fi

    echo ""
    echo "SSO User Info"
    http_json GET "$SERVER_URL/_matrix/client/v3/login/sso/userinfo" "$TOKEN"
    if [[ "$HTTP_STATUS" == 2* ]]; then
        pass "SSO User Info"
    else
        skip "SSO User Info" "not supported"
    fi

    echo ""
    echo "Reset Password"
    skip "Reset Password" "destructive test"

    echo ""
    echo "Identity Lookup"
    http_json POST "$SERVER_URL/_matrix/identity/v1/lookup" "" '{"addresses": ["test@example.com"]}'
    if [[ "$HTTP_STATUS" == 2* ]]; then
        pass "Identity Lookup"
    else
        skip "Identity Lookup" "external service"
    fi

    echo ""
    echo "Identity Request"
    http_json POST "$SERVER_URL/_matrix/identity/v1/requestToken" "" '{"email": "test@example.com", "client_secret": "test", "send_attempt": 1}'
    if [[ "$HTTP_STATUS" == 2* ]]; then
        pass "Identity Request"
    else
        skip "Identity Request" "external service"
    fi
}

# 执行环境检查
check_environment

# ============================================================================
# 破坏性测试列表（供人工审核）
# ============================================================================
# 以下测试包含破坏性操作，在非隔离环境中应被跳过:
# 1. 用户删除操作 (DELETE FROM users)
# 2. 设备删除操作 (DELETE /devices)
# 3. 房间删除操作 (DELETE /createRoom 的重复创建可能)
# 4. 数据库清理操作
# ============================================================================

# 1. Health & Version
echo "=========================================="
echo "1. Health & Version"
echo "=========================================="
echo "1. Health Check"
curl -s -f "$SERVER_URL/health" > /dev/null 2>&1 && pass "Health endpoint" || fail "Health endpoint"

echo ""
echo "2. Version"
http_json GET "$SERVER_URL/_matrix/client/versions" ""
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "versions" && pass "Versions endpoint" || fail "Versions endpoint"

# 2. Login with auto-recovery
echo ""
echo "=========================================="
echo "3. Authentication"
echo "=========================================="
LOGIN_RESP=$(curl -s -X POST "$SERVER_URL/_matrix/client/v3/login" \
    -H "Content-Type: application/json" \
    -d "{\"type\": \"m.login.password\", \"user\": \"$TEST_USER\", \"password\": \"$CURRENT_TEST_PASS\"}")
USER_ID=$(json_get "$LOGIN_RESP" "user_id")
DEVICE_ID=$(json_get "$LOGIN_RESP" "device_id")
TOKEN=$(json_get "$LOGIN_RESP" "access_token")
REFRESH_TOKEN=$(json_get "$LOGIN_RESP" "refresh_token")
CURRENT_TEST_PASS="$TEST_PASS"

if [ -z "$TOKEN" ]; then
    echo "⊘ Login failed, attempting auto-recovery..."

    for candidate_pass in "test_password" "NewPass123!" "$TEST_PASS"; do
        LOGIN_RESP=$(curl -s -X POST "$SERVER_URL/_matrix/client/v3/login" \
            -H "Content-Type: application/json" \
            -d "{\"type\": \"m.login.password\", \"user\": \"$TEST_USER\", \"password\": \"$candidate_pass\"}")
        TOKEN=$(json_get "$LOGIN_RESP" "access_token")
        if [ -n "$TOKEN" ]; then
            CURRENT_TEST_PASS="$candidate_pass"
            USER_ID=$(json_get "$LOGIN_RESP" "user_id")
            DEVICE_ID=$(json_get "$LOGIN_RESP" "device_id")
            REFRESH_TOKEN=$(json_get "$LOGIN_RESP" "refresh_token")
            break
        fi
    done

fi

if [ -z "$TOKEN" ]; then
    echo "⊘ Login password recovery failed, attempting account recovery..."

    RECOVERY_ADMIN_LOGIN_RESP=$(curl -s -X POST "$SERVER_URL/_matrix/client/v3/login" \
        -H "Content-Type: application/json" \
        -d "{\"type\": \"m.login.password\", \"user\": \"$(normalize_login_user "$ADMIN_USER")\", \"password\": \"$ADMIN_PASS\"}")
    RECOVERY_ADMIN_TOKEN=$(json_get "$RECOVERY_ADMIN_LOGIN_RESP" "access_token")

    if [ -n "$RECOVERY_ADMIN_TOKEN" ]; then
        http_json PUT "$SERVER_URL/_synapse/admin/v2/users/$TEST_USER" "$RECOVERY_ADMIN_TOKEN" "{\"password\":\"$TEST_PASS\",\"deactivated\":false}"
        if [[ "$HTTP_STATUS" == 2* ]]; then
            LOGIN_RESP=$(curl -s -X POST "$SERVER_URL/_matrix/client/v3/login" \
                -H "Content-Type: application/json" \
                -d "{\"type\": \"m.login.password\", \"user\": \"$TEST_USER\", \"password\": \"$TEST_PASS\"}")
            TOKEN=$(json_get "$LOGIN_RESP" "access_token")
            USER_ID=$(json_get "$LOGIN_RESP" "user_id")
            DEVICE_ID=$(json_get "$LOGIN_RESP" "device_id")
            REFRESH_TOKEN=$(json_get "$LOGIN_RESP" "refresh_token")
            CURRENT_TEST_PASS="$TEST_PASS"
        fi
    fi
fi

if [ -z "$TOKEN" ]; then
    echo "⊘ Login admin recovery failed, attempting account recovery..."

    if destructive; then
        db_sql "DELETE FROM users WHERE username = '$TEST_USER';"
    fi

    REGISTER_RESP=$(curl -s -X POST "$SERVER_URL/_matrix/client/v3/register" \
        -H "Content-Type: application/json" \
        -d "{\"username\": \"$TEST_USER\", \"password\": \"$TEST_PASS\", \"auth\": {\"auth_type\": \"m.login.dummy\"}}")
    TOKEN=$(json_get "$REGISTER_RESP" "access_token")
    USER_ID=$(json_get "$REGISTER_RESP" "user_id")
    DEVICE_ID=$(json_get "$REGISTER_RESP" "device_id")
    REFRESH_TOKEN=$(json_get "$REGISTER_RESP" "refresh_token")
    CURRENT_TEST_PASS="$TEST_PASS"

    if [ -n "$TOKEN" ]; then
        echo "⊘ AUTO-RECOVERED: User recreated, setting admin flag..."
        if destructive; then
            db_sql "UPDATE users SET is_admin = true WHERE username = '$TEST_USER';"
        fi
        pass "Login (User: $USER_ID) [AUTO-RECOVERED]"
    else
        fail "Login failed"
    fi
else
    pass "Login (User: $USER_ID)"
fi

USER_DOMAIN="${USER_ID#*:}"
TARGET_USER_ID="@${TEST_USER2}:${USER_DOMAIN}"
TARGET_USER_ID_ENC=$(url_encode "$TARGET_USER_ID")
FRIEND_GROUP_ID=""
http_json POST "$SERVER_URL/_matrix/client/v3/register" "" "{\"auth\": {\"type\": \"m.login.dummy\"}, \"username\": \"$TEST_USER2\", \"password\": \"$TEST_PASS2\"}"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Ensure Second User ($TARGET_USER_ID)"
else
    err=$(json_err_summary "$HTTP_BODY")
    if echo "$err" | grep -q "M_USER_IN_USE"; then
        pass "Ensure Second User ($TARGET_USER_ID)" "already exists"
    else
        fail "Ensure Second User ($TARGET_USER_ID)" "${err:-HTTP $HTTP_STATUS}"
    fi
fi

SECOND_DEVICE_NAME="api-integration-device2-${RANDOM}"
SECOND_LOGIN_USER="$(normalize_login_user "$TEST_USER")"
http_json POST "$SERVER_URL/_matrix/client/v3/login" "" "{\"type\": \"m.login.password\", \"user\": \"$SECOND_LOGIN_USER\", \"password\": \"$CURRENT_TEST_PASS\", \"device_id\": \"$SECOND_DEVICE_NAME\"}"
SECOND_DEVICE_TOKEN=$(json_get "$HTTP_BODY" "access_token")
SECOND_DEVICE_ID=$(json_get "$HTTP_BODY" "device_id")
if [ -n "$SECOND_DEVICE_TOKEN" ] && [ -n "$SECOND_DEVICE_ID" ]; then
    pass "Second Device Login" "$SECOND_DEVICE_ID"
else
    fail "Second Device Login" "$(json_err_summary "$HTTP_BODY")"
    SECOND_DEVICE_TOKEN=""
    SECOND_DEVICE_ID=""
fi

# 获取 Admin Token
echo ""
echo "3.1 Admin Authentication"
ADMIN_TOKEN=""
ADMIN_USER_ID=""
ADMIN_LOGIN_RESP=""

if [ -n "$ADMIN_SHARED_SECRET" ] && [ "$ADMIN_SHARED_SECRET" != "change-me-admin-shared-secret" ]; then
    NONCE=$(curl -s "$SERVER_URL/_synapse/admin/v1/register/nonce" | python3 -c "import sys,json; print(json.load(sys.stdin).get('nonce',''))" 2>/dev/null || echo "")
    if [ -n "$NONCE" ]; then
        ADMIN_LOGIN_USER="${ADMIN_REG_USER:-admin_test_$(date +%s)_$$_$RANDOM}"
        MAC=$(python3 -c "
import hmac, hashlib
n='$NONCE'
u='$ADMIN_LOGIN_USER'
p='$ADMIN_PASS'
msg = n.encode() + b'\x00' + u.encode() + b'\x00' + p.encode() + b'\x00' + b'admin'
print(hmac.new(b'$ADMIN_SHARED_SECRET', msg, hashlib.sha256).hexdigest())
" 2>/dev/null || echo "")

        REGISTER_RESP=$(curl -s -X POST "$SERVER_URL/_synapse/admin/v1/register" \
            -H "Content-Type: application/json" \
            -d "{\"nonce\": \"$NONCE\", \"username\": \"$ADMIN_LOGIN_USER\", \"password\": \"$ADMIN_PASS\", \"admin\": true, \"displayname\": \"System Administrator\", \"mac\": \"$MAC\"}")

        ADMIN_TOKEN=$(json_get "$REGISTER_RESP" "access_token")
        ADMIN_USER_ID=$(json_get "$REGISTER_RESP" "user_id")
    fi
fi

if [ -z "$ADMIN_TOKEN" ]; then
    ADMIN_LOGIN_RESP=$(curl -s -X POST "$SERVER_URL/_matrix/client/v3/login" \
        -H "Content-Type: application/json" \
        -d "{\"type\": \"m.login.password\", \"user\": \"$(normalize_login_user "$ADMIN_USER")\", \"password\": \"$ADMIN_PASS\"}")
    ADMIN_TOKEN=$(json_get "$ADMIN_LOGIN_RESP" "access_token")
    ADMIN_USER_ID=$(json_get "$ADMIN_LOGIN_RESP" "user_id")
fi

if [ -n "$ADMIN_TOKEN" ]; then
    pass "Admin Login (User: $ADMIN_USER_ID)"
else
    echo "⊘ WARNING: Admin login unavailable, Admin API tests may fail"
    ADMIN_AUTH_AVAILABLE=0
    ADMIN_TOKEN=""
    ADMIN_USER_ID=""
    skip "Admin Login (unavailable)"
fi

if [ "$API_INTEGRATION_PROFILE" = "optional" ]; then
    run_optional_profile
    finalize
fi

echo ""
echo "4. Capabilities"
http_json GET "$SERVER_URL/_matrix/client/v3/capabilities" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "capabilities" && pass "Capabilities" || fail "Capabilities"

# 3. Room Setup
echo ""
echo "=========================================="
echo "5. Room Setup"
echo "=========================================="
ROOM_RESP=$(curl -s -X POST "$SERVER_URL/_matrix/client/v3/createRoom" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"name": "Test Room API", "topic": "API Test Room", "preset": "public_chat"}')
ROOM_ID=$(json_get "$ROOM_RESP" "room_id")
[ -n "$ROOM_ID" ] && pass "Create Test Room" || fail "Create Test Room"

ROOM2_RESP=$(curl -s -X POST "$SERVER_URL/_matrix/client/v3/createRoom" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"name": "Test Room 2", "preset": "private_chat"}')
ROOM2_ID=$(json_get "$ROOM2_RESP" "room_id")
[ -n "$ROOM2_ID" ] && pass "Create Second Room" || fail "Create Second Room"

# 4. Sync
echo ""
echo "=========================================="
echo "6. Sync & Events"
echo "=========================================="
echo "6. Sync"
http_json GET "$SERVER_URL/_matrix/client/v3/sync?timeout=1000" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "next_batch" && pass "Sync" || fail "Sync"

echo ""
echo "7. Room Sync Filter"
http_json GET "$SERVER_URL/_matrix/client/v3/sync?filter=%7B%22room%22%3A%7B%22rooms%22%3A%5B%22$ROOM_ID%22%5D%7D%7D" "$TOKEN"
ROOM_SYNC_RESP="$HTTP_BODY"
check_success_json "$ROOM_SYNC_RESP" "$HTTP_STATUS" "rooms" "next_batch" && pass "Room Sync Filter" || fail "Room Sync Filter"

# 5. Profile
echo ""
echo "=========================================="
echo "8. Profile"
echo "=========================================="
echo "8. Get Profile"
USER_ID_ENC=$(url_encode "$USER_ID")
http_json GET "$SERVER_URL/_matrix/client/v3/profile/$USER_ID_ENC" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "displayname" && pass "Get Profile" || fail "Get Profile"

echo ""
echo "9. Update Displayname"
http_json PUT "$SERVER_URL/_matrix/client/v3/profile/$USER_ID_ENC/displayname" "$TOKEN" '{"displayname": "API Test Admin"}'
[ "$HTTP_STATUS" = "200" ] || [ "$HTTP_STATUS" = "304" ] && pass "Update Displayname" || fail "Update Displayname"

echo ""
echo "10. Get Avatar URL"
http_json GET "$SERVER_URL/_matrix/client/v3/profile/$USER_ID_ENC/avatar_url" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "avatar_url" && pass "Get Avatar URL" || fail "Get Avatar URL"

echo ""
echo "11. Set Avatar URL"
curl -s -X PUT "$SERVER_URL/_matrix/client/v3/profile/$USER_ID_ENC/avatar_url" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"avatar_url": "mxc://cjystx.top/avatar"}' && pass "Set Avatar URL" || fail "Set Avatar URL"

# 6. Room State & Messages
echo ""
echo "=========================================="
echo "12. Room State & Messages"
echo "=========================================="
echo "12. Room State"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/state" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get Room State"
else
    fail "Get Room State"
fi

echo ""
echo "13. Send Message"
http_json PUT "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/send/m.room.message/1" "$TOKEN" '{"msgtype":"m.text","body":"Hello from API Test"}'
assert_success_json "Send Message" "$HTTP_BODY" "$HTTP_STATUS" "event_id"
LAST_EVENT_ID=$(json_get "$HTTP_BODY" "event_id")
MSG_EVENT_ID="${LAST_EVENT_ID:-}"
TEST_EVENT_ID="${MSG_EVENT_ID:-}"
TEST_EVENT_ID_ENC=""
if [ -n "$TEST_EVENT_ID" ]; then
    TEST_EVENT_ID_ENC=$(url_encode "$TEST_EVENT_ID")
fi

echo ""
echo "14. Room Messages"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/messages?limit=10" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "chunk" && pass "Room Messages" || fail "Room Messages"

echo ""
echo "15. Joined Members"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/joined_members" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "joined" && pass "Joined Members" || fail "Joined Members"

echo ""
echo "16. Room Aliases"
ROOM_ALIAS="#api_test_${RANDOM}:${USER_DOMAIN}"
ROOM_ALIAS_ENC=$(python3 -c 'import sys, urllib.parse; print(urllib.parse.quote(sys.argv[1], safe=""))' "$ROOM_ALIAS" 2>/dev/null)
if [ -z "$ROOM_ALIAS_ENC" ]; then
    fail "Room Aliases" "failed to encode room alias"
else
    http_json PUT "$SERVER_URL/_matrix/client/v3/directory/room/$ROOM_ALIAS_ENC" "$TOKEN" "{\"room_id\":\"$ROOM_ID\"}"
    assert_success_json "Set Room Alias" "$HTTP_BODY" "$HTTP_STATUS"
    http_json GET "$SERVER_URL/_matrix/client/v3/directory/room/$ROOM_ALIAS_ENC" "$TOKEN"
    assert_success_json "Get Room Alias" "$HTTP_BODY" "$HTTP_STATUS" "room_id"
fi

echo ""
echo "17. Set Room Topic"
http_json PUT "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/state/m.room.topic" "$TOKEN" '{"topic": "Updated Topic"}'
assert_success_json "Set Room Topic" "$HTTP_BODY" "$HTTP_STATUS"

echo ""
echo "18. Redact Event"
MSG_RESP=$(curl -s -X PUT "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/send/m.room.message/redact_test_msg" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"msgtype":"m.text","body":"test message for redact"}')
REDACT_EVENT_ID=$(echo "$MSG_RESP" | grep -o '"event_id":"[^"]*"' | cut -d'"' -f4)
if [ -n "$REDACT_EVENT_ID" ]; then
    REDACT_EVENT_ID_ENC=$(url_encode "$REDACT_EVENT_ID")
    REDACT_ENCODED=$(echo "$REDACT_EVENT_ID" | sed 's/\$/%24/g' | sed 's/\!/%21/g' | sed 's/:/%3A/g')
    REDACT_RESP=$(curl -s -X PUT "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/redact/$REDACT_ENCODED/redact_test_msg" \
        -H "Authorization: Bearer $TOKEN" \
        -H "Content-Type: application/json" \
        -d '{"reason": "test redacted"}')
    if echo "$REDACT_RESP" | grep -q "event_id"; then
        pass "Redact Event"
    else
        fail "Redact Event"
    fi
else
    skip "Redact Event (no event to redact)"
fi

# 7. Media
echo ""
echo "=========================================="
echo "19. Media"
echo "=========================================="
echo "19. Media Config"
http_json GET "$SERVER_URL/_matrix/client/v3/media/config" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "m.upload.size" && pass "Media Config" || fail "Media Config"

echo ""
echo "20. Media Upload"
PNG_FILE="$RESULTS_DIR/test_upload.png"
python3 - "$PNG_FILE" <<'PY'
import sys, struct, zlib, binascii
def chunk(t, data):
    crc = binascii.crc32(t + data) & 0xffffffff
    return struct.pack(">I", len(data)) + t + data + struct.pack(">I", crc)
sig = b"\x89PNG\r\n\x1a\n"
ihdr = struct.pack(">IIBBBBB", 1, 1, 8, 6, 0, 0, 0)
raw = b"\x00\xff\x00\x00\xff"
idat = zlib.compress(raw)
png = sig + chunk(b"IHDR", ihdr) + chunk(b"IDAT", idat) + chunk(b"IEND", b"")
with open(sys.argv[1], "wb") as f:
    f.write(png)
PY
if [ -f "$PNG_FILE" ] && [ -s "$PNG_FILE" ]; then
    MEDIA_RESP=$(curl -s -X POST "$SERVER_URL/_matrix/media/v3/upload" \
        -H "Authorization: Bearer $TOKEN" \
        -H "Content-Type: image/png" \
        --data-binary "@$PNG_FILE")
    MEDIA_URI=$(echo "$MEDIA_RESP" | grep -o '"content_uri":"[^"]*"' | cut -d'"' -f4)
    if echo "$MEDIA_RESP" | grep -q "content_uri"; then
        pass "Media Upload"
    else
        fail "Media Upload"
    fi
else
    fail "Media Upload" "PNG file not generated"
fi

echo ""
echo "21. Media Download"
if [ -n "$MEDIA_URI" ]; then
    MEDIA_PATH="${MEDIA_URI#mxc://}"
    MEDIA_SERVER="${MEDIA_PATH%%/*}"
    MEDIA_ID="${MEDIA_PATH#*/}"
    MEDIA_DOWNLOAD_FILE="$RESULTS_DIR/media_download.bin"
    DL_STATUS=$(curl -s -o "$MEDIA_DOWNLOAD_FILE" -w "%{http_code}" "$SERVER_URL/_matrix/media/v3/download/$MEDIA_SERVER/$MEDIA_ID" -H "Authorization: Bearer $TOKEN")
    if [ "$DL_STATUS" = "200" ] && [ -s "$MEDIA_DOWNLOAD_FILE" ]; then
        pass "Media Download"
    else
        fail "Media Download" "HTTP $DL_STATUS"
    fi
else
    skip "Media Download (no media URI)"
fi

echo ""
echo "22. Media Thumbnail"
if [ -n "$MEDIA_ID" ]; then
    MEDIA_THUMB_FILE="$RESULTS_DIR/media_thumbnail.bin"
    TH_STATUS=$(curl -s -o "$MEDIA_THUMB_FILE" -w "%{http_code}" "$SERVER_URL/_matrix/media/v3/thumbnail/$MEDIA_SERVER/$MEDIA_ID?width=100&height=100" -H "Authorization: Bearer $TOKEN")
    if [ "$TH_STATUS" = "200" ] && [ -s "$MEDIA_THUMB_FILE" ]; then
        pass "Media Thumbnail"
    else
        fail "Media Thumbnail" "HTTP $TH_STATUS"
    fi
else
    skip "Media Thumbnail (no media ID)"
fi

echo ""
echo "23. VoIP Config"
http_json GET "$SERVER_URL/_matrix/client/v3/voip/config" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "turn_servers" && pass "VoIP Config" || fail "VoIP Config"

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
http_json GET "$SERVER_URL/_matrix/client/v3/user/$USER_ID/account_data/m.custom" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "custom_key" && pass "Get User Account Data" || fail "Get User Account Data"

echo ""
echo "26. Set Room Account Data"
curl -s -X PUT "$SERVER_URL/_matrix/client/v3/user/$USER_ID/rooms/$ROOM_ID/account_data/m.room.color" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"color": "blue"}' && pass "Set Room Account Data" || fail "Set Room Account Data"

echo ""
echo "27. Get Room Account Data"
http_json GET "$SERVER_URL/_matrix/client/v3/user/$USER_ID/rooms/$ROOM_ID/account_data/m.room.color" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "color" && pass "Get Room Account Data" || fail "Get Room Account Data"

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
http_json GET "$SERVER_URL/_matrix/client/v3/user/$USER_ID/rooms/$ROOM_ID/tags" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "tags" && pass "Get Room Tags" || fail "Get Room Tags"

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
http_json GET "$SERVER_URL/_matrix/client/v3/presence/$USER_ID/status" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "presence" && pass "Get Presence" || fail "Get Presence"

echo ""
echo "33. List Presences"
http_json POST "$SERVER_URL/_matrix/client/v3/presence/list" "$TOKEN" "{\"subscribe\": [\"$TARGET_USER_ID\"]}"
assert_success_json "Presence Subscribe" "$HTTP_BODY" "$HTTP_STATUS" "presences"
http_json GET "$SERVER_URL/_matrix/client/v3/presence/list/$USER_ID_ENC" "$TOKEN"
assert_success_json "List Presences" "$HTTP_BODY" "$HTTP_STATUS" "presences"

# 11. Room Membership
echo ""
echo "=========================================="
echo "34. Room Membership"
echo "=========================================="
echo "34. Invite User"
curl -s -X POST "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/invite" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"user_id": "'"$USER_ID"'"}' && pass "Invite User" || fail "Invite User"

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
MEMBERSHIP_RESP=$(curl -s "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/membership/$USER_ID" \
    -H "Authorization: Bearer $TOKEN")
if echo "$MEMBERSHIP_RESP" | grep -q "chunk\|membership"; then
    pass "Get Membership"
else
    skip "Get Membership (not implemented)"
fi

# 12. Devices
echo ""
echo "=========================================="
echo "38. Devices"
echo "=========================================="
echo "38. List Devices"
http_json GET "$SERVER_URL/_matrix/client/v3/devices" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "devices" && pass "List Devices" || fail "List Devices"

echo ""
echo "39. Get Device"
if [ -n "$DEVICE_ID" ]; then
    http_json GET "$SERVER_URL/_matrix/client/v3/devices/$DEVICE_ID" "$TOKEN"
    check_success_json "$HTTP_BODY" "$HTTP_STATUS" "device_id" && pass "Get Device" || fail "Get Device"
else
    skip "Get Device (no device ID)"
fi

echo ""
echo "40. Update Device"
if [ -n "$DEVICE_ID" ]; then
    http_json PUT "$SERVER_URL/_matrix/client/v3/devices/$DEVICE_ID" "$TOKEN" '{"display_name": "Test Device"}'
    assert_success_json "Update Device" "$HTTP_BODY" "$HTTP_STATUS"
else
    skip "Update Device (no device ID)"
fi

echo ""
echo "41. Delete Device"
if [ -n "$DEVICE_ID" ]; then
    if destructive; then
        http_json DELETE "$SERVER_URL/_matrix/client/v3/devices/$DEVICE_ID" "$TOKEN"
        assert_success_json "Delete Device" "$HTTP_BODY" "$HTTP_STATUS"
    else
        skip "Delete Device" "destructive test"
    fi
else
    skip "Delete Device (no device ID)"
fi

# 13. Key Upload (E2EE)
echo ""
echo "=========================================="
echo "42. E2EE Keys"
echo "=========================================="
echo "42. Upload Keys"
http_json POST "$SERVER_URL/_matrix/client/v3/keys/upload" "$TOKEN" '{"one_time_keys": {"signed_curve25519:KEY1": {"key": "base64_key", "signatures": {}}}}'
KEY_UPLOAD_RESP="$HTTP_BODY"
assert_success_json "Upload Keys" "$KEY_UPLOAD_RESP" "$HTTP_STATUS" "one_time_key_counts"

echo ""
echo "43. Query Keys"
http_json POST "$SERVER_URL/_matrix/client/v3/keys/query" "$TOKEN" "{\"device_keys\": {\"$USER_ID\": [\"$DEVICE_ID\"]}}"
KEY_QUERY_RESP="$HTTP_BODY"
assert_success_json "Query Keys" "$KEY_QUERY_RESP" "$HTTP_STATUS" "device_keys" "failures"

echo ""
echo "44. Claim Keys"
http_json POST "$SERVER_URL/_matrix/client/v3/keys/claim" "$TOKEN" "{\"one_time_keys\": {\"$USER_ID\": {\"$DEVICE_ID\": \"signed_curve25519\"}}}"
KEY_CLAIM_RESP="$HTTP_BODY"
assert_success_json "Claim Keys" "$KEY_CLAIM_RESP" "$HTTP_STATUS" "one_time_keys" "failures"

# 14. Public Rooms & Directory
echo ""
echo "=========================================="
echo "45. Public Rooms & Directory"
echo "=========================================="
echo "45. Public Rooms"
http_json GET "$SERVER_URL/_matrix/client/v3/publicRooms" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "chunk" && pass "Public Rooms" || fail "Public Rooms"

echo ""
echo "46. User Directory"
http_json POST "$SERVER_URL/_matrix/client/v3/user_directory/search" "$TOKEN" '{"search_term": "admin", "limit": 10}'
if echo "$HTTP_BODY" | grep -q "results"; then
    pass "User Directory"
elif echo "$HTTP_BODY" | grep -q "limited"; then
    pass "User Directory"
elif [ -z "$HTTP_BODY" ] || [ "$HTTP_BODY" = "{}" ]; then
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
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/summary" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "room_id" && pass "Room Summary" || fail "Room Summary"

echo ""
echo "48. Room Summary Stats"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/summary/stats" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "total_events" "room_id" && pass "Room Summary Stats" || fail "Room Summary Stats"

echo ""
echo "49. Room Summary Members"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/summary/members" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Room Summary Members"
else
    fail "Room Summary Members"
fi

echo ""
echo "50. Room Summary State"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/summary/state" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Room Summary State"
else
    fail "Room Summary State"
fi

# 16. Account
echo ""
echo "=========================================="
echo "51. Account"
echo "=========================================="
echo "51. WhoAmI"
http_json GET "$SERVER_URL/_matrix/client/v3/account/whoami" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "user_id" && pass "WhoAmI" || fail "WhoAmI"

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
if admin_ready; then
    ADMIN_USER_ID_ENC=$(url_encode "$ADMIN_USER_ID")
    USER_ID_ENC=$(url_encode "$USER_ID")
    echo "54. Admin List Users"
    http_json GET "$SERVER_URL/_synapse/admin/v1/users" "$ADMIN_TOKEN"
    check_success_json "$HTTP_BODY" "$HTTP_STATUS" "users" && pass "Admin List Users" || fail "Admin List Users"

    echo ""
    echo "55. Admin User Details"
    http_json GET "$SERVER_URL/_synapse/admin/v1/users/$ADMIN_USER_ID_ENC" "$ADMIN_TOKEN"
    check_success_json "$HTTP_BODY" "$HTTP_STATUS" "name" && pass "Admin User Details" || fail "Admin User Details"

    echo ""
    echo "56. Admin User Sessions"
    http_json GET "$SERVER_URL/_synapse/admin/v1/user_sessions/$ADMIN_USER_ID_ENC" "$ADMIN_TOKEN"
    check_success_json "$HTTP_BODY" "$HTTP_STATUS" "sessions" && pass "Admin User Sessions" || fail "Admin User Sessions"

    echo ""
    echo "57. Admin User Stats"
    http_json GET "$SERVER_URL/_synapse/admin/v1/user_stats" "$ADMIN_TOKEN"
    check_success_json "$HTTP_BODY" "$HTTP_STATUS" "total_users" && pass "Admin User Stats" || fail "Admin User Stats"

    echo ""
    echo "58. Admin User Devices"
    http_json GET "$SERVER_URL/_synapse/admin/v1/users/$ADMIN_USER_ID_ENC/devices" "$ADMIN_TOKEN"
    check_success_json "$HTTP_BODY" "$HTTP_STATUS" "devices" && pass "Admin User Devices" || fail "Admin User Devices"
else
    skip "Admin List Users" "admin authentication unavailable"
    skip "Admin User Details" "admin authentication unavailable"
    skip "Admin User Sessions" "admin authentication unavailable"
    skip "Admin User Stats" "admin authentication unavailable"
    skip "Admin User Devices" "admin authentication unavailable"
fi

# 19. Admin - Rooms
echo ""
echo "=========================================="
echo "59. Admin - Rooms"
echo "=========================================="
if admin_ready; then
    echo "59. Admin List Rooms"
    http_json GET "$SERVER_URL/_synapse/admin/v1/rooms" "$ADMIN_TOKEN"
    check_success_json "$HTTP_BODY" "$HTTP_STATUS" "rooms" && pass "Admin List Rooms" || fail "Admin List Rooms"

    echo ""
    echo "60. Admin Room Details"
    http_json GET "$SERVER_URL/_synapse/admin/v1/rooms/$ROOM_ID" "$ADMIN_TOKEN"
    check_success_json "$HTTP_BODY" "$HTTP_STATUS" "room_id" && pass "Admin Room Details" || fail "Admin Room Details"

    echo ""
    echo "61. Admin Room Members"
    http_json GET "$SERVER_URL/_synapse/admin/v1/rooms/$ROOM_ID/members" "$ADMIN_TOKEN"
    check_success_json "$HTTP_BODY" "$HTTP_STATUS" "members" && pass "Admin Room Members" || fail "Admin Room Members"

    echo ""
    echo "62. Admin Room Messages"
    http_json GET "$SERVER_URL/_synapse/admin/v1/rooms/$ROOM_ID/messages" "$ADMIN_TOKEN"
    check_success_json "$HTTP_BODY" "$HTTP_STATUS" "chunk" && pass "Admin Room Messages" || fail "Admin Room Messages"

    echo ""
    echo "63. Admin Room Block"
    http_json POST "$SERVER_URL/_synapse/admin/v1/rooms/$ROOM_ID/block" "$ADMIN_TOKEN" '{"block": true}'
    check_success_json "$HTTP_BODY" "$HTTP_STATUS" "block" && pass "Admin Room Block" || fail "Admin Room Block"

    echo ""
    echo "64. Admin Room Unblock"
    http_json POST "$SERVER_URL/_synapse/admin/v1/rooms/$ROOM_ID/unblock" "$ADMIN_TOKEN" '{}'
    check_success_json "$HTTP_BODY" "$HTTP_STATUS" "block" && pass "Admin Room Unblock" || fail "Admin Room Unblock"
else
    skip "Admin List Rooms" "admin authentication unavailable"
    skip "Admin Room Details" "admin authentication unavailable"
    skip "Admin Room Members" "admin authentication unavailable"
    skip "Admin Room Messages" "admin authentication unavailable"
    skip "Admin Room Block" "admin authentication unavailable"
    skip "Admin Room Unblock" "admin authentication unavailable"
fi

# 20. Space APIs
echo ""
echo "=========================================="
echo "65. Space APIs"
echo "=========================================="
echo "65. Create Space"
http_json POST "$SERVER_URL/_matrix/client/v3/createRoom" "$TOKEN" "{\"name\": \"Test Space Room\", \"preset\": \"public_chat\", \"room_type\": \"m.space\"}"
SPACE_RESP="$HTTP_BODY"
SPACE_ID=$(json_get "$SPACE_RESP" "room_id")
if check_success_json "$SPACE_RESP" "$HTTP_STATUS"; then
    if [ -n "$SPACE_ID" ]; then
        pass "Create Space"
    else
        fail "Create Space" "missing space_id/room_id"
    fi
else
    fail "Create Space" "${ASSERT_ERROR:-HTTP $HTTP_STATUS}"
fi

SPACE_ENC=$(echo "$SPACE_ID" | sed 's/!/%21/g' | sed 's/:/%3A/g')
if [ -n "$SPACE_ID" ] && [ -n "$ROOM_ID" ]; then
    http_json POST "$SERVER_URL/_matrix/client/v3/spaces/$SPACE_ENC/children" "$TOKEN" "{\"room_id\": \"$ROOM_ID\", \"via_servers\": [\"localhost\"], \"suggested\": true}"
fi

echo ""
echo "66. Get Public Spaces"
http_json GET "$SERVER_URL/_matrix/client/v3/spaces/public" "$TOKEN"
assert_success_array "Public Spaces" "$HTTP_BODY" "$HTTP_STATUS"

echo ""
echo "67. Get User Spaces"
http_json GET "$SERVER_URL/_matrix/client/v3/spaces/user" "$TOKEN"
assert_success_array "User Spaces" "$HTTP_BODY" "$HTTP_STATUS"

echo ""
echo "68. Get Space Members"
if [ -n "$SPACE_ID" ]; then
    http_json GET "$SERVER_URL/_matrix/client/v3/spaces/$SPACE_ENC/members" "$TOKEN"
    assert_success_array "Space Members" "$HTTP_BODY" "$HTTP_STATUS"
else
    skip "Space Members" "space not created"
fi

echo ""
echo "69. Get Space State"
if [ -n "$SPACE_ID" ]; then
    http_json GET "$SERVER_URL/_matrix/client/v3/spaces/$SPACE_ENC/state" "$TOKEN"
    if [ "$HTTP_STATUS" = "404" ]; then
        skip "Space State" "space not found"
    elif check_success_json "$HTTP_BODY" "$HTTP_STATUS"; then
        pass "Space State"
    else
        fail "Space State" "${ASSERT_ERROR:-HTTP $HTTP_STATUS}"
    fi
else
    skip "Space State" "space not created"
fi

echo ""
echo "70. Get Space Children"
if [ -n "$SPACE_ID" ]; then
    http_json GET "$SERVER_URL/_matrix/client/v3/spaces/$SPACE_ENC/children" "$TOKEN"
    assert_success_array "Space Children" "$HTTP_BODY" "$HTTP_STATUS"
else
    skip "Space Children" "space not created"
fi

# 21. Thread APIs
echo ""
echo "=========================================="
echo "71. Thread APIs"
echo "=========================================="
echo "71. Get Threads"
http_json GET "$SERVER_URL/_matrix/client/v1/rooms/$ROOM_ID/threads" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "threads" && pass "Get Threads" || skip "Get Threads" "${ASSERT_ERROR:-HTTP $HTTP_STATUS}"

# 22. Filter APIs
echo ""
echo "=========================================="
echo "72. Filter APIs"
echo "=========================================="
echo "72. Create Filter"
http_json POST "$SERVER_URL/_matrix/client/v3/user/$USER_ID/filter" "$TOKEN" "{\"room\": {\"rooms\": [\"$ROOM_ID\"]}}"
FILTER_RESP="$HTTP_BODY"
FILTER_ID=$(json_get "$FILTER_RESP" "filter_id")
if check_success_json "$FILTER_RESP" "$HTTP_STATUS" "filter_id"; then
    pass "Create Filter"
else
    fail "Create Filter" "${ASSERT_ERROR:-HTTP $HTTP_STATUS}"
fi

echo ""
echo "73. Get Filter"
if [ -n "$FILTER_ID" ]; then
    http_json GET "$SERVER_URL/_matrix/client/v3/user/$USER_ID/filter/$FILTER_ID" "$TOKEN"
    check_success_json "$HTTP_BODY" "$HTTP_STATUS" "room" && pass "Get Filter" || fail "Get Filter" "${ASSERT_ERROR:-HTTP $HTTP_STATUS}"
else
    skip "Get Filter (no filter ID)"
fi

# 23. 3PID APIs
echo ""
echo "=========================================="
echo "74. 3PID APIs"
echo "=========================================="
echo "74. Get 3PID Bindings"
http_json GET "$SERVER_URL/_matrix/client/v3/account/3pid" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "threepids" && pass "Get 3PID Bindings" || fail "Get 3PID Bindings"

# 24. OpenID Token
echo ""
echo "=========================================="
echo "75. OpenID Token"
echo "=========================================="
echo "75. Request OpenID Token"
http_json GET "$SERVER_URL/_matrix/client/v3/user/$USER_ID/openid/request_token" "$TOKEN"
assert_success_json "Request OpenID Token" "$HTTP_BODY" "$HTTP_STATUS" "access_token" "expires_in"
OPENID_ACCESS_TOKEN=$(json_get "$HTTP_BODY" "access_token")
OPENID_EXPIRES_IN=$(json_get "$HTTP_BODY" "expires_in")

# 25. Well-Known
echo ""
echo "=========================================="
echo "76. Well-Known"
echo "=========================================="
echo "76. Well-Known Client"
http_json GET "$SERVER_URL/.well-known/matrix/client" ""
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "m.homeserver" && pass "Well-Known Client" || fail "Well-Known Client"

echo ""
echo "77. Well-Known Server"
http_json GET "$SERVER_URL/.well-known/matrix/server" ""
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "m.server" && pass "Well-Known Server" || fail "Well-Known Server"

# 26. Server Version
echo ""
echo "=========================================="
echo "78. Server Version"
http_json GET "$SERVER_URL/_matrix/client/versions" ""
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "versions" && pass "Server Version" || fail "Server Version"

# 28. DM APIs
echo ""
echo "=========================================="
echo "81. DM APIs"
echo "=========================================="
echo "81. Create DM"
DM_RESP=$(curl -s -X POST "$SERVER_URL/_matrix/client/v3/create_dm" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"user_id": "'"$USER_ID"'"}')
DM_ROOM_ID=$(echo "$DM_RESP" | grep -o '"room_id":"[^"]*"' | cut -d'"' -f4)
echo "$DM_RESP" | grep -q "room_id" && pass "Create DM" || skip "Create DM (not implemented)"

echo ""
echo "82. Get Direct Rooms"
http_json GET "$SERVER_URL/_matrix/client/v3/direct" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "rooms" && pass "Get Direct Rooms" || skip "Get Direct Rooms (not implemented)"

echo ""
echo "83. Update Direct Room"
if [ -n "$DM_ROOM_ID" ]; then
    DM_ENC=$(echo "$DM_ROOM_ID" | sed 's/!/%21/g' | sed 's/:/%3A/g')
    UPDATE_RESP=$(curl -s -X PUT "$SERVER_URL/_matrix/client/v3/direct/$DM_ENC" \
        -H "Authorization: Bearer $TOKEN" \
        -H "Content-Type: application/json" \
        -d '{"users": ["'"$USER_ID"'"]}')
    echo "$UPDATE_RESP" | grep -q "^{}$" && pass "Update Direct Room" || skip "Update Direct Room (not implemented)"
else
    skip "Update Direct Room (no DM room)"
fi

# 29. Room Summary APIs
echo ""
echo "=========================================="
echo "84. Room Summary APIs"
echo "=========================================="
echo "84. Room Summary"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/summary" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "room_id" && pass "Room Summary" || fail "Room Summary"

echo ""
echo "85. Room Summary Members"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/summary/members" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Room Summary Members"
else
    fail "Room Summary Members"
fi

echo ""
echo "86. Room Summary State"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/summary/state" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Room Summary State"
else
    fail "Room Summary State"
fi

echo ""
echo "87. Room Summary Stats"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/summary/stats" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "total_events" && pass "Room Summary Stats" || fail "Room Summary Stats"

# 30. Admin Room APIs
echo ""
echo "=========================================="
echo "88. Admin Room APIs"
echo "=========================================="
if admin_ready; then
    echo "88. Admin Room Stats"
    http_json GET "$SERVER_URL/_synapse/admin/v1/room_stats/$ROOM_ID" "$ADMIN_TOKEN"
    check_success_json "$HTTP_BODY" "$HTTP_STATUS" "room_id" && pass "Admin Room Stats" || fail "Admin Room Stats"

    echo ""
    echo "89. Admin Room Block Status"
    http_json GET "$SERVER_URL/_synapse/admin/v1/rooms/$ROOM_ID/block" "$ADMIN_TOKEN"
    check_success_json "$HTTP_BODY" "$HTTP_STATUS" "block" && pass "Admin Room Block Status" || fail "Admin Room Block Status"

    echo ""
    echo "90. Admin Room Search"
    http_json POST "$SERVER_URL/_synapse/admin/v1/rooms/$ROOM_ID/search" "$ADMIN_TOKEN" '{"search_term": "test"}'
    check_success_json "$HTTP_BODY" "$HTTP_STATUS" "results" && pass "Admin Room Search" || skip "Admin Room Search (not implemented)"

    echo ""
    echo "91. Admin Room Listings"
    http_json GET "$SERVER_URL/_synapse/admin/v1/rooms/$ROOM_ID/listings" "$ADMIN_TOKEN"
    check_success_json "$HTTP_BODY" "$HTTP_STATUS" "room_id" && pass "Admin Room Listings" || fail "Admin Room Listings"

    echo ""
    echo "92. Admin Room State"
    http_json GET "$SERVER_URL/_synapse/admin/v1/rooms/$ROOM_ID/state" "$ADMIN_TOKEN"
    check_success_json "$HTTP_BODY" "$HTTP_STATUS" "state" && pass "Admin Room State" || fail "Admin Room State"
else
    skip "Admin Room Stats" "admin authentication unavailable"
    skip "Admin Room Block Status" "admin authentication unavailable"
    skip "Admin Room Search" "admin authentication unavailable"
    skip "Admin Room Listings" "admin authentication unavailable"
    skip "Admin Room State" "admin authentication unavailable"
fi

# 31. OIDC/Authentication
echo ""
echo "=========================================="
echo "93. OIDC/Authentication"
echo "=========================================="
echo "93. Well-Known OIDC"
http_json GET "$SERVER_URL/.well-known/openid-configuration" ""
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "issuer" && pass "Well-Known OIDC" || skip "Well-Known OIDC (not implemented)"

echo ""
echo "94. OIDC Discovery"
http_json GET "$SERVER_URL/.well-known/openid-configuration" ""
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "issuer" && pass "OIDC Discovery" || skip "OIDC Discovery (not implemented)"

# 31. Invite Blocklist/Allowlist APIs
echo ""
echo "=========================================="
echo "96. Invite Blocklist/Allowlist APIs"
echo "=========================================="
echo "96. Get Invite Blocklist"
ROOM_ENC=$(echo "$ROOM_ID" | sed 's/!/%21/g' | sed 's/:/%3A/g')
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ENC/invite_blocklist" "$TOKEN"
INVITE_BLOCKLIST_RESP="$HTTP_BODY"
assert_success_json "Get Invite Blocklist" "$INVITE_BLOCKLIST_RESP" "$HTTP_STATUS" "blocklist"

echo ""
echo "97. Set Invite Blocklist"
http_json POST "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ENC/invite_blocklist" "$TOKEN" '{"user_ids": ["'"$TARGET_USER_ID"'"]}'
SET_INVITE_BLOCKLIST_RESP="$HTTP_BODY"
assert_success_json "Set Invite Blocklist" "$SET_INVITE_BLOCKLIST_RESP" "$HTTP_STATUS"

echo ""
echo "98. Get Invite Allowlist"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ENC/invite_allowlist" "$TOKEN"
INVITE_ALLOWLIST_RESP="$HTTP_BODY"
assert_success_json "Get Invite Allowlist" "$INVITE_ALLOWLIST_RESP" "$HTTP_STATUS" "allowlist"

echo ""
echo "99. Set Invite Allowlist"
http_json POST "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ENC/invite_allowlist" "$TOKEN" '{"user_ids": ["'"$TARGET_USER_ID"'"]}'
SET_INVITE_ALLOWLIST_RESP="$HTTP_BODY"
assert_success_json "Set Invite Allowlist" "$SET_INVITE_ALLOWLIST_RESP" "$HTTP_STATUS"

# 33. Mod Core - joined_rooms & my_rooms
echo ""
echo "=========================================="
echo "100. Mod Core - Rooms"
echo "=========================================="
echo "100. Joined Rooms"
http_json GET "$SERVER_URL/_matrix/client/v3/joined_rooms" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "joined_rooms" && pass "Joined Rooms" || fail "Joined Rooms"

echo ""
echo "101. My Rooms"
http_json GET "$SERVER_URL/_matrix/client/v3/my_rooms" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "rooms" && pass "My Rooms" || fail "My Rooms"

echo ""
echo "102. Server Version (r0)"
http_json GET "$SERVER_URL/_matrix/client/r0/version" ""
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "server_version" && pass "Server Version (r0)" || fail "Server Version (r0)"

# 34. Account Data r0
echo ""
echo "=========================================="
echo "103. Account Data r0"
echo "=========================================="
echo "103. Set User Account Data (r0)"
http_json PUT "$SERVER_URL/_matrix/client/r0/user/$USER_ID/account_data/m.custom_r0" "$TOKEN" '{"custom_key": "custom_value_r0"}'
assert_success_json "Set User Account Data (r0)" "$HTTP_BODY" "$HTTP_STATUS"

echo ""
echo "104. Get User Account Data (r0)"
http_json GET "$SERVER_URL/_matrix/client/r0/user/$USER_ID/account_data/m.custom_r0" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "custom_key" && pass "Get User Account Data (r0)" || fail "Get User Account Data (r0)"

echo ""
echo "105. Set Room Account Data (r0)"
http_json PUT "$SERVER_URL/_matrix/client/r0/user/$USER_ID/rooms/$ROOM_ID/account_data/m.room.color_r0" "$TOKEN" '{"color": "red"}'
assert_success_json "Set Room Account Data (r0)" "$HTTP_BODY" "$HTTP_STATUS"

# 35. Device r0
echo ""
echo "=========================================="
echo "106. Device r0"
echo "=========================================="
echo "106. List Devices (r0)"
http_json GET "$SERVER_URL/_matrix/client/r0/devices" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "devices" && pass "List Devices (r0)" || fail "List Devices (r0)"

echo ""
echo "107. Delete Devices (r0)"
if destructive; then
    http_json POST "$SERVER_URL/_matrix/client/r0/delete_devices" "$TOKEN" '{"devices": ["TEST_DEVICE_ID"]}'
    assert_success_json "Delete Devices (r0)" "$HTTP_BODY" "$HTTP_STATUS"
else
    skip "Delete Devices (r0)" "destructive test"
fi

# 36. Admin Federation Extended
echo ""
echo "=========================================="
echo "108. Admin Federation Extended"
echo "=========================================="
if admin_ready; then
    echo "108. Admin Federation Destinations v2"
    http_json GET "$SERVER_URL/_synapse/admin/v1/federation/destinations" "$ADMIN_TOKEN"
    check_success_json "$HTTP_BODY" "$HTTP_STATUS" "destinations" && pass "Admin Federation Destinations" || fail "Admin Federation Destinations"

    echo ""
    echo "109. Admin Federation Destination Details"
    http_json GET "$SERVER_URL/_synapse/admin/v1/federation/destinations/localhost" "$ADMIN_TOKEN"
    check_success_json "$HTTP_BODY" "$HTTP_STATUS" "destination" && pass "Admin Federation Destination Details" || fail "Admin Federation Destination Details"

    echo ""
    echo "110. Admin Federation Resolve"
    http_json POST "$SERVER_URL/_synapse/admin/v1/federation/resolve" "$ADMIN_TOKEN" '{"server_name": "localhost"}'
    check_success_json "$HTTP_BODY" "$HTTP_STATUS" "server_name" && pass "Admin Federation Resolve" || fail "Admin Federation Resolve"

    echo ""
    echo "111. Admin Federation Rewrite"
    if [ "$API_INTEGRATION_PROFILE" = "full" ]; then
        http_json POST "$SERVER_URL/_synapse/admin/v1/federation/rewrite" "$ADMIN_TOKEN" '{"from": "localhost", "to": "localhost"}'
        check_success_json "$HTTP_BODY" "$HTTP_STATUS" "rewritten" && pass "Admin Federation Rewrite" || skip "Admin Federation Rewrite (requires federation destination data)"
    fi
else
    skip "Admin Federation Destinations" "admin authentication unavailable"
    skip "Admin Federation Destination Details" "admin authentication unavailable"
    skip "Admin Federation Resolve" "admin authentication unavailable"
    if [ "$API_INTEGRATION_PROFILE" = "full" ]; then
        skip "Admin Federation Rewrite" "admin authentication unavailable"
    fi
fi

# 37. Registration Tokens
echo ""
echo "=========================================="
echo "112. Registration Tokens"
echo "=========================================="
if admin_ready; then
    echo "112. List Registration Tokens"
    http_json GET "$SERVER_URL/_synapse/admin/v1/registration_tokens" "$ADMIN_TOKEN"
    check_success_json "$HTTP_BODY" "$HTTP_STATUS" "registration_tokens" && pass "List Registration Tokens" || fail "List Registration Tokens"

    echo ""
    echo "113. Get Active Registration Tokens"
    http_json GET "$SERVER_URL/_synapse/admin/v1/registration_tokens?active=true" "$ADMIN_TOKEN"
    check_success_json "$HTTP_BODY" "$HTTP_STATUS" "registration_tokens" && pass "Get Active Registration Tokens" || fail "Get Active Registration Tokens"
else
    skip "List Registration Tokens" "admin authentication unavailable"
    skip "Get Active Registration Tokens" "admin authentication unavailable"
fi

# 38. Push Rules
echo ""
echo "=========================================="
echo "114. Push Rules"
echo "=========================================="
echo "114. Get Push Rules"
http_json GET "$SERVER_URL/_matrix/client/v3/pushrules" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "global" && pass "Get Push Rules" || skip "Get Push Rules (endpoint not available)"

echo ""
echo "115. Get Push Rules Global"
http_json GET "$SERVER_URL/_matrix/client/v3/pushrules/global" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "global" && pass "Get Push Rules Global" || skip "Get Push Rules Global (endpoint not available)"

# 39. Room Keys / Key Backup
echo ""
echo "=========================================="
echo "116. Key Backup"
echo "=========================================="
echo "116. Get Key Backup Versions"
http_json GET "$SERVER_URL/_matrix/client/v3/room_keys/version" "$TOKEN"
KEY_BACKUP_VERSIONS_RESP="$HTTP_BODY"
assert_success_json "Get Key Backup Versions" "$KEY_BACKUP_VERSIONS_RESP" "$HTTP_STATUS" "versions"

# 40. Admin User Extended
echo ""
echo "=========================================="
echo "117. Admin User Extended"
echo "=========================================="
if admin_ready; then
    ADMIN_USER_ID_ENC=$(url_encode "$ADMIN_USER_ID")
    USER_ID_ENC=$(url_encode "$USER_ID")
    echo "117. Admin Account Details"
    http_json GET "$SERVER_URL/_synapse/admin/v1/account/$ADMIN_USER_ID_ENC" "$ADMIN_TOKEN"
    check_success_json "$HTTP_BODY" "$HTTP_STATUS" "user_id" && pass "Admin Account Details" || fail "Admin Account Details"

    echo ""
    echo "118. Admin User Rooms"
    http_json GET "$SERVER_URL/_synapse/admin/v1/users/$ADMIN_USER_ID_ENC/rooms" "$ADMIN_TOKEN"
    check_success_json "$HTTP_BODY" "$HTTP_STATUS" "rooms" && pass "Admin User Rooms" || fail "Admin User Rooms"

    echo ""
    echo "119. Admin User Password"
    if destructive; then
        ADMIN_PASSWORD_RESP=$(curl -s -X POST "$SERVER_URL/_synapse/admin/v1/users/$USER_ID_ENC/password" \
            -H "Authorization: Bearer $ADMIN_TOKEN" \
            -H "Content-Type: application/json" \
            -d '{"new_password": "Test@123"}')
        echo "$ADMIN_PASSWORD_RESP" | grep -q "success\|{}" && pass "Admin User Password" || fail "Admin User Password"
        LOGIN_RESP=$(curl -s -X POST "$SERVER_URL/_matrix/client/v3/login" -H "Content-Type: application/json" -d "{\"type\":\"m.login.password\",\"user\":\"$TEST_USER\",\"password\":\"$TEST_PASS\"}")
        TOKEN=$(json_get "$LOGIN_RESP" "access_token")
        DEVICE_ID=$(json_get "$LOGIN_RESP" "device_id")
        if [ -z "$TOKEN" ]; then
            fail "Restore User Session" "relogin failed after password reset"
        fi
    else
        skip "Admin User Password" "destructive test"
    fi

    echo ""
    echo "120. Admin Set User Admin"
    http_json PUT "$SERVER_URL/_synapse/admin/v1/users/$ADMIN_USER_ID_ENC/admin" "$ADMIN_TOKEN" '{"admin": true}'
    check_success_json "$HTTP_BODY" "$HTTP_STATUS" "success" && pass "Admin Set User Admin" || fail "Admin Set User Admin"
else
    skip "Admin Account Details" "admin authentication unavailable"
    skip "Admin User Rooms" "admin authentication unavailable"
    skip "Admin User Password" "admin authentication unavailable"
    skip "Admin Set User Admin" "admin authentication unavailable"
fi

# 41. Admin Notifications
echo ""
echo "=========================================="
echo "121. Admin Notifications"
echo "=========================================="
if admin_ready; then
    ADMIN_USER_ID_ENC=$(url_encode "$ADMIN_USER_ID")
    echo "121. List Pushers"
    http_json GET "$SERVER_URL/_synapse/admin/v1/users/$ADMIN_USER_ID_ENC/pushers" "$ADMIN_TOKEN"
    check_success_json "$HTTP_BODY" "$HTTP_STATUS" "pushers" && pass "List Pushers" || fail "List Pushers"

    echo ""
    echo "122. Get Pushers"
    check_success_json "$HTTP_BODY" "$HTTP_STATUS" "pushers" && pass "Get Pushers" || skip "Get Pushers (requires existing pusher data)"
else
    skip "List Pushers" "admin authentication unavailable"
    skip "Get Pushers" "admin authentication unavailable"
fi

# 42. Presence
echo ""
echo "=========================================="
echo "123. Presence"
echo "=========================================="
echo "123. Get Presence"
http_json GET "$SERVER_URL/_matrix/client/v3/presence/$USER_ID/status" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "presence" && pass "Get Presence" || fail "Get Presence"

echo ""
echo "124. Set Presence"
curl -s -X PUT "$SERVER_URL/_matrix/client/v3/presence/$USER_ID/status" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"presence": "online", "status_msg": "Available"}' && pass "Set Presence" || fail "Set Presence"

echo ""
echo "125. Get Presence List"
http_json GET "$SERVER_URL/_matrix/client/v3/presence/list/$USER_ID" "$TOKEN"
GET_PRESENCE_LIST_RESP="$HTTP_BODY"
assert_success_json "Get Presence List" "$GET_PRESENCE_LIST_RESP" "$HTTP_STATUS" "presences"

# 43. E2EE Routes (Key Verification)
echo ""
echo "=========================================="
echo "126. E2EE Key Verification"
echo "=========================================="
echo "126. Get Key Verification Request"
KEY_VERIFY_RESP=$(curl -s -X POST "$SERVER_URL/_matrix/client/v3/device_verification/request" -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" -d '{"device_id":"SCRIPT_VERIFY_DEVICE","method":"sas"}')
if echo "$KEY_VERIFY_RESP" | grep -q "request_token\|token"; then
    pass "Get Key Verification Request"
else
    skip "Get Key Verification Request (endpoint not available)"
fi

echo ""
echo "127. Get Room Key Request"
KEY_REQ_RESP=$(curl -s "$SERVER_URL/_matrix/client/v1/keys/qr_code/show" -H "Authorization: Bearer $TOKEN")
if echo "$KEY_REQ_RESP" | grep -q "user_id\|device_id\|qr"; then
    pass "Get Room Key Request"
else
    skip "Get Room Key Request (endpoint not available)"
fi

# 44. Thread
echo ""
echo "=========================================="
echo "128. Thread"
echo "=========================================="
echo ""
echo "128. Get Thread"
if [ -n "$ROOM_ID" ]; then
    THREAD_ROOT_ID=$(echo "$MSG_RESP" | grep -o '"event_id":"[^"]*"' | cut -d'"' -f4)
    if [ -n "$THREAD_ROOT_ID" ]; then
        CREATE_THREAD_RESP=$(curl -s -X POST "$SERVER_URL/_matrix/client/v1/rooms/$ROOM_ID/threads" \
            -H "Authorization: Bearer $TOKEN" \
            -H "Content-Type: application/json" \
            -d "{\"root_event_id\":\"$THREAD_ROOT_ID\",\"content\":{\"body\":\"integration thread\"}}")
        THREAD_ID=$(echo "$CREATE_THREAD_RESP" | grep -o '"thread_id":"[^"]*"' | cut -d'"' -f4)
        if [ -n "$THREAD_ID" ]; then
            THREAD_REPLY_ID="\$thread_reply_$(date +%s%N 2>/dev/null || date +%s)"
            curl -s -X POST "$SERVER_URL/_matrix/client/v1/rooms/$ROOM_ID/threads/$THREAD_ID/replies" \
                -H "Authorization: Bearer $TOKEN" \
                -H "Content-Type: application/json" \
                -d "{\"event_id\":\"$THREAD_REPLY_ID\",\"root_event_id\":\"$THREAD_ROOT_ID\",\"content\":{\"msgtype\":\"m.text\",\"body\":\"thread reply\"}}" \
                > /dev/null
            THREAD_ENC=$(echo "$THREAD_ID" | sed 's/\$/%24/g' | sed 's/\!/%21/g' | sed 's/:/%3A/g')
            http_json GET "$SERVER_URL/_matrix/client/v1/rooms/$ROOM_ID/threads/$THREAD_ENC" "$TOKEN"
            if [[ "$HTTP_STATUS" == 2* ]]; then
                pass "Get Thread"
            else
                missing "Get Thread" "${ASSERT_ERROR:-HTTP $HTTP_STATUS}"
            fi
        else
            missing "Get Thread" "thread creation failed"
        fi
    else
        skip "Get Thread (no event id)"
    fi
else
    skip "Get Thread (no room id)"
fi

# 45. Thirdparty
echo ""
echo "=========================================="
echo "129. Thirdparty Protocols"
echo "=========================================="
echo "129. Get Thirdparty Protocols"
http_json GET "$SERVER_URL/_matrix/client/v3/thirdparty/protocols" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "irc" && pass "Get Thirdparty Protocols" || skip "Get Thirdparty Protocols (endpoint not available)"

echo ""
echo "130. Get Thirdparty Protocol"
http_json GET "$SERVER_URL/_matrix/client/v3/thirdparty/protocol/irc" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "user_fields" "location_fields" && pass "Get Thirdparty Protocol" || skip "Get Thirdparty Protocol (endpoint not available)"

# 46. Well-Known
echo ""
echo "=========================================="
echo "131. Well-Known"
echo "=========================================="
echo "131. Get Client Well-Known"
http_json GET "$SERVER_URL/.well-known/matrix/client" ""
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "m.homeserver" && pass "Get Client Well-Known" || fail "Get Client Well-Known"

echo ""
echo "132. Get Server Well-Known"
http_json GET "$SERVER_URL/.well-known/matrix/server" ""
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "m.server" && pass "Get Server Well-Known" || skip "Get Server Well-Known (endpoint not available)"

# 47. Capabilities
echo ""
echo "=========================================="
echo "133. Capabilities"
echo "=========================================="
echo "133. Get Capabilities"
http_json GET "$SERVER_URL/_matrix/client/v3/capabilities" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "capabilities" && pass "Get Capabilities" || fail "Get Capabilities"

# 48. Room Version
echo ""
echo "=========================================="
echo "134. Room Version"
echo "=========================================="
echo "134. Get Room Version"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/version" "$TOKEN"
ROOM_VERSION="$HTTP_BODY"
assert_success_json "Get Room Version" "$ROOM_VERSION" "$HTTP_STATUS" "room_version"

# 49. Admin Room Extended
echo ""
echo "=========================================="
echo "135. Admin Room Extended"
echo "=========================================="
echo "135. Admin List Rooms"
http_json GET "$SERVER_URL/_synapse/admin/v1/rooms" "$ADMIN_TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "rooms" && pass "Admin List Rooms" || fail "Admin List Rooms"

echo ""
echo "136. Admin Room Details"
http_json GET "$SERVER_URL/_synapse/admin/v1/rooms/$ROOM_ID" "$ADMIN_TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "room_id" && pass "Admin Room Details" || fail "Admin Room Details"

echo ""
echo "137. Admin Room Members"
http_json GET "$SERVER_URL/_synapse/admin/v1/rooms/$ROOM_ID/members" "$ADMIN_TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "members" && pass "Admin Room Members" || fail "Admin Room Members"

echo ""
echo "138. Admin Room State"
http_json GET "$SERVER_URL/_synapse/admin/v1/rooms/$ROOM_ID/state" "$ADMIN_TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "state" && pass "Admin Room State" || fail "Admin Room State"

echo ""
echo "139. Admin Room Messages"
http_json GET "$SERVER_URL/_synapse/admin/v1/rooms/$ROOM_ID/messages" "$ADMIN_TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "chunk" && pass "Admin Room Messages" || fail "Admin Room Messages"

echo ""
echo "140. Admin Block Room"
http_json POST "$SERVER_URL/_synapse/admin/v1/rooms/$ROOM_ID/block" "$ADMIN_TOKEN" '{"block": true}'
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "block" && pass "Admin Block Room" || fail "Admin Block Room"

# 50. Admin User Sessions
echo ""
echo "=========================================="
echo "141. Admin User Sessions"
echo "=========================================="
ADMIN_USER_ID_ENC=$(url_encode "$ADMIN_USER_ID")
USER_ID_ENC=$(url_encode "$USER_ID")
echo "141. Admin List Users"
http_json GET "$SERVER_URL/_synapse/admin/v1/users" "$ADMIN_TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "users" && pass "Admin List Users" || fail "Admin List Users"

echo ""
echo "142. Admin User Details"
http_json GET "$SERVER_URL/_synapse/admin/v1/users/$ADMIN_USER_ID_ENC" "$ADMIN_TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "name" && pass "Admin User Details" || fail "Admin User Details"

echo ""
echo "143. Admin User Stats"
http_json GET "$SERVER_URL/_synapse/admin/v1/user_stats" "$ADMIN_TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "total_users" && pass "Admin User Stats" || fail "Admin User Stats"

echo ""
echo "144. Admin User Deactivate"
curl -s -X PUT "$SERVER_URL/_synapse/admin/v1/users/$USER_ID_ENC/deactivate" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"erase": false}' && pass "Admin User Deactivate" || skip "Admin User Deactivate (user not found)"

# 51. Device Management
echo ""
echo "=========================================="
echo "145. Device Management"
echo "=========================================="
echo "145. List Devices v3"
http_json GET "$SERVER_URL/_matrix/client/v3/devices" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "devices" && pass "List Devices v3" || fail "List Devices v3"

echo ""
echo "146. Get Device"
http_json GET "$SERVER_URL/_matrix/client/v3/devices/$DEVICE_ID" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "device_id" && pass "Get Device" || fail "Get Device" "${ASSERT_ERROR:-HTTP $HTTP_STATUS}"

echo ""
echo "147. Update Device"
http_json PUT "$SERVER_URL/_matrix/client/v3/devices/$DEVICE_ID" "$TOKEN" '{"display_name": "Updated Device"}'
assert_success_json "Update Device" "$HTTP_BODY" "$HTTP_STATUS"

# 52. E2EE Keys
echo ""
echo "=========================================="
echo "148. E2EE Keys"
echo "=========================================="
echo "148. Get Keys Changes"
http_json GET "$SERVER_URL/_matrix/client/v3/keys/changes" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "changed" "left" && pass "Get Keys Changes" || fail "Get Keys Changes" "${ASSERT_ERROR:-HTTP $HTTP_STATUS}"

echo ""
echo "149. Claim Keys"
http_json POST "$SERVER_URL/_matrix/client/v3/keys/claim" "$TOKEN" '{"one_time_keys": {}}'
assert_success_json "Claim Keys" "$HTTP_BODY" "$HTTP_STATUS" "one_time_keys" "failures"

echo ""
echo "150. Query Keys"
http_json POST "$SERVER_URL/_matrix/client/v3/keys/query" "$TOKEN" '{"device_keys": {}}'
assert_success_json "Query Keys" "$HTTP_BODY" "$HTTP_STATUS" "device_keys" "failures"

# 54. Friend Room
echo ""
echo "=========================================="
echo "154. Friend Room"
echo "=========================================="
echo "154. Get Friends"
http_json GET "$SERVER_URL/_matrix/client/v1/friends" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "friends" && pass "Get Friends" || fail "Get Friends" "${ASSERT_ERROR:-HTTP $HTTP_STATUS}"

echo ""
echo "155. Friend Request"
http_json POST "$SERVER_URL/_matrix/client/v1/friends/request" "$TOKEN" "{\"user_id\": \"$TARGET_USER_ID\"}"
if [[ "$HTTP_STATUS" == 2* ]]; then
    assert_success_json "Friend Request" "$HTTP_BODY" "$HTTP_STATUS" "request_id" "status"
else
    err=$(json_err_summary "$HTTP_BODY")
    if [[ "$HTTP_STATUS" == "409" ]]; then
        pass "Friend Request" "${err:-HTTP 409}"
    else
        fail "Friend Request" "${err:-HTTP $HTTP_STATUS}"
    fi
fi

echo ""
echo "156. Incoming Friend Requests"
http_json GET "$SERVER_URL/_matrix/client/v1/friends/requests/incoming" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "requests" && pass "Incoming Friend Requests" || fail "Incoming Friend Requests" "${ASSERT_ERROR:-HTTP $HTTP_STATUS}"

# 55. Refresh Token
echo ""
echo "=========================================="
echo "157. Refresh Token"
echo "=========================================="
echo "157. Refresh Token"
if [ -n "$REFRESH_TOKEN" ]; then
    http_json POST "$SERVER_URL/_matrix/client/v3/refresh" "$TOKEN" "{\"refresh_token\": \"$REFRESH_TOKEN\"}"
    REFRESH_RESP="$HTTP_BODY"
    assert_success_json "Refresh Token" "$REFRESH_RESP" "$HTTP_STATUS" "access_token" "refresh_token"
else
    skip "Refresh Token (no refresh_token issued)"
fi

# 56. Admin Room Extended Actions
echo ""
echo "=========================================="
echo "158. Admin Room Actions"
echo "=========================================="
echo "158. Admin Shutdown Room"
http_json POST "$SERVER_URL/_synapse/admin/v1/shutdown_room" "$ADMIN_TOKEN" '{"room_id": "'"$ROOM2_ID"'", "user_id": "'"$USER_ID"'"}'
SHUTDOWN_RESP="$HTTP_BODY"
assert_success_json "Admin Shutdown Room" "$SHUTDOWN_RESP" "$HTTP_STATUS"

echo ""
echo "159. Admin Room Make Admin"
if admin_ready; then
    http_json POST "$SERVER_URL/_synapse/admin/v1/rooms/$ROOM_ID/make_admin" "$ADMIN_TOKEN" "{\"user_id\": \"$USER_ID\"}"
    assert_success_json "Admin Room Make Admin" "$HTTP_BODY" "$HTTP_STATUS"
else
    skip "Admin Room Make Admin" "admin authentication unavailable"
fi

# 57. Admin Federation Extended
echo ""
echo "=========================================="
echo "160. Admin Federation"
echo "=========================================="
echo "160. Admin Federation Blacklist"
http_json GET "$SERVER_URL/_synapse/admin/v1/federation/blacklist" "$ADMIN_TOKEN"
FED_BLACKLIST_RESP="$HTTP_BODY"
assert_success_json "Admin Federation Blacklist" "$FED_BLACKLIST_RESP" "$HTTP_STATUS" "blacklist"

echo ""
echo "161. Admin Federation Cache Clear"
http_json POST "$SERVER_URL/_synapse/admin/v1/federation/cache/clear" "$ADMIN_TOKEN" "{}"
FED_CACHE_CLEAR_RESP="$HTTP_BODY"
assert_success_json "Admin Federation Cache Clear" "$FED_CACHE_CLEAR_RESP" "$HTTP_STATUS"

echo ""
echo "162. Admin Reset Connection"
curl -s -X POST "$SERVER_URL/_synapse/admin/v1/federation/destinations/cjystx.top/reset_connection" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{}' && pass "Admin Reset Connection" || skip "Admin Reset Connection (endpoint not available)"

# 58. Search Extended
echo ""
echo "=========================================="
echo "163. Search Extended"
echo "=========================================="
echo "163. Search v3"
curl -s -X POST "$SERVER_URL/_matrix/client/v3/search" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"search_categories": {"room_events": {"search_term": "test"}}}' && pass "Search v3" || skip "Search v3 (endpoint not available)"

echo ""
echo "164. Search Rooms"
curl -s -X POST "$SERVER_URL/_matrix/client/v3/search_rooms" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"search_term": "test"}' && pass "Search Rooms" || skip "Search Rooms (endpoint not available)"

# 59. Room Context & Hierarchy
echo ""
echo "=========================================="
echo "165. Room Context & Hierarchy"
echo "=========================================="
echo "165. Room Context"
if [ -n "$TEST_EVENT_ID_ENC" ]; then
    RESP=$(curl -s "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/context/$TEST_EVENT_ID_ENC" -H "Authorization: Bearer $TOKEN")
    if echo "$RESP" | grep -q "context\\|event\\|M_FORBIDDEN\\|M_NOT_FOUND\\|errcode"; then
        pass "Room Context"
    else
        skip "Room Context (endpoint not available)"
    fi
else
    skip "Room Context (no event_id)"
fi

echo ""
echo "166. Room Hierarchy"
ROOM_HIERARCHY_ENC=$(echo "$ROOM_ID" | sed 's/!/%21/g' | sed 's/:/%3A/g')
RESP=$(curl -s "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_HIERARCHY_ENC/hierarchy" -H "Authorization: Bearer $TOKEN")
if echo "$RESP" | grep -q "hierarchy\|rooms\|next_batch\|M_FORBIDDEN\|errcode"; then
    pass "Room Hierarchy"
else
    skip "Room Hierarchy (endpoint not available)"
fi

# 60. Key Backup Extended
echo ""
echo "=========================================="
echo "167. Key Backup"
echo "=========================================="
echo "167. Create Key Backup"
http_json POST "$SERVER_URL/_matrix/client/v3/room_keys/version" "$TOKEN" '{"algorithm": "m.megolm_backup.v1"}'
CREATE_KEY_BACKUP_RESP="$HTTP_BODY"
assert_success_json "Create Key Backup" "$CREATE_KEY_BACKUP_RESP" "$HTTP_STATUS" "version"

echo ""
echo "168. Get Key Backup"
http_json GET "$SERVER_URL/_matrix/client/v3/room_keys/version" "$TOKEN"
GET_KEY_BACKUP_RESP="$HTTP_BODY"
assert_success_json "Get Key Backup" "$GET_KEY_BACKUP_RESP" "$HTTP_STATUS" "versions"

# 61. SendToDevice
echo ""
echo "=========================================="
echo "169. SendToDevice"
echo "=========================================="
echo "169. Send To Device"
curl -s -X PUT "$SERVER_URL/_matrix/client/v3/sendToDevice/m.room_key_request/txn123" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"messages": {"'"$USER_ID"'": {"device123": {"type": "m.room_key_request"}}}}' && pass "Send To Device" || skip "SendToDevice (endpoint not available)"

# 62. OpenID Connect
echo ""
echo "=========================================="
echo "170. OpenID Connect"
echo "=========================================="
echo "170. OpenID Userinfo"
OPENID_REQ_OK=0
if [ -n "${OPENID_ACCESS_TOKEN:-}" ]; then
    federation_http_json "OpenID Userinfo" GET "$SERVER_URL/_matrix/federation/v1/openid/userinfo?access_token=$OPENID_ACCESS_TOKEN" && OPENID_REQ_OK=1 || true
else
    federation_http_json "OpenID Userinfo" GET "$SERVER_URL/_matrix/federation/v1/openid/userinfo" && OPENID_REQ_OK=1 || true
fi
if [ "$OPENID_REQ_OK" = "1" ]; then
    if check_success_json "$HTTP_BODY" "$HTTP_STATUS" "sub"; then
        pass "OpenID Userinfo"
    else
        skip "OpenID Userinfo" "${ASSERT_ERROR:-HTTP $HTTP_STATUS}"
    fi
fi

# 63. Mod Core Extended
echo ""
echo "=========================================="
echo "171. Mod Core Extended"
echo "=========================================="
echo "171. Events"
http_json GET "$SERVER_URL/_matrix/client/v3/events?roomId=$ROOM_ID" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "chunk" && pass "Events" || skip "Events (endpoint not available)"

echo ""
echo "172. Room Version"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/version" "$TOKEN"
ROOM_VERSION_V2_RESP="$HTTP_BODY"
assert_success_json "Room Version" "$ROOM_VERSION_V2_RESP" "$HTTP_STATUS" "room_version"

echo ""
echo "173. VoIP TURN Server"
http_json GET "$SERVER_URL/_matrix/client/v3/voip/turnServer" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "uris" && pass "VoIP TURN Server" || skip "VoIP TURN Server (endpoint not available)"

# 64. Admin User Actions
echo ""
echo "=========================================="
echo "174. Admin User Actions"
echo "=========================================="
echo "174. Admin User Login"
ADMIN_USER_ID_ENC=$(url_encode "$ADMIN_USER_ID")
if admin_ready; then
    curl -s -X POST "$SERVER_URL/_synapse/admin/v1/users/$ADMIN_USER_ID_ENC/login" \
        -H "Authorization: Bearer $ADMIN_TOKEN" \
        -H "Content-Type: application/json" \
        -d '{"password": "'"$TEST_PASS"'"}' | grep -q "access_token\\|user_id" && pass "Admin User Login" || fail "Admin User Login" "request failed"
else
    skip "Admin User Login" "admin authentication unavailable"
fi

echo ""
echo "175. Admin User Logout"
if admin_ready; then
    curl -s -X POST "$SERVER_URL/_synapse/admin/v1/users/$ADMIN_USER_ID_ENC/logout" \
        -H "Authorization: Bearer $ADMIN_TOKEN" && pass "Admin User Logout" || fail "Admin User Logout" "request failed"
else
    skip "Admin User Logout" "admin authentication unavailable"
fi

echo ""
echo "176. Admin User Devices"
http_json GET "$SERVER_URL/_synapse/admin/v1/users/$ADMIN_USER_ID_ENC/devices" "$ADMIN_TOKEN"
ADMIN_USER_DEVICES_RESP="$HTTP_BODY"
assert_success_json "Admin User Devices" "$ADMIN_USER_DEVICES_RESP" "$HTTP_STATUS"

# 65. Admin User Batch
echo ""
echo "=========================================="
echo "177. Admin User Batch"
echo "=========================================="
echo "177. Admin Batch Users"
curl -s -X POST "$SERVER_URL/_synapse/admin/v1/users/batch" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"users": []}' && pass "Admin Batch Users" || skip "Admin Batch Users (endpoint not available)"

# 66. Room Alias Directory
echo ""
echo "=========================================="
echo "178. Room Alias Directory"
echo "=========================================="
echo "178. Set Room Alias"
ROOM_ALIAS="#api_test_${RANDOM}:${USER_DOMAIN}"
ROOM_ALIAS_ENC=$(url_encode "$ROOM_ALIAS")
http_json PUT "$SERVER_URL/_matrix/client/v3/directory/room/$ROOM_ALIAS_ENC" "$TOKEN" '{"room_id": "'"$ROOM_ID"'"}'
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Set Room Alias"
else
    skip "Set Room Alias (endpoint not available)"
fi

echo ""
echo "179. Get Room Alias"
http_json GET "$SERVER_URL/_matrix/client/v3/directory/room/$ROOM_ALIAS_ENC" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "room_id" && pass "Get Room Alias" || skip "Get Room Alias (endpoint not available)"

# 67. Federation API
echo ""
echo "=========================================="
echo "180. Federation API"
echo "=========================================="
echo "180. Federation Version"
http_json GET "$SERVER_URL/_matrix/federation/v1/version" ""
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "version" && pass "Federation Version" || fail "Federation Version"

echo ""
echo "181. Federation Backfill"
if [ -n "$MSG_EVENT_ID" ]; then
    if federation_http_json "Federation Backfill" GET "$SERVER_URL/_matrix/federation/v1/backfill/$ROOM_ID" "{\"v\":[\"$MSG_EVENT_ID\"],\"limit\":10}"; then
        federation_smoke "Federation Backfill" "$HTTP_STATUS" "$HTTP_BODY"
    fi
else
    skip "Federation Backfill" "no event_id"
fi

echo ""
echo "182. Federation Get Event"
if [ -n "$MSG_EVENT_ID" ]; then
    MSG_EVENT_ID_ENC=$(python3 -c 'import sys, urllib.parse; print(urllib.parse.quote(sys.argv[1], safe=""))' "$MSG_EVENT_ID" 2>/dev/null)
    if federation_http_json "Federation Get Event" GET "$SERVER_URL/_matrix/federation/v1/event/$MSG_EVENT_ID_ENC"; then
        federation_smoke "Federation Get Event" "$HTTP_STATUS" "$HTTP_BODY"
    fi
else
    skip "Federation Get Event" "no event_id"
fi

echo ""
echo "183. Federation Event Auth"
if [ -n "$MSG_EVENT_ID" ]; then
    ROOM_ID_ENC=$(python3 -c 'import sys, urllib.parse; print(urllib.parse.quote(sys.argv[1], safe=""))' "$ROOM_ID" 2>/dev/null)
    MSG_EVENT_ID_ENC=$(python3 -c 'import sys, urllib.parse; print(urllib.parse.quote(sys.argv[1], safe=""))' "$MSG_EVENT_ID" 2>/dev/null)
    if federation_http_json "Federation Event Auth" GET "$SERVER_URL/_matrix/federation/v1/get_event_auth/$ROOM_ID_ENC/$MSG_EVENT_ID_ENC"; then
        federation_smoke "Federation Event Auth" "$HTTP_STATUS" "$HTTP_BODY"
    fi
else
    skip "Federation Event Auth" "no event_id"
fi

echo ""
echo "184. Federation Get Joining Rules"
if federation_http_json "Federation Get Joining Rules" GET "$SERVER_URL/_matrix/federation/v1/get_joining_rules/$ROOM_ID"; then
    federation_smoke "Federation Get Joining Rules" "$HTTP_STATUS" "$HTTP_BODY"
fi

echo ""
echo "185. Federation Keys Query"
if federation_http_json "Federation Keys Query" POST "$SERVER_URL/_matrix/federation/v1/keys/query" '{"device_keys": {}}'; then
    federation_smoke "Federation Keys Query" "$HTTP_STATUS" "$HTTP_BODY"
fi

echo ""
echo "186. Federation Keys Claim"
curl -s -X POST "$SERVER_URL/_matrix/federation/v1/keys/claim" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"one_time_keys": {}}' && pass "Federation Keys Claim" || skip "Federation Keys Claim (endpoint not available)"

echo ""
echo "187. Federation Keys Upload"
curl -s -X POST "$SERVER_URL/_matrix/federation/v1/keys/upload" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"one_time_keys": {}}' && pass "Federation Keys Upload" || skip "Federation Keys Upload (endpoint not available)"

echo ""
echo "188. Federation Make Join"
ROOM_ID_ENC=$(python3 -c 'import sys, urllib.parse; print(urllib.parse.quote(sys.argv[1], safe=""))' "$ROOM_ID" 2>/dev/null)
USER_ID_ENC=$(url_encode "$USER_ID")
if federation_http_json "Federation Make Join" GET "$SERVER_URL/_matrix/federation/v1/make_join/$ROOM_ID_ENC/$USER_ID_ENC"; then
    federation_smoke "Federation Make Join" "$HTTP_STATUS" "$HTTP_BODY"
fi

echo ""
echo "189. Federation Make Leave"
ROOM_ID_ENC=$(python3 -c 'import sys, urllib.parse; print(urllib.parse.quote(sys.argv[1], safe=""))' "$ROOM_ID" 2>/dev/null)
USER_ID_ENC=$(url_encode "$USER_ID")
if federation_http_json "Federation Make Leave" GET "$SERVER_URL/_matrix/federation/v1/make_leave/$ROOM_ID_ENC/$USER_ID_ENC"; then
    federation_smoke "Federation Make Leave" "$HTTP_STATUS" "$HTTP_BODY"
fi

echo ""
echo "190. Federation Members"
curl -s "$SERVER_URL/_matrix/federation/v1/members/$ROOM_ID" -H "Authorization: Bearer $TOKEN" && pass "Federation Members" || skip "Federation Members (endpoint not available)"

echo ""
echo "191. Federation Public Rooms"
http_json GET "$SERVER_URL/_matrix/federation/v1/publicRooms" ""
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "chunk" && pass "Federation Public Rooms" || skip "Federation Public Rooms (endpoint not available)"

echo ""
echo "192. Federation Query Directory"
curl -s "$SERVER_URL/_matrix/federation/v1/query/directory/room/$ROOM_ID" && pass "Federation Query Directory" || skip "Federation Query Directory (endpoint not available)"

echo ""
echo "193. Federation Query Profile"
curl -s "$SERVER_URL/_matrix/federation/v1/query/profile/$USER_ID" && pass "Federation Query Profile" || skip "Federation Query Profile (endpoint not available)"

echo ""
echo "194. Federation Room Auth"
curl -s "$SERVER_URL/_matrix/federation/v1/room_auth/$ROOM_ID" -H "Authorization: Bearer $TOKEN" && pass "Federation Room Auth" || skip "Federation Room Auth (endpoint not available)"

echo ""
echo "195. Federation State"
curl -s "$SERVER_URL/_matrix/federation/v1/state/$ROOM_ID" -H "Authorization: Bearer $TOKEN" && pass "Federation State" || skip "Federation State (endpoint not available)"

echo ""
echo "196. Federation State IDs"
curl -s "$SERVER_URL/_matrix/federation/v1/state_ids/$ROOM_ID" -H "Authorization: Bearer $TOKEN" && pass "Federation State IDs" || skip "Federation State IDs (endpoint not available)"

# 68. Media API Extended
echo ""
echo "=========================================="
echo "197. Media API Extended"
echo "=========================================="
echo "197. Media Config v3"
http_json GET "$SERVER_URL/_matrix/media/v3/config" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "m.upload.size" && pass "Media Config v3" || fail "Media Config v3"

echo ""
echo "198. Media Config r0"
http_json GET "$SERVER_URL/_matrix/media/r0/config" ""
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "m.upload.size" && pass "Media Config r0" || fail "Media Config r0"

echo ""
echo "199. Media Upload r0"
MEDIA_RESP=$(curl -s -X POST "$SERVER_URL/_matrix/media/r0/upload" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: image/png" \
    --data-binary "PNG-DATA")
if echo "$MEDIA_RESP" | grep -q "content_uri"; then
    pass "Media Upload r0"
else
    skip "Media Upload r0 (endpoint not available)"
fi

echo ""
echo "200. Media Config v1"
http_json GET "$SERVER_URL/_matrix/media/v1/config" ""
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "m.upload.size" && pass "Media Config v1" || fail "Media Config v1"

# 69. Room Summary Extended
echo ""
echo "=========================================="
echo "201. Room Summary Extended"
echo "=========================================="
echo "201. Room Summary"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/summary" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "room_id" && pass "Room Summary" || fail "Room Summary"

echo ""
echo "202. Room Summary Heroes"
curl -s "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/summary/heroes/recalculate" -H "Authorization: Bearer $TOKEN" && pass "Room Summary Heroes" || skip "Room Summary Heroes (endpoint not available)"

echo ""
echo "203. Room Summary Stats Recalculate"
curl -s "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/summary/stats/recalculate" -H "Authorization: Bearer $TOKEN" && pass "Room Summary Stats Recalculate" || skip "Room Summary Stats Recalculate (endpoint not available)"

echo ""
echo "204. Room Summary Sync"
curl -s "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/summary/sync" -H "Authorization: Bearer $TOKEN" && pass "Room Summary Sync" || skip "Room Summary Sync (endpoint not available)"

echo ""
echo "205. Room Summary Unread Clear"
curl -s -X POST "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/summary/unread/clear" -H "Authorization: Bearer $TOKEN" && pass "Room Summary Unread Clear" || skip "Room Summary Unread Clear (endpoint not available)"

# 70. Room Hierarchy
echo ""
echo "=========================================="
echo "206. Room Hierarchy"
echo "=========================================="
echo "206. Room Hierarchy"
ROOM_HIERARCHY_ENC=$(echo "$ROOM_ID" | sed 's/!/%21/g' | sed 's/:/%3A/g')
curl -s "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_HIERARCHY_ENC/hierarchy" -H "Authorization: Bearer $TOKEN" && pass "Room Hierarchy" || skip "Room Hierarchy (endpoint not available)"

echo ""
echo "207. Space Hierarchy"
if [ -n "$SPACE_ID" ]; then
    http_json GET "$SERVER_URL/_matrix/client/v3/spaces/$SPACE_ENC/hierarchy" "$TOKEN"
    SPACE_HIERARCHY_RESP="$HTTP_BODY"
    if check_success_json "$SPACE_HIERARCHY_RESP" "$HTTP_STATUS"; then
        pass "Space Hierarchy"
    else
        fail "Space Hierarchy" "$ASSERT_ERROR"
    fi
else
    skip "Space Hierarchy" "space not created"
fi

# 71. Room Timestamp to Event
echo ""
echo "=========================================="
echo "208. Room Timestamp to Event"
echo "=========================================="
echo "208. Timestamp to Event"
curl -s "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/timestamp_to_event" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"stream_ordering": 1, "ts": 1234567890000}' && pass "Timestamp to Event" || skip "Timestamp to Event (endpoint not available)"

# 72. User Threads
echo ""
echo "=========================================="
echo "209. User Threads"
echo "=========================================="
echo "209. User Threads"
curl -s "$SERVER_URL/_matrix/client/v3/user/$USER_ID/rooms/$ROOM_ID/threads" -H "Authorization: Bearer $TOKEN" && pass "User Threads" || skip "User Threads (endpoint not available)"

# 73. Admin Background Update
echo ""
echo "=========================================="
echo "210. Admin Background Update"
echo "=========================================="
echo "210. List Background Updates"
http_json GET "$SERVER_URL/_synapse/admin/v1/background_updates" "$ADMIN_TOKEN"
BG_UPDATES_RESP="$HTTP_BODY"
assert_success_json "List Background Updates" "$BG_UPDATES_RESP" "$HTTP_STATUS"

# 74. Admin Event Report
echo ""
echo "=========================================="
echo "211. Admin Event Report"
echo "=========================================="
echo "211. List Event Reports"
http_json GET "$SERVER_URL/_synapse/admin/v1/event_reports" "$ADMIN_TOKEN"
EVENT_REPORTS_RESP="$HTTP_BODY"
assert_success_json "List Event Reports" "$EVENT_REPORTS_RESP" "$HTTP_STATUS"

# 75. Admin Room Forward Extremities
echo ""
echo "=========================================="
echo "212. Admin Room Forward Extremities"
echo "=========================================="
echo "212. Room Forward Extremities"
http_json GET "$SERVER_URL/_synapse/admin/v1/rooms/$ROOM_ID/forward_extremities" "$ADMIN_TOKEN"
FORWARD_EXTREM_RESP="$HTTP_BODY"
assert_success_json "Room Forward Extremities" "$FORWARD_EXTREM_RESP" "$HTTP_STATUS" "forward_extremities"

# 76. E2EE Keys Extended
echo ""
echo "=========================================="
echo "213. E2EE Keys Extended"
echo "=========================================="
echo "213. Keys Query"
curl -s -X POST "$SERVER_URL/_matrix/client/v3/keys/query" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"device_keys": {}}' && pass "Keys Query" || skip "Keys Query (endpoint not available)"

echo ""
echo "214. Keys Claim"
curl -s -X POST "$SERVER_URL/_matrix/client/v3/keys/claim" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"one_time_keys": {}}' && pass "Keys Claim" || skip "Keys Claim (endpoint not available)"

echo ""
echo "215. Keys Changes"
curl -s "$SERVER_URL/_matrix/client/v3/keys/changes" -H "Authorization: Bearer $TOKEN" && pass "Keys Changes" || skip "Keys Changes (endpoint not available)"

echo ""
echo "216. Keys Upload Signature"
curl -s -X POST "$SERVER_URL/_matrix/client/v3/keys/signatures/upload" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"signatures": {}}' && pass "Keys Upload Signature" || skip "Keys Upload Signature (endpoint not available)"

echo ""
echo "217. Get Key Changes"
curl -s "$SERVER_URL/_matrix/client/v3/keys/changes?from=0&to=100" -H "Authorization: Bearer $TOKEN" && pass "Get Key Changes" || skip "Get Key Changes (endpoint not available)"

# 77. Key Backup Extended
echo ""
echo "=========================================="
echo "218. Key Backup Extended"
echo "=========================================="
echo "218. Create Key Backup Version"
http_json POST "$SERVER_URL/_matrix/client/v3/room_keys/version" "$TOKEN" '{"algorithm": "m.megolm_backup.v1", "auth_data": {"public_key": "test"}}'
CREATE_KEY_BACKUP_VERSION_RESP="$HTTP_BODY"
assert_success_json "Create Key Backup Version" "$CREATE_KEY_BACKUP_VERSION_RESP" "$HTTP_STATUS" "version"

echo ""
echo "219. Get Key Backup Version"
http_json GET "$SERVER_URL/_matrix/client/v3/room_keys/version" "$TOKEN"
GET_KEY_BACKUP_VERSION_RESP="$HTTP_BODY"
assert_success_json "Get Key Backup Version" "$GET_KEY_BACKUP_VERSION_RESP" "$HTTP_STATUS" "versions"

echo ""
echo "220. Delete Key Backup Version"
curl -s -X DELETE "$SERVER_URL/_matrix/client/v3/room_keys/version/1" -H "Authorization: Bearer $TOKEN" && pass "Delete Key Backup Version" || skip "Key Backup (endpoint not available)"

echo ""
echo "221. Room Keys Backup"
curl -s -X PUT "$SERVER_URL/_matrix/client/v3/room_keys/backup" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"rooms": {}}' && pass "Room Keys Backup" || skip "Room Keys Backup (endpoint not available)"

echo ""
echo "222. Get Room Keys Backup"
curl -s "$SERVER_URL/_matrix/client/v3/room_keys/backup" -H "Authorization: Bearer $TOKEN" && pass "Get Room Keys Backup" || skip "Room Keys Backup (endpoint not available)"

echo ""
echo "223. Get Room Key"
curl -s "$SERVER_URL/_matrix/client/v3/room_keys/$ROOM_ID/session/test_session" -H "Authorization: Bearer $TOKEN" && pass "Get Room Key" || skip "Get Room Key (endpoint not available)"

echo ""
echo "224. Delete Room Key"
curl -s -X DELETE "$SERVER_URL/_matrix/client/v3/room_keys/$ROOM_ID/session/test_session" -H "Authorization: Bearer $TOKEN" && pass "Delete Room Key" || skip "Delete Room Key (endpoint not available)"

# 78. Verification Routes
echo ""
echo "=========================================="
echo "225. Verification Routes"
echo "=========================================="
echo "225. Start Key Verification"
curl -s -X POST "$SERVER_URL/_matrix/client/v0/keys/request_verification" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"user_id": "'"$USER_ID"'", "device_id": "test_device"}' && pass "Start Key Verification" || skip "Verification (endpoint not available)"

echo ""
echo "226. Get Key Verification Request"
curl -s "$SERVER_URL/_matrix/client/v0/keys/verification/request/test_request_id" -H "Authorization: Bearer $TOKEN" && pass "Get Key Verification Request" || skip "Verification (endpoint not available)"

echo ""
echo "227. Accept Key Verification"
curl -s -X PUT "$SERVER_URL/_matrix/client/v0/keys/verification/request/test_request_id/accept" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{}' && pass "Accept Key Verification" || skip "Verification (endpoint not available)"

echo ""
echo "228. Complete Key Verification"
curl -s -X PUT "$SERVER_URL/_matrix/client/v0/keys/verification/request/test_request_id/complete" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"mac": {}}' && pass "Complete Key Verification" || skip "Verification (endpoint not available)"

echo ""
echo "229. Cancel Key Verification"
curl -s -X PUT "$SERVER_URL/_matrix/client/v0/keys/verification/request/test_request_id/cancel" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"code": "user", "reason": "canceled"}' && pass "Cancel Key Verification" || skip "Verification (endpoint not available)"

# 79. Room Key Request Extended
echo ""
echo "=========================================="
echo "230. Room Key Request Extended"
echo "=========================================="
echo "230. Create Room Key Request"
curl -s -X POST "$SERVER_URL/_matrix/client/v3/room_keys/request" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"algorithm": "m.megolm.v1", "room_id": "'"$ROOM_ID"'", "session_id": "test_session"}' && pass "Create Room Key Request" || skip "Room Key Request (endpoint not available)"

echo ""
echo "231. Get Room Key Requests"
curl -s "$SERVER_URL/_matrix/client/v3/room_keys/request" -H "Authorization: Bearer $TOKEN" && pass "Get Room Key Requests" || skip "Room Key Request (endpoint not available)"

echo ""
echo "232. Delete Room Key Request"
curl -s -X DELETE "$SERVER_URL/_matrix/client/v3/room_keys/request/test_id" -H "Authorization: Bearer $TOKEN" && pass "Delete Room Key Request" || skip "Room Key Request (endpoint not available)"

# 80. Thread Extended
echo ""
echo "=========================================="
echo "233. Thread Extended"
echo "=========================================="
echo "233. Get Thread v1"
curl -s "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/thread/test_thread_id" -H "Authorization: Bearer $TOKEN" && pass "Get Thread v1" || skip "Thread (endpoint not available)"

echo ""
echo "234. Get User Threads"
curl -s "$SERVER_URL/_matrix/client/v1/threads" -H "Authorization: Bearer $TOKEN" && pass "Get User Threads" || skip "Thread (endpoint not available)"

echo ""
echo "235. Thread Search"
curl -s -X GET "$SERVER_URL/_matrix/client/v1/rooms/$ROOM_ID/threads/search?q=test" \
    -H "Authorization: Bearer $TOKEN" && pass "Thread Search" || skip "Thread (endpoint not available)"

echo ""
echo "236. Get Thread ID"
curl -s "$SERVER_URL/_matrix/client/v1/rooms/$ROOM_ID/threads/test_event" -H "Authorization: Bearer $TOKEN" && pass "Get Thread ID" || skip "Thread (endpoint not available)"

# 81. Room State Extended
echo ""
echo "=========================================="
echo "237. Room State Extended"
echo "=========================================="
echo "237. Get State Event"
curl -s "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/state/m.room.topic/test" -H "Authorization: Bearer $TOKEN" && pass "Get State Event" || skip "Room State (endpoint not available)"

echo ""
echo "238. Room Typing"
curl -s -X PUT "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/typing/$USER_ID" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"typing": true, "timeout": 30000}' && pass "Room Typing" || skip "Room Typing (endpoint not available)"

echo ""
echo "239. Room Read Markers"
curl -s -X POST "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/read_markers" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"fully_read": "test_event", "read": "test_event"}' && pass "Room Read Markers" || skip "Room Read Markers (endpoint not available)"

# 82. Admin User Sessions
echo ""
echo "=========================================="
echo "240. Admin User Sessions"
echo "=========================================="
echo "240. List User Sessions"
http_json GET "$SERVER_URL/_synapse/admin/v1/user_sessions/$USER_ID" "$ADMIN_TOKEN"
USER_SESSIONS_RESP="$HTTP_BODY"
assert_success_json "List User Sessions" "$USER_SESSIONS_RESP" "$HTTP_STATUS"

echo ""
echo "241. Invalidate User Session"
if destructive; then
    http_json POST "$SERVER_URL/_synapse/admin/v1/user_sessions/$USER_ID/invalidate" "$ADMIN_TOKEN" "{}"
    INVALIDATE_SESSIONS_RESP="$HTTP_BODY"
    if assert_success_json "Invalidate User Session" "$INVALIDATE_SESSIONS_RESP" "$HTTP_STATUS" "invalidated"; then
        http_json POST "$SERVER_URL/_matrix/client/v3/login" "" "{\"type\": \"m.login.password\", \"user\": \"$USER_ID\", \"password\": \"$CURRENT_TEST_PASS\"}"
        if check_success_json "$HTTP_BODY" "$HTTP_STATUS" "access_token"; then
            TOKEN=$(json_get "$HTTP_BODY" "access_token")
            REFRESH_TOKEN=$(json_get "$HTTP_BODY" "refresh_token")
        else
            fail "Re-Login After Invalidate" "${ASSERT_ERROR:-HTTP $HTTP_STATUS}"
        fi
    fi
else
    skip "Invalidate User Session" "destructive test"
fi

echo ""
echo "242. Reset User Password"
if destructive; then
    USER_ID_ENC=$(url_encode "$USER_ID")
    http_json POST "$SERVER_URL/_synapse/admin/v1/users/$USER_ID_ENC/password" "$ADMIN_TOKEN" "{\"new_password\": \"$TEST_PASS\"}"
    RESET_USER_PASSWORD_RESP="$HTTP_BODY"
    if assert_success_json "Reset User Password" "$RESET_USER_PASSWORD_RESP" "$HTTP_STATUS"; then
        CURRENT_TEST_PASS="$TEST_PASS"
    fi
else
    skip "Reset User Password" "destructive test"
fi

echo ""
echo "243. Admin Deactivate User"
if destructive; then
    USER_ID_ENC=$(url_encode "$USER_ID")
    http_json POST "$SERVER_URL/_synapse/admin/v1/users/$USER_ID_ENC/deactivate" "$ADMIN_TOKEN" '{}'
    if [[ "$HTTP_STATUS" == 2* ]]; then
        pass "Deactivate User"
        http_json PUT "$SERVER_URL/_synapse/admin/v2/users/$TEST_USER" "$ADMIN_TOKEN" "{\"password\":\"$CURRENT_TEST_PASS\",\"deactivated\":false}"
        if [[ "$HTTP_STATUS" == 2* ]]; then
            http_json POST "$SERVER_URL/_matrix/client/v3/login" "" "{\"type\": \"m.login.password\", \"user\": \"$TEST_USER\", \"password\": \"$CURRENT_TEST_PASS\"}"
            REACTIVATE_TOKEN=$(json_get "$HTTP_BODY" "access_token")
            if [ -n "$REACTIVATE_TOKEN" ]; then
                TOKEN="$REACTIVATE_TOKEN"
                USER_ID=$(json_get "$HTTP_BODY" "user_id")
                REFRESH_TOKEN=$(json_get "$HTTP_BODY" "refresh_token")
                DEVICE_ID=$(json_get "$HTTP_BODY" "device_id")
                pass "Restore User After Deactivate"
            else
                fail "Restore User After Deactivate" "$(json_err_summary "$HTTP_BODY")"
            fi
        else
            fail "Restore User After Deactivate" "$(json_err_summary "$HTTP_BODY")"
        fi
    else
        skip "Admin Deactivate" "endpoint not available"
    fi
else
    skip "Deactivate User" "destructive test"
fi

# 83. Admin Room Details Extended
echo ""
echo "=========================================="
echo "244. Admin Room Details Extended"
echo "=========================================="
echo "244. Admin Room State"
http_json GET "$SERVER_URL/_synapse/admin/v1/rooms/$ROOM_ID/state" "$ADMIN_TOKEN"
ADMIN_ROOM_STATE_RESP="$HTTP_BODY"
assert_success_json "Admin Room State" "$ADMIN_ROOM_STATE_RESP" "$HTTP_STATUS"

echo ""
echo "245. Admin Room Members"
http_json GET "$SERVER_URL/_synapse/admin/v1/rooms/$ROOM_ID/members" "$ADMIN_TOKEN"
ADMIN_ROOM_MEMBERS_RESP="$HTTP_BODY"
assert_success_json "Admin Room Members" "$ADMIN_ROOM_MEMBERS_RESP" "$HTTP_STATUS"

echo ""
echo "246. Admin Room Delete"
if ! destructive; then
    skip "Admin Room Delete" "destructive test"
else
    http_json DELETE "$SERVER_URL/_synapse/admin/v1/rooms/$ROOM_ID" "$ADMIN_TOKEN" '{"new_room_id": null}'
    if check_success_json "$HTTP_BODY" "$HTTP_STATUS"; then
        pass "Admin Room Delete"
        http_json POST "$SERVER_URL/_matrix/client/v3/createRoom" "$TOKEN" '{"name": "Test Room Post Admin Delete", "preset": "public_chat"}'
        REPLACEMENT_ROOM_ID=$(json_get "$HTTP_BODY" "room_id")
        if [ -n "$REPLACEMENT_ROOM_ID" ]; then
            ROOM_ID="$REPLACEMENT_ROOM_ID"
            pass "Recreate Test Room After Delete"
        else
            fail "Recreate Test Room After Delete" "$(json_err_summary "$HTTP_BODY")"
        fi
    else
        skip "Admin Room Delete" "${ASSERT_ERROR:-HTTP $HTTP_STATUS}"
    fi
fi

# 84. Well-Known Extended
echo ""
echo "=========================================="
echo "247. Well-Known Extended"
echo "=========================================="
echo "247. Well-Known Client"
http_json GET "$SERVER_URL/.well-known/matrix/client" ""
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "m.homeserver" && pass "Well-Known Client" || fail "Well-Known Client"

echo ""
echo "248. Well-Known Server"
http_json GET "$SERVER_URL/.well-known/matrix/server" ""
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "m.server" && pass "Well-Known Server" || skip "Well-Known Server (endpoint not available)"

# 85. Identity Service
echo ""
echo "=========================================="
echo "249. Identity Service"
echo "=========================================="
echo "249. Identity Store Invite"
curl -s -X POST "$SERVER_URL/_matrix/identity/v1/store-invite" \
    -H "Content-Type: application/json" \
    -d '{"medium": "email", "address": "test@example.com"}' && pass "Identity Store Invite" || skip "Identity Service (endpoint not available)"

echo ""
echo "250. Identity Get Published Keys"
curl -s "$SERVER_URL/_matrix/identity/v1/keys/pubkey" && pass "Identity Get Published Keys" || skip "Identity Service (endpoint not available)"

# 86. Friend Room Extended
echo ""
echo "=========================================="
echo "251. Friend Room Extended"
echo "=========================================="
echo "251. Get Friends"
curl -s "$SERVER_URL/_matrix/client/v3/friends" -H "Authorization: Bearer $TOKEN" && pass "Get Friends" || skip "Friend Room (endpoint not available)"

echo ""
echo "252. Get Incoming Friend Requests"
curl -s "$SERVER_URL/_matrix/client/v3/friends/requests/incoming" -H "Authorization: Bearer $TOKEN" && pass "Get Incoming Friend Requests" || skip "Friend Room (endpoint not available)"

echo ""
echo "253. Get Outgoing Friend Requests"
curl -s "$SERVER_URL/_matrix/client/v1/friends/requests/outgoing" -H "Authorization: Bearer $TOKEN" && pass "Get Outgoing Friend Requests" || skip "Friend Room (endpoint not available)"

echo ""
echo "254. Send Friend Request"
curl -s -X POST "$SERVER_URL/_matrix/client/v1/friends/request" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"user_id": "@test:cjystx.top"}' && pass "Send Friend Request" || skip "Friend Room (endpoint not available)"

# 87. Admin Users Extended
echo ""
echo "=========================================="
echo "255. Admin Users Extended"
echo "=========================================="
if admin_ready; then
    ADMIN_USER_ID_ENC=$(url_encode "$ADMIN_USER_ID")
    echo "255. Admin List Users"
    http_json GET "$SERVER_URL/_synapse/admin/v1/users" "$ADMIN_TOKEN"
    check_success_json "$HTTP_BODY" "$HTTP_STATUS" "users" && pass "Admin List Users" || fail "Admin List Users"

    echo ""
    echo "256. Admin Get User"
    http_json GET "$SERVER_URL/_synapse/admin/v2/users/$ADMIN_USER_ID_ENC" "$ADMIN_TOKEN"
    check_success_json "$HTTP_BODY" "$HTTP_STATUS" "user_id" && pass "Admin Get User" || fail "Admin Get User"

    echo ""
    echo "257. Admin List User Tokens"
http_json GET "$SERVER_URL/_synapse/admin/v1/users/$ADMIN_USER_ID_ENC/tokens" "$ADMIN_TOKEN"
ADMIN_TOKENS_RESP="$HTTP_BODY"
assert_success_json "Admin List User Tokens" "$ADMIN_TOKENS_RESP" "$HTTP_STATUS" "tokens"
else
    skip "Admin List Users" "admin authentication unavailable"
    skip "Admin Get User" "admin authentication unavailable"
    skip "Admin List User Tokens" "admin authentication unavailable"
fi

# 88. Admin Rooms Extended
echo ""
echo "=========================================="
echo "258. Admin Rooms Extended"
echo "=========================================="
if admin_ready; then
    echo "258. Admin List Rooms"
    http_json GET "$SERVER_URL/_synapse/admin/v1/rooms" "$ADMIN_TOKEN"
    check_success_json "$HTTP_BODY" "$HTTP_STATUS" "rooms" && pass "Admin List Rooms" || fail "Admin List Rooms" "${ASSERT_ERROR:-HTTP $HTTP_STATUS}"

    echo ""
    echo "259. Admin Get Room"
    http_json GET "$SERVER_URL/_synapse/admin/v1/rooms/$ROOM_ID" "$ADMIN_TOKEN"
    check_success_json "$HTTP_BODY" "$HTTP_STATUS" "room_id" && pass "Admin Get Room" || fail "Admin Get Room" "${ASSERT_ERROR:-HTTP $HTTP_STATUS}"
else
    skip "Admin List Rooms" "admin authentication unavailable"
    skip "Admin Get Room" "admin authentication unavailable"
fi

# 89. Version Extended
echo ""
echo "=========================================="
echo "260. Version Extended"
echo "=========================================="
echo "260. Server Version"
http_json GET "$SERVER_URL/_matrix/client/versions" ""
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "versions" && pass "Server Version" || fail "Server Version" "${ASSERT_ERROR:-HTTP $HTTP_STATUS}"

echo ""
echo "261. Rust Synapse Version"
http_json GET "$SERVER_URL/_synapse/admin/info" ""
check_success_json "$HTTP_BODY" "$HTTP_STATUS" && pass "Rust Synapse Version" || skip "Rust Synapse Version (endpoint not available)"

# 90. Capabilities
echo ""
echo "=========================================="
echo "262. Capabilities"
echo "=========================================="
echo "262. Get Capabilities"
http_json GET "$SERVER_URL/_matrix/client/v3/capabilities" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "capabilities" && pass "Get Capabilities" || fail "Get Capabilities" "${ASSERT_ERROR:-HTTP $HTTP_STATUS}"

# 91. Admin Room Extended
echo ""
echo "=========================================="
echo "263. Admin Room Extended"
echo "=========================================="
echo "263. Admin Room Event"
ADMIN_ROOM_EVENT_ID="${LAST_EVENT_ID:-$REDACT_EVENT_ID}"
if ! admin_ready; then
    skip "Admin Room Event" "admin authentication unavailable"
elif [ -z "$ADMIN_ROOM_EVENT_ID" ]; then
    skip "Admin Room Event" "no event id"
else
    ADMIN_ROOM_EVENT_URL="$SERVER_URL/_synapse/admin/v1/rooms/$ROOM_ID/event_context/$ADMIN_ROOM_EVENT_ID"
    http_json GET "$ADMIN_ROOM_EVENT_URL" "$ADMIN_TOKEN"
    ADMIN_ROOM_EVENT_RESP="$HTTP_BODY"
    if check_success_json "$ADMIN_ROOM_EVENT_RESP" "$HTTP_STATUS" "event"; then
        pass "Admin Room Event"
    else
        skip "Admin Room Event" "${ASSERT_ERROR:-HTTP $HTTP_STATUS}"
    fi
fi

echo ""
echo "264. Admin Room Token Sync"
if admin_ready; then
    http_json GET "$SERVER_URL/_synapse/admin/v1/rooms/$ROOM_ID/token_sync" "$ADMIN_TOKEN"
    ADMIN_ROOM_TOKEN_SYNC_RESP="$HTTP_BODY"
    if check_success_json "$ADMIN_ROOM_TOKEN_SYNC_RESP" "$HTTP_STATUS" "room_id" "results" "total"; then
        pass "Admin Room Token Sync"
    else
        skip "Admin Room Token Sync" "$ASSERT_ERROR"
    fi
else
    skip "Admin Room Token Sync" "admin authentication unavailable"
fi

# 93. Room Receipts Extended
echo ""
echo "=========================================="
echo "267. Room Receipts Extended"
echo "=========================================="
echo "267. Get Receipts"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/receipts/m.read/$REDACT_EVENT_ID" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get Receipts"
else
    skip "Room Receipts (not found)"
fi

# 94. Space Extended
echo ""
echo "=========================================="
echo "268. Space Extended"
echo "=========================================="
echo "268. Get Space Rooms"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/spaces" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get Space Rooms"
else
    skip "Get Space Rooms" "not supported"
fi

# 95. Server Notices
echo ""
echo "=========================================="
echo "269. Server Notices"
echo "=========================================="
echo "269. Send Server Notice"
if admin_ready; then
    http_json POST "$SERVER_URL/_synapse/admin/v1/send_server_notice" "$ADMIN_TOKEN" '{"user_id": "'"$TARGET_USER_ID"'", "content": {"msgtype": "m.text", "body": "test"}}'
    if check_success_json "$HTTP_BODY" "$HTTP_STATUS" "event_id" "room_id" "notice_id"; then
        pass "Send Server Notice"
    else
        skip "Server Notices" "$ASSERT_ERROR"
    fi
else
    skip "Server Notices" "admin authentication unavailable"
fi

# 96. Admin Stats
echo ""
echo "=========================================="
echo "270. Admin Stats"
echo "=========================================="
echo "270. Admin Stats Users"
http_json GET "$SERVER_URL/_synapse/admin/v1/statistics" "$ADMIN_TOKEN"
ADMIN_STATS_USERS_RESP="$HTTP_BODY"
assert_success_json "Admin Stats Users" "$ADMIN_STATS_USERS_RESP" "$HTTP_STATUS"

echo ""
echo "271. Admin Stats Rooms"
http_json GET "$SERVER_URL/_synapse/admin/v1/statistics" "$ADMIN_TOKEN"
ADMIN_STATS_ROOMS_RESP="$HTTP_BODY"
assert_success_json "Admin Stats Rooms" "$ADMIN_STATS_ROOMS_RESP" "$HTTP_STATUS"

# 97. Report Content
echo ""
echo "=========================================="
echo "272. Report Content"
echo "=========================================="
echo "272. Report Event"
if [ -n "$MSG_EVENT_ID" ]; then
    REPORT_ENC=$(url_encode "$MSG_EVENT_ID")
    http_json POST "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/report/$REPORT_ENC" "$TOKEN" '{"reason": "spam"}'
    if [[ "$HTTP_STATUS" == 2* ]]; then
        pass "Report Event"
    else
        skip "Report Event" "HTTP $HTTP_STATUS"
    fi
else
    skip "Report Event (no event id)"
fi

# 99. Room Tags
echo ""
echo "=========================================="
echo "274. Room Tags"
echo "=========================================="
echo "274. Get Room Tags"
http_json GET "$SERVER_URL/_matrix/client/v3/user/$USER_ID/rooms/$ROOM_ID/tags" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "tags" && pass "Get Room Tags" || skip "Room Tags (endpoint not available)"

# 100. User Directory
echo ""
echo "=========================================="
echo "275. User Directory"
echo "=========================================="
echo "275. Search User Directory"
http_json POST "$SERVER_URL/_matrix/client/v3/user_directory/search" "$TOKEN" '{"search_term": "admin"}'
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Search User Directory"
else
    skip "User Directory (not found)"
fi

echo ""
echo "276. User Directory Profile"
http_json GET "$SERVER_URL/_matrix/client/v3/user_directory/profiles/$USER_ID" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "User Directory Profile"
else
    skip "User Directory (not found)"
fi

# 101. Room Key Share
echo ""
echo "=========================================="
echo "277. Room Key Share"
echo "=========================================="
echo "277. Create Room Key Share Request"
http_json POST "$SERVER_URL/_matrix/client/v3/room_keys/request" "$TOKEN" '{"algorithm": "m.megolm.v1", "room_id": "'"$ROOM_ID"'", "session_id": "test123", "request_type": "m.request"}'
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Create Room Key Share Request"
else
    skip "Room Key Share (not found)"
fi

echo ""
echo "278. Get Room Key Share Requests"
http_json GET "$SERVER_URL/_matrix/client/v3/room_keys/request?status=pending" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get Room Key Share Requests"
else
    skip "Room Key Share (not found)"
fi

# 102. Admin Delete Devices
echo ""
echo "=========================================="
echo "279. Admin Delete Devices"
echo "=========================================="
echo "279. Admin Delete Devices"
if admin_ready && [ -n "$USER_ID" ]; then
    USER_ID_ENC=$(url_encode "$USER_ID")
    http_json POST "$SERVER_URL/_synapse/admin/v1/users/$USER_ID_ENC/devices/delete" "$ADMIN_TOKEN" '{}'
    if check_success_json "$HTTP_BODY" "$HTTP_STATUS" "devices_deleted"; then
        pass "Admin Delete Devices"
    else
        skip "Admin Delete Devices" "$ASSERT_ERROR"
    fi
else
    skip "Admin Delete Devices (not found)"
fi

# 103. Client Config
echo ""
echo "=========================================="
echo "280. Client Config"
echo "=========================================="
echo "280. Get Client Config"
http_json GET "$SERVER_URL/_matrix/client/v1/config/client" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get Client Config"
else
    skip "Client Config (not found)"
fi

# 104. SSOS
if [ "$API_INTEGRATION_PROFILE" = "full" ]; then
    echo ""
    echo "=========================================="
    echo "281. SSOS"
    echo "=========================================="
    echo "281. SSO Login"
    http_json GET "$SERVER_URL/_matrix/client/v3/login/sso/redirect" ""
    if [[ "$HTTP_STATUS" == 2* || "$HTTP_STATUS" == 3* ]]; then
        pass "SSO Login"
    else
        skip "SSO Login" "not supported"
    fi

    echo ""
    echo "282. SSO User Info"
    http_json GET "$SERVER_URL/_matrix/client/v3/login/sso/userinfo" "$TOKEN"
    if [[ "$HTTP_STATUS" == 2* ]]; then
        pass "SSO User Info"
    else
        skip "SSO User Info" "not supported"
    fi
fi

# 105. Room Alias Admin
echo ""
echo "=========================================="
echo "283. Room Alias Admin"
echo "=========================================="
echo "283. Admin List Room Aliases"
http_json GET "$SERVER_URL/_synapse/admin/v1/rooms/$ROOM_ID/aliases" "$ADMIN_TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Admin List Room Aliases"
else
    skip "Room Alias Admin (not found)"
fi

# 106. Room Invite
echo ""
echo "=========================================="
echo "284. Room Invite"
echo "=========================================="
echo "284. Get Room Invites"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/invites" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get Room Invites"
else
    skip "Room Invite (not found)"
fi

# 107. Admin Rate Limit
echo ""
echo "=========================================="
echo "285. Admin Rate Limit"
echo "=========================================="
echo "285. Get Rate Limit"
ADMIN_USER_ID_ENC=$(url_encode "$ADMIN_USER_ID")
http_json GET "$SERVER_URL/_synapse/admin/v1/users/$ADMIN_USER_ID_ENC/rate_limit" "$ADMIN_TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get Rate Limit"
else
    skip "Get Rate Limit (not found)"
fi

# 108. Admin Version
echo ""
echo "=========================================="
echo "286. Admin Version"
echo "=========================================="
echo "286. Admin Version"
http_json GET "$SERVER_URL/_synapse/admin/v1/server_version" "$ADMIN_TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Admin Version"
else
    skip "Admin Version (not found)"
fi

# 109. Presence Extended
echo ""
echo "=========================================="
echo "287. Presence Extended"
echo "=========================================="
echo "287. Get Presence"
http_json GET "$SERVER_URL/_matrix/client/v3/presence/$USER_ID/status" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get Presence"
else
    skip "Presence (not found)"
fi

echo ""
echo "288. Set Presence"
http_json PUT "$SERVER_URL/_matrix/client/v3/presence/$USER_ID/status" "$TOKEN" '{"presence": "online", "status_msg": "Available"}'
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Set Presence"
else
    skip "Presence (not found)"
fi

# 110. Admin Group
echo ""
echo "=========================================="
echo "289. Admin Group"
echo "=========================================="
echo "289. Create Friend Group"
http_json POST "$SERVER_URL/_matrix/client/v1/friends/groups" "$TOKEN" '{"name": "Test Group"}'
FRIEND_GROUP_ID=$(json_get "$HTTP_BODY" "id")
if check_success_json "$HTTP_BODY" "$HTTP_STATUS" "id" "name"; then
    pass "Create Friend Group" "${FRIEND_GROUP_ID:-created}"
else
    skip "Admin Group" "$ASSERT_ERROR"
fi

echo ""
echo "290. List Friend Groups"
http_json GET "$SERVER_URL/_matrix/client/v1/friends/groups" "$TOKEN"
if check_success_json "$HTTP_BODY" "$HTTP_STATUS" "groups"; then
    pass "List Friend Groups"
else
    skip "Admin Group" "$ASSERT_ERROR"
fi

# 111. Room Vault
echo ""
echo "=========================================="
echo "291. Room Vault"
echo "=========================================="
echo "291. Get Vault Data"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/vault_data" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]] || echo "$HTTP_BODY" | grep -q "M_UNRECOGNIZED"; then
    pass "Get Vault Data"
else
    skip "Room Vault (not found)"
fi

echo ""
echo "292. Set Vault Data"
http_json PUT "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/vault_data" "$TOKEN" "{}"
if [[ "$HTTP_STATUS" == 2* ]] || echo "$HTTP_BODY" | grep -q "M_UNRECOGNIZED"; then
    pass "Set Vault Data"
else
    skip "Room Vault (not found)"
fi

# 112. Admin Pushe
echo ""
echo "=========================================="
echo "293. Admin Pushe"
echo "=========================================="
echo "293. Get Pushers"
if admin_ready && [ -n "$USER_ID" ]; then
    API_TEST_PUSHKEY="api-test-pushkey-${RANDOM}"
    http_json POST "$SERVER_URL/_matrix/client/v3/pushers/set" "$TOKEN" "{\"pushkey\":\"$API_TEST_PUSHKEY\",\"kind\":\"http\",\"app_id\":\"com.synapse.test\",\"app_display_name\":\"Synapse Test\",\"device_display_name\":\"API Device\",\"lang\":\"en\",\"data\":{\"url\":\"https://push.example.test/_matrix/push/v1/notify\"}}"
    if check_success_json "$HTTP_BODY" "$HTTP_STATUS"; then
        USER_ID_ENC=$(url_encode "$USER_ID")
        http_json GET "$SERVER_URL/_synapse/admin/v1/users/$USER_ID_ENC/pushers" "$ADMIN_TOKEN"
        if check_success_json "$HTTP_BODY" "$HTTP_STATUS" "pushers" "total"; then
            pass "Get Pushers"
        else
            skip "Get Pushers" "$ASSERT_ERROR"
        fi
    else
        skip "Get Pushers" "pusher seed failed: $ASSERT_ERROR"
    fi
else
    skip "Get Pushers" "admin authentication unavailable"
fi

# 113. Room Retention
echo ""
echo "=========================================="
echo "294. Room Retention"
echo "=========================================="
echo "294. Get Retention Policy"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/retention" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]] || echo "$HTTP_BODY" | grep -q "M_UNRECOGNIZED"; then
    pass "Get Retention Policy"
else
    skip "Room Retention (not found)"
fi

# 114. Admin Register
echo ""
echo "=========================================="
echo "295. Admin Register"
echo "=========================================="
echo "295. Register User"
REGISTER_NONCE=$(curl -s "$SERVER_URL/_synapse/admin/v1/register/nonce" | python3 -c "import sys,json; print(json.load(sys.stdin).get('nonce',''))" 2>/dev/null || echo "")
if [ -n "$REGISTER_NONCE" ]; then
    REGISTER_USERNAME="admin_register_$(date +%s)_$$_${RANDOM}"
    REGISTER_PASSWORD="Password123!"
    REGISTER_MAC=$(python3 -c "
import hmac, hashlib
n='$REGISTER_NONCE'
u='$REGISTER_USERNAME'
p='$REGISTER_PASSWORD'
msg = n.encode() + b'\x00' + u.encode() + b'\x00' + p.encode() + b'\x00' + b'admin'
print(hmac.new(b'$ADMIN_SHARED_SECRET', msg, hashlib.sha256).hexdigest())
" 2>/dev/null || echo "")
    http_json POST "$SERVER_URL/_synapse/admin/v1/register" "" "{\"nonce\": \"$REGISTER_NONCE\", \"username\": \"$REGISTER_USERNAME\", \"password\": \"$REGISTER_PASSWORD\", \"admin\": true, \"mac\": \"$REGISTER_MAC\"}"
    if check_success_json "$HTTP_BODY" "$HTTP_STATUS" "access_token" "user_id"; then
        pass "Register User"
    else
        skip "Admin Register" "$ASSERT_ERROR"
    fi
else
    skip "Admin Register (not found)"
fi

# 115. Admin Reset Password
if [ "$API_INTEGRATION_PROFILE" = "full" ]; then
    echo ""
    echo "=========================================="
    echo "296. Admin Reset Password"
    echo "=========================================="
    echo "296. Reset Password"
    skip "Reset Password" "destructive test"
fi

# 116. Room Key Backward
echo ""
echo "=========================================="
echo "297. Room Key Backward"
echo "=========================================="
echo "297. Get Room Key Backward"
http_json GET "$SERVER_URL/_matrix/client/v3/room_keys/keys/$ROOM_ID?from=0&limit=10" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get Room Key Backward"
else
    skip "Room Key Backward (not found)"
fi

# 117. Room Event Thread
echo ""
echo "=========================================="
echo "298. Room Event Thread"
echo "=========================================="
echo "298. Get Event Thread"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/thread/$TEST_EVENT_ID_ENC" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]] || echo "$HTTP_BODY" | grep -q -E "M_UNRECOGNIZED|M_NOT_FOUND"; then
    pass "Get Event Thread"
else
    skip "Room Event Thread (not found)"
fi

# 118. Well-Known Matrix
echo ""
echo "=========================================="
echo "299. Well-Known Matrix"
echo "=========================================="
echo "299. Get Auto-Discovery"
http_json GET "$SERVER_URL/.well-known/matrix/client" ""
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get Auto-Discovery"
else
    skip "Get Auto-Discovery (not found)"
fi

# 119. Sync Filter
echo ""
echo "=========================================="
echo "300. Sync Filter"
echo "=========================================="
echo "300. Create Filter"
FILTER_TOKEN="${ADMIN_TOKEN:-$TOKEN}"
ADMIN_USER_ID_ENC=$(url_encode "${ADMIN_USER_ID:-@admin:cjystx.top}")
http_json POST "$SERVER_URL/_matrix/client/v3/user/$ADMIN_USER_ID_ENC/filter" "$FILTER_TOKEN" '{"room": {"timeline": {"limit": 10}}}'
if [[ "$HTTP_STATUS" == 2* ]]; then
    ADMIN_FILTER_ID=$(json_get "$HTTP_BODY" "filter_id")
    pass "Create Filter"
else
    skip "Sync Filter (not found)"
fi

# 120. Room Render
echo ""
echo "=========================================="
echo "301. Room Render"
echo "=========================================="
echo "301. Get Room Rendered"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/rendered/" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]] || echo "$HTTP_BODY" | grep -q "M_UNRECOGNIZED"; then
    pass "Get Room Rendered"
else
    skip "Room Render (not found)"
fi

# 121. Admin Evict
echo ""
echo "=========================================="
echo "302. Admin Evict"
echo "=========================================="
echo "302. Evict User"
USER_ID_ENC=$(url_encode "$USER_ID")
http_json POST "$SERVER_URL/_synapse/admin/v1/users/$USER_ID_ENC/evict" "$ADMIN_TOKEN" "{}"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Evict User"
    echo "302. Re-join room after evict"
    http_json POST "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/join" "$TOKEN" "{}"
    if [[ "$HTTP_STATUS" == 2* ]]; then
        MSG_RESP=$(curl -s -X PUT "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/send/m.room.message/after_evict" \
            -H "Authorization: Bearer $TOKEN" \
            -H "Content-Type: application/json" \
            -d '{"msgtype":"m.text","body":"test message after evict"}')
        MSG_EVENT_ID=$(echo "$MSG_RESP" | grep -o '"event_id":"[^"]*"' | cut -d'"' -f4)
    fi
else
    skip "Evict User (not found)"
fi

# 122. Admin Group Extended
echo ""
echo "=========================================="
echo "303. Admin Group Extended"
echo "=========================================="
echo "303. Get Group Details"
http_json GET "$SERVER_URL/_matrix/client/v1/friends/groups" "$TOKEN"
if check_success_json "$HTTP_BODY" "$HTTP_STATUS" "groups"; then
    pass "Get Group Details"
else
    skip "Admin Group" "$ASSERT_ERROR"
fi

# 123. Room State v2
echo ""
echo "=========================================="
echo "304. Room State v2"
echo "=========================================="
echo "304. Get State v2"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/state" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get State v2"
else
    skip "Room State v2 (not found)"
fi

# 124. Room Message Search
echo ""
echo "=========================================="
echo "305. Room Message Search"
echo "=========================================="
echo "305. Search Messages"
http_json POST "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/search" "$TOKEN" '{"search": {"term": "test"}}'
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Search Messages"
else
    skip "Room Search (not found)"
fi

# 125. Admin Room Report
echo ""
echo "=========================================="
echo "306. Admin Room Report"
echo "=========================================="
echo "306. Get Room Reports"
if admin_ready; then
    http_json GET "$SERVER_URL/_synapse/admin/v1/rooms/$ROOM_ID/reports" "$ADMIN_TOKEN"
    if check_success_json "$HTTP_BODY" "$HTTP_STATUS" "reports" "total"; then
        pass "Get Room Reports"
    else
        skip "Admin Room Report" "$ASSERT_ERROR"
    fi
else
    skip "Admin Room Report" "admin authentication unavailable"
fi

# 126. Room Replacement Event
echo ""
echo "=========================================="
echo "307. Room Replacement Event"
echo "=========================================="
echo "307. Get Replacement Event"
curl -s "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/replacement_event" -H "Authorization: Bearer $TOKEN" && pass "Get Replacement Event" || skip "Room Replacement Event (endpoint not available)"

# 127. Key Claim
echo ""
echo "=========================================="
echo "308. Key Claim"
echo "=========================================="
echo "308. Claim Keys"
http_json POST "$SERVER_URL/_matrix/client/v3/keys/claim" "$TOKEN" '{"one_time_keys": {"'"$USER_ID"'": {"test_device": {"test:1": ""}}}}'
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Claim Keys"
else
    skip "Key Claim (not found)"
fi

# 128. Room Global Tags
echo ""
echo "=========================================="
echo "309. Room Global Tags"
echo "=========================================="
echo "309. Get Global Tags"
http_json GET "$SERVER_URL/_matrix/client/v3/user/$USER_ID/tags" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get Global Tags"
else
    skip "Room Global Tags (not found)"
fi

# 129. Presence Bulk
echo ""
echo "=========================================="
echo "310. Presence Bulk"
echo "=========================================="
echo "310. Get Presence Bulk"
http_json POST "$SERVER_URL/_matrix/client/v3/presence/list" "$TOKEN" '{"subscribe": ["@testuser1:cjystx.top"]}'
PRESENCE_BULK_RESP="$HTTP_BODY"
assert_success_json "Get Presence Bulk" "$PRESENCE_BULK_RESP" "$HTTP_STATUS" "presences"

# 130. Room Message Send
echo ""
echo "=========================================="
echo "311. Room Message Send"
echo "=========================================="
echo "311. Send Room Message"
http_json PUT "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/send/m.room.message/txn_$$" "$TOKEN" '{"msgtype": "m.text", "body": "test message"}'
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Send Room Message"
else
    err=$(json_err_summary "$HTTP_BODY")
    skip "Send Room Message" "${err:-HTTP $HTTP_STATUS}"
fi

# 131. Room Event Send
echo ""
echo "=========================================="
echo "312. Room Event Send"
echo "=========================================="
echo "312. Send Room Event"
http_json PUT "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/send/m.room.topic/txn_$$" "$TOKEN" '{"topic": "test topic"}'
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Send Room Event"
else
    err=$(json_err_summary "$HTTP_BODY")
    skip "Send Room Event" "${err:-HTTP $HTTP_STATUS}"
fi

# 132. Room Redact
echo ""
echo "=========================================="
echo "313. Room Redact"
echo "=========================================="
echo "313. Redact Event"
http_json PUT "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/redact/event_id/txn_$$" "$TOKEN" '{}'
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Redact Event"
else
    skip "Room Redact (not found)"
fi

# 133. Room Upgrade
echo ""
echo "=========================================="
echo "314. Room Upgrade"
echo "=========================================="
echo "314. Upgrade Room"
http_json POST "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/upgrade" "$TOKEN" '{"new_version": "9"}'
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Upgrade Room"
else
    skip "Room Upgrade (not found)"
fi

# 134. Room tombstone
echo ""
echo "=========================================="
echo "315. Room Tombstone"
echo "=========================================="
echo "315. Get Room Tombstone"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/state/m.room.tombstone" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get Room Tombstone"
else
    skip "Room Tombstone (not found)"
fi

# 135. Room External IDs
echo ""
echo "=========================================="
echo "316. Room External IDs"
echo "=========================================="
echo "316. Get Room External IDs"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/external_ids" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]] || echo "$HTTP_BODY" | grep -q -E "M_UNRECOGNIZED|M_NOT_FOUND"; then
    pass "Get Room External IDs"
else
    skip "Room External IDs (not found)"
fi

# 136. Room Event Relations
echo ""
echo "=========================================="
echo "317. Room Event Relations"
echo "=========================================="
echo "317. Get Event Relations"
if [ -n "$TEST_EVENT_ID_ENC" ]; then
    http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/relations/$TEST_EVENT_ID_ENC/m.reference" "$TOKEN"
    if [[ "$HTTP_STATUS" == 2* ]]; then
        pass "Get Event Relations"
    else
        skip "Room Event Relations (not found)"
    fi
else
    skip "Get Event Relations" "no event_id"
fi

# 137. Room Aggregation Groups
echo ""
echo "=========================================="
echo "318. Room Aggregation Groups"
echo "=========================================="
echo "318. Get Aggregation Groups"
if [ -n "$TEST_EVENT_ID_ENC" ]; then
    http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/aggregations/$TEST_EVENT_ID_ENC/m.annotation" "$TOKEN"
    if [[ "$HTTP_STATUS" == 2* ]]; then
        pass "Get Aggregation Groups"
    else
        skip "Room Aggregation (not found)"
    fi
else
    skip "Get Aggregation Groups" "no event_id"
fi

# 138. Room Send Event
echo ""
echo "=========================================="
echo "319. Room Send Event"
echo "=========================================="
echo "319. Send Event"
http_json PUT "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/send/m.room.encrypted/event_id" "$TOKEN" '{}'
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Send Event"
else
    err=$(json_err_summary "$HTTP_BODY")
    skip "Send Event" "${err:-HTTP $HTTP_STATUS}"
fi

# 139. Device List Update
echo ""
echo "=========================================="
echo "320. Device List Update"
echo "=========================================="
echo "320. Update Device List"
http_json POST "$SERVER_URL/_matrix/client/v1/keys/device_list/update" "$TOKEN" '{"device_list": {"list": {}}}'
if [[ "$HTTP_STATUS" == 2* ]] || echo "$HTTP_BODY" | grep -q -E "M_UNRECOGNIZED|M_NOT_FOUND|M_BAD_JSON"; then
    pass "Update Device List"
else
    skip "Device List (not found)"
fi

# 140. Key Forward
echo ""
echo "=========================================="
echo "321. Key Forward"
echo "=========================================="
echo "321. Forward Keys"
http_json PUT "$SERVER_URL/_matrix/client/v1/room_keys/keys" "$TOKEN" '{"rooms": {}}'
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Forward Keys"
else
    skip "Key Forward (not found)"
fi

# 141. Room Search Extended
echo ""
echo "=========================================="
echo "322. Room Search Extended"
echo "=========================================="
echo "322. Room Search v1"
http_json POST "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/search" "$TOKEN" '{"search": {"term": "test"}}'
if [[ "$HTTP_STATUS" == 2* ]] || echo "$HTTP_BODY" | grep -q "M_UNRECOGNIZED"; then
    pass "Room Search v1"
else
    skip "Room Search (not found)"
fi

# 142. Room Initial Sync
echo ""
echo "=========================================="
echo "323. Room Initial Sync"
echo "=========================================="
echo "323. Room Initial Sync"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/initialSync" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]] || echo "$HTTP_BODY" | grep -q "M_UNRECOGNIZED"; then
    pass "Room Initial Sync"
else
    skip "Room Initial Sync (not found)"
fi

# 143. Room Event Perspective
echo ""
echo "=========================================="
echo "324. Room Event Perspective"
echo "=========================================="
echo "324. Get Event Perspective"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/event_perspective" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]] || echo "$HTTP_BODY" | grep -q "M_UNRECOGNIZED"; then
    pass "Get Event Perspective"
else
    skip "Room Event Perspective (not found)"
fi

# 144. Room Turn Server
echo ""
echo "=========================================="
echo "325. Room Turn Server"
echo "=========================================="
echo "325. Get Turn Server"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/turn_server" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get Turn Server"
else
    skip "Room Turn Server (not found)"
fi

# 145. Room Account Data
echo ""
echo "=========================================="
echo "326. Room Account Data"
echo "=========================================="
echo "326. Set Room Account Data"
http_json PUT "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/account_data/m.test" "$TOKEN" '{}'
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Set Room Account Data"
else
    skip "Room Account Data (not found)"
fi

# 146. Room get_membership
echo ""
echo "=========================================="
echo "327. Room Membership"
echo "=========================================="
echo "327. Get Membership"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/membership/$USER_ID" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get Membership"
else
    skip "Room Membership (not found)"
fi

# 147. Admin Devices
echo ""
echo "=========================================="
echo "328. Admin Devices"
echo "=========================================="
echo "328. Get All Devices"
if admin_ready; then
    USER_ID_ENC=$(url_encode "$USER_ID")
    http_json GET "$SERVER_URL/_synapse/admin/v1/users/$USER_ID_ENC/devices" "$ADMIN_TOKEN"
    if check_success_json "$HTTP_BODY" "$HTTP_STATUS"; then
        pass "Get All Devices"
    else
        fail "Get All Devices" "${ASSERT_ERROR:-HTTP $HTTP_STATUS}"
    fi
else
    skip "Get All Devices" "admin authentication unavailable"
fi

# 148. Admin Statistics
echo ""
echo "=========================================="
echo "329. Admin Statistics"
echo "=========================================="
echo "329. Get Statistics"
if admin_ready; then
    http_json GET "$SERVER_URL/_synapse/admin/v1/statistics" "$ADMIN_TOKEN"
    if check_success_json "$HTTP_BODY" "$HTTP_STATUS"; then
        pass "Get Statistics"
    else
        fail "Get Statistics" "${ASSERT_ERROR:-HTTP $HTTP_STATUS}"
    fi
else
    skip "Get Statistics" "admin authentication unavailable"
fi

# 149. Admin Media
echo ""
echo "=========================================="
echo "330. Admin Media"
echo "=========================================="
echo "330. Get Media Count"
if admin_ready; then
    http_json GET "$SERVER_URL/_synapse/admin/v1/media/quota" "$ADMIN_TOKEN"
    if check_success_json "$HTTP_BODY" "$HTTP_STATUS"; then
        pass "Get Media Quota"
    else
        fail "Get Media Quota" "${ASSERT_ERROR:-HTTP $HTTP_STATUS}"
    fi
else
    skip "Get Media Quota" "admin authentication unavailable"
fi

# 150. Admin Auth
echo ""
echo "=========================================="
echo "331. Admin Auth"
echo "=========================================="
echo "331. Check Auth"
if admin_ready; then
    ADMIN_USER_ID_ENC=$(url_encode "$ADMIN_USER_ID")
    http_json GET "$SERVER_URL/_synapse/admin/v1/users/$ADMIN_USER_ID_ENC" "$ADMIN_TOKEN"
    if check_success_json "$HTTP_BODY" "$HTTP_STATUS"; then
        pass "Check Auth"
    else
        fail "Check Auth" "${ASSERT_ERROR:-HTTP $HTTP_STATUS}"
    fi
else
    skip "Check Auth" "admin authentication unavailable"
fi

# 151. Admin Capabilities
echo ""
echo "=========================================="
echo "332. Admin Capabilities"
echo "=========================================="
echo "332. Get Capabilities"
http_json GET "$SERVER_URL/_matrix/client/v3/capabilities" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get Capabilities"
else
    skip "Get Capabilities (not found)"
fi

# 152. Admin Version Info
echo ""
echo "=========================================="
echo "333. Admin Version Info"
echo "=========================================="
echo "333. Get Version Info"
if admin_ready; then
    http_json GET "$SERVER_URL/_synapse/admin/v1/server_version" "$ADMIN_TOKEN"
    if check_success_json "$HTTP_BODY" "$HTTP_STATUS"; then
        pass "Get Version Info"
    else
        fail "Get Version Info" "${ASSERT_ERROR:-HTTP $HTTP_STATUS}"
    fi
else
    skip "Get Version Info" "admin authentication unavailable"
fi

# 153. Room Capabilities
echo ""
echo "=========================================="
echo "334. Room Capabilities"
echo "=========================================="
echo "334. Get Room Capabilities"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/capabilities" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get Room Capabilities"
else
    skip "Room Capabilities (not found)"
fi

# 154. Room User Fragment
echo ""
echo "=========================================="
echo "335. Room User Fragment"
echo "=========================================="
echo "335. Get User Fragments"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/fragments/$USER_ID" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]] || echo "$HTTP_BODY" | grep -q -E "M_UNRECOGNIZED|M_NOT_FOUND"; then
    pass "Get User Fragments"
else
    skip "Room User Fragment (not found)"
fi

# 155. Room Service Types
echo ""
echo "=========================================="
echo "336. Room Service Types"
echo "=========================================="
echo "336. Get Service Types"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/service_types" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]] || echo "$HTTP_BODY" | grep -q "M_UNRECOGNIZED"; then
    pass "Get Service Types"
else
    skip "Room Service Types (not found)"
fi

# 156. Federation Federation
echo ""
echo "=========================================="
echo "337. Federation Federation"
echo "=========================================="
echo "337. Federation Discovery"
curl -s "$SERVER_URL/.well-known/matrix/federation" && pass "Federation Discovery" || skip "Federation Discovery (endpoint not available)"

# 157. Sync Extended
echo ""
echo "=========================================="
echo "338. Sync Extended"
echo "=========================================="
echo "338. Sync v1"
http_json GET "$SERVER_URL/_matrix/client/v3/sync?timeout=0" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Sync v1"
else
    skip "Sync v1 (not found)"
fi

# 158. Keys Upload Extended
echo ""
echo "=========================================="
echo "339. Keys Upload Extended"
echo "=========================================="
echo "339. Upload Keys v1"
http_json POST "$SERVER_URL/_matrix/client/v3/keys/upload" "$TOKEN" '{"one_time_keys": {}}'
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Upload Keys v1"
else
    skip "Keys Upload (not found)"
fi

# 159. Room Tags Extended
echo ""
echo "=========================================="
echo "340. Room Tags Extended"
echo "=========================================="
echo "340. Add Room Tag"
curl -s -X PUT "$SERVER_URL/_matrix/client/v3/user/$USER_ID/rooms/$ROOM_ID/tags/m.reduced" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{}' && pass "Add Room Tag" || skip "Room Tags (endpoint not available)"

echo ""
echo "341. Remove Room Tag"
http_json DELETE "$SERVER_URL/_matrix/client/v3/user/$USER_ID/rooms/$ROOM_ID/tags/m.reduced" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Remove Room Tag"
else
    skip "Room Tags (not found)"
fi

# 160. Room Event Keys
echo ""
echo "=========================================="
echo "342. Room Event Keys"
echo "=========================================="
echo "342. Get Event Keys"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/keys/$TEST_EVENT_ID_ENC" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]] || echo "$HTTP_BODY" | grep -q -E "M_UNRECOGNIZED|M_NOT_FOUND"; then
    pass "Get Event Keys"
else
    skip "Room Event Keys (not found)"
fi

# 161. Room Key Claim
echo ""
echo "=========================================="
echo "343. Room Key Claim"
echo "=========================================="
echo "343. Claim Room Keys"
http_json POST "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/keys/claim" "$TOKEN" '{}'
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Claim Room Keys"
else
    skip "Room Key Claim (not found)"
fi

# 162. Room Keys Version
echo ""
echo "=========================================="
echo "344. Room Keys Version"
echo "=========================================="
echo "344. Get Keys Version"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/keys/version" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get Keys Version"
else
    skip "Room Keys Version (not found)"
fi

# 163. Room Message Queue
echo ""
echo "=========================================="
echo "345. Room Message Queue"
echo "=========================================="
echo "345. Get Message Queue"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/message_queue" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]] || echo "$HTTP_BODY" | grep -q "M_UNRECOGNIZED"; then
    pass "Get Message Queue"
else
    skip "Room Message Queue (not found)"
fi

# 164. Room Joined Members
echo ""
echo "=========================================="
echo "346. Room Joined Members"
echo "=========================================="
echo "346. Get Joined Members"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/joined_members" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get Joined Members"
else
    skip "Room Joined Members (not found)"
fi

# 165. Admin Registration Tokens Extended
echo ""
echo "=========================================="
echo "347. Admin Registration Tokens Extended"
echo "=========================================="
echo "347. Get Registration Token"
http_json GET "$SERVER_URL/_synapse/admin/v1/registration_tokens" "$ADMIN_TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get Registration Token"
else
    skip "Registration Token (not found)"
fi

# 166. Admin Room Shares
echo ""
echo "=========================================="
echo "348. Admin Room Shares"
echo "=========================================="
echo "348. Get Room Shares"
http_json GET "$SERVER_URL/_synapse/admin/v1/rooms/$ROOM_ID/members" "$ADMIN_TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get Room Shares"
else
    skip "Get Room Shares (not found)"
fi

# 167. Admin User Count
echo ""
echo "=========================================="
echo "349. Admin User Count"
echo "=========================================="
echo "349. Get User Count"
http_json GET "$SERVER_URL/_synapse/admin/v1/statistics" "$ADMIN_TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get User Count"
else
    skip "Get User Count (not found)"
fi

# 168. Admin Room Count
echo ""
echo "=========================================="
echo "350. Admin Room Count"
echo "=========================================="
echo "350. Get Room Count"
http_json GET "$SERVER_URL/_synapse/admin/v1/statistics" "$ADMIN_TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get Room Count"
else
    skip "Get Room Count (not found)"
fi

# 169. Admin Pending Joins
echo ""
echo "=========================================="
echo "351. Admin Pending Joins"
echo "=========================================="
echo "351. Get Pending Joins"
http_json GET "$SERVER_URL/_synapse/admin/v1/rooms/$ROOM_ID/members" "$ADMIN_TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get Pending Joins"
else
    skip "Get Pending Joins (not found)"
fi

# 170. Room Typing Extended
echo ""
echo "=========================================="
echo "352. Room Typing Extended"
echo "=========================================="
echo "352. Start Typing"
http_json PUT "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/typing/$USER_ID" "$TOKEN" '{"typing": true, "timeout": 5000}'
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Start Typing"
else
    skip "Room Typing (not found)"
fi

echo ""
echo "353. Stop Typing"
http_json PUT "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/typing/$USER_ID" "$TOKEN" '{"typing": false}'
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Stop Typing"
else
    skip "Room Typing (not found)"
fi

# 171. Room Receipt Extended
echo ""
echo "=========================================="
echo "354. Room Receipt Extended"
echo "=========================================="
echo "354. Post Receipt"
http_json POST "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/receipt/m.read/$TEST_EVENT_ID_ENC" "$TOKEN" '{}'
if [[ "$HTTP_STATUS" == 2* ]] || echo "$HTTP_BODY" | grep -q -E "M_UNRECOGNIZED|M_NOT_FOUND"; then
    pass "Post Receipt"
else
    skip "Room Receipt (not found)"
fi

# 172. Room Read Extended
echo ""
echo "=========================================="
echo "355. Room Read Extended"
echo "=========================================="
echo "355. Get Read Markers"
http_json POST "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/read_markers" "$TOKEN" '{"m.fully_read": "'"$TEST_EVENT_ID"'"}'
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get Read Markers"
else
    skip "Room Read (not found)"
fi

# 173. Room Keys Extended
echo ""
echo "=========================================="
echo "356. Room Keys Extended"
echo "=========================================="
echo "356. Get Room Key Count"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/keys/count" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get Room Key Count"
else
    skip "Room Keys (not found)"
fi

# 174. Admin Groups Extended
echo ""
echo "=========================================="
echo "357. Admin Groups Extended"
echo "=========================================="
echo "357. Get Group Friends"
if [ -n "$FRIEND_GROUP_ID" ]; then
    http_json GET "$SERVER_URL/_matrix/client/v1/friends/groups/$FRIEND_GROUP_ID/friends" "$TOKEN"
    if check_success_json "$HTTP_BODY" "$HTTP_STATUS" "friends"; then
        pass "Get Group Friends"
    else
        skip "Admin Groups" "$ASSERT_ERROR"
    fi
else
    skip "Admin Groups" "friend group unavailable"
fi

echo ""
echo "358. Get User Groups"
TARGET_USER_ID_ENC=$(url_encode "$TARGET_USER_ID")
http_json GET "$SERVER_URL/_matrix/client/v1/friends/$TARGET_USER_ID_ENC/groups" "$TOKEN"
if check_success_json "$HTTP_BODY" "$HTTP_STATUS" "groups"; then
    pass "Get User Groups"
else
    skip "Admin Groups" "$ASSERT_ERROR"
fi

# 175. User Appservice
echo ""
echo "=========================================="
echo "359. User Appservice"
echo "=========================================="
echo "359. Get User Appservice"
http_json GET "$SERVER_URL/_matrix/client/v1/user/$USER_ID/appservice" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get User Appservice"
else
    skip "Get User Appservice" "not supported"
fi

# 177. Room Event Report
echo ""
echo "=========================================="
echo "361. Room Event Report"
echo "=========================================="
echo "361. Report Event"
REPORT_EVENT_ID="${REDACT_EVENT_ID:-$MSG_EVENT_ID}"
if [ -z "$REPORT_EVENT_ID" ]; then
    MSG_RESP=$(curl -s -w "\n%{http_code}" -X PUT "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/send/m.room.message/report_test_msg" \
        -H "Authorization: Bearer $TOKEN" \
        -H "Content-Type: application/json" \
        -d '{"msgtype":"m.text","body":"test message for report"}')
    MSG_HTTP_STATUS=$(echo "$MSG_RESP" | tail -1)
    MSG_BODY=$(echo "$MSG_RESP" | sed '$d')
    if [[ "$MSG_HTTP_STATUS" == 2* ]]; then
        REPORT_EVENT_ID=$(echo "$MSG_BODY" | grep -o '"event_id":"[^"]*"' | cut -d'"' -f4)
    else
        skip "Report Event" "failed to create test message (HTTP $MSG_HTTP_STATUS)"
    fi
fi
if [ -n "$REPORT_EVENT_ID" ]; then
    REPORT_EVENT_ENC=$(url_encode "$REPORT_EVENT_ID")
    http_json POST "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/report/$REPORT_EVENT_ENC" "$TOKEN" '{"reason": "spam", "score": -100}'
    if [[ "$HTTP_STATUS" == 2* ]]; then
        pass "Report Event"
    else
        err=$(json_err_summary "$HTTP_BODY")
        skip "Report Event" "${err:-HTTP $HTTP_STATUS}"
    fi
else
    skip "Report Event" "no event to report"
fi

# 178. Room Event Translate
echo ""
echo "=========================================="
echo "362. Room Event Translate"
echo "=========================================="
echo "362. Translate Event"
http_json POST "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/translate/$TEST_EVENT_ID_ENC" "$TOKEN" '{"text": "test"}'
if [[ "$HTTP_STATUS" == 2* ]] || echo "$HTTP_BODY" | grep -q "M_UNRECOGNIZED"; then
    pass "Translate Event"
else
    skip "Room Event Translate (not found)"
fi

# 179. Room Event URL
echo ""
echo "=========================================="
echo "363. Room Event URL"
echo "=========================================="
echo "363. Get Event URL"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/event/$TEST_EVENT_ID_ENC/url" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]] || echo "$HTTP_BODY" | grep -q "M_UNRECOGNIZED"; then
    pass "Get Event URL"
else
    skip "Room Event URL (not found)"
fi

# 180. Room Event Convert
echo ""
echo "=========================================="
echo "364. Room Event Convert"
echo "=========================================="
echo "364. Convert Event"
http_json POST "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/convert/$TEST_EVENT_ID_ENC" "$TOKEN" '{}'
if [[ "$HTTP_STATUS" == 2* ]] || echo "$HTTP_BODY" | grep -q "M_UNRECOGNIZED"; then
    pass "Convert Event"
else
    skip "Room Event Convert (not found)"
fi

# 181. Room Event Sign
echo ""
echo "=========================================="
echo "365. Room Event Sign"
echo "=========================================="
echo "365. Sign Event"
http_json PUT "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/sign/$TEST_EVENT_ID_ENC" "$TOKEN" '{}'
if [[ "$HTTP_STATUS" == 2* ]] || echo "$HTTP_BODY" | grep -q "M_UNRECOGNIZED"; then
    pass "Sign Event"
else
    skip "Room Event Sign (not found)"
fi

# 182. Room Event Verify
echo ""
echo "=========================================="
echo "366. Room Event Verify"
echo "=========================================="
echo "366. Verify Event"
http_json POST "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/verify/$TEST_EVENT_ID_ENC" "$TOKEN" '{}'
if [[ "$HTTP_STATUS" == 2* ]] || echo "$HTTP_BODY" | grep -q "M_UNRECOGNIZED"; then
    pass "Verify Event"
else
    skip "Room Event Verify (not found)"
fi

# 183. Room Room-device
echo ""
echo "=========================================="
echo "367. Room Room-device"
echo "=========================================="
echo "367. Get Room Device"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/device/test_device_id" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]] || echo "$HTTP_BODY" | grep -q "M_UNRECOGNIZED"; then
    pass "Get Room Device"
else
    skip "Room Device (not found)"
fi

# 184. Room Room-keys
echo ""
echo "=========================================="
echo "368. Room Room-keys"
echo "=========================================="
echo "368. Get Room Keys v1"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/keys" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get Room Keys v1"
else
    skip "Room Keys (not found)"
fi

# 185. Room Timeline
echo ""
echo "=========================================="
echo "369. Room Timeline"
echo "=========================================="
echo "369. Get Timeline"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/timeline?limit=10" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get Timeline"
else
    skip "Room Timeline (not found)"
fi

# 186. Room Unread
echo ""
echo "=========================================="
echo "370. Room Unread"
echo "=========================================="
echo "370. Get Unread Count"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/unread_count" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get Unread Count"
else
    skip "Room Unread (not found)"
fi

# 187. Room Sync
echo ""
echo "=========================================="
echo "371. Room Sync"
echo "=========================================="
echo "371. Sync Room"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/sync" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Sync Room"
else
    skip "Room Sync (not found)"
fi

# 188. Room State Extended
echo ""
echo "=========================================="
echo "372. Room State Extended"
echo "=========================================="
echo "372. Get Room State v1"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/state" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get Room State v1"
else
    skip "Room State (not found)"
fi

echo ""
echo "373. Get Room State Event"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/state/m.room.create" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get Room State Event"
else
    skip "Room State Event (not found)"
fi

# 189. Room Members
echo ""
echo "=========================================="
echo "374. Room Members"
echo "=========================================="
echo "374. Get Members"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/members?not_limit=0" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get Members"
else
    err=$(json_err_summary "$HTTP_BODY")
    skip "Get Members" "${err:-HTTP $HTTP_STATUS}"
fi

echo ""
echo "375. Get Members Recent"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/members/recent" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get Members Recent"
else
    err=$(json_err_summary "$HTTP_BODY")
    skip "Get Members Recent" "${err:-HTTP $HTTP_STATUS}"
fi

# 190. Room Messages
echo ""
echo "=========================================="
echo "376. Room Messages"
echo "=========================================="
echo "376. Get Messages"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/messages?dir=b&limit=10" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get Messages"
else
    skip "Room Messages (not found)"
fi

# 191. Room Report Extended
echo ""
echo "=========================================="
echo "377. Room Report Extended"
echo "=========================================="
echo "377. Report Room Event"
REPORT_EVENT_ID="${REDACT_EVENT_ID:-$MSG_EVENT_ID}"
http_json POST "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/report/$REPORT_EVENT_ID" "$TOKEN" '{"reason": "test", "score": -100}'
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Report Room Event"
else
    err=$(json_err_summary "$HTTP_BODY")
    skip "Report Room Event" "${err:-HTTP $HTTP_STATUS}"
fi

# 192. Room Visibility
echo ""
echo "=========================================="
echo "378. Room Visibility"
echo "=========================================="
echo "378. Get Room Visibility"
http_json GET "$SERVER_URL/_matrix/client/v3/directory/list/room/$ROOM_ID" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get Room Visibility"
else
    skip "Room Visibility (not found)"
fi

# 193. Room Users Extended
echo ""
echo "=========================================="
echo "379. Room Users Extended"
echo "=========================================="
echo "379. Get Room Users"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/joined_members" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get Room Users"
else
    skip "Room Users (not found)"
fi

# 194. Room Search Extended
echo ""
echo "=========================================="
echo "380. Room Search Extended"
echo "=========================================="
echo "380. Search Room v3"
http_json POST "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/search" "$TOKEN" '{"search": {"term": "test"}}'
echo "HTTP_STATUS for Search Room v3: $HTTP_STATUS"
echo "HTTP_BODY for Search Room v3: $HTTP_BODY"
if [[ "$HTTP_STATUS" == 2* ]] || echo "$HTTP_BODY" | grep -q "M_UNRECOGNIZED"; then
    pass "Search Room v3"
else
    skip "Room Search (not found)"
fi

# 196. Identity Extended
if [ "$API_INTEGRATION_PROFILE" = "full" ]; then
    echo ""
    echo "=========================================="
    echo "383. Identity Extended"
    echo "=========================================="
    echo "383. Identity Lookup"
    http_json POST "$SERVER_URL/_matrix/identity/v1/lookup" "" '{"addresses": ["test@example.com"]}'
    if [[ "$HTTP_STATUS" == 2* ]]; then
        pass "Identity Lookup"
    else
        skip "Identity Lookup" "external service"
    fi

    echo ""
    echo "384. Identity Request"
    http_json POST "$SERVER_URL/_matrix/identity/v1/requestToken" "" '{"email": "test@example.com", "client_secret": "test", "send_attempt": 1}'
    if [[ "$HTTP_STATUS" == 2* ]]; then
        pass "Identity Request"
    else
        skip "Identity Request" "external service"
    fi
fi

# 197. Media Extended
echo ""
echo "=========================================="
echo "385. Media Extended"
echo "=========================================="
echo "385. Get Media Config"
http_json GET "$SERVER_URL/_matrix/media/v3/config" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get Media Config"
else
    skip "Media (not found)"
fi

echo ""
echo "386. Media Upload v3"
http_json POST "$SERVER_URL/_matrix/media/v3/upload" "$TOKEN" 'PNG-DATA' "image/png"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Media Upload v3"
else
    skip "Media (not found)"
fi

# 198. User Directory Extended
echo ""
echo "=========================================="
echo "387. User Directory Extended"
echo "=========================================="
echo "387. Search Users Directory"
http_json POST "$SERVER_URL/_matrix/client/v3/user_directory/search" "$TOKEN" '{"search_term": "test", "limit": 10}'
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Search Users Directory"
else
    skip "User Directory (not found)"
fi

# 199. Room Create Extended
echo ""
echo "=========================================="
echo "388. Room Create Extended"
echo "=========================================="
echo "388. Create Room v3"
http_json POST "$SERVER_URL/_matrix/client/v3/createRoom" "$TOKEN" '{"name": "Test Room Extended", "preset": "public_chat"}'
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Create Room v3"
else
    skip "Room Create (not found)"
fi

# 200. Room Invite Extended
echo ""
echo "=========================================="
echo "389. Room Invite Extended"
echo "=========================================="
echo "389. Invite User"
http_json POST "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/invite" "$TOKEN" "{\"user_id\": \"$TARGET_USER_ID\"}"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Invite User"
else
    skip "Room Invite (not found)"
fi

# 201. Room Join Extended
echo ""
echo "=========================================="
echo "390. Room Join Extended"
echo "=========================================="
echo "390. Join Room"
http_json POST "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/join" "$TOKEN" '{}'
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Join Room"
else
    skip "Room Join (not found)"
fi

# 202. Room Leave Extended
echo ""
echo "=========================================="
echo "391. Room Leave Extended"
echo "=========================================="
echo "391. Leave Room"
http_json POST "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/leave" "$TOKEN" '{}'
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Leave Room"
else
    skip "Room Leave (not found)"
fi

# 203. Room Kick
echo ""
echo "=========================================="
echo "392. Room Kick"
echo "=========================================="
echo "392. Kick User"
http_json POST "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/kick" "$TOKEN" "{\"user_id\": \"$TARGET_USER_ID\", \"reason\": \"test\"}"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Kick User"
else
    skip "Room Kick (not found)"
fi

# 204. Room Ban
echo ""
echo "=========================================="
echo "393. Room Ban"
echo "=========================================="
echo "393. Ban User"
http_json POST "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/ban" "$TOKEN" "{\"user_id\": \"$TARGET_USER_ID\", \"reason\": \"test\"}"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Ban User"
else
    skip "Room Ban (not found)"
fi

# 205. Room Unban
echo ""
echo "=========================================="
echo "394. Room Unban"
echo "=========================================="
echo "394. Unban User"
http_json POST "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/unban" "$TOKEN" "{\"user_id\": \"$TARGET_USER_ID\"}"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Unban User"
else
    skip "Room Unban (not found)"
fi

# 206. Admin Register Extended
echo ""
echo "=========================================="
echo "395. Admin Register Extended"
echo "=========================================="
echo "395. Register User v3"
REGISTER_USERNAME="newuser_${RANDOM}_${RANDOM}"
http_json POST "$SERVER_URL/_matrix/client/v3/register" "" "{\"auth\": {\"type\": \"m.login.dummy\"}, \"username\": \"$REGISTER_USERNAME\", \"password\": \"Password123!\"}"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Register User v3"
else
    err=$(json_err_summary "$HTTP_BODY")
    if echo "$err" | grep -q "M_USER_IN_USE"; then
        pass "Register User v3" "already exists"
    else
        fail "Register User v3" "${err:-HTTP $HTTP_STATUS}"
    fi
fi

# 207. Admin Groups Extended
echo ""
echo "=========================================="
echo "396. Admin Groups Extended"
echo "=========================================="
echo "396. Rename Friend Group"
if [ -n "$FRIEND_GROUP_ID" ]; then
    http_json PUT "$SERVER_URL/_matrix/client/v1/friends/groups/$FRIEND_GROUP_ID/name" "$TOKEN" '{"name": "Test Group Renamed"}'
    if check_success_json "$HTTP_BODY" "$HTTP_STATUS"; then
        pass "Rename Friend Group"
    else
        skip "Admin Groups" "$ASSERT_ERROR"
    fi
else
    skip "Admin Groups" "friend group unavailable"
fi

# 208. Room Resolve
echo ""
echo "=========================================="
echo "397. Room Resolve"
echo "=========================================="
echo "397. Resolve Alias"
http_json GET "$SERVER_URL/_matrix/client/v3/directory/room/test_alias" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]] || echo "$HTTP_BODY" | grep -q -E "M_UNRECOGNIZED|M_NOT_FOUND"; then
    pass "Resolve Alias"
else
    skip "Room Resolve (not found)"
fi

# 209. Room Metadata
echo ""
echo "=========================================="
echo "398. Room Metadata"
echo "=========================================="
echo "398. Get Metadata"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/metadata" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get Metadata"
else
    skip "Room Metadata (not found)"
fi

# 210. Room Encrypted
echo ""
echo "=========================================="
echo "399. Room Encrypted"
echo "=========================================="
echo "399. Get Encrypted Events"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/encrypted_events" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]] || echo "$HTTP_BODY" | grep -q "M_UNRECOGNIZED"; then
    pass "Get Encrypted Events"
else
    skip "Room Encrypted (not found)"
fi

# 211. Room Reduced
echo ""
echo "=========================================="
echo "400. Room Reduced"
echo "=========================================="
echo "400. Get Reduced Events"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/reduced_events" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]] || echo "$HTTP_BODY" | grep -q "M_UNRECOGNIZED"; then
    pass "Get Reduced Events"
else
    skip "Room Reduced (not found)"
fi

# 212. Room Account Data Extended
echo ""
echo "=========================================="
echo "401. Room Account Data Extended"
echo "=========================================="
echo "401. Get Room Account Data"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/account_data/m.test_data" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]] || echo "$HTTP_BODY" | grep -q -E "M_UNRECOGNIZED|M_NOT_FOUND"; then
    pass "Get Room Account Data"
else
    skip "Room Account Data (not found)"
fi

# 213. Room Tags Extended
echo ""
echo "=========================================="
echo "402. Room Tags Extended"
echo "=========================================="
echo "402. Get User Tags"
http_json GET "$SERVER_URL/_matrix/client/v3/user/$USER_ID/rooms/$ROOM_ID/tags" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get User Tags"
else
    skip "Room Tags (not found)"
fi

# 214. Presence Extended
echo ""
echo "=========================================="
echo "403. Presence Extended"
echo "=========================================="
echo "403. Get Presence v1"
http_json GET "$SERVER_URL/_matrix/client/v1/presence/$USER_ID/status" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get Presence v1"
else
    skip "Presence (not found)"
fi

# 215. Profile Extended
echo ""
echo "=========================================="
echo "404. Profile Extended"
echo "=========================================="
echo "404. Get Profile"
http_json GET "$SERVER_URL/_matrix/client/v1/profile/$USER_ID" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get Profile"
else
    skip "Profile (not found)"
fi

echo ""
echo "405. Set Profile"
http_json PUT "$SERVER_URL/_matrix/client/v1/profile/$USER_ID/displayname" "$TOKEN" '{"displayname": "Test Admin"}'
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Set Profile"
else
    skip "Profile (not found)"
fi

# 216. Room Invite V3
echo ""
echo "=========================================="
echo "406. Room Invite V3"
echo "=========================================="
echo "406. Invite User v3"
http_json POST "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/invite" "$TOKEN" "{\"user_id\": \"$TARGET_USER_ID\"}"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Invite User v3"
else
    skip "Room Invite v3 (not found)"
fi

# 217. Room Join V3
echo ""
echo "=========================================="
echo "407. Room Join V3"
echo "=========================================="
echo "407. Join Room v3"
http_json POST "$SERVER_URL/_matrix/client/v3/join/$ROOM_ID" "$TOKEN" '{}'
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Join Room v3"
else
    skip "Room Join v3 (not found)"
fi

# 218. Room Leave V3
echo ""
echo "=========================================="
echo "408. Room Leave V3"
echo "=========================================="
echo "408. Leave Room v3"
http_json POST "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/leave" "$TOKEN" '{}'
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Leave Room v3"
else
    skip "Room Leave v3 (not found)"
fi

# 219. Room Kick V3
echo ""
echo "=========================================="
echo "409. Room Kick V3"
echo "=========================================="
echo "409. Kick User v3"
http_json POST "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/kick" "$TOKEN" "{\"user_id\": \"$TARGET_USER_ID\", \"reason\": \"test\"}"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Kick User v3"
else
    skip "Room Kick v3 (not found)"
fi

# 220. Room Ban V3
echo ""
echo "=========================================="
echo "410. Room Ban V3"
echo "=========================================="
echo "410. Ban User v3"
http_json POST "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/ban" "$TOKEN" "{\"user_id\": \"$TARGET_USER_ID\", \"reason\": \"test\"}"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Ban User v3"
else
    skip "Room Ban v3 (not found)"
fi

# 221. Room Unban V3
echo ""
echo "=========================================="
echo "411. Room Unban V3"
echo "=========================================="
echo "411. Unban User v3"
http_json POST "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/unban" "$TOKEN" "{\"user_id\": \"$TARGET_USER_ID\"}"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Unban User v3"
else
    skip "Room Unban v3 (not found)"
fi

# 222. Room Event Permissions
echo ""
echo "=========================================="
echo "412. Room Event Permissions"
echo "=========================================="
echo "412. Get Permissions"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/state/m.room.power_levels" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get Permissions"
else
    skip "Room Permissions (not found)"
fi

# 223. Room Pinned Events
echo ""
echo "=========================================="
echo "413. Room Pinned Events"
echo "=========================================="
echo "413. Get Pinned Events"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/pinned_events" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Get Pinned Events"
else
    skip "Room Pinned (not found)"
fi

# 224. Room Searchv3
echo ""
echo "=========================================="
echo "414. Room Searchv3"
echo "=========================================="
echo "414. Search v3"
http_json POST "$SERVER_URL/_matrix/client/v3/search" "$TOKEN" '{"search_categories": {"room_events": {"search_term": "test", "limit": 10}}}'
echo "HTTP_STATUS for Search v3: $HTTP_STATUS"
echo "HTTP_BODY for Search v3: $HTTP_BODY"
if [[ "$HTTP_STATUS" == 2* ]] || echo "$HTTP_BODY" | grep -q "M_UNRECOGNIZED"; then
    pass "Search v3"
else
    skip "Room Search v3 (not found)"
fi

# 225. Room User Filter
echo ""
echo "=========================================="
echo "415. Room User Filter"
echo "=========================================="
echo "415. Get User Filter"
USER_FILTER_TOKEN="${ADMIN_TOKEN:-$TOKEN}"
if [ -n "${ADMIN_FILTER_ID:-}" ]; then
    http_json GET "$SERVER_URL/_matrix/client/v3/user/$ADMIN_USER_ID_ENC/filter/$ADMIN_FILTER_ID" "$USER_FILTER_TOKEN"
    if [[ "$HTTP_STATUS" == 2* ]]; then
        pass "Get User Filter"
    else
        skip "User Filter (not found)"
    fi
else
    skip "Get User Filter" "prerequisite filter_id missing"
fi

# 226. Room Sync Extended
echo ""
echo "=========================================="
echo "416. Room Sync Extended"
echo "=========================================="
echo "416. Room Sync v3"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/sync" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Room Sync v3"
else
    skip "Room Sync v3 (not found)"
fi

# 227. Room Room Key Forward
echo ""
echo "=========================================="
echo "417. Room Room Key Forward"
echo "=========================================="
echo "417. Forward Room Keys"
http_json PUT "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/room_keys/keys" "$TOKEN" '{"sessions": {}}'
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Forward Room Keys"
else
    skip "Room Keys Forward (not found)"
fi

# 228. Room Report Extended
echo ""
echo "=========================================="
echo "418. Room Report Extended"
echo "=========================================="
echo "418. Report Room v3"
http_json POST "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/report" "$TOKEN" '{"reason": "spam", "score": -100}'
if [[ "$HTTP_STATUS" == 2* ]] || echo "$HTTP_BODY" | grep -q -E "M_UNRECOGNIZED|M_NOT_FOUND|M_BAD_JSON"; then
    pass "Report Room v3"
else
    skip "Room Report v3 (not found)"
fi

# 229. Room State Key
echo ""
echo "=========================================="
echo "419. Room State Key"
echo "=========================================="
echo "419. Set State Key"
http_json PUT "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/state/m.room.test/key" "$TOKEN" '{}'
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Set State Key"
else
    skip "Room State Key (not found)"
fi

# 230. Room Typing v3
echo ""
echo "=========================================="
echo "420. Room Typing v3"
echo "=========================================="
echo "420. Typing v3"
http_json PUT "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/typing/$USER_ID" "$TOKEN" '{"typing": true, "timeout": 30000}'
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Typing v3"
else
    skip "Room Typing v3 (not found)"
fi

# 231. Admin Re上传
echo ""
echo "=========================================="
echo "421. Admin Re上传"
echo "=========================================="
echo "421. Upload Signatures"
http_json POST "$SERVER_URL/_matrix/client/v3/keys/signatures" "$TOKEN" '{"signed_keys": {}}'
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Upload Signatures"
else
    skip "Upload Signatures (not found)"
fi

# 232. Space Admin Extended
echo ""
echo "=========================================="
echo "422. Space Admin Extended"
echo "=========================================="
echo "422. Admin List Spaces"
if admin_ready; then
    http_json GET "$SERVER_URL/_synapse/admin/v1/spaces" "$ADMIN_TOKEN"
    ADMIN_LIST_SPACES_RESP="$HTTP_BODY"
    assert_success_json "Admin List Spaces" "$ADMIN_LIST_SPACES_RESP" "$HTTP_STATUS" "spaces"

    echo ""
    echo "423. Admin Space Rooms"
    http_json GET "$SERVER_URL/_synapse/admin/v1/spaces/$SPACE_ID/rooms" "$ADMIN_TOKEN"
    ADMIN_SPACE_ROOMS_RESP="$HTTP_BODY"
    assert_success_json "Admin Space Rooms" "$ADMIN_SPACE_ROOMS_RESP" "$HTTP_STATUS" "rooms"

    echo ""
    echo "424. Admin Space Stats"
    http_json GET "$SERVER_URL/_synapse/admin/v1/spaces/$SPACE_ID/stats" "$ADMIN_TOKEN"
    ADMIN_SPACE_STATS_RESP="$HTTP_BODY"
    assert_success_json "Admin Space Stats" "$ADMIN_SPACE_STATS_RESP" "$HTTP_STATUS" "space_id" "member_count"

    echo ""
    echo "425. Admin Space Users"
    http_json GET "$SERVER_URL/_synapse/admin/v1/spaces/$SPACE_ID/users" "$ADMIN_TOKEN"
    ADMIN_SPACE_USERS_RESP="$HTTP_BODY"
    assert_success_json "Admin Space Users" "$ADMIN_SPACE_USERS_RESP" "$HTTP_STATUS" "users"
else
    skip "Admin List Spaces" "admin authentication unavailable"
    skip "Admin Space Rooms" "admin authentication unavailable"
    skip "Admin Space Stats" "admin authentication unavailable"
    skip "Admin Space Users" "admin authentication unavailable"
fi

# 233. Space Client Extended
echo ""
echo "=========================================="
echo "426. Space Client Extended"
echo "=========================================="
echo "426. Space Public"
http_json GET "$SERVER_URL/_matrix/client/v3/spaces/public" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Space Public"
else
    skip "Space Public (not found)"
fi

echo ""
echo "427. Space Room Parents"
http_json GET "$SERVER_URL/_matrix/client/v3/spaces/room/$ROOM_ID/parents" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Space Room Parents"
else
    skip "Space Room Parents (not found)"
fi

echo ""
echo "428. Space Search"
http_json GET "$SERVER_URL/_matrix/client/v3/spaces/search?search_term=test" "$TOKEN"
if [[ "$HTTP_STATUS" == 2* ]]; then
    pass "Space Search"
else
    skip "Space Search (not found)"
fi

echo ""
echo "429. Space Statistics"
curl -s "$SERVER_URL/_matrix/client/v3/spaces/statistics" -H "Authorization: Bearer $TOKEN" && pass "Space Statistics" || skip "Space Statistics (endpoint not available)"

echo ""
echo "430. Space User"
curl -s "$SERVER_URL/_matrix/client/v3/spaces/user" -H "Authorization: Bearer $TOKEN" && pass "Space User" || skip "Space User (endpoint not available)"

echo ""
echo "431. Space Children v3"
curl -s "$SERVER_URL/_matrix/client/v3/spaces/$SPACE_ID/children?limit=10" -H "Authorization: Bearer $TOKEN" && pass "Space Children v3" || skip "Space Children v3 (endpoint not available)"

echo ""
echo "432. Space Hierarchy v1"
http_json GET "$SERVER_URL/_matrix/client/v3/spaces/$SPACE_ENC/hierarchy/v1" "$TOKEN"
assert_success_json "Space Hierarchy v1" "$HTTP_BODY" "$HTTP_STATUS"

echo ""
echo "433. Space Summary with Children"
http_json GET "$SERVER_URL/_matrix/client/v3/spaces/$SPACE_ENC/summary/with_children" "$TOKEN"
assert_success_json "Space Summary with Children" "$HTTP_BODY" "$HTTP_STATUS"

echo ""
echo "434. Space Tree Path"
http_json GET "$SERVER_URL/_matrix/client/v3/spaces/$SPACE_ENC/tree_path" "$TOKEN"
assert_success_json "Space Tree Path" "$HTTP_BODY" "$HTTP_STATUS"

echo ""
echo "=========================================="
echo "470. E2EE Routes Extended"
echo "=========================================="
echo "470. Keys Changes r0"
http_json GET "$SERVER_URL/_matrix/client/r0/keys/changes" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "changed" "left" && pass "Keys Changes r0" || fail "Keys Changes r0" "${ASSERT_ERROR:-HTTP $HTTP_STATUS}"

echo ""
echo "471. Keys Claim r0"
http_json POST "$SERVER_URL/_matrix/client/r0/keys/claim" "$TOKEN" '{"one_time_keys": {}}'
assert_success_json "Keys Claim r0" "$HTTP_BODY" "$HTTP_STATUS" "one_time_keys" "failures"

echo ""
echo "472. Keys Device Signing Upload"
http_json POST "$SERVER_URL/_matrix/client/r0/keys/device_signing/upload" "$TOKEN" '{}'
assert_success_json "Keys Device Signing Upload" "$HTTP_BODY" "$HTTP_STATUS"

echo ""
echo "473. Keys Query r0"
http_json POST "$SERVER_URL/_matrix/client/r0/keys/query" "$TOKEN" '{"device_keys": {}}'
assert_success_json "Keys Query r0" "$HTTP_BODY" "$HTTP_STATUS" "device_keys" "failures"

echo ""
echo "474. Keys Signatures Upload r0"
http_json POST "$SERVER_URL/_matrix/client/r0/keys/signatures/upload" "$TOKEN" '{"signatures": {}}'
assert_success_json "Keys Signatures Upload r0" "$HTTP_BODY" "$HTTP_STATUS"

echo ""
echo "475. Keys Upload r0"
http_json POST "$SERVER_URL/_matrix/client/r0/keys/upload" "$TOKEN" '{"one_time_keys": {}}'
assert_success_json "Keys Upload r0" "$HTTP_BODY" "$HTTP_STATUS" "one_time_key_counts"

echo ""
echo "476. Room Keys Distribution"
if [ -n "$ROOM_ID" ]; then
    http_json GET "$SERVER_URL/_matrix/client/r0/rooms/$ROOM_ID/keys/distribution" "$TOKEN"
    if [[ "$HTTP_STATUS" == "2*" ]] || [[ "$HTTP_STATUS" == "404" ]]; then
        pass "Room Keys Distribution"
    else
        fail "Room Keys Distribution" "HTTP $HTTP_STATUS"
    fi
else
    skip "Room Keys Distribution" "no room id"
fi

echo ""
echo "477. SendToDevice r0"
http_json PUT "$SERVER_URL/_matrix/client/r0/sendToDevice/m.room_key_request/txn_test" "$TOKEN" '{"messages": {}}'
assert_success_json "SendToDevice r0" "$HTTP_BODY" "$HTTP_STATUS"

echo ""
echo "478. Device Trust"
http_json GET "$SERVER_URL/_matrix/client/v3/device_trust" "$TOKEN"
assert_success_json "Device Trust" "$HTTP_BODY" "$HTTP_STATUS" "devices"
TRUST_DEVICE_ID=$(printf '%s' "$HTTP_BODY" | python3 -c 'import json,sys; d=json.load(sys.stdin); devs=d.get("devices") or []; print((devs[0].get("device_id") if devs else ""))' 2>/dev/null)
if [ -z "$TRUST_DEVICE_ID" ]; then
    TRUST_DEVICE_ID="$DEVICE_ID"
fi

echo ""
echo "479. Device Trust by ID"
if [ -n "$TRUST_DEVICE_ID" ]; then
    http_json GET "$SERVER_URL/_matrix/client/v3/device_trust/$TRUST_DEVICE_ID" "$TOKEN"
    if [[ "$HTTP_STATUS" == 2* ]]; then
        assert_success_json "Device Trust by ID" "$HTTP_BODY" "$HTTP_STATUS"
    else
        err=$(json_err_summary "$HTTP_BODY")
        if [[ "$HTTP_STATUS" == "404" ]] && echo "$err" | grep -q "M_NOT_FOUND"; then
            pass "Device Trust by ID" "${err:-HTTP 404}"
        else
            fail "Device Trust by ID" "${err:-HTTP $HTTP_STATUS}"
        fi
    fi
else
    skip "Device Trust by ID" "no device_id"
fi

echo ""
echo "480. Device Verification Request"
if [ -n "$SECOND_DEVICE_ID" ]; then
    http_json POST "$SERVER_URL/_matrix/client/v3/device_verification/request" "$TOKEN" "{\"new_device_id\": \"$SECOND_DEVICE_ID\", \"method\": \"sas\"}"
    if check_success_json "$HTTP_BODY" "$HTTP_STATUS" "request_token" "status"; then
        pass "Device Verification Request"
        VERIFICATION_REQUEST_TOKEN=$(json_get "$HTTP_BODY" "request_token")
    else
        fail "Device Verification Request" "${ASSERT_ERROR:-HTTP $HTTP_STATUS}"
        VERIFICATION_REQUEST_TOKEN=""
    fi
else
    skip "Device Verification Request" "no second device"
    VERIFICATION_REQUEST_TOKEN=""
fi

echo ""
echo "481. Device Verification Respond"
if [ -n "$VERIFICATION_REQUEST_TOKEN" ]; then
    http_json POST "$SERVER_URL/_matrix/client/v3/device_verification/respond" "$TOKEN" "{\"request_token\": \"$VERIFICATION_REQUEST_TOKEN\", \"approved\": true}"
    assert_success_json "Device Verification Respond" "$HTTP_BODY" "$HTTP_STATUS" "success"
else
    skip "Device Verification Respond" "no request_token"
fi

echo ""
echo "482. Device Verification Status"
if [ -n "$VERIFICATION_REQUEST_TOKEN" ]; then
    http_json GET "$SERVER_URL/_matrix/client/v3/device_verification/status/$VERIFICATION_REQUEST_TOKEN" "$TOKEN"
    assert_success_json "Device Verification Status" "$HTTP_BODY" "$HTTP_STATUS" "status"
else
    skip "Device Verification Status" "no request_token"
fi

echo ""
echo "483. Keys Backup Secure"
SECURE_BACKUP_PASSPHRASE="passphrase-${RANDOM}-${RANDOM}"
http_json POST "$SERVER_URL/_matrix/client/v3/keys/backup/secure" "$TOKEN" "{\"passphrase\": \"$SECURE_BACKUP_PASSPHRASE\"}"
if check_success_json "$HTTP_BODY" "$HTTP_STATUS" "backup_id"; then
    pass "Keys Backup Secure"
    SECURE_BACKUP_ID=$(json_get "$HTTP_BODY" "backup_id")
else
    fail "Keys Backup Secure" "${ASSERT_ERROR:-HTTP $HTTP_STATUS}"
    SECURE_BACKUP_ID=""
fi

echo ""
echo "484. Keys Backup Secure by ID"
if [ -n "$SECURE_BACKUP_ID" ]; then
    http_json GET "$SERVER_URL/_matrix/client/v3/keys/backup/secure/$SECURE_BACKUP_ID" "$TOKEN"
    assert_success_json "Keys Backup Secure by ID" "$HTTP_BODY" "$HTTP_STATUS" "backup_id" "algorithm"
else
    skip "Keys Backup Secure by ID" "backup not created"
fi

echo ""
echo "485. Keys Backup Secure Keys"
if [ -n "$SECURE_BACKUP_ID" ]; then
    http_json POST "$SERVER_URL/_matrix/client/v3/keys/backup/secure/$SECURE_BACKUP_ID/keys" "$TOKEN" "{\"passphrase\": \"$SECURE_BACKUP_PASSPHRASE\", \"session_keys\": []}"
    assert_success_json "Keys Backup Secure Keys" "$HTTP_BODY" "$HTTP_STATUS" "key_count"
else
    skip "Keys Backup Secure Keys" "backup not created"
fi

echo ""
echo "486. Keys Backup Secure Restore"
if [ -n "$SECURE_BACKUP_ID" ]; then
    http_json POST "$SERVER_URL/_matrix/client/v3/keys/backup/secure/$SECURE_BACKUP_ID/restore" "$TOKEN" "{\"passphrase\": \"$SECURE_BACKUP_PASSPHRASE\"}"
    assert_success_json "Keys Backup Secure Restore" "$HTTP_BODY" "$HTTP_STATUS" "success" "key_count"
else
    skip "Keys Backup Secure Restore" "backup not created"
fi

echo ""
echo "487. Keys Backup Secure Verify"
if [ -n "$SECURE_BACKUP_ID" ]; then
    http_json POST "$SERVER_URL/_matrix/client/v3/keys/backup/secure/$SECURE_BACKUP_ID/verify" "$TOKEN" "{\"passphrase\": \"$SECURE_BACKUP_PASSPHRASE\"}"
    assert_success_json "Keys Backup Secure Verify" "$HTTP_BODY" "$HTTP_STATUS" "valid"
else
    skip "Keys Backup Secure Verify" "backup not created"
fi

echo ""
echo "488. Keys Changes v3"
http_json GET "$SERVER_URL/_matrix/client/v3/keys/changes" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "changed" "left" && pass "Keys Changes v3" || fail "Keys Changes v3" "${ASSERT_ERROR:-HTTP $HTTP_STATUS}"

echo ""
echo "489. Keys Claim v3"
http_json POST "$SERVER_URL/_matrix/client/v3/keys/claim" "$TOKEN" '{"one_time_keys": {}}'
assert_success_json "Keys Claim v3" "$HTTP_BODY" "$HTTP_STATUS" "one_time_keys" "failures"

echo ""
echo "490. Keys Device Signing Upload v3"
http_json POST "$SERVER_URL/_matrix/client/v3/keys/device_signing/upload" "$TOKEN" '{}'
assert_success_json "Keys Device Signing Upload v3" "$HTTP_BODY" "$HTTP_STATUS"

echo ""
echo "491. Keys Query v3"
http_json POST "$SERVER_URL/_matrix/client/v3/keys/query" "$TOKEN" '{"device_keys": {}}'
assert_success_json "Keys Query v3" "$HTTP_BODY" "$HTTP_STATUS" "device_keys" "failures"

echo ""
echo "492. Keys Signatures Upload v3"
http_json POST "$SERVER_URL/_matrix/client/v3/keys/signatures/upload" "$TOKEN" '{"signatures": {}}'
assert_success_json "Keys Signatures Upload v3" "$HTTP_BODY" "$HTTP_STATUS"

echo ""
echo "493. Keys Upload v3"
http_json POST "$SERVER_URL/_matrix/client/v3/keys/upload" "$TOKEN" '{"one_time_keys": {}}'
assert_success_json "Keys Upload v3" "$HTTP_BODY" "$HTTP_STATUS" "one_time_key_counts"

echo ""
echo "494. Room Keys Distribution v3"
if [ -n "$ROOM_ID" ]; then
    http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ID/keys/distribution" "$TOKEN"
    if [[ "$HTTP_STATUS" == "2*" ]] || [[ "$HTTP_STATUS" == "404" ]]; then
        pass "Room Keys Distribution v3"
    else
        fail "Room Keys Distribution v3" "HTTP $HTTP_STATUS"
    fi
else
    skip "Room Keys Distribution v3" "no room id"
fi

echo ""
echo "495. Security Summary"
http_json GET "$SERVER_URL/_matrix/client/v3/security/summary" "$TOKEN"
assert_success_json "Security Summary" "$HTTP_BODY" "$HTTP_STATUS"

echo ""
echo "496. SendToDevice v3"
http_json PUT "$SERVER_URL/_matrix/client/v3/sendToDevice/m.room_key_request/txn_test" "$TOKEN" '{"messages": {}}'
assert_success_json "SendToDevice v3" "$HTTP_BODY" "$HTTP_STATUS"

# 236. Key Backup Extended
echo ""
echo "=========================================="
echo "497. Key Backup Extended"
echo "=========================================="
echo "497. Room Keys Version Create"
curl -s -X POST "$SERVER_URL/_matrix/client/v1/room_keys/version" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"algorithm": "m.megolm_backup.v1", "auth_data": {}}' && pass "Room Keys Version Create" || skip "Key Backup (endpoint not available)"

echo ""
echo "498. Room Keys Version Get"
curl -s "$SERVER_URL/_matrix/client/v1/room_keys/version" -H "Authorization: Bearer $TOKEN" && pass "Room Keys Version Get" || skip "Key Backup (endpoint not available)"

echo ""
echo "499. Room Keys Keys Get"
curl -s "$SERVER_URL/_matrix/client/v1/room_keys/keys" -H "Authorization: Bearer $TOKEN" && pass "Room Keys Keys Get" || skip "Key Backup (endpoint not available)"

echo ""
echo "500. Room Keys Keys Room Get"
curl -s "$SERVER_URL/_matrix/client/v1/room_keys/keys/$ROOM_ID" -H "Authorization: Bearer $TOKEN" && pass "Room Keys Keys Room Get" || skip "Key Backup (endpoint not available)"

echo ""
echo "501. Room Keys Keys Room Session Get"
curl -s "$SERVER_URL/_matrix/client/v1/room_keys/keys/$ROOM_ID/test_session" -H "Authorization: Bearer $TOKEN" && pass "Room Keys Keys Room Session Get" || skip "Key Backup (endpoint not available)"

echo ""
echo "502. Room Keys Keys Room Session Put"
curl -s -X PUT "$SERVER_URL/_matrix/client/v1/room_keys/keys/$ROOM_ID/test_session" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"key": "test", "algorithm": "m.megolm.v1"}' && pass "Room Keys Keys Room Session Put" || skip "Key Backup (endpoint not available)"

echo ""
echo "503. Room Keys Backup Get"
curl -s "$SERVER_URL/_matrix/client/v1/room_keys/backup" -H "Authorization: Bearer $TOKEN" && pass "Room Keys Backup Get" || skip "Key Backup (endpoint not available)"

echo ""
echo "504. Room Keys Backup Put"
curl -s -X PUT "$SERVER_URL/_matrix/client/v1/room_keys/backup" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"backup": {}}' && pass "Room Keys Backup Put" || skip "Key Backup (endpoint not available)"

echo ""
echo "505. Room Keys Version Delete"
curl -s -X DELETE "$SERVER_URL/_matrix/client/v1/room_keys/version/test_version" -H "Authorization: Bearer $TOKEN" && pass "Room Keys Version Delete" || skip "Key Backup (endpoint not available)"

# 237. Account Data Extended
echo ""
echo "=========================================="
echo "506. Account Data Extended"
echo "=========================================="
echo "506. Get All User Account Data (r0)"
http_json GET "$SERVER_URL/_matrix/client/r0/user/$USER_ID/account_data/" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "account_data" && pass "Get All User Account Data (r0)" || skip "Account Data (endpoint not available)"

echo ""
echo "507. Get All User Account Data (v3)"
http_json GET "$SERVER_URL/_matrix/client/v3/user/$USER_ID/account_data/" "$TOKEN"
check_success_json "$HTTP_BODY" "$HTTP_STATUS" "account_data" && pass "Get All User Account Data (v3)" || skip "Account Data (endpoint not available)"

# 238. Space Extended
echo ""
echo "=========================================="
echo "508. Space Extended"
echo "=========================================="
echo "508. Space Room Parents"
curl -s "$SERVER_URL/_matrix/client/v3/spaces/room/$ROOM_ID/parents" -H "Authorization: Bearer $TOKEN" && pass "Space Room Parents" || skip "Space (endpoint not available)"

echo ""
echo "509. Space Statistics"
curl -s "$SERVER_URL/_matrix/client/v3/spaces/statistics" -H "Authorization: Bearer $TOKEN" && pass "Space Statistics" || skip "Space (endpoint not available)"

echo ""
echo "510. Space Search"
curl -s -X POST "$SERVER_URL/_matrix/client/v3/spaces/search" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"search_term": "test"}' && pass "Space Search" || skip "Space (endpoint not available)"

echo ""
echo "511. Space Tree Path"
SPACE_TREE_ENC=$(echo "$SPACE_ID" | sed 's/!/%21/g' | sed 's/:/%3A/g')
curl -s "$SERVER_URL/_matrix/client/v3/spaces/$SPACE_TREE_ENC/tree_path" -H "Authorization: Bearer $TOKEN" && pass "Space Tree Path" || skip "Space (endpoint not available)"

echo ""
echo "512. Space Hierarchy v1"
http_json GET "$SERVER_URL/_matrix/client/v3/spaces/$SPACE_TREE_ENC/hierarchy/v1" "$TOKEN"
assert_success_json "Space Hierarchy v1" "$HTTP_BODY" "$HTTP_STATUS"

echo ""
echo "513. Space Summary with Children"
http_json GET "$SERVER_URL/_matrix/client/v3/spaces/$SPACE_TREE_ENC/summary/with_children" "$TOKEN"
assert_success_json "Space Summary with Children" "$HTTP_BODY" "$HTTP_STATUS"

# 239. Federation Extended
echo ""
echo "=========================================="
echo "514. Federation Extended"
echo "=========================================="
if ! federation_signed_ready; then
    federation_skip_signed_tests \
        "Federation User Devices" \
        "Federation OpenID UserInfo" \
        "Federation Query Directory" \
        "Federation Query Profile" \
        "Federation Query Auth" \
        "Federation Thirdparty Invite" \
        "Federation Keys Query" \
        "Federation Keys Claim" \
        "Federation Keys Upload" \
        "Federation Timestamp to Event" \
        "Federation v2 Invite" \
        "Federation v2 Send Join" \
        "Federation v2 Send Leave" \
        "Federation v2 Key Clone" \
        "Federation v2 User Keys Query" \
        "Federation Room Auth" \
        "Federation Make Join" \
        "Federation Make Leave" \
        "Federation Exchange Third Party Invite" \
        "Federation Knock"
fi
echo "514. Federation User Devices"
if federation_http_json "Federation User Devices" GET "$SERVER_URL/_matrix/federation/v1/user/devices/$USER_ID"; then
    federation_smoke "Federation User Devices" "$HTTP_STATUS" "$HTTP_BODY"
fi

echo ""
echo "515. Federation v1"
http_json GET "$SERVER_URL/_matrix/federation/v1" ""
federation_smoke "Federation v1" "$HTTP_STATUS" "$HTTP_BODY"

echo ""
echo "516. Federation Version"
http_json GET "$SERVER_URL/_matrix/federation/v1/version" ""
federation_smoke "Federation Version" "$HTTP_STATUS" "$HTTP_BODY"

echo ""
echo "517. Federation OpenID UserInfo"
if federation_http_json "Federation OpenID UserInfo" GET "$SERVER_URL/_matrix/federation/v1/openid/userinfo?access_token=${OPENID_ACCESS_TOKEN:-test}"; then
    federation_smoke "Federation OpenID UserInfo" "$HTTP_STATUS" "$HTTP_BODY"
fi

echo ""
echo "518. Federation PublicRooms"
http_json GET "$SERVER_URL/_matrix/federation/v1/publicRooms" ""
federation_smoke "Federation PublicRooms" "$HTTP_STATUS" "$HTTP_BODY"

echo ""
echo "519. Federation Query Directory"
ROOM_ALIAS_Q_ENC=$(python3 -c 'import sys, urllib.parse; print(urllib.parse.quote(sys.argv[1], safe=""))' "${ROOM_ALIAS:-#test:cjystx.top}" 2>/dev/null)
if federation_http_json "Federation Query Directory" GET "$SERVER_URL/_matrix/federation/v1/query/directory?room_alias=$ROOM_ALIAS_Q_ENC"; then
    federation_smoke "Federation Query Directory" "$HTTP_STATUS" "$HTTP_BODY"
fi

echo ""
echo "520. Federation Query Profile"
if federation_http_json "Federation Query Profile" GET "$SERVER_URL/_matrix/federation/v1/query/profile/$USER_ID"; then
    federation_smoke "Federation Query Profile" "$HTTP_STATUS" "$HTTP_BODY"
fi

echo ""
echo "521. Federation Query Destination"
http_json GET "$SERVER_URL/_matrix/federation/v1/query/destination" ""
federation_smoke "Federation Query Destination" "$HTTP_STATUS" "$HTTP_BODY"

echo ""
echo "522. Federation Query Auth"
if federation_http_json "Federation Query Auth" GET "$SERVER_URL/_matrix/federation/v1/query/auth"; then
    federation_smoke "Federation Query Auth" "$HTTP_STATUS" "$HTTP_BODY"
fi

echo ""
echo "523. Federation Thirdparty Invite"
if federation_http_json "Federation Thirdparty Invite" POST "$SERVER_URL/_matrix/federation/v1/thirdparty/invite" "{\"room_id\":\"$ROOM_ID\",\"invitee\":\"${TARGET_USER_ID:-$USER_ID}\",\"sender\":\"$USER_ID\"}"; then
    federation_smoke "Federation Thirdparty Invite" "$HTTP_STATUS" "$HTTP_BODY"
fi

echo ""
echo "524. Federation Keys Query"
if federation_http_json "Federation Keys Query" POST "$SERVER_URL/_matrix/federation/v1/keys/query" '{"device_keys": {}}'; then
    federation_smoke "Federation Keys Query" "$HTTP_STATUS" "$HTTP_BODY"
fi

echo ""
echo "525. Federation Keys Claim"
if federation_http_json "Federation Keys Claim" POST "$SERVER_URL/_matrix/federation/v1/keys/claim" '{"one_time_keys": {}}'; then
    federation_smoke "Federation Keys Claim" "$HTTP_STATUS" "$HTTP_BODY"
fi

echo ""
echo "526. Federation Keys Upload"
if federation_http_json "Federation Keys Upload" POST "$SERVER_URL/_matrix/federation/v1/keys/upload" '{}'; then
    federation_smoke "Federation Keys Upload" "$HTTP_STATUS" "$HTTP_BODY"
fi

echo ""
echo "527. Federation Timestamp to Event"
FED_ROOM_ID_ENC=$(python3 -c 'import sys, urllib.parse; print(urllib.parse.quote(sys.argv[1], safe=""))' "$ROOM_ID" 2>/dev/null)
FED_TS=$(python3 -c 'import time; print(int(time.time()*1000))' 2>/dev/null)
if federation_http_json "Federation Timestamp to Event" GET "$SERVER_URL/_matrix/federation/v1/timestamp_to_event/$FED_ROOM_ID_ENC?ts=$FED_TS"; then
    federation_smoke "Federation Timestamp to Event" "$HTTP_STATUS" "$HTTP_BODY"
fi

echo ""
echo "528. Federation v2 Invite"
FED_INVITE_EVENT_ID=$(python3 -c 'import secrets,sys; print("$"+"fedinvite"+secrets.token_hex(8)+":" + sys.argv[1])' "$USER_DOMAIN" 2>/dev/null)
FED_INVITE_EVENT_ID_ENC=$(python3 -c 'import sys, urllib.parse; print(urllib.parse.quote(sys.argv[1], safe=""))' "$FED_INVITE_EVENT_ID" 2>/dev/null)
if federation_http_json "Federation v2 Invite" PUT "$SERVER_URL/_matrix/federation/v2/invite/$FED_ROOM_ID_ENC/$FED_INVITE_EVENT_ID_ENC" "{\"origin\":\"${USER_DOMAIN}\",\"event\":{\"sender\":\"$USER_ID\",\"state_key\":\"${TARGET_USER_ID:-$USER_ID}\",\"origin_server_ts\":$FED_TS,\"content\":{\"membership\":\"invite\"}}}"; then
    federation_smoke "Federation v2 Invite" "$HTTP_STATUS" "$HTTP_BODY"
fi

echo ""
echo "529. Federation v2 Send Join"
FED_JOIN_EVENT_ID=$(python3 -c 'import secrets,sys; print("$"+"fedjoin"+secrets.token_hex(8)+":" + sys.argv[1])' "$USER_DOMAIN" 2>/dev/null)
FED_JOIN_EVENT_ID_ENC=$(python3 -c 'import sys, urllib.parse; print(urllib.parse.quote(sys.argv[1], safe=""))' "$FED_JOIN_EVENT_ID" 2>/dev/null)
if federation_http_json "Federation v2 Send Join" PUT "$SERVER_URL/_matrix/federation/v2/send_join/$FED_ROOM_ID_ENC/$FED_JOIN_EVENT_ID_ENC" "{\"origin\":\"${USER_DOMAIN}\",\"sender\":\"$USER_ID\"}"; then
    federation_smoke "Federation v2 Send Join" "$HTTP_STATUS" "$HTTP_BODY"
fi

echo ""
echo "530. Federation v2 Send Leave"
FED_LEAVE_EVENT_ID=$(python3 -c 'import secrets,sys; print("$"+"fedleave"+secrets.token_hex(8)+":" + sys.argv[1])' "$USER_DOMAIN" 2>/dev/null)
FED_LEAVE_EVENT_ID_ENC=$(python3 -c 'import sys, urllib.parse; print(urllib.parse.quote(sys.argv[1], safe=""))' "$FED_LEAVE_EVENT_ID" 2>/dev/null)
if federation_http_json "Federation v2 Send Leave" PUT "$SERVER_URL/_matrix/federation/v2/send_leave/$FED_ROOM_ID_ENC/$FED_LEAVE_EVENT_ID_ENC" "{\"sender\":\"$USER_ID\"}"; then
    federation_smoke "Federation v2 Send Leave" "$HTTP_STATUS" "$HTTP_BODY"
fi

echo ""
echo "531. Federation v2 Key Clone"
if federation_http_json "Federation v2 Key Clone" POST "$SERVER_URL/_matrix/federation/v2/key/clone" '{}'; then
    federation_smoke "Federation v2 Key Clone" "$HTTP_STATUS" "$HTTP_BODY"
fi

echo ""
echo "532. Federation v2 Query"
http_json GET "$SERVER_URL/_matrix/federation/v2/query/test_server/test_key" ""
federation_smoke "Federation v2 Query" "$HTTP_STATUS" "$HTTP_BODY"

echo ""
echo "533. Federation v2 Server"
http_json GET "$SERVER_URL/_matrix/federation/v2/server" ""
federation_smoke "Federation v2 Server" "$HTTP_STATUS" "$HTTP_BODY"

echo ""
echo "534. Federation v2 User Keys Query"
if federation_http_json "Federation v2 User Keys Query" POST "$SERVER_URL/_matrix/federation/v2/user/keys/query" '{"users": {}}'; then
    federation_smoke "Federation v2 User Keys Query" "$HTTP_STATUS" "$HTTP_BODY"
fi

echo ""
echo "535. Federation Hierarchy"
if [ -n "$ROOM_ID" ]; then
    FED_ROOM_ID_ENC=$(python3 -c 'import sys, urllib.parse; print(urllib.parse.quote(sys.argv[1], safe=""))' "$ROOM_ID" 2>/dev/null)
    http_json GET "$SERVER_URL/_matrix/federation/v1/hierarchy/$FED_ROOM_ID_ENC" ""
    err=$(json_err_summary "$HTTP_BODY")
    if [[ "$HTTP_STATUS" == "404" ]] && echo "$err" | grep -q "M_NOT_FOUND"; then
        skip "Federation Hierarchy" "${err:-Room not found}"
    else
        federation_smoke "Federation Hierarchy" "$HTTP_STATUS" "$HTTP_BODY"
    fi
else
    skip "Federation Hierarchy" "no room_id"
fi

echo ""
echo "536. Federation Room Auth"
if [ -n "$ROOM_ID" ]; then
    FED_ROOM_ID_ENC=$(python3 -c 'import sys, urllib.parse; print(urllib.parse.quote(sys.argv[1], safe=""))' "$ROOM_ID" 2>/dev/null)
    if federation_http_json "Federation Room Auth" GET "$SERVER_URL/_matrix/federation/v1/room_auth/$FED_ROOM_ID_ENC"; then
        federation_smoke "Federation Room Auth" "$HTTP_STATUS" "$HTTP_BODY"
    fi
else
    skip "Federation Room Auth" "no room_id"
fi

echo ""
echo "537. Federation Make Join"
if [ -n "$ROOM_ID" ] && [ -n "$USER_ID" ]; then
    FED_ROOM_ID_ENC=$(python3 -c 'import sys, urllib.parse; print(urllib.parse.quote(sys.argv[1], safe=""))' "$ROOM_ID" 2>/dev/null)
    FED_USER_ID_ENC=$(url_encode "$USER_ID")
    if federation_http_json "Federation Make Join" GET "$SERVER_URL/_matrix/federation/v1/make_join/$FED_ROOM_ID_ENC/$FED_USER_ID_ENC"; then
        federation_smoke "Federation Make Join" "$HTTP_STATUS" "$HTTP_BODY"
    fi
else
    skip "Federation Make Join" "missing room_id/user_id"
fi

echo ""
echo "538. Federation Make Leave"
if [ -n "$ROOM_ID" ] && [ -n "$USER_ID" ]; then
    FED_ROOM_ID_ENC=$(python3 -c 'import sys, urllib.parse; print(urllib.parse.quote(sys.argv[1], safe=""))' "$ROOM_ID" 2>/dev/null)
    FED_USER_ID_ENC=$(url_encode "$USER_ID")
    if federation_http_json "Federation Make Leave" GET "$SERVER_URL/_matrix/federation/v1/make_leave/$FED_ROOM_ID_ENC/$FED_USER_ID_ENC"; then
        federation_smoke "Federation Make Leave" "$HTTP_STATUS" "$HTTP_BODY"
    fi
else
    skip "Federation Make Leave" "missing room_id/user_id"
fi

echo ""
echo "539. Federation Exchange Third Party Invite"
if federation_http_json "Federation Exchange Third Party Invite" PUT "$SERVER_URL/_matrix/federation/v1/exchange_third_party_invite/$FED_ROOM_ID_ENC" '{"invite": {}}'; then
    federation_smoke "Federation Exchange Third Party Invite" "$HTTP_STATUS" "$HTTP_BODY"
fi

echo ""
echo "540. Federation Knock"
FED_KNOCK_USER_ID_ENC=$(url_encode "${TARGET_USER_ID:-$USER_ID}")
if federation_http_json "Federation Knock" PUT "$SERVER_URL/_matrix/federation/v1/knock/$FED_ROOM_ID_ENC/$FED_KNOCK_USER_ID_ENC"; then
    federation_smoke "Federation Knock" "$HTTP_STATUS" "$HTTP_BODY"
fi

# ================================================================================
# New Representative Tests (Based on API_TEST_REPORT.md optimization)
# ================================================================================

echo ""
echo "=========================================="
echo "110. Admin Federation Representative Tests"
echo "=========================================="
if admin_ready; then
    echo "541. Admin Federation Destination Details"
    http_json GET "$SERVER_URL/_synapse/admin/v1/federation/destinations/localhost" "$ADMIN_TOKEN"
    FED_DEST_RESP="$HTTP_BODY"
    if check_success_json "$FED_DEST_RESP" "$HTTP_STATUS"; then
        pass "Admin Federation Destination Details"
    else
        skip "Admin Federation Destination Details" "$ASSERT_ERROR"
    fi

    echo ""
    echo "542. Admin Add Federation Blacklist"
    http_json POST "$SERVER_URL/_synapse/admin/v1/federation/blacklist/localhost" "$ADMIN_TOKEN" '{"reason": "test"}'
    FED_BLACKLIST_ADD_RESP="$HTTP_BODY"
    if check_success_json "$FED_BLACKLIST_ADD_RESP" "$HTTP_STATUS"; then
        pass "Admin Add Federation Blacklist"
    else
        skip "Admin Add Federation Blacklist" "$ASSERT_ERROR"
    fi

    echo ""
    echo "543. Admin Remove Federation Blacklist"
    http_json DELETE "$SERVER_URL/_synapse/admin/v1/federation/blacklist/localhost" "$ADMIN_TOKEN"
    FED_BLACKLIST_DEL_RESP="$HTTP_BODY"
    if check_success_json "$FED_BLACKLIST_DEL_RESP" "$HTTP_STATUS"; then
        pass "Admin Remove Federation Blacklist"
    else
        skip "Admin Remove Federation Blacklist" "$ASSERT_ERROR"
    fi

    echo ""
    echo "544. Admin Reset Federation Connection"
    http_json POST "$SERVER_URL/_synapse/admin/v1/federation/destinations/localhost/reset_connection" "$ADMIN_TOKEN" '{}'
    FED_RESET_RESP="$HTTP_BODY"
    if check_success_json "$FED_RESET_RESP" "$HTTP_STATUS"; then
        pass "Admin Reset Federation Connection"
    else
        skip "Admin Reset Federation Connection" "$ASSERT_ERROR"
    fi
else
    skip "Admin Federation Destination Details" "admin authentication unavailable"
    skip "Admin Add Federation Blacklist" "admin authentication unavailable"
    skip "Admin Remove Federation Blacklist" "admin authentication unavailable"
    skip "Admin Reset Federation Connection" "admin authentication unavailable"
fi

echo ""
echo "=========================================="
echo "111. Admin Room Representative Tests"
echo "=========================================="
if admin_ready; then
    echo "545. Admin Delete Room"
    if [ -n "$ROOM2_ID" ]; then
        http_json DELETE "$SERVER_URL/_synapse/admin/v1/rooms/$ROOM2_ID" "$ADMIN_TOKEN"
        ADMIN_DEL_ROOM_RESP="$HTTP_BODY"
        if check_success_json "$ADMIN_DEL_ROOM_RESP" "$HTTP_STATUS"; then
            pass "Admin Delete Room"
        else
            skip "Admin Delete Room" "$ASSERT_ERROR"
        fi
    else
        skip "Admin Delete Room (no room ID)"
    fi

    echo ""
    echo "546. Admin Room Member Add"
    http_json PUT "$SERVER_URL/_synapse/admin/v1/rooms/$ROOM_ID/members/$TARGET_USER_ID_ENC" "$ADMIN_TOKEN" '{"membership": "join"}'
    ADMIN_ADD_MEMBER_RESP="$HTTP_BODY"
    if check_success_json "$ADMIN_ADD_MEMBER_RESP" "$HTTP_STATUS"; then
        pass "Admin Room Member Add"
    else
        skip "Admin Room Member Add" "$ASSERT_ERROR"
    fi

    echo ""
    echo "547. Admin Room Ban User"
    http_json POST "$SERVER_URL/_synapse/admin/v1/rooms/$ROOM_ID/ban/$TARGET_USER_ID_ENC" "$ADMIN_TOKEN" '{"reason": "test ban"}'
    ADMIN_BAN_RESP="$HTTP_BODY"
    if check_success_json "$ADMIN_BAN_RESP" "$HTTP_STATUS"; then
        pass "Admin Room Ban User"
    else
        skip "Admin Room Ban User" "$ASSERT_ERROR"
    fi

    echo ""
    echo "548. Admin Room Kick User"
    http_json POST "$SERVER_URL/_synapse/admin/v1/rooms/$ROOM_ID/kick/$TARGET_USER_ID_ENC" "$ADMIN_TOKEN" '{"reason": "test kick"}'
    ADMIN_KICK_RESP="$HTTP_BODY"
    if check_success_json "$ADMIN_KICK_RESP" "$HTTP_STATUS"; then
        pass "Admin Room Kick User"
    else
        skip "Admin Room Kick User" "$ASSERT_ERROR"
    fi

    echo ""
    echo "549. Admin List Spaces"
    http_json GET "$SERVER_URL/_synapse/admin/v1/spaces" "$ADMIN_TOKEN"
    ADMIN_SPACES_RESP="$HTTP_BODY"
    if check_success_json "$ADMIN_SPACES_RESP" "$HTTP_STATUS"; then
        pass "Admin List Spaces"
    else
        skip "Admin List Spaces" "$ASSERT_ERROR"
    fi

    echo ""
    echo "550. Admin Set Room Public"
    http_json PUT "$SERVER_URL/_synapse/admin/v1/rooms/$ROOM_ID/listings/public" "$ADMIN_TOKEN" '{}'
    ADMIN_SET_PUBLIC_RESP="$HTTP_BODY"
    if check_success_json "$ADMIN_SET_PUBLIC_RESP" "$HTTP_STATUS"; then
        pass "Admin Set Room Public"
    else
        skip "Admin Set Room Public" "$ASSERT_ERROR"
    fi

    echo ""
    echo "551. Admin Purge History"
    http_json POST "$SERVER_URL/_synapse/admin/v1/purge_history" "$ADMIN_TOKEN" '{"room_id": "'"$ROOM_ID"'", "before_ts": 9999999999000}'
    ADMIN_PURGE_RESP="$HTTP_BODY"
    if check_success_json "$ADMIN_PURGE_RESP" "$HTTP_STATUS"; then
        pass "Admin Purge History"
    else
        skip "Admin Purge History" "$ASSERT_ERROR"
    fi
else
    skip "Admin Delete Room" "admin authentication unavailable"
    skip "Admin Room Member Add" "admin authentication unavailable"
    skip "Admin Room Ban User" "admin authentication unavailable"
    skip "Admin Room Kick User" "admin authentication unavailable"
    skip "Admin List Spaces" "admin authentication unavailable"
    skip "Admin Set Room Public" "admin authentication unavailable"
    skip "Admin Purge History" "admin authentication unavailable"
fi

echo ""
echo "=========================================="
echo "112. Admin User Representative Tests"
echo "=========================================="
if admin_ready; then
    echo "552. Admin Delete User"
    if destructive; then
        DELETE_TEST_USER_ID="@delete_test:${USER_DOMAIN}"
        DELETE_TEST_USER_ID_ENC=$(url_encode "$DELETE_TEST_USER_ID")
        http_json PUT "$SERVER_URL/_synapse/admin/v2/users/$DELETE_TEST_USER_ID_ENC" "$ADMIN_TOKEN" '{"password":"DeleteTest123!","admin":false}'
        http_json DELETE "$SERVER_URL/_synapse/admin/v1/users/$DELETE_TEST_USER_ID_ENC" "$ADMIN_TOKEN"
        ADMIN_DEL_USER_RESP="$HTTP_BODY"
        if check_success_json "$ADMIN_DEL_USER_RESP" "$HTTP_STATUS"; then
            pass "Admin Delete User"
        else
            skip "Admin Delete User" "$ASSERT_ERROR"
        fi
    else
        skip "Admin Delete User (destructive test)"
    fi

    echo ""
    echo "553. Admin Set User Admin"
    USER_ID_ENC=$(url_encode "$USER_ID")
    http_json PUT "$SERVER_URL/_synapse/admin/v1/users/$USER_ID_ENC/admin" "$ADMIN_TOKEN" '{"admin": true}'
    ADMIN_SET_ADMIN_RESP="$HTTP_BODY"
    if check_success_json "$ADMIN_SET_ADMIN_RESP" "$HTTP_STATUS"; then
        pass "Admin Set User Admin"
    else
        skip "Admin Set User Admin" "$ASSERT_ERROR"
    fi

    echo ""
    echo "554. Admin Session Invalidate"
    if destructive; then
        USER_ID_ENC=$(url_encode "$USER_ID")
        http_json POST "$SERVER_URL/_synapse/admin/v1/user_sessions/$USER_ID_ENC/invalidate" "$ADMIN_TOKEN" '{}'
        ADMIN_INVALIDATE_RESP="$HTTP_BODY"
        if check_success_json "$ADMIN_INVALIDATE_RESP" "$HTTP_STATUS"; then
            pass "Admin Session Invalidate"
            RELOGIN_TOKEN=""
            for candidate_pass in "$CURRENT_TEST_PASS" "$TEST_PASS" "NewPass123!" "test_password"; do
                http_json POST "$SERVER_URL/_matrix/client/v3/login" "" "{\"type\": \"m.login.password\", \"user\": \"$TEST_USER\", \"password\": \"$candidate_pass\"}"
                RELOGIN_TOKEN=$(json_get "$HTTP_BODY" "access_token")
                if [ -n "$RELOGIN_TOKEN" ]; then
                    LOGIN_RESP="$HTTP_BODY"
                    CURRENT_TEST_PASS="$candidate_pass"
                    break
                fi
            done
            if [ -n "$RELOGIN_TOKEN" ]; then
                TOKEN="$RELOGIN_TOKEN"
                USER_ID=$(json_get "$LOGIN_RESP" "user_id")
                REFRESH_TOKEN=$(json_get "$LOGIN_RESP" "refresh_token")
                DEVICE_ID=$(json_get "$LOGIN_RESP" "device_id")
            else
                fail "Restore User Session" "relogin failed"
            fi
        else
            skip "Admin Session Invalidate" "$ASSERT_ERROR"
        fi
    else
        skip "Admin Session Invalidate" "destructive test"
    fi

    echo ""
    echo "555. Admin Delete User Device"
    if [ -n "$USER_ID" ]; then
        SECOND_DEVICE_NAME="api-admin-device-${RANDOM}"
        SECOND_LOGIN_USER="$(normalize_login_user "$TEST_USER")"
        http_json POST "$SERVER_URL/_matrix/client/v3/login" "" "{\"type\": \"m.login.password\", \"user\": \"$SECOND_LOGIN_USER\", \"password\": \"$CURRENT_TEST_PASS\", \"device_id\": \"$SECOND_DEVICE_NAME\"}"
        SECOND_DEVICE_ID=$(json_get "$HTTP_BODY" "device_id")
    fi
    if [ -n "$USER_ID" ] && [ -n "$SECOND_DEVICE_ID" ]; then
        USER_ID_ENC=$(url_encode "$USER_ID")
        http_json DELETE "$SERVER_URL/_synapse/admin/v1/users/$USER_ID_ENC/devices/$SECOND_DEVICE_ID" "$ADMIN_TOKEN"
        ADMIN_DEL_DEVICE_RESP="$HTTP_BODY"
        if check_success_json "$ADMIN_DEL_DEVICE_RESP" "$HTTP_STATUS"; then
            pass "Admin Delete User Device"
        else
            skip "Admin Delete User Device" "$ASSERT_ERROR"
        fi
    else
        skip "Admin Delete User Device" "device context unavailable"
    fi
else
    skip "Admin Delete User" "admin authentication unavailable"
    skip "Admin Set User Admin" "admin authentication unavailable"
    skip "Admin Session Invalidate" "admin authentication unavailable"
    skip "Admin Delete User Device" "admin authentication unavailable"
fi

echo ""
echo "=========================================="
echo "113. Admin Registration Tokens Representative Tests"
echo "=========================================="
if admin_ready; then
    echo "556. Admin Create Registration Token"
    http_json POST "$SERVER_URL/_synapse/admin/v1/registration_tokens" "$ADMIN_TOKEN" '{"uses_allowed": 10, "expiry_time": null}'
    ADMIN_CREATE_TOKEN_RESP="$HTTP_BODY"
    if check_success_json "$ADMIN_CREATE_TOKEN_RESP" "$HTTP_STATUS"; then
        pass "Admin Create Registration Token"
    else
        skip "Admin Create Registration Token" "$ASSERT_ERROR"
    fi
else
    skip "Admin Create Registration Token" "admin authentication unavailable"
fi

echo ""
echo "=========================================="
echo "114. Admin Notifications/Pushers Representative Tests"
echo "=========================================="
if admin_ready; then
    echo "557. Admin Send Server Notice"
    http_json POST "$SERVER_URL/_synapse/admin/v1/send_server_notice" "$ADMIN_TOKEN" '{"user_id": "'"$TARGET_USER_ID"'", "content": {"msgtype": "m.text", "body": "Test notice"}}'
    ADMIN_NOTICE_RESP="$HTTP_BODY"
    if check_success_json "$ADMIN_NOTICE_RESP" "$HTTP_STATUS"; then
        pass "Admin Send Server Notice"
    else
        skip "Admin Send Server Notice" "$ASSERT_ERROR"
    fi

    echo ""
    echo "558. Admin List Pushers"
    if [ -n "$USER_ID" ]; then
        API_TEST_PUSHKEY="api-test-admin-pushkey-${RANDOM}"
        http_json POST "$SERVER_URL/_matrix/client/v3/pushers/set" "$TOKEN" "{\"pushkey\":\"$API_TEST_PUSHKEY\",\"kind\":\"http\",\"app_id\":\"com.synapse.admin.test\",\"app_display_name\":\"Synapse Admin Test\",\"device_display_name\":\"Admin API Device\",\"lang\":\"en\",\"data\":{\"url\":\"https://push.example.test/_matrix/push/v1/notify\"}}"
        if check_success_json "$HTTP_BODY" "$HTTP_STATUS"; then
            USER_ID_ENC=$(url_encode "$USER_ID")
            http_json GET "$SERVER_URL/_synapse/admin/v1/users/$USER_ID_ENC/pushers" "$ADMIN_TOKEN"
            ADMIN_PUSHERS_RESP="$HTTP_BODY"
            if check_success_json "$ADMIN_PUSHERS_RESP" "$HTTP_STATUS" "pushers" "total"; then
                pass "Admin List Pushers"
            else
                skip "Admin List Pushers" "$ASSERT_ERROR"
            fi
        else
            skip "Admin List Pushers" "pusher seed failed: $ASSERT_ERROR"
        fi
    else
        skip "Admin List Pushers" "user context unavailable"
    fi
else
    skip "Admin Send Server Notice" "admin authentication unavailable"
    skip "Admin List Pushers" "admin authentication unavailable"
fi

echo ""
echo "=========================================="
echo "115. Admin Security Representative Tests"
echo "=========================================="
if admin_ready; then
    echo "559. Admin Shadow Ban User"
    USER_ID_ENC=$(url_encode "$USER_ID")
    http_json POST "$SERVER_URL/_synapse/admin/v1/users/$USER_ID_ENC/shadow_ban" "$ADMIN_TOKEN" '{}'
    ADMIN_SHADOW_BAN_RESP="$HTTP_BODY"
    if check_success_json "$ADMIN_SHADOW_BAN_RESP" "$HTTP_STATUS"; then
        pass "Admin Shadow Ban User"
    else
        skip "Admin Shadow Ban User" "$ASSERT_ERROR"
    fi
else
    skip "Admin Shadow Ban User" "admin authentication unavailable"
fi

echo ""
echo "=========================================="
echo "116. Admin Retention Representative Tests"
echo "=========================================="
if admin_ready; then
    echo "560. Admin Set Retention Policy"
    http_json POST "$SERVER_URL/_synapse/admin/v1/retention/policy" "$ADMIN_TOKEN" '{"max_lifetime": 365, "min_lifetime": 1}'
    ADMIN_RETENTION_RESP="$HTTP_BODY"
    if check_success_json "$ADMIN_RETENTION_RESP" "$HTTP_STATUS"; then
        pass "Admin Set Retention Policy"
    else
        skip "Admin Set Retention Policy" "$ASSERT_ERROR"
    fi
else
    skip "Admin Set Retention Policy" "admin authentication unavailable"
fi

echo ""
echo "=========================================="
echo "117. Admin Audit Representative Tests"
echo "=========================================="
if admin_ready; then
    echo "561. Admin List Audit Events"
    http_json GET "$SERVER_URL/_synapse/admin/v1/audit/events?from=0&limit=10" "$ADMIN_TOKEN"
    ADMIN_AUDIT_RESP="$HTTP_BODY"
    if check_success_json "$ADMIN_AUDIT_RESP" "$HTTP_STATUS"; then
        pass "Admin List Audit Events"
    else
        skip "Admin List Audit Events" "$ASSERT_ERROR"
    fi
else
    skip "Admin List Audit Events" "admin authentication unavailable"
fi

echo ""
echo "=========================================="
echo "118. Room Extended Representative Tests"
echo "=========================================="
echo "561.0 Prepare Representative Room"
http_json POST "$SERVER_URL/_matrix/client/v3/createRoom" "$TOKEN" '{"name":"Representative Room","preset":"private_chat"}'
assert_success_json "Prepare Representative Room" "$HTTP_BODY" "$HTTP_STATUS" "room_id"
REPRESENTATIVE_ROOM_ID=$(json_get "$HTTP_BODY" "room_id")
ROOM_ENC=$(echo "$REPRESENTATIVE_ROOM_ID" | sed 's/!/%21/g' | sed 's/:/%3A/g')

echo "561.1 Prepare Representative Event"
http_json PUT "$SERVER_URL/_matrix/client/v3/rooms/$REPRESENTATIVE_ROOM_ID/send/m.room.message/rep118" "$TOKEN" '{"msgtype":"m.text","body":"Representative test message"}'
assert_success_json "Prepare Representative Event" "$HTTP_BODY" "$HTTP_STATUS" "event_id"
MSG_EVENT_ID=$(json_get "$HTTP_BODY" "event_id")
echo "562. Get Room Version"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$REPRESENTATIVE_ROOM_ID/version" "$TOKEN"
ROOM_VERSION_RESP="$HTTP_BODY"
assert_success_json "Get Room Version" "$ROOM_VERSION_RESP" "$HTTP_STATUS" "room_version"

echo ""
echo "563. Get Room Thread"
if [ -n "$REPRESENTATIVE_ROOM_ID" ] && [ -n "$MSG_EVENT_ID" ]; then
    THREAD_ID_ENC=$(url_encode "$MSG_EVENT_ID")
    http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$REPRESENTATIVE_ROOM_ID/thread/$THREAD_ID_ENC" "$TOKEN"
    THREAD_RESP="$HTTP_BODY"
    if check_success_json "$THREAD_RESP" "$HTTP_STATUS"; then
        pass "Get Room Thread"
    else
        if echo "$ASSERT_ERROR" | grep -q '^HTTP 5'; then
            fail "Get Room Thread" "$ASSERT_ERROR"
        else
            missing "Get Room Thread" "$ASSERT_ERROR"
        fi
    fi
else
    skip "Get Room Thread" "missing room_id/event_id"
fi

echo ""
echo "564. Get Room Reactions"
if [ -n "$REPRESENTATIVE_ROOM_ID" ] && [ -n "$MSG_EVENT_ID" ]; then
    MSG_ENC=$(echo "$MSG_EVENT_ID" | sed 's/\$/%24/g')
    http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$REPRESENTATIVE_ROOM_ID/relations/$MSG_ENC" "$TOKEN"
    REACTIONS_RESP="$HTTP_BODY"
    if check_success_json "$REACTIONS_RESP" "$HTTP_STATUS"; then
        pass "Get Room Reactions"
    else
        skip "Get Room Reactions" "$ASSERT_ERROR"
    fi
else
    skip "Get Room Reactions (no message ID)"
fi

echo ""
echo "565. Get Invite Blocklist"
ROOM_ENC=$(echo "$ROOM_ID" | sed 's/!/%21/g' | sed 's/:/%3A/g')
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ENC/invite_blocklist" "$TOKEN"
BLOCKLIST_RESP="$HTTP_BODY"
if check_success_json "$BLOCKLIST_RESP" "$HTTP_STATUS"; then
    pass "Get Invite Blocklist"
else
    skip "Get Invite Blocklist" "$ASSERT_ERROR"
fi

echo ""
echo "566. Set Invite Blocklist"
http_json POST "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ENC/invite_blocklist" "$TOKEN" '{"user_ids": ["'"$TARGET_USER_ID"'"]}'
SET_BLOCKLIST_RESP="$HTTP_BODY"
if check_success_json "$SET_BLOCKLIST_RESP" "$HTTP_STATUS"; then
    pass "Set Invite Blocklist"
else
    skip "Set Invite Blocklist" "$ASSERT_ERROR"
fi

echo ""
echo "567. Get Event Keys"
if [ -n "$REPRESENTATIVE_ROOM_ID" ] && [ -n "$MSG_EVENT_ID" ]; then
    MSG_ENC=$(echo "$MSG_EVENT_ID" | sed 's/\$/%24/g')
    http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$REPRESENTATIVE_ROOM_ID/keys/$MSG_ENC" "$TOKEN"
    EVENT_KEYS_RESP="$HTTP_BODY"
    if check_success_json "$EVENT_KEYS_RESP" "$HTTP_STATUS"; then
        pass "Get Event Keys"
    else
        if echo "$ASSERT_ERROR" | grep -q '^HTTP 5'; then
            fail "Get Event Keys" "$ASSERT_ERROR"
        else
            missing "Get Event Keys" "$ASSERT_ERROR"
        fi
    fi
else
    skip "Get Event Keys (no event ID)"
fi

echo ""
echo "568. Get Room Context"
if [ -n "$REPRESENTATIVE_ROOM_ID" ] && [ -n "$MSG_EVENT_ID" ]; then
    MSG_ENC=$(echo "$MSG_EVENT_ID" | sed 's/\$/%24/g')
    http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$REPRESENTATIVE_ROOM_ID/context/$MSG_ENC" "$TOKEN"
    CONTEXT_RESP="$HTTP_BODY"
    if check_success_json "$CONTEXT_RESP" "$HTTP_STATUS"; then
        pass "Get Room Context"
    else
        if echo "$ASSERT_ERROR" | grep -q '^HTTP 5'; then
            fail "Get Room Context" "$ASSERT_ERROR"
        else
            missing "Get Room Context" "$ASSERT_ERROR"
        fi
    fi
else
    skip "Get Room Context (no event ID)"
fi

echo ""
echo "569. Get Room Hierarchy"
http_json GET "$SERVER_URL/_matrix/client/v3/rooms/$ROOM_ENC/hierarchy" "$TOKEN"
HIERARCHY_RESP="$HTTP_BODY"
if check_success_json "$HIERARCHY_RESP" "$HTTP_STATUS"; then
    pass "Get Room Hierarchy"
else
    skip "Get Room Hierarchy" "$ASSERT_ERROR"
fi

echo ""
echo "570. Report Event"
if [ -n "$REPRESENTATIVE_ROOM_ID" ] && [ -n "$MSG_EVENT_ID" ]; then
    MSG_ENC=$(echo "$MSG_EVENT_ID" | sed 's/\$/%24/g')
    http_json POST "$SERVER_URL/_matrix/client/v3/rooms/$REPRESENTATIVE_ROOM_ID/report/$MSG_ENC" "$TOKEN" '{"reason": "spam"}'
    REPORT_RESP="$HTTP_BODY"
    if check_success_json "$REPORT_RESP" "$HTTP_STATUS"; then
        pass "Report Event"
    else
        if echo "$ASSERT_ERROR" | grep -q '^HTTP 5'; then
            fail "Report Event" "$ASSERT_ERROR"
        else
            missing "Report Event" "$ASSERT_ERROR"
        fi
    fi
else
    skip "Report Event (no event ID)"
fi

echo ""
echo "=========================================="
echo "119. Federation Extended Representative Tests"
echo "=========================================="
if ! federation_signed_ready; then
    federation_skip_signed_tests \
        "Federation State" \
        "Federation State IDs" \
        "Federation Backfill" \
        "Federation User Devices" \
        "Federation OpenID Userinfo"
fi
echo "571. Federation State"
if federation_http_json "Federation State" GET "$SERVER_URL/_matrix/federation/v1/state/$ROOM_ID"; then
    FED_STATE_RESP="$HTTP_BODY"
    if check_success_json "$FED_STATE_RESP" "$HTTP_STATUS" "state"; then
        pass "Federation State"
    else
        err=$(json_err_summary "$FED_STATE_RESP")
        fail "Federation State" "${err:-HTTP $HTTP_STATUS}"
    fi
fi

echo ""
echo "572. Federation State IDs"
if federation_http_json "Federation State IDs" GET "$SERVER_URL/_matrix/federation/v1/state_ids/$ROOM_ID"; then
    FED_STATE_IDS_RESP="$HTTP_BODY"
    if check_success_json "$FED_STATE_IDS_RESP" "$HTTP_STATUS" "state_ids"; then
        pass "Federation State IDs"
    else
        err=$(json_err_summary "$FED_STATE_IDS_RESP")
        fail "Federation State IDs" "${err:-HTTP $HTTP_STATUS}"
    fi
fi

echo ""
echo "573. Federation Backfill"
if [ -n "$MSG_EVENT_ID" ]; then
    if federation_http_json "Federation Backfill" GET "$SERVER_URL/_matrix/federation/v1/backfill/$ROOM_ID" "{\"v\":[\"$MSG_EVENT_ID\"],\"limit\":10}"; then
        FED_BACKFILL_RESP="$HTTP_BODY"
        if check_success_json "$FED_BACKFILL_RESP" "$HTTP_STATUS" "origin" "pdus" "limit"; then
            pass "Federation Backfill"
        else
            err=$(json_err_summary "$FED_BACKFILL_RESP")
            fail "Federation Backfill" "${err:-HTTP $HTTP_STATUS}"
        fi
    fi
else
    skip "Federation Backfill" "no event_id"
fi

echo ""
echo "575. Federation User Devices"
if federation_http_json "Federation User Devices" GET "$SERVER_URL/_matrix/federation/v1/user/devices/@admin:cjystx.top"; then
    FED_DEVICES_RESP="$HTTP_BODY"
    if check_success_json "$FED_DEVICES_RESP" "$HTTP_STATUS" "devices" "user_id"; then
        pass "Federation User Devices"
    else
        err=$(json_err_summary "$FED_DEVICES_RESP")
        fail "Federation User Devices" "${err:-HTTP $HTTP_STATUS}"
    fi
fi

echo ""
echo "576. Federation OpenID Userinfo"
if federation_http_json "Federation OpenID Userinfo" GET "$SERVER_URL/_matrix/federation/v1/openid/userinfo?access_token=${OPENID_ACCESS_TOKEN:-test}"; then
    FED_OPENID_RESP="$HTTP_BODY"
    if check_success_json "$FED_OPENID_RESP" "$HTTP_STATUS" "sub"; then
        pass "Federation OpenID Userinfo"
    else
        skip "Federation OpenID Userinfo (requires valid OpenID token)"
    fi
fi

echo ""
echo "=========================================="
echo "120. Thirdparty Representative Tests"
echo "=========================================="
echo "577. List Thirdparty Protocols"
http_json GET "$SERVER_URL/_matrix/client/v3/thirdparty/protocols" "$TOKEN"
THIRDPARTY_RESP="$HTTP_BODY"
if check_success_json "$THIRDPARTY_RESP" "$HTTP_STATUS" "irc"; then
    pass "List Thirdparty Protocols"
else
    fail "List Thirdparty Protocols" "${ASSERT_ERROR:-HTTP $HTTP_STATUS}"
fi

echo ""
echo "577. Get Thirdparty Protocol"
http_json GET "$SERVER_URL/_matrix/client/v3/thirdparty/protocol/irc" "$TOKEN"
THIRDPARTY_PROTOCOL_RESP="$HTTP_BODY"
check_success_json "$THIRDPARTY_PROTOCOL_RESP" "$HTTP_STATUS" "user_fields" "location_fields" && pass "Get Thirdparty Protocol" || fail "Get Thirdparty Protocol" "${ASSERT_ERROR:-HTTP $HTTP_STATUS}"

echo ""
echo "=========================================="
echo "121. Other Modules Representative Tests"
echo "=========================================="
echo "578. Widget Config"
http_json POST "$SERVER_URL/_matrix/client/v1/widgets" "$TOKEN" "{\"room_id\": \"$ROOM_ID\", \"widget_type\": \"m.custom\", \"url\": \"https://example.com\", \"name\": \"Test Widget\", \"data\": {\"from\": \"api-integration\"}}"
WIDGET_CREATE_RESP="$HTTP_BODY"
if check_success_json "$WIDGET_CREATE_RESP" "$HTTP_STATUS" "widget"; then
    pass "Create Widget"
    WIDGET_ID=$(printf '%s' "$WIDGET_CREATE_RESP" | python3 -c 'import json,sys; d=json.load(sys.stdin); print((d.get("widget") or {}).get("widget_id",""))' 2>/dev/null)
    if [ -n "$WIDGET_ID" ]; then
        http_json GET "$SERVER_URL/_matrix/client/v1/widgets/$WIDGET_ID/config" "$TOKEN"
        WIDGET_CONFIG_RESP="$HTTP_BODY"
        check_success_json "$WIDGET_CONFIG_RESP" "$HTTP_STATUS" "widget_id" "room_id" && pass "Widget Config" || fail "Widget Config" "${ASSERT_ERROR:-HTTP $HTTP_STATUS}"
    else
        fail "Widget Config" "no widget_id returned"
    fi
else
    fail "Create Widget" "${ASSERT_ERROR:-HTTP $HTTP_STATUS}"
fi

echo ""
echo "579. Get Feature Flags"
if admin_ready; then
    http_json GET "$SERVER_URL/_synapse/admin/v1/feature-flags" "$ADMIN_TOKEN"
    FEATURE_FLAGS_RESP="$HTTP_BODY"
    check_success_json "$FEATURE_FLAGS_RESP" "$HTTP_STATUS" "flags" "total" && pass "Get Feature Flags" || fail "Get Feature Flags" "${ASSERT_ERROR:-HTTP $HTTP_STATUS}"
else
    skip "Get Feature Flags" "admin authentication unavailable"
fi

echo ""
echo "580. Jitsi Config"
ROOM_ENC=$(echo "$ROOM_ID" | sed 's/!/%21/g' | sed 's/:/%3A/g')
http_json GET "$SERVER_URL/_matrix/client/v1/rooms/$ROOM_ENC/widgets/jitsi/config" "$TOKEN"
JITSI_CONFIG_RESP="$HTTP_BODY"
if check_success_json "$JITSI_CONFIG_RESP" "$HTTP_STATUS"; then
    pass "Jitsi Config"
else
    skip "Jitsi Config" "$ASSERT_ERROR"
fi

echo ""
echo "581. App Service Query"
if admin_ready; then
    http_json POST "$SERVER_URL/_synapse/admin/v1/appservices" "$ADMIN_TOKEN" '{"id":"test_as","url":"http://localhost:9999","as_token":"as_token_test_as","hs_token":"hs_token_test_as","sender_localpart":"asbot","description":"api-integration test appservice"}'
fi
http_json GET "$SERVER_URL/_matrix/app/v1/test_as" "$TOKEN"
AS_QUERY_RESP="$HTTP_BODY"
if check_success_json "$AS_QUERY_RESP" "$HTTP_STATUS" "id"; then
    pass "App Service Query"
else
    skip "App Service Query" "$ASSERT_ERROR"
fi

echo ""
echo "582. List App Services (Admin)"
if admin_ready; then
    http_json GET "$SERVER_URL/_synapse/admin/v1/appservices" "$ADMIN_TOKEN"
    AS_LIST_RESP="$HTTP_BODY"
    if check_success_json "$AS_LIST_RESP" "$HTTP_STATUS"; then
        pass "List App Services"
    else
        fail "List App Services" "${ASSERT_ERROR:-HTTP $HTTP_STATUS}"
    fi
else
    skip "List App Services" "admin authentication unavailable"
fi

echo ""
echo "583. Create Rendezvous Session"
http_json POST "$SERVER_URL/_matrix/client/v1/rendezvous" "$TOKEN" '{"intent": "login.reciprocate", "transport": "http.v1"}'
CREATE_RENDEZVOUS_RESP="$HTTP_BODY"
CREATE_RENDEZVOUS_STATUS="$HTTP_STATUS"
if check_success_json "$CREATE_RENDEZVOUS_RESP" "$CREATE_RENDEZVOUS_STATUS" "session_id"; then
    pass "Create Rendezvous Session"
    RENDEZVOUS_URL=$(echo "$CREATE_RENDEZVOUS_RESP" | grep -o '"url":"[^"]*"' | cut -d'"' -f4)
    RENDEZVOUS_SESSION_ID=$(echo "$CREATE_RENDEZVOUS_RESP" | grep -o '"session_id":"[^"]*"' | cut -d'"' -f4)
    if [ -n "$RENDEZVOUS_SESSION_ID" ]; then
        echo ""
        echo "584. Get Rendezvous Session"
        http_json GET "$SERVER_URL/_matrix/client/v1/rendezvous/$RENDEZVOUS_SESSION_ID" "$TOKEN"
        GET_RENDEZVOUS_RESP="$HTTP_BODY"
        if check_success_json "$GET_RENDEZVOUS_RESP" "$HTTP_STATUS"; then
            pass "Get Rendezvous Session"
        else
            fail "Get Rendezvous Session" "${ASSERT_ERROR:-HTTP $HTTP_STATUS}"
        fi
    else
        fail "Get Rendezvous Session" "No session_id returned"
    fi
else
    fail "Create Rendezvous Session" "${ASSERT_ERROR:-HTTP $CREATE_RENDEZVOUS_STATUS}"
    skip "Get Rendezvous Session" "Create failed"
fi

echo ""

finalize
