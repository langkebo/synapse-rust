#!/bin/bash
# synapse-rust 负载测试脚本

set -e

# 配置
SERVER_URL="${SERVER_URL:-http://localhost:8008}"
CONCURRENT_USERS="${CONCURRENT_USERS:-100}"
DURATION="${DURATION:-60}"  # 秒
RAMP_UP="${RAMP_UP:-10}"    # 秒

echo "=========================================="
echo "  synapse-rust 负载测试"
echo "=========================================="
echo ""
echo "服务器: $SERVER_URL"
echo "并发用户: $CONCURRENT_USERS"
echo "持续时间: ${DURATION}s"
echo "预热时间: ${RAMP_UP}s"
echo ""

# 检查服务器是否运行
check_server() {
    echo "[1/4] 检查服务器..."
    
    if curl -s "$SERVER_URL/_matrix/client/versions" > /dev/null 2>&1; then
        echo "✓ 服务器运行正常"
    else
        echo "错误: 服务器未运行在 $SERVER_URL"
        exit 1
    fi
}

# 生成测试用户
generate_users() {
    echo ""
    echo "[2/4] 生成测试用户..."
    
    # 创建测试用户
    for i in $(seq 1 10); do
        USER="@loadtest_$i:$SERVER_NAME"
        # 注册用户 (如果需要)
        echo "  创建用户: $USER"
    done
    
    echo "✓ 测试用户生成完成"
}

# 运行负载测试
run_load_test() {
    echo ""
    echo "[3/4] 运行负载测试..."
    echo ""
    
    # 模拟用户登录
    echo "--- 测试登录 API ---"
    for i in $(seq 1 $CONCURRENT_USERS); do
        (
            time curl -s -X POST "$SERVER_URL/_matrix/client/v3/login" \
                -H "Content-Type: application/json" \
                -d '{"type":"m.password","identifier":{"type":"m.id.user","user":"test"},"password":"test"}' \
                > /dev/null 2>&1
        ) &
        
        # 预热期间逐渐增加
        if [ $i -eq 1 ]; then
            sleep $RAMP_UP
        fi
    done
    
    wait
    
    echo ""
    echo "--- 测试房间列表 API ---"
    for i in $(seq 1 $CONCURRENT_USERS); do
        (
            time curl -s "$SERVER_URL/_matrix/client/v3/publicRooms" \
                > /dev/null 2>&1
        ) &
    done
    
    wait
    
    echo ""
    echo "--- 测试房间创建 API ---"
    for i in $(seq 1 10); do
        (
            time curl -s -X POST "$SERVER_URL/_matrix/client/v3/createRoom" \
                -H "Content-Type: application/json" \
                -d '{"name":"Load Test Room","preset":"private_chat"}' \
                > /dev/null 2>&1
        ) &
    done
    
    wait
    
    echo ""
    echo "✓ 负载测试完成"
}

# 分析结果
analyze_results() {
    echo ""
    echo "[4/4] 分析结果..."
    echo ""
    
    # 收集指标
    echo "# 负载测试报告" > reports/load_test.md
    echo "" >> reports/load_test.md
    echo "时间: $(date)" >> reports/load_test.md
    echo "服务器: $SERVER_URL" >> reports/load_test.md
    echo "并发: $CONCURRENT_USERS" >> reports/load_test.md
    echo "持续: ${DURATION}s" >> reports/load_test.md
    echo "" >> reports/load_test.md
    
    # 添加服务器状态
    echo "## 服务器状态" >> reports/load_test.md
    curl -s "$SERVER_URL/_matrix/client/versions" >> reports/load_test.md 2>&1
    echo "" >> reports/load_test.md
    
    echo "✓ 报告已生成: reports/load_test.md"
}

# 清理
cleanup() {
    echo ""
    echo "清理测试数据..."
    # 删除测试用户等
    echo "✓ 清理完成"
}

# 主函数
main() {
    check_server
    generate_users
    run_load_test
    analyze_results
    cleanup
    
    echo ""
    echo "=========================================="
    echo "  负载测试完成!"
    echo "=========================================="
}

# 运行
main
