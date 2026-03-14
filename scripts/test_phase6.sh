#!/bin/bash

BASE_URL="http://localhost:8008"

echo "=============================================="
echo "Phase 6 API 测试 (模块 26-30)"
echo "=============================================="

echo -e "\n[1] 注册新用户..."
REGISTER=$(curl -s -X POST "$BASE_URL/_matrix/client/v3/register" \
  -H "Content-Type: application/json" \
  -d '{"username":"ph6test1","password":"Test@123456","device_id":"PH6_TEST1"}')

TOKEN=$(echo "$REGISTER" | grep -o '"access_token":"[^"]*"' | sed 's/"access_token":"//;s/"//')

if [ -z "$TOKEN" ]; then
  echo "[错误] 无法获取Token"
  exit 1
fi

echo "[成功] Token: ${TOKEN:0:30}..."

TOTAL=0
PASSED=0
FAILED=0

test_api() {
  local name=$1
  local method=$2
  local endpoint=$3
  local data=$4
  local expected=${5:-200}

  TOTAL=$((TOTAL + 1))

  if [ "$method" = "GET" ]; then
    response=$(curl -s -w "\n%{http_code}" -H "Authorization: Bearer $TOKEN" "$BASE_URL$endpoint")
  elif [ "$method" = "POST" ]; then
    response=$(curl -s -w "\n%{http_code}" -X POST -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" -d "$data" "$BASE_URL$endpoint")
  elif [ "$method" = "PUT" ]; then
    response=$(curl -s -w "\n%{http_code}" -X PUT -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" -d "$data" "$BASE_URL$endpoint")
  elif [ "$method" = "DELETE" ]; then
    response=$(curl -s -w "\n%{http_code}" -X DELETE -H "Authorization: Bearer $TOKEN" "$BASE_URL$endpoint")
  fi

  http_code=$(echo "$response" | tail -n1)

  if [ "$http_code" -eq "$expected" ] || ([ "$http_code" -ge 200 ] && [ "$http_code" -lt 300 ]); then
    echo "[PASS] $name (HTTP $http_code)"
    PASSED=$((PASSED + 1))
    return 0
  else
    echo "[FAIL] $name (HTTP $http_code, expected $expected)"
    FAILED=$((FAILED + 1))
    return 1
  fi
}

echo -e "\n===== 模块26: 注册令牌 API ====="
test_api "获取令牌列表" "GET" "/_synapse/admin/v1/registration_tokens" "" "200"

echo -e "\n===== 模块27: 媒体配额 API ====="
test_api "检查配额" "GET" "/_matrix/media/v1/quota/check" "" "200"
test_api "获取统计" "GET" "/_matrix/media/v1/quota/stats" "" "200"

echo -e "\n===== 模块28: CAS 认证 API ====="
test_api "CAS登录" "GET" "/_matrix/client/r0/auth/cas/login" "" "200"

echo -e "\n===== 模块29: SAML 认证 API ====="
test_api "SAML重定向" "GET" "/_matrix/client/r0/login/sso/redirect/saml" "" "200"
test_api "SAML元数据" "GET" "/_matrix/client/r0/saml/metadata" "" "200"

echo -e "\n===== 模块30: OIDC 认证 API ====="
test_api "OIDC发现" "GET" "/.well-known/openid-configuration" "" "200"
test_api "OIDC用户信息" "GET" "/_matrix/client/r0/oidc/userinfo" "" "401"

echo -e "\n=============================================="
echo "测试统计 (模块 26-30)"
echo "=============================================="
echo "总计: $TOTAL"
echo "通过: $PASSED"
echo "失败: $FAILED"
if [ $TOTAL -gt 0 ]; then
  echo "通过率: $(( PASSED * 100 / TOTAL ))%"
fi
echo "=============================================="

echo -e "\n[清理] 登出..."
curl -s -X POST "$BASE_URL/_matrix/client/v3/logout" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "{\"access_token\":\"$TOKEN\"}" > /dev/null
