#!/usr/bin/env bash
#
# 验证 Phase 4 安全加固 (含 HTTPS)
#
set -euo pipefail

BASE_URL="http://localhost:8081"
HTTPS_URL="https://localhost:8443"
# 管理员密码：优先取环境变量，未设置时回退到本地默认值（勿用于生产环境）
PRO_PASS="${PRO_PASS:-SecurePassword123ChangeMe!}"

echo "=== Phase 4 安全加固验证 ==="
echo ""

echo "1. 测试无认证访问 HTTP(应返回 401 或 301)..."
CODE=$(curl -s -o /dev/null -w "%{http_code}" "${BASE_URL}/prometheus/")
if [[ "${CODE}" == "401" ]] || [[ "${CODE}" == "301" ]]; then
    echo "  ✅ HTTP 无认证访问被拒绝 (${CODE})"
else
    echo "  ❌ 预期 401/301，实际 ${CODE}"
fi

echo ""
echo "2. 测试 HTTPS 无认证访问 (应返回 401)..."
CODE=$(curl -sk -o /dev/null -w "%{http_code}" "${HTTPS_URL}/prometheus/")
if [[ "${CODE}" == "401" ]]; then
    echo "  ✅ HTTPS 无认证访问被拒绝 (${CODE})"
else
    echo "  ❌ 预期 401，实际 ${CODE}"
fi

CODE=$(curl -sk -o /dev/null -w "%{http_code}" "${HTTPS_URL}/grafana/")
if [[ "${CODE}" == "401" ]]; then
    echo "  ✅ Grafana 无认证访问被拒绝 (${CODE})"
else
    echo "  ❌ 预期 401，实际 ${CODE}"
fi

echo ""
echo "3. 测试 HTTPS 有认证访问 (应返回 200)..."
CODE=$(curl -sk -o /dev/null -w "%{http_code}" -u "admin:${PRO_PASS}" "${HTTPS_URL}/prometheus/api/v1/query?query=up")
if [[ "${CODE}" == "200" ]]; then
    echo "  ✅ Prometheus API 认证访问正常 (${CODE})"
else
    echo "  ❌ 预期 200，实际 ${CODE}"
fi

CODE=$(curl -sk -o /dev/null -w "%{http_code}" -u "admin:${PRO_PASS}" "${HTTPS_URL}/grafana/")
if [[ "${CODE}" == "200" ]] || [[ "${CODE}" == "302" ]]; then
    echo "  ✅ Grafana 认证访问正常 (${CODE})"
else
    echo "  ❌ 预期 200 或 302，实际 ${CODE}"
fi

echo ""
echo "4. 测试 HTTP 自动重定向 (应返回 301)..."
CODE=$(curl -s -o /dev/null -w "%{http_code}" "${BASE_URL}/nginx-health")
if [[ "${CODE}" == "200" ]]; then
    echo "  ✅ 健康检查绕过重定向 (${CODE})"
else
    echo "  ❌ 预期 200，实际 ${CODE}"
fi

CODE=$(curl -sI "${BASE_URL}/prometheus/" | head -1 | grep -o "301" || true)
if [[ "${CODE}" == "301" ]]; then
    echo "  ✅ HTTP 自动重定向到 HTTPS (301)"
else
    echo "  ⚠️  HTTP 未重定向"
fi

echo ""
echo "=== 验证完成 ==="
echo ""
echo "下一步:"
echo "1. 修改默认密码：./docker/deploy/nginx/generate_htpasswd.sh admin your_secure_password"
echo "2. 重启 nginx-proxy: docker compose -f docker/docker-compose.monitoring.yml up -d nginx-proxy"
echo "3. 生产环境部署 CA 证书：cp docker/deploy/nginx/certs/ca.crt /etc/ssl/certs/"
