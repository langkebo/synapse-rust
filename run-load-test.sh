#!/usr/bin/env bash
set -euo pipefail

echo "=== Phase 3 大规模负载测试 ==="
echo "开始时间：$(date -Iseconds)"
echo ""

export BASE_URL="http://127.0.0.1:8008"
export ADMIN_USER="admin"
# SEC-02：管理员口令不内置默认值（set -u 生效，缺失即退出并提示）。
: "${ADMIN_PASSWORD:?ADMIN_PASSWORD must be set (no default credential is shipped)}"
export ADMIN_PASSWORD

# 100 VUs, 持续 30 秒
mkdir -p load-test-results
/opt/homebrew/bin/k6 run --vus 100 --duration 30s \
    scripts/load-test/matrix-load-test.js 2>&1 | tee load-test-results/run-$(date +%s).log

echo ""
echo "测试完成时间：$(date -Iseconds)"
