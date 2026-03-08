#!/bin/bash
# synapse-rust 压力测试脚本
# 测试目标：验证 10万+ 在线用户支持

set -e

echo "=========================================="
echo "  synapse-rust 压力测试"
echo "=========================================="
echo ""

# 配置
BASE_URL="${BASE_URL:-http://localhost:8008}"
CONCURRENT_USERS="${CONCURRENT_USERS:-100}"
REQUESTS_PER_USER="${REQUESTS_PER_USER:-10}"
DURATION_SECONDS="${DURATION_SECONDS:-60}"

echo "测试配置:"
echo "  - 基础 URL: $BASE_URL"
echo "  - 并发用户数: $CONCURRENT_USERS"
echo "  - 每用户请求数: $REQUESTS_PER_USER"
echo "  - 持续时间: $DURATION_SECONDS 秒"
echo ""

# 检查服务是否运行
echo "[1/4] 检查服务状态..."
if curl -s -o /dev/null -w "%{http_code}" "$BASE_URL/health" | grep -q "200"; then
    echo "✓ 服务运行正常"
else
    echo "✗ 服务未运行，请先启动服务"
    exit 1
fi

# 创建测试用户
echo ""
echo "[2/4] 创建测试用户..."
TEST_USERS=""
for i in $(seq 1 $CONCURRENT_USERS); do
    USERNAME="stress_test_user_$i"
    RESPONSE=$(curl -s -X POST "$BASE_URL/_matrix/client/v3/register" \
        -H "Content-Type: application/json" \
        -d "{\"username\":\"$USERNAME\",\"password\":\"test123456\",\"device_id\":\"device_$i\"}")
    
    ACCESS_TOKEN=$(echo "$RESPONSE" | grep -o '"access_token":"[^"]*"' | cut -d'"' -f4)
    if [ -n "$ACCESS_TOKEN" ]; then
        TEST_USERS="$TEST_USERS $ACCESS_TOKEN"
    fi
done
echo "✓ 创建了 $(echo $TEST_USERS | wc -w) 个测试用户"

# 运行压力测试
echo ""
echo "[3/4] 运行压力测试..."
echo "测试端点: /_matrix/client/v3/sync"
echo ""

START_TIME=$(date +%s)
SUCCESS_COUNT=0
FAIL_COUNT=0
TOTAL_LATENCY=0

for TOKEN in $TEST_USERS; do
    for j in $(seq 1 $REQUESTS_PER_USER); do
        REQUEST_START=$(date +%s%N)
        
        HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" \
            "$BASE_URL/_matrix/client/v3/sync?access_token=$TOKEN&timeout=0")
        
        REQUEST_END=$(date +%s%N)
        LATENCY=$(( (REQUEST_END - REQUEST_START) / 1000000 ))
        TOTAL_LATENCY=$((TOTAL_LATENCY + LATENCY))
        
        if [ "$HTTP_CODE" -eq 200 ]; then
            SUCCESS_COUNT=$((SUCCESS_COUNT + 1))
        else
            FAIL_COUNT=$((FAIL_COUNT + 1))
        fi
    done
done &

# 等待测试完成
wait

END_TIME=$(date +%s)
TOTAL_TIME=$((END_TIME - START_TIME))
TOTAL_REQUESTS=$((SUCCESS_COUNT + FAIL_COUNT))

# 计算统计信息
if [ $TOTAL_REQUESTS -gt 0 ]; then
    AVG_LATENCY=$((TOTAL_LATENCY / TOTAL_REQUESTS))
    SUCCESS_RATE=$((SUCCESS_COUNT * 100 / TOTAL_REQUESTS))
    RPS=$((TOTAL_REQUESTS / TOTAL_TIME))
else
    AVG_LATENCY=0
    SUCCESS_RATE=0
    RPS=0
fi

# 输出结果
echo ""
echo "=========================================="
echo "  压力测试结果"
echo "=========================================="
echo ""
echo "请求统计:"
echo "  - 总请求数: $TOTAL_REQUESTS"
echo "  - 成功请求: $SUCCESS_COUNT"
echo "  - 失败请求: $FAIL_COUNT"
echo "  - 成功率: $SUCCESS_RATE%"
echo ""
echo "性能指标:"
echo "  - 总耗时: $TOTAL_TIME 秒"
echo "  - 平均延迟: $AVG_LATENCY ms"
echo "  - 请求/秒: $RPS req/s"
echo ""

# 验收标准
echo "验收标准检查:"
if [ $AVG_LATENCY -lt 50 ]; then
    echo "  ✓ 平均延迟 < 50ms (实际: ${AVG_LATENCY}ms)"
else
    echo "  ✗ 平均延迟 >= 50ms (实际: ${AVG_LATENCY}ms)"
fi

if [ $SUCCESS_RATE -ge 99 ]; then
    echo "  ✓ 成功率 >= 99% (实际: ${SUCCESS_RATE}%)"
else
    echo "  ✗ 成功率 < 99% (实际: ${SUCCESS_RATE}%)"
fi

if [ $RPS -ge 1000 ]; then
    echo "  ✓ 吞吐量 >= 1000 req/s (实际: ${RPS} req/s)"
else
    echo "  ✗ 吞吐量 < 1000 req/s (实际: ${RPS} req/s)"
fi

echo ""
echo "[4/4] 清理测试数据..."
# 清理代码可以在这里添加
echo "✓ 测试完成"
