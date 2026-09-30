#!/usr/bin/env bash
#
# 验证 Phase 4 安全加固
#
set -euo pipefail

BASE_URL="http://localhost:8081"
PRO_PASS="SecurePassword123ChangeMe!"  # 默认密码

echo "=== Phase 4 安全加固验证 ==="
echo ""

echo "1. 测试无认证访问（应返回 401）..."
CODE=$(curl -s -o /dev/null -w "%{http_code}" "${BASE_URL}/prometheus/")
if [[ "${CODE}" == "401" ]]; then
  echo "  ✅ Prometheus API 无认证访问被拒绝 (${CODE})"
else
  echo "  ❌ 预期 401，实际 ${CODE}"
fi

CODE=$(curl -s -o /dev/null -w "%{http_code}" "${BASE_URL}/grafana/")
if [[ "${CODE}" == "401" ]]; then
  echo "  ✅ Grafana 无认证访问被拒绝 (${CODE})"
else
  echo "  ❌ 预期 401，实际 ${CODE}"
fi

echo ""
echo "2. 测试有认证访问（应返回 200）..."
CODE=$(curl -s -o /dev/null -w "%{http_code}" -u "admin:${PRO_PASS}" "${BASE_URL}/prometheus/api/v1/query?query=up")
if [[ "${CODE}" == "200" ]]; then
  echo "  ✅ Prometheus API 认证访问正常 (${CODE})"
else
  echo "  ❌ 预期 200，实际 ${CODE}"
fi

CODE=$(curl -s -o /dev/null -w "%{http_code}" -u "admin:${PRO_PASS}" "${BASE_URL}/grafana/")
if [[ "${CODE}" == "200" ]] || [[ "${CODE}" == "302" ]]; then
  echo "  ✅ Grafana 认证访问正常 (${CODE})"
else
  echo "  ❌ 预期 200 或 302，实际 ${CODE}"
fi

echo ""
echo "3. 验证直接端口访问（应被限制在 127.0.0.1）..."
CODE=$(curl -s -o /dev/null -w "%{http_code}" "http://127.0.0.1:9092/")
if [[ "${CODE}" == "200" ]]; then
  echo "  ✅ Prometheus 回环端口 9092 可访问"
else
  echo "  ⚠️  Prometheus 回环端口不可用 (${CODE})"
fi

CODE=$(curl -s -o /dev/null -w "%{http_code}" "http://127.0.0.1:3000/")
if [[ "${CODE}" == "200" ]]; then
  echo "  ✅ Grafana 回环端口 3000 可访问"
else
  echo "  ⚠️  Grafana 回环端口不可用 (${CODE})"
fi

echo ""
echo "=== 验证完成 ==="
echo ""
echo "下一步:"
echo "1. 修改默认密码：./docker/deploy/nginx/generate_htpasswd.sh admin your_secure_password"
echo "2. 重启 nginx-proxy：docker compose -f docker/docker-compose.monitoring.yml up -d nginx-proxy"
echo "3. 生产环境配置 HTTPS（参考 docs/monitoring/nginx-security.md）"
