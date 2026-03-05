#!/bin/bash
set -euo pipefail

BASE_URL="${BASE_URL:-http://localhost:8008}"
ADMIN_USER="${ADMIN_USER:-admin}"
ADMIN_PASSWORD="${ADMIN_PASSWORD:-Admin@123}"
TEST_USER="${TEST_USER:-testuser1}"
TEST_PASSWORD="${TEST_PASSWORD:-Test@123}"
ROOM_ID="${ROOM_ID:-!Kob8lgucASQ7dZvnmdjxAxBH:cjystx.top}"

pass=0
fail=0
total=0

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "[FATAL] missing command: $1"
    exit 1
  }
}

require_cmd curl
require_cmd jq

health_code=$(curl -s -o /dev/null -w "%{http_code}" "${BASE_URL}/health" || true)
if [[ "$health_code" != "200" ]]; then
  echo "[FATAL] server health check failed: ${BASE_URL}/health -> ${health_code}"
  exit 2
fi

login() {
  local user="$1"
  local password="$2"
  curl -sS -X POST "${BASE_URL}/_matrix/client/r0/login" \
    -H "Content-Type: application/json" \
    -d "{\"type\":\"m.login.password\",\"user\":\"${user}\",\"password\":\"${password}\"}"
}

ADMIN_TOKEN=$(login "$ADMIN_USER" "$ADMIN_PASSWORD" | jq -r '.access_token // empty')
TEST_TOKEN=$(login "$TEST_USER" "$TEST_PASSWORD" | jq -r '.access_token // empty')

if [[ -z "$ADMIN_TOKEN" || -z "$TEST_TOKEN" ]]; then
  echo "[FATAL] failed to get tokens"
  exit 3
fi

check_endpoint() {
  local name="$1"
  local method="$2"
  local endpoint="$3"
  local token="$4"
  local body="${5:-}"
  local expected_mode="${6:-not500}"
  total=$((total + 1))
  local response
  local code
  if [[ "$method" == "GET" ]]; then
    response=$(curl -sS -w "\n%{http_code}" -H "Authorization: Bearer ${token}" "${BASE_URL}${endpoint}" || true)
  elif [[ "$method" == "POST" ]]; then
    response=$(curl -sS -w "\n%{http_code}" -X POST -H "Authorization: Bearer ${token}" -H "Content-Type: application/json" -d "${body}" "${BASE_URL}${endpoint}" || true)
  else
    echo "[FAIL] ${name}: unsupported method ${method}"
    fail=$((fail + 1))
    return
  fi
  code=$(echo "$response" | tail -n1)
  local ok=0
  if [[ "$expected_mode" == "2xx" ]]; then
    [[ "$code" =~ ^2[0-9][0-9]$ ]] && ok=1
  else
    [[ "$code" != "500" && "$code" != "000" ]] && ok=1
  fi
  if [[ "$ok" -eq 1 ]]; then
    echo "[PASS] ${name}: ${code}"
    pass=$((pass + 1))
  else
    echo "[FAIL] ${name}: ${code}"
    fail=$((fail + 1))
  fi
}

APP_SERVICE_ID="as_regression_$(date +%s)"
APP_SERVICE_TOKEN="as_token_$(date +%s)"
HS_SERVICE_TOKEN="hs_token_$(date +%s)"
REG_TOKEN="reg_$(date +%s)"

check_endpoint "application_services list" "GET" "/_synapse/admin/v1/application_services" "$ADMIN_TOKEN" "" "not500"
check_endpoint "application_services create" "POST" "/_synapse/admin/v1/application_services" "$ADMIN_TOKEN" "{\"as_id\":\"${APP_SERVICE_ID}\",\"name\":\"Regression AS\",\"url\":\"http://127.0.0.1:9999\",\"as_token\":\"${APP_SERVICE_TOKEN}\",\"hs_token\":\"${HS_SERVICE_TOKEN}\",\"sender\":\"@as-bot:cjystx.top\"}" "not500"
check_endpoint "background_updates cleanup_locks" "POST" "/_synapse/admin/v1/background_updates/cleanup_locks" "$ADMIN_TOKEN" "{}" "not500"
check_endpoint "room summary members" "GET" "/_matrix/client/v3/rooms/${ROOM_ID}/summary/members" "$TEST_TOKEN" "" "not500"
check_endpoint "room summary stats" "GET" "/_matrix/client/v3/rooms/${ROOM_ID}/summary/stats" "$TEST_TOKEN" "" "2xx"
check_endpoint "registration_tokens create" "POST" "/_synapse/admin/v1/registration_tokens" "$ADMIN_TOKEN" "{\"token\":\"${REG_TOKEN}\",\"uses_allowed\":1,\"expiry_time\":4102444800000,\"token_type\":\"single_use\"}" "not500"

echo "=========="
echo "total: ${total}"
echo "pass: ${pass}"
echo "fail: ${fail}"
echo "=========="

if [[ "$fail" -gt 0 ]]; then
  exit 10
fi
