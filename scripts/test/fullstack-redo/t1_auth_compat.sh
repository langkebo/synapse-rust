#!/bin/bash
# t1_auth_compat.sh — T1 auth_compat module API tests.
# Covers: register, login, logout, logout/all, refresh, register/available,
#         register/email/requestToken, register/email/submitToken
set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

MODULE="auth_compat"
load_creds || exit 1

echo ""
echo "=========================================="
echo "T1 Module: $MODULE"
echo "=========================================="

# Unique suffix for new test accounts/credentials
TS=$(date +%s)
UNIQ="t1ac${TS}"

# Rate-limit pacing: /_matrix/client/v3/register* is rate-limited at 1 req/sec
# burst 3 (rate_limit.yaml). Insert a 1.2s sleep between register-family calls
# so we don't trip M_LIMIT_EXCEEDED.
rl_register_sleep() { sleep 1.2; }
rl_login_sleep() { sleep 0.25; }   # 5/sec burst 50
rl_refresh_sleep() { sleep 0.55; } # 2/sec burst 5
rl_email_sleep() { sleep 1.2; }    # 1/sec burst 3

# ---------------------------------------------------------------------------
# POST /_matrix/client/v3/register
# ---------------------------------------------------------------------------
# AUTHC-001: Normal registration
RESP=$(http POST "/_matrix/client/v3/register" none \
    '{"username":"ac_new_'"$UNIQ"'","password":"Test@1234","auth":{"type":"m.login.password"}}')
test_case "AUTHC-001" "正常注册新用户" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

rl_register_sleep
# AUTHC-002: Duplicate username (P-006 verification — should be 400 M_USER_IN_USE)
RESP=$(http POST "/_matrix/client/v3/register" none \
    '{"username":"e2etest1","password":"Test@1234","auth":{"type":"m.login.password"}}')
test_case "AUTHC-002" "重复用户名注册返回 400 M_USER_IN_USE (P-006)" "400" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

rl_register_sleep
# AUTHC-003: Password too short
RESP=$(http POST "/_matrix/client/v3/register" none \
    '{"username":"ac_short_'"$UNIQ"'","password":"x","auth":{"type":"m.login.password"}}')
test_case "AUTHC-003" "密码过短注册被拒" "400" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

rl_register_sleep
# AUTHC-004: Empty password
RESP=$(http POST "/_matrix/client/v3/register" none \
    '{"username":"ac_nopass_'"$UNIQ"'","password":"","auth":{"type":"m.login.password"}}')
test_case "AUTHC-004" "空密码注册被拒" "400" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

rl_register_sleep
# AUTHC-005: Missing both username/password — should return 401 with auth flows
RESP=$(http POST "/_matrix/client/v3/register" none '{}')
test_case "AUTHC-005" "无 username/password 触发 UIAuth 401" "401" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

rl_register_sleep
# AUTHC-006: Guest registration via ?kind=guest
RESP=$(http POST "/_matrix/client/v3/register?kind=guest" none '{}')
test_case "AUTHC-006" "kind=guest 注册访客账号" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

rl_register_sleep
# ---------------------------------------------------------------------------
# GET /_matrix/client/v3/register/available
# ---------------------------------------------------------------------------
# AUTHC-007: Available username
RESP=$(http GET "/_matrix/client/v3/register/available?username=ac_avail_$UNIQ" none "")
test_case "AUTHC-007" "查询可用用户名返回 available=true" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

rl_register_sleep
# AUTHC-008: Taken username (e2etest1)
RESP=$(http GET "/_matrix/client/v3/register/available?username=e2etest1" none "")
test_case "AUTHC-008" "查询已占用用户名返回 available=false" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

rl_register_sleep
# AUTHC-009: Missing username param
RESP=$(http GET "/_matrix/client/v3/register/available" none "")
test_case "AUTHC-009" "缺失 username 参数返回 400" "400" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# ---------------------------------------------------------------------------
# POST /_matrix/client/v3/login
# ---------------------------------------------------------------------------
# AUTHC-010: Successful password login
RESP=$(http POST "/_matrix/client/v3/login" none \
    '{"type":"m.login.password","identifier":{"type":"m.id.user","user":"e2etest1"},"password":"Test@1234"}')
test_case "AUTHC-010" "密码登录成功" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# AUTHC-011: Wrong password — should be 401 (P-007 verification)
RESP=$(http POST "/_matrix/client/v3/login" none \
    '{"type":"m.login.password","identifier":{"type":"m.id.user","user":"e2etest1"},"password":"WrongPass"}')
test_case "AUTHC-011" "错误密码登录返回 401 (P-007)" "401" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# AUTHC-012: Non-existent user
RESP=$(http POST "/_matrix/client/v3/login" none \
    '{"type":"m.login.password","identifier":{"type":"m.id.user","user":"does_not_exist_xyz"},"password":"Test@1234"}')
test_case "AUTHC-012" "不存在用户登录返回 401" "401" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# AUTHC-013: Empty body
RESP=$(http POST "/_matrix/client/v3/login" none '{}')
test_case "AUTHC-013" "空 body 登录返回 400" "400" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

rl_login_sleep
# AUTHC-014: m.login.token type (without valid token)
# Note: login flow advertises m.login.token but the handler only accepts
# username/password. The handler returns 400 "Username required" before
# ever inspecting the token. This is recorded as issue P-XXX.
RESP=$(http POST "/_matrix/client/v3/login" none \
    '{"type":"m.login.token","token":"invalid-token-xyz"}')
test_case "AUTHC-014" "m.login.token 无效 token 返回 4xx" "400" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"
if [ "$(echo "$RESP" | cut -f1)" != "403" ]; then
    record_issue "$MODULE" "High" "AUTHC-014" \
        "GET /login advertises m.login.token flow but POST /login handler requires username/password (returns 400 Username required for m.login.token request). m.login.token SSO/QR login flow cannot complete."
fi

# AUTHC-015: GET login flows
RESP=$(http GET "/_matrix/client/v3/login" none "")
test_case "AUTHC-015" "GET /login 返回登录流程列表" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# ---------------------------------------------------------------------------
# POST /_matrix/client/v3/logout
# ---------------------------------------------------------------------------
# AUTHC-016: Logout with valid token
LOGOUT_TOKEN=$(login_user "e2etest1" "Test@1234")
RESP=$(http POST "/_matrix/client/v3/logout" "$LOGOUT_TOKEN" '{}')
test_case "AUTHC-016" "有效 token 登出返回 200" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# AUTHC-017: Logout with invalid token
RESP=$(http POST "/_matrix/client/v3/logout" "invalid-token-xyz" '{}')
test_case "AUTHC-017" "无效 token 登出返回 401" "401" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# AUTHC-018: Logout already-logged-out token
RESP=$(http POST "/_matrix/client/v3/logout" "$LOGOUT_TOKEN" '{}')
test_case "AUTHC-018" "已登出 token 再次登出返回 401" "401" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# ---------------------------------------------------------------------------
# POST /_matrix/client/v3/logout/all
# ---------------------------------------------------------------------------
# AUTHC-019: logout/all with valid token
LOGOUT_ALL_TOKEN=$(login_user "e2etest1" "Test@1234")
RESP=$(http POST "/_matrix/client/v3/logout/all" "$LOGOUT_ALL_TOKEN" '{}')
test_case "AUTHC-019" "logout/all 有效 token 返回 200" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# AUTHC-020: logout/all with invalid token
RESP=$(http POST "/_matrix/client/v3/logout/all" "invalid-token-xyz" '{}')
test_case "AUTHC-020" "logout/all 无效 token 返回 401" "401" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# ---------------------------------------------------------------------------
# POST /_matrix/client/v3/refresh
# ---------------------------------------------------------------------------
# AUTHC-021: Refresh with valid refresh_token
# Login returns refresh_token in response
LOGIN_RESP=$(http POST "/_matrix/client/v3/login" none \
    '{"type":"m.login.password","identifier":{"type":"m.id.user","user":"e2etest1"},"password":"Test@1234"}')
REFRESH_TOKEN=$(echo "$LOGIN_RESP" | cut -f2- | python3 -c "import sys,json; print(json.load(sys.stdin).get('refresh_token',''))" 2>/dev/null)
RESP=$(http POST "/_matrix/client/v3/refresh" none \
    '{"refresh_token":"'"$REFRESH_TOKEN"'"}')
test_case "AUTHC-021" "有效 refresh_token 刷新返回 200" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

rl_refresh_sleep
# AUTHC-022: Refresh with invalid token
RESP=$(http POST "/_matrix/client/v3/refresh" none \
    '{"refresh_token":"invalid-refresh-token-xyz"}')
test_case "AUTHC-022" "无效 refresh_token 返回 4xx" "401" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

rl_refresh_sleep
# AUTHC-023: Refresh with empty body
RESP=$(http POST "/_matrix/client/v3/refresh" none '{}')
test_case "AUTHC-023" "缺失 refresh_token 参数返回 400" "400" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

rl_refresh_sleep
# AUTHC-024: Refresh with already-used refresh_token (rotation)
RESP=$(http POST "/_matrix/client/v3/refresh" none \
    '{"refresh_token":"'"$REFRESH_TOKEN"'"}')
test_case "AUTHC-024" "已使用的 refresh_token 二次刷新返回 401" "401" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

# ---------------------------------------------------------------------------
# POST /_matrix/client/v3/register/email/requestToken
# ---------------------------------------------------------------------------
# AUTHC-025: Request email verification token (smoke — backend may not actually send mail)
RESP=$(http POST "/_matrix/client/v3/register/email/requestToken" none \
    '{"email":"e2e-ac-'"$UNIQ"'@example.com","client_secret":"cs-'"$UNIQ"'","send_attempt":1}')
test_case "AUTHC-025" "请求邮箱验证 token 返回 200" "200" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"
SID=$(echo "$RESP" | cut -f2- | python3 -c "import sys,json; print(json.load(sys.stdin).get('sid',''))" 2>/dev/null)

rl_email_sleep
# AUTHC-026: Missing email
RESP=$(http POST "/_matrix/client/v3/register/email/requestToken" none \
    '{"client_secret":"cs-'"$UNIQ"'","send_attempt":1}')
test_case "AUTHC-026" "缺失 email 字段返回 400" "400" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

rl_email_sleep
# AUTHC-027: Invalid email format
RESP=$(http POST "/_matrix/client/v3/register/email/requestToken" none \
    '{"email":"not-an-email","client_secret":"cs-'"$UNIQ"'","send_attempt":1}')
test_case "AUTHC-027" "无效 email 格式返回 400" "400" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

rl_register_sleep
# ---------------------------------------------------------------------------
# POST /_matrix/client/v3/register/email/submitToken
# ---------------------------------------------------------------------------
# AUTHC-028: Submit with missing fields
RESP=$(http POST "/_matrix/client/v3/register/email/submitToken" none '{}')
test_case "AUTHC-028" "submitToken 缺失字段返回 400" "400" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

rl_register_sleep
# AUTHC-029: Submit with invalid sid
RESP=$(http POST "/_matrix/client/v3/register/email/submitToken" none \
    '{"sid":"invalid","client_secret":"cs-'"$UNIQ"'","token":"wrong-token"}')
test_case "AUTHC-029" "submitToken 无效 sid 返回 400" "400" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

rl_register_sleep
# AUTHC-030: Submit with valid sid but wrong token — should be 400 / M_INVALID_TOKEN
RESP=$(http POST "/_matrix/client/v3/register/email/submitToken" none \
    '{"sid":"'"$SID"'","client_secret":"cs-'"$UNIQ"'","token":"wrong-token"}')
test_case "AUTHC-030" "submitToken 错误 token 返回 4xx" "400" "$(echo "$RESP" | cut -f1)" "$(echo "$RESP" | cut -f2-)"

emit_summary
