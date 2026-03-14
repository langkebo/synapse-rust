#!/bin/bash

# Schema 同步检查工具 - 检查代码与数据库一致性
# 用于检查 Rust 代码中的字段名与数据库表结构是否一致

set -e

echo "========================================"
echo "Schema 同步检查工具"
echo "========================================"
echo ""

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# 检查结果
ERRORS=0
WARNINGS=0

# 检查代码中是否使用了 created_at
check_created_at_in_code() {
    echo "检查代码中的 created_at 使用..."
    
    CREATED_AT_FILES=$(grep -r "created_at:" src/ --include="*.rs" 2>/dev/null | wc -l | tr -d ' ')
    
    if [ "$CREATED_AT_FILES" -gt 0 ]; then
        echo -e "${RED}✗ 发现 $CREATED_AT_FILES 处使用 created_at 字段，应使用 created_ts${NC}"
        grep -r "created_at:" src/ --include="*.rs" 2>/dev/null | head -5
        ERRORS=$((ERRORS + 1))
    else
        echo -e "${GREEN}✓ 代码中没有使用 created_at 字段${NC}"
    fi
}

# 检查代码中是否使用了 updated_at
check_updated_at_in_code() {
    echo ""
    echo "检查代码中的 updated_at 使用..."
    
    UPDATED_AT_FILES=$(grep -r "updated_at:" src/ --include="*.rs" 2>/dev/null | wc -l | tr -d ' ')
    
    if [ "$UPDATED_AT_FILES" -gt 0 ]; then
        echo -e "${YELLOW}⚠ 发现 $UPDATED_AT_FILES 处使用 updated_at 字段，建议使用 updated_ts${NC}"
        grep -r "updated_at:" src/ --include="*.rs" 2>/dev/null | head -5
        WARNINGS=$((WARNINGS + 1))
    else
        echo -e "${GREEN}✓ 代码中没有使用 updated_at 字段${NC}"
    fi
}

# 检查代码中是否使用了 expires_ts
check_expires_ts_in_code() {
    echo ""
    echo "检查代码中的 expires_ts 使用..."
    
    EXPIRES_TS_FILES=$(grep -r "expires_ts:" src/ --include="*.rs" 2>/dev/null | wc -l | tr -d ' ')
    
    if [ "$EXPIRES_TS_FILES" -gt 0 ]; then
        echo -e "${RED}✗ 发现 $EXPIRES_TS_FILES 处使用 expires_ts 字段，应使用 expires_at${NC}"
        grep -r "expires_ts:" src/ --include="*.rs" 2>/dev/null | head -5
        ERRORS=$((ERRORS + 1))
    else
        echo -e "${GREEN}✓ 代码中没有使用 expires_ts 字段${NC}"
    fi
}

# 检查代码中是否使用了 revoked_ts
check_revoked_ts_in_code() {
    echo ""
    echo "检查代码中的 revoked_ts 使用..."
    
    REVOKED_TS_FILES=$(grep -r "revoked_ts:" src/ --include="*.rs" 2>/dev/null | wc -l | tr -d ' ')
    
    if [ "$REVOKED_TS_FILES" -gt 0 ]; then
        echo -e "${RED}✗ 发现 $REVOKED_TS_FILES 处使用 revoked_ts 字段，应使用 revoked_at${NC}"
        grep -r "revoked_ts:" src/ --include="*.rs" 2>/dev/null | head -5
        ERRORS=$((ERRORS + 1))
    else
        echo -e "${GREEN}✓ 代码中没有使用 revoked_ts 字段${NC}"
    fi
}

# 检查代码中是否使用了 enabled (布尔字段)
check_enabled_in_code() {
    echo ""
    echo "检查代码中的 enabled 使用..."
    
    ENABLED_FILES=$(grep -r "pub enabled:" src/ --include="*.rs" 2>/dev/null | wc -l | tr -d ' ')
    
    if [ "$ENABLED_FILES" -gt 0 ]; then
        echo -e "${RED}✗ 发现 $ENABLED_FILES 处使用 enabled 字段，应使用 is_enabled${NC}"
        grep -r "pub enabled:" src/ --include="*.rs" 2>/dev/null | head -5
        ERRORS=$((ERRORS + 1))
    else
        echo -e "${GREEN}✓ 代码中没有使用 enabled 字段${NC}"
    fi
}

# 检查 SQL 语句中的字段名
check_sql_field_names() {
    echo ""
    echo "检查 SQL 语句中的字段名..."
    
    # 检查 INSERT 语句是否包含 created_ts
    INSERT_WITHOUT_CREATED=$(grep -r "INSERT INTO" src/ --include="*.rs" -A 5 2>/dev/null | grep -v "created_ts" | grep "INSERT INTO" | wc -l | tr -d ' ')
    
    if [ "$INSERT_WITHOUT_CREATED" -gt 0 ]; then
        echo -e "${YELLOW}⚠ 发现 $INSERT_WITHOUT_CREATED 个 INSERT 语句可能缺少 created_ts 字段${NC}"
        WARNINGS=$((WARNINGS + 1))
    else
        echo -e "${GREEN}✓ 所有 INSERT 语句都包含 created_ts 字段${NC}"
    fi
}

# 检查结构体字段与数据库是否匹配
check_struct_db_match() {
    echo ""
    echo "检查结构体字段与数据库匹配..."
    
    # 获取所有 FromRow 结构体
    FROMROW_STRUCTS=$(grep -r "#\[derive.*FromRow" src/ --include="*.rs" -A 20 2>/dev/null | grep "pub struct" | wc -l | tr -d ' ')
    
    echo -e "${GREEN}✓ 发现 $FROMROW_STRUCTS 个 FromRow 结构体${NC}"
    echo "  请确保这些结构体的字段名与数据库表结构一致"
}

# 主函数
main() {
    echo "开始 Schema 同步检查..."
    echo ""
    
    check_created_at_in_code
    check_updated_at_in_code
    check_expires_ts_in_code
    check_revoked_ts_in_code
    check_enabled_in_code
    check_sql_field_names
    check_struct_db_match
    
    echo ""
    echo "========================================"
    echo "检查结果"
    echo "========================================"
    echo -e "错误: ${RED}$ERRORS${NC}"
    echo -e "警告: ${YELLOW}$WARNINGS${NC}"
    echo ""
    
    if [ $ERRORS -gt 0 ]; then
        echo -e "${RED}✗ Schema 同步检查失败，请修复上述错误${NC}"
        exit 1
    elif [ $WARNINGS -gt 0 ]; then
        echo -e "${YELLOW}⚠ Schema 同步检查通过，但存在警告${NC}"
        exit 0
    else
        echo -e "${GREEN}✓ Schema 同步检查通过${NC}"
        exit 0
    fi
}

main "$@"
