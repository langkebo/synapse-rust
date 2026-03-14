#!/bin/bash

# 综合检查工具 - 运行所有检查和测试
# 用于在部署前进行全面验证

set -e

echo "========================================"
echo "synapse-rust 综合检查工具"
echo "========================================"
echo ""

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# 检查结果
TOTAL_ERRORS=0
TOTAL_WARNINGS=0

# 运行检查函数
run_check() {
    local name=$1
    local script=$2
    
    echo -e "${BLUE}========================================${NC}"
    echo -e "${BLUE}运行: $name${NC}"
    echo -e "${BLUE}========================================${NC}"
    echo ""
    
    if [ -f "$script" ]; then
        chmod +x "$script"
        if bash "$script"; then
            echo -e "${GREEN}✓ $name 通过${NC}"
        else
            echo -e "${RED}✗ $name 失败${NC}"
            TOTAL_ERRORS=$((TOTAL_ERRORS + 1))
        fi
    else
        echo -e "${YELLOW}⚠ 脚本不存在: $script${NC}"
        TOTAL_WARNINGS=$((TOTAL_WARNINGS + 1))
    fi
    
    echo ""
}

# 主菜单
show_menu() {
    echo "请选择要运行的检查:"
    echo ""
    echo "1) 运行所有检查"
    echo "2) Schema 验证 (迁移前验证)"
    echo "3) Schema 同步检查 (代码与数据库一致性)"
    echo "4) 编译时字段名检查"
    echo "5) 编译检查 (cargo check)"
    echo "6) 单元测试 (cargo test)"
    echo "7) API 性能测试"
    echo "8) 退出"
    echo ""
    read -p "请输入选项 (1-8): " choice
}

# 运行所有检查
run_all_checks() {
    echo -e "${BLUE}开始运行所有检查...${NC}"
    echo ""
    
    run_check "Schema 验证" "scripts/schema_validator.sh"
    run_check "Schema 同步检查" "scripts/schema_sync_check.sh"
    run_check "编译时字段名检查" "scripts/compile_time_check.sh"
    
    echo -e "${BLUE}========================================${NC}"
    echo -e "${BLUE}运行编译检查${NC}"
    echo -e "${BLUE}========================================${NC}"
    echo ""
    
    if cargo check --release 2>&1 | tail -5; then
        echo -e "${GREEN}✓ 编译检查通过${NC}"
    else
        echo -e "${RED}✗ 编译检查失败${NC}"
        TOTAL_ERRORS=$((TOTAL_ERRORS + 1))
    fi
    
    echo ""
    
    echo -e "${BLUE}========================================${NC}"
    echo -e "${BLUE}运行单元测试${NC}"
    echo -e "${BLUE}========================================${NC}"
    echo ""
    
    if cargo test --release 2>&1 | tail -10; then
        echo -e "${GREEN}✓ 单元测试通过${NC}"
    else
        echo -e "${RED}✗ 单元测试失败${NC}"
        TOTAL_ERRORS=$((TOTAL_ERRORS + 1))
    fi
    
    show_summary
}

# 显示总结
show_summary() {
    echo ""
    echo "========================================"
    echo "检查总结"
    echo "========================================"
    echo -e "错误: ${RED}$TOTAL_ERRORS${NC}"
    echo -e "警告: ${YELLOW}$TOTAL_WARNINGS${NC}"
    echo ""
    
    if [ $TOTAL_ERRORS -gt 0 ]; then
        echo -e "${RED}✗ 检查失败，请修复上述错误${NC}"
        exit 1
    elif [ $TOTAL_WARNINGS -gt 0 ]; then
        echo -e "${YELLOW}⚠ 检查通过，但存在警告${NC}"
        exit 0
    else
        echo -e "${GREEN}✓ 所有检查通过${NC}"
        exit 0
    fi
}

# 主循环
main() {
    if [ "$1" == "--all" ]; then
        run_all_checks
        exit $?
    fi
    
    while true; do
        show_menu
        
        case $choice in
            1)
                run_all_checks
                ;;
            2)
                run_check "Schema 验证" "scripts/schema_validator.sh"
                ;;
            3)
                run_check "Schema 同步检查" "scripts/schema_sync_check.sh"
                ;;
            4)
                run_check "编译时字段名检查" "scripts/compile_time_check.sh"
                ;;
            5)
                echo -e "${BLUE}运行编译检查...${NC}"
                cargo check --release
                ;;
            6)
                echo -e "${BLUE}运行单元测试...${NC}"
                cargo test --release
                ;;
            7)
                run_check "API 性能测试" "scripts/performance_test.sh"
                ;;
            8)
                echo "退出"
                exit 0
                ;;
            *)
                echo -e "${RED}无效选项，请重新选择${NC}"
                ;;
        esac
        
        echo ""
        read -p "按 Enter 继续..."
    done
}

main "$@"
