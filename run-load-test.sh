#!/usr/bin/env bash
set -euo pipefail

echo "=== Phase 3 大规模负载测试 ==="
echo "开始时间：$(date -Iseconds)"
echo ""

export BASE_URL="http://127.0.0.1:8008"
export ADMIN_USER="admin"
export ADMIN_PASSWORD="Admin@123"

# 100 VUs, 持续 30 秒
mkdir -p load-test-results
/opt/homebrew/bin/k6 run --vus 100 --duration 30s \
    scripts/load-test/matrix-load-test.js 2>&1 | tee load-test-results/run-$(date +%s).log

echo ""
echo "测试完成时间：$(date -Iseconds)"
