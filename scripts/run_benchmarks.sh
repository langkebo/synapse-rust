#!/bin/bash
# synapse-rust 性能基准测试脚本

set -e

echo "=========================================="
echo "  synapse-rust 性能基准测试"
echo "=========================================="
echo ""

# 检查依赖
check_dependencies() {
    echo "[1/5] 检查依赖..."
    
    if ! command -v cargo &> /dev/null; then
        echo "错误: 未找到 cargo，请先安装 Rust"
        exit 1
    fi
    
    if ! command -v psql &> /dev/null; then
        echo "警告: 未找到 psql，数据库测试将跳过"
    fi
    
    if ! command -v redis-cli &> /dev/null; then
        echo "警告: 未找到 redis-cli，Redis 测试将跳过"
    fi
    
    echo "✓ 依赖检查完成"
}

# 运行基准测试
run_benchmarks() {
    echo ""
    echo "[2/5] 运行基准测试..."
    echo ""
    
    echo "--- API Criterion 基准 ---"
    cargo bench --bench performance_api_benchmarks || echo "API 基准测试失败"
    
    echo ""
    echo "--- Federation Criterion 基准 ---"
    cargo bench --bench performance_federation_benchmarks || echo "联邦基准测试失败"
}

# 运行单元测试
run_tests() {
    echo ""
    echo "[3/5] 运行单元测试..."
    echo ""
    
    cargo test --lib --release
    
    echo ""
    echo "✓ 单元测试完成"
}

# API 性能测试
api_benchmark() {
    echo ""
    echo "[4/5] API 性能测试..."
    echo ""
    
    # 启动服务器 (后台)
    cargo run --release &
    SERVER_PID=$!
    
    # 等待服务器启动
    echo "等待服务器启动..."
    sleep 10
    
    # 测试 API 响应时间
    echo "测试 API 响应时间..."
    
    # 版本 API
    time curl -s http://localhost:8008/_matrix/client/versions > /dev/null
    
    # 健康检查
    time curl -s http://localhost:8008/health > /dev/null
    
    # 停止服务器
    kill $SERVER_PID
    
    echo ""
    echo "✓ API 性能测试完成"
}

# 生成报告
generate_report() {
    echo ""
    echo "[5/5] 生成报告..."
    echo ""
    
    mkdir -p reports/benchmark
    
    # 收集基准测试结果
    echo "# synapse-rust 性能基准报告" > reports/benchmark/results.md
    echo "" >> reports/benchmark/results.md
    echo "生成时间: $(date)" >> reports/benchmark/results.md
    echo "" >> reports/benchmark/results.md
    
    # 添加基准测试结果
    if [ -d "benches/results" ]; then
        cp -r benches/results/* reports/benchmark/ 2>/dev/null || true
    fi
    
    echo "✓ 报告已生成: reports/benchmark/results.md"
}

# 主函数
main() {
    check_dependencies
    run_benchmarks
    run_tests
    api_benchmark
    generate_report
    
    echo ""
    echo "=========================================="
    echo "  基准测试完成!"
    echo "=========================================="
}

# 运行主函数
main
