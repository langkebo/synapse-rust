#!/bin/bash

# 编译时字段名检查工具
# 用于检查 Rust 代码中的字段名是否符合 DATABASE_FIELD_STANDARDS.md 规范

set -e

echo "========================================"
echo "编译时字段名检查工具"
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

# 定义禁止使用的字段
FORBIDDEN_FIELDS=(
    "created_at:BIGINT:created_ts"
    "updated_at:BIGINT:updated_ts"
    "expires_ts:BIGINT:expires_at"
    "revoked_ts:BIGINT:revoked_at"
    "validated_ts:BIGINT:validated_at"
    "invalidated:BOOLEAN:is_revoked"
    "enabled:BOOLEAN:is_enabled"
)

# 检查结构体定义
check_struct_definitions() {
    echo "检查结构体定义中的字段名..."
    
    for rule in "${FORBIDDEN_FIELDS[@]}"; do
        IFS=':' read -r forbidden type correct <<< "$rule"
        
        # 检查结构体字段定义
        COUNT=$(grep -r "pub $forbidden:" src/ --include="*.rs" 2>/dev/null | wc -l | tr -d ' ')
        
        if [ "$COUNT" -gt 0 ]; then
            echo -e "${RED}✗ 发现 $COUNT 处使用 $forbidden 字段，应使用 $correct${NC}"
            grep -r "pub $forbidden:" src/ --include="*.rs" 2>/dev/null | head -3
            ERRORS=$((ERRORS + 1))
        fi
    done
    
    if [ $ERRORS -eq 0 ]; then
        echo -e "${GREEN}✓ 所有结构体字段定义都符合规范${NC}"
    fi
}

# 检查 SQL 查询中的字段名
check_sql_queries() {
    echo ""
    echo "检查 SQL 查询中的字段名..."
    
    for rule in "${FORBIDDEN_FIELDS[@]}"; do
        IFS=':' read -r forbidden type correct <<< "$rule"
        
        # 检查 SELECT 语句
        SELECT_COUNT=$(grep -r "SELECT.*$forbidden" src/ --include="*.rs" 2>/dev/null | wc -l | tr -d ' ')
        
        if [ "$SELECT_COUNT" -gt 0 ]; then
            echo -e "${RED}✗ 发现 $SELECT_COUNT 处 SELECT 语句使用 $forbidden 字段，应使用 $correct${NC}"
            grep -r "SELECT.*$forbidden" src/ --include="*.rs" 2>/dev/null | head -3
            ERRORS=$((ERRORS + 1))
        fi
        
        # 检查 INSERT 语句
        INSERT_COUNT=$(grep -r "INSERT INTO.*$forbidden" src/ --include="*.rs" 2>/dev/null | wc -l | tr -d ' ')
        
        if [ "$INSERT_COUNT" -gt 0 ]; then
            echo -e "${RED}✗ 发现 $INSERT_COUNT 处 INSERT 语句使用 $forbidden 字段，应使用 $correct${NC}"
            grep -r "INSERT INTO.*$forbidden" src/ --include="*.rs" 2>/dev/null | head -3
            ERRORS=$((ERRORS + 1))
        fi
        
        # 检查 UPDATE 语句
        UPDATE_COUNT=$(grep -r "UPDATE.*SET.*$forbidden" src/ --include="*.rs" 2>/dev/null | wc -l | tr -d ' ')
        
        if [ "$UPDATE_COUNT" -gt 0 ]; then
            echo -e "${RED}✗ 发现 $UPDATE_COUNT 处 UPDATE 语句使用 $forbidden 字段，应使用 $correct${NC}"
            grep -r "UPDATE.*SET.*$forbidden" src/ --include="*.rs" 2>/dev/null | head -3
            ERRORS=$((ERRORS + 1))
        fi
        
        # 检查 WHERE 条件
        WHERE_COUNT=$(grep -r "WHERE.*$forbidden" src/ --include="*.rs" 2>/dev/null | wc -l | tr -d ' ')
        
        if [ "$WHERE_COUNT" -gt 0 ]; then
            echo -e "${RED}✗ 发现 $WHERE_COUNT 处 WHERE 条件使用 $forbidden 字段，应使用 $correct${NC}"
            grep -r "WHERE.*$forbidden" src/ --include="*.rs" 2>/dev/null | head -3
            ERRORS=$((ERRORS + 1))
        fi
    done
    
    if [ $ERRORS -eq 0 ]; then
        echo -e "${GREEN}✓ 所有 SQL 查询中的字段名都符合规范${NC}"
    fi
}

# 检查字段访问
check_field_access() {
    echo ""
    echo "检查字段访问..."
    
    for rule in "${FORBIDDEN_FIELDS[@]}"; do
        IFS=':' read -r forbidden type correct <<< "$rule"
        
        # 检查 .field_name 访问
        COUNT=$(grep -r "\.$forbidden" src/ --include="*.rs" 2>/dev/null | wc -l | tr -d ' ')
        
        if [ "$COUNT" -gt 0 ]; then
            echo -e "${RED}✗ 发现 $COUNT 处访问 $forbidden 字段，应使用 $correct${NC}"
            grep -r "\.$forbidden" src/ --include="*.rs" 2>/dev/null | head -3
            ERRORS=$((ERRORS + 1))
        fi
    done
    
    if [ $ERRORS -eq 0 ]; then
        echo -e "${GREEN}✓ 所有字段访问都符合规范${NC}"
    fi
}

# 检查 bind 调用
check_bind_calls() {
    echo ""
    echo "检查 bind 调用..."
    
    for rule in "${FORBIDDEN_FIELDS[@]}"; do
        IFS=':' read -r forbidden type correct <<< "$rule"
        
        # 检查 .bind(&xxx.forbidden)
        COUNT=$(grep -r "\.bind(.*\.$forbidden" src/ --include="*.rs" 2>/dev/null | wc -l | tr -d ' ')
        
        if [ "$COUNT" -gt 0 ]; then
            echo -e "${RED}✗ 发现 $COUNT 处 bind 调用使用 $forbidden 字段，应使用 $correct${NC}"
            grep -r "\.bind(.*\.$forbidden" src/ --include="*.rs" 2>/dev/null | head -3
            ERRORS=$((ERRORS + 1))
        fi
    done
    
    if [ $ERRORS -eq 0 ]; then
        echo -e "${GREEN}✓ 所有 bind 调用都符合规范${NC}"
    fi
}

# 检查 JSON 序列化
check_json_serialization() {
    echo ""
    echo "检查 JSON 序列化..."
    
    # 检查 serde rename 是否正确
    RENAME_COUNT=$(grep -r "#\[serde(rename" src/ --include="*.rs" 2>/dev/null | wc -l | tr -d ' ')
    
    echo -e "${GREEN}✓ 发现 $RENAME_COUNT 处 serde rename 注解${NC}"
    echo "  请确保 rename 的目标字段名符合规范"
}

# 主函数
main() {
    echo "开始编译时字段名检查..."
    echo ""
    
    check_struct_definitions
    check_sql_queries
    check_field_access
    check_bind_calls
    check_json_serialization
    
    echo ""
    echo "========================================"
    echo "检查结果"
    echo "========================================"
    echo -e "错误: ${RED}$ERRORS${NC}"
    echo -e "警告: ${YELLOW}$WARNINGS${NC}"
    echo ""
    
    if [ $ERRORS -gt 0 ]; then
        echo -e "${RED}✗ 编译时字段名检查失败，请修复上述错误${NC}"
        exit 1
    elif [ $WARNINGS -gt 0 ]; then
        echo -e "${YELLOW}⚠ 编译时字段名检查通过，但存在警告${NC}"
        exit 0
    else
        echo -e "${GREEN}✓ 编译时字段名检查通过${NC}"
        exit 0
    fi
}

main "$@"
