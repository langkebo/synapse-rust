#!/bin/bash
# 移除 set -e 以允许所有角色运行测试，即使有失败

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# 清理旧结果
RESULTS_BASE_DIR="$SCRIPT_DIR/test-results-matrix"
rm -rf "$RESULTS_BASE_DIR"
mkdir -p "$RESULTS_BASE_DIR"

# 定义角色
ROLES="super_admin admin user"

export SERVER_URL="http://localhost:28008"
export TEST_ENV="dev"

for ROLE in $ROLES; do
    echo "------------------------------------------"
    echo "Running tests for role: $ROLE"
    echo "------------------------------------------"
    
    case $ROLE in
        super_admin)
            USER="admin"
            PASS="Admin@123"
            ;;
        admin)
            USER="testuser1"
            PASS="Test@123"
            ;;
        user)
            USER="testuser2"
            PASS="Test@123"
            ;;
    esac
    
    # 重要：将 ADMIN_USER 和 ADMIN_PASS 设置为当前角色的凭证
    # 这样集成测试脚本中的 login_admin 就会使用对应角色的账号
    export ADMIN_USER="$USER"
    export ADMIN_PASS="$PASS"
    
    # 创建角色专属目录
    ROLE_DIR="$RESULTS_BASE_DIR/$ROLE"
    mkdir -p "$ROLE_DIR"
    
    # 运行测试
    # 使用 API_INTEGRATION_PROFILE="core" 且跳过 federation 相关的挂起风险
    # 我们通过设置环境变量来控制
    TEST_ROLE="$ROLE" \
    TEST_USER="$USER" \
    TEST_PASS="$PASS" \
    API_INTEGRATION_PROFILE="core" \
    OUTPUT_DIR="$ROLE_DIR" \
    ./api-integration_test_optimized.sh
    
    echo "Tests for $ROLE completed."
done

echo ""
echo "Merging results for analyze_results.py..."
# 合并所有 roles 的 responses.jsonl 到一个主文件中，供 analyze_results.py 使用
COMBINED_LOG="$SCRIPT_DIR/test-results/api-integration.responses.jsonl"
mkdir -p "$SCRIPT_DIR/test-results"
rm -f "$COMBINED_LOG"

for ROLE in $ROLES; do
    if [ -f "$RESULTS_BASE_DIR/$ROLE/api-integration.responses.jsonl" ]; then
        cat "$RESULTS_BASE_DIR/$ROLE/api-integration.responses.jsonl" >> "$COMBINED_LOG"
    fi
done

echo "Generating permission matrix report..."
cd ../..
python3 analyze_results.py
