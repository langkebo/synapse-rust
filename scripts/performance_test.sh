#!/bin/bash

# API 性能测试脚本
# 用于测试 API 端点的响应时间和吞吐量

set -e

echo "========================================"
echo "API 性能测试脚本"
echo "========================================"
echo ""

# 配置
BASE_URL="${BASE_URL:-https://localhost}"
HOST="${HOST:-matrix.cjystx.top}"
TEST_USER="${TEST_USER:-testadmin2}"
TEST_PASS="${TEST_PASS:-Admin@123456}"
REQUESTS="${REQUESTS:-100}"
CONCURRENCY="${CONCURRENCY:-10}"

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# 结果存储
RESULTS_DIR="performance_results"
mkdir -p $RESULTS_DIR
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
RESULT_FILE="$RESULTS_DIR/performance_$TIMESTAMP.json"

# 获取 Token
echo "获取测试 Token..."
TOKEN=$(curl -sk -X POST "$BASE_URL/_matrix/client/v3/login" \
  -H "Host: $HOST" \
  -H "Content-Type: application/json" \
  -d "{\"type\":\"m.login.password\",\"user\":\"$TEST_USER\",\"password\":\"$TEST_PASS\"}" | jq -r '.access_token')

if [ -z "$TOKEN" ] || [ "$TOKEN" == "null" ]; then
    echo -e "${RED}✗ 获取 Token 失败${NC}"
    exit 1
fi

echo -e "${GREEN}✓ Token 获取成功${NC}"
echo ""

# 性能测试函数
test_endpoint() {
    local name=$1
    local method=$2
    local endpoint=$3
    local data=$4
    
    echo -e "${BLUE}测试: $name${NC}"
    
    local start_time=$(date +%s%N)
    local response
    local http_code
    
    if [ -z "$data" ]; then
        response=$(curl -sk -X $method "$BASE_URL$endpoint" \
          -H "Host: $HOST" \
          -H "Authorization: Bearer $TOKEN" \
          -w "\n%{http_code}" 2>/dev/null)
    else
        response=$(curl -sk -X $method "$BASE_URL$endpoint" \
          -H "Host: $HOST" \
          -H "Authorization: Bearer $TOKEN" \
          -H "Content-Type: application/json" \
          -d "$data" \
          -w "\n%{http_code}" 2>/dev/null)
    fi
    
    local end_time=$(date +%s%N)
    local duration=$(( ($end_time - $start_time) / 1000000 ))
    http_code=$(echo "$response" | tail -n 1)
    
    if [ "$http_code" -ge 200 ] && [ "$http_code" -lt 300 ]; then
        echo -e "  状态: ${GREEN}$http_code${NC}"
    else
        echo -e "  状态: ${RED}$http_code${NC}"
    fi
    
    echo -e "  耗时: ${YELLOW}${duration}ms${NC}"
    
    # 记录结果
    echo "{\"name\": \"$name\", \"endpoint\": \"$endpoint\", \"method\": \"$method\", \"duration_ms\": $duration, \"http_code\": $http_code}," >> $RESULT_FILE
    
    echo ""
}

# 批量性能测试
batch_test() {
    local name=$1
    local method=$2
    local endpoint=$3
    local data=$4
    
    echo -e "${BLUE}批量测试: $name (${REQUESTS}次, 并发${CONCURRENCY})${NC}"
    
    local total_time=0
    local success_count=0
    local fail_count=0
    local min_time=999999
    local max_time=0
    
    for i in $(seq 1 $REQUESTS); do
        local start_time=$(date +%s%N)
        local http_code
        
        if [ -z "$data" ]; then
            http_code=$(curl -sk -X $method "$BASE_URL$endpoint" \
              -H "Host: $HOST" \
              -H "Authorization: Bearer $TOKEN" \
              -o /dev/null -w "%{http_code}" 2>/dev/null)
        else
            http_code=$(curl -sk -X $method "$BASE_URL$endpoint" \
              -H "Host: $HOST" \
              -H "Authorization: Bearer $TOKEN" \
              -H "Content-Type: application/json" \
              -d "$data" \
              -o /dev/null -w "%{http_code}" 2>/dev/null)
        fi
        
        local end_time=$(date +%s%N)
        local duration=$(( ($end_time - $start_time) / 1000000 ))
        
        total_time=$((total_time + duration))
        
        if [ "$duration" -lt "$min_time" ]; then
            min_time=$duration
        fi
        
        if [ "$duration" -gt "$max_time" ]; then
            max_time=$duration
        fi
        
        if [ "$http_code" -ge 200 ] && [ "$http_code" -lt 300 ]; then
            success_count=$((success_count + 1))
        else
            fail_count=$((fail_count + 1))
        fi
    done
    
    local avg_time=$((total_time / REQUESTS))
    local success_rate=$((success_count * 100 / REQUESTS))
    
    echo -e "  成功率: ${GREEN}${success_rate}%${NC} ($success_count/$REQUESTS)"
    echo -e "  平均耗时: ${YELLOW}${avg_time}ms${NC}"
    echo -e "  最小耗时: ${GREEN}${min_time}ms${NC}"
    echo -e "  最大耗时: ${RED}${max_time}ms${NC}"
    
    # 记录结果
    echo "{\"name\": \"$name\", \"endpoint\": \"$endpoint\", \"method\": \"$method\", \"requests\": $REQUESTS, \"success_rate\": $success_rate, \"avg_ms\": $avg_time, \"min_ms\": $min_time, \"max_ms\": $max_time}," >> $RESULT_FILE
    
    echo ""
}

# 开始测试
echo "[" > $RESULT_FILE

echo "========================================"
echo "1. 基础服务 API 性能测试"
echo "========================================"
echo ""

test_endpoint "健康检查" "GET" "/health"
test_endpoint "客户端版本" "GET" "/_matrix/client/versions"
test_endpoint "服务器版本" "GET" "/_matrix/client/r0/version"

echo "========================================"
echo "2. 用户认证 API 性能测试"
echo "========================================"
echo ""

test_endpoint "获取当前用户" "GET" "/_matrix/client/v3/account/whoami"
test_endpoint "获取设备列表" "GET" "/_matrix/client/v3/devices"

echo "========================================"
echo "3. 房间管理 API 性能测试"
echo "========================================"
echo ""

test_endpoint "创建房间" "POST" "/_matrix/client/v3/createRoom" '{"name":"PerfTest","preset":"private_chat"}'

echo "========================================"
echo "4. 推送通知 API 性能测试"
echo "========================================"
echo ""

test_endpoint "获取推送器列表" "GET" "/_matrix/client/v3/pushers"
test_endpoint "获取推送规则" "GET" "/_matrix/client/v3/pushrules"

echo "========================================"
echo "5. 批量性能测试 (高负载)"
echo "========================================"
echo ""

batch_test "健康检查批量" "GET" "/health"
batch_test "设备列表批量" "GET" "/_matrix/client/v3/devices"

# 结束测试
echo "]" >> $RESULT_FILE

# 移除最后一个逗号
sed -i '' '$ s/,$//' $RESULT_FILE

echo "========================================"
echo "性能测试完成"
echo "========================================"
echo ""
echo "结果已保存到: $RESULT_FILE"
echo ""

# 生成报告
echo "生成性能报告..."

cat << EOF > $RESULTS_DIR/report_$TIMESTAMP.md
# API 性能测试报告

> 测试时间: $(date)
> 测试环境: $BASE_URL
> 请求数: $REQUESTS
> 并发数: $CONCURRENCY

## 测试结果

$(cat $RESULT_FILE | jq -r '.[] | "- **\(.name)**: \(.avg_ms // .duration_ms)ms (\(.success_rate // "N/A")% 成功率)"' 2>/dev/null || echo "请查看 JSON 文件获取详细结果")

## 建议

- 响应时间 < 100ms: 优秀
- 响应时间 100-500ms: 良好
- 响应时间 > 500ms: 需要优化
EOF

echo -e "${GREEN}✓ 性能报告已生成: $RESULTS_DIR/report_$TIMESTAMP.md${NC}"
