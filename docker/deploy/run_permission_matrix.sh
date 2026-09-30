#!/bin/bash
# 权限矩阵回归：按角色（super_admin / admin / user）依次运行集成测试并汇总结果。
# 刻意不使用 set -e：单个角色失败后仍需跑完其余角色，最后统一判定退出码。

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

TEST_SCRIPT="$SCRIPT_DIR/api-integration_test.sh"
if [ ! -x "$TEST_SCRIPT" ]; then
    echo "ERROR: 未找到可执行的集成测试脚本: $TEST_SCRIPT" >&2
    exit 1
fi

# 清理旧结果
RESULTS_BASE_DIR="$SCRIPT_DIR/test-results-matrix"
rm -rf "$RESULTS_BASE_DIR"
mkdir -p "$RESULTS_BASE_DIR"

# 定义角色
ROLES="super_admin admin user"

export SERVER_URL="${SERVER_URL:-http://localhost:8008}"
export TEST_ENV="${TEST_ENV:-dev}"

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
        RESULTS_DIR="$ROLE_DIR" \
        "$TEST_SCRIPT"

    echo "Tests for $ROLE completed."
done

# 统计单个结果文件的行数（文件不存在或为空时输出 0）
count_lines() {
    local file="$1"
    if [ -f "$file" ]; then
        grep -c . "$file" 2>/dev/null || true
    else
        echo 0
    fi
}

echo ""
echo "=============== 权限矩阵汇总 ==============="
printf '%-14s %8s %8s %8s\n' "ROLE" "PASSED" "FAILED" "SKIPPED"
TOTAL_FAILED=0
for ROLE in $ROLES; do
    ROLE_DIR="$RESULTS_BASE_DIR/$ROLE"
    ROLE_PASSED=$(count_lines "$ROLE_DIR/api-integration.passed.txt")
    ROLE_FAILED=$(count_lines "$ROLE_DIR/api-integration.failed.txt")
    ROLE_SKIPPED=$(count_lines "$ROLE_DIR/api-integration.skipped.txt")
    printf '%-14s %8s %8s %8s\n' "$ROLE" "$ROLE_PASSED" "$ROLE_FAILED" "$ROLE_SKIPPED"
    TOTAL_FAILED=$((TOTAL_FAILED + ROLE_FAILED))
done
echo "============================================"
echo "结果目录: $RESULTS_BASE_DIR"

if [ "$TOTAL_FAILED" -gt 0 ]; then
    echo "权限矩阵回归失败: 共 ${TOTAL_FAILED} 个用例不通过"
    exit 1
fi
echo "权限矩阵回归通过"
