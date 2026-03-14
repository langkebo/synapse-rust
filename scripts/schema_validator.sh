#!/bin/bash

# Schema 验证工具 - 迁移前验证
# 用于在执行数据库迁移前验证 Schema 是否符合规范

set -e

echo "========================================"
echo "Schema 验证工具"
echo "========================================"
echo ""

# 数据库连接信息
DB_HOST="${DB_HOST:-localhost}"
DB_PORT="${DB_PORT:-5432}"
DB_NAME="${DB_NAME:-synapse_test}"
DB_USER="${DB_USER:-synapse}"

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# 检查结果
ERRORS=0
WARNINGS=0

# 检查函数
check_field_naming() {
    echo "检查字段命名规范..."
    
    # 检查是否存在 created_at 字段 (应该使用 created_ts)
    CREATED_AT_COUNT=$(psql -h $DB_HOST -p $DB_PORT -U $DB_USER -d $DB_NAME -t -c "
        SELECT COUNT(*) FROM information_schema.columns 
        WHERE column_name = 'created_at' 
        AND table_schema = 'public'
    " 2>/dev/null | tr -d ' ')
    
    if [ "$CREATED_AT_COUNT" -gt 0 ]; then
        echo -e "${RED}✗ 发现 $CREATED_AT_COUNT 个表使用 created_at 字段，应使用 created_ts${NC}"
        ERRORS=$((ERRORS + 1))
    else
        echo -e "${GREEN}✓ 所有表都使用 created_ts 字段${NC}"
    fi
    
    # 检查是否存在 updated_at 字段 (应该使用 updated_ts)
    UPDATED_AT_COUNT=$(psql -h $DB_HOST -p $DB_PORT -U $DB_USER -d $DB_NAME -t -c "
        SELECT COUNT(*) FROM information_schema.columns 
        WHERE column_name = 'updated_at' 
        AND table_schema = 'public'
    " 2>/dev/null | tr -d ' ')
    
    if [ "$UPDATED_AT_COUNT" -gt 0 ]; then
        echo -e "${YELLOW}⚠ 发现 $UPDATED_AT_COUNT 个表使用 updated_at 字段，建议使用 updated_ts${NC}"
        WARNINGS=$((WARNINGS + 1))
    else
        echo -e "${GREEN}✓ 所有表都使用 updated_ts 字段${NC}"
    fi
    
    # 检查是否存在 expires_ts 字段 (应该使用 expires_at)
    EXPIRES_TS_COUNT=$(psql -h $DB_HOST -p $DB_PORT -U $DB_USER -d $DB_NAME -t -c "
        SELECT COUNT(*) FROM information_schema.columns 
        WHERE column_name = 'expires_ts' 
        AND table_schema = 'public'
    " 2>/dev/null | tr -d ' ')
    
    if [ "$EXPIRES_TS_COUNT" -gt 0 ]; then
        echo -e "${RED}✗ 发现 $EXPIRES_TS_COUNT 个表使用 expires_ts 字段，应使用 expires_at${NC}"
        ERRORS=$((ERRORS + 1))
    else
        echo -e "${GREEN}✓ 所有表都使用 expires_at 字段${NC}"
    fi
    
    # 检查是否存在 revoked_ts 字段 (应该使用 revoked_at)
    REVOKED_TS_COUNT=$(psql -h $DB_HOST -p $DB_PORT -U $DB_USER -d $DB_NAME -t -c "
        SELECT COUNT(*) FROM information_schema.columns 
        WHERE column_name = 'revoked_ts' 
        AND table_schema = 'public'
    " 2>/dev/null | tr -d ' ')
    
    if [ "$REVOKED_TS_COUNT" -gt 0 ]; then
        echo -e "${RED}✗ 发现 $REVOKED_TS_COUNT 个表使用 revoked_ts 字段，应使用 revoked_at${NC}"
        ERRORS=$((ERRORS + 1))
    else
        echo -e "${GREEN}✓ 所有表都使用 revoked_at 字段${NC}"
    fi
}

check_boolean_fields() {
    echo ""
    echo "检查布尔字段命名规范..."
    
    # 检查是否存在 enabled 字段 (应该使用 is_enabled)
    ENABLED_COUNT=$(psql -h $DB_HOST -p $DB_PORT -U $DB_USER -d $DB_NAME -t -c "
        SELECT COUNT(*) FROM information_schema.columns 
        WHERE column_name = 'enabled' 
        AND data_type = 'boolean'
        AND table_schema = 'public'
    " 2>/dev/null | tr -d ' ')
    
    if [ "$ENABLED_COUNT" -gt 0 ]; then
        echo -e "${RED}✗ 发现 $ENABLED_COUNT 个表使用 enabled 字段，应使用 is_enabled${NC}"
        ERRORS=$((ERRORS + 1))
    else
        echo -e "${GREEN}✓ 所有布尔字段都使用 is_ 前缀${NC}"
    fi
    
    # 检查是否存在 invalidated 字段 (应该使用 is_revoked)
    INVALIDATED_COUNT=$(psql -h $DB_HOST -p $DB_PORT -U $DB_USER -d $DB_NAME -t -c "
        SELECT COUNT(*) FROM information_schema.columns 
        WHERE column_name = 'invalidated' 
        AND data_type = 'boolean'
        AND table_schema = 'public'
    " 2>/dev/null | tr -d ' ')
    
    if [ "$INVALIDATED_COUNT" -gt 0 ]; then
        echo -e "${RED}✗ 发现 $INVALIDATED_COUNT 个表使用 invalidated 字段，应使用 is_revoked${NC}"
        ERRORS=$((ERRORS + 1))
    else
        echo -e "${GREEN}✓ 没有使用 invalidated 字段${NC}"
    fi
}

check_timestamp_types() {
    echo ""
    echo "检查时间戳字段类型..."
    
    # 检查时间戳字段是否使用 BIGINT
    NON_BIGINT_TS=$(psql -h $DB_HOST -p $DB_PORT -U $DB_USER -d $DB_NAME -t -c "
        SELECT COUNT(*) FROM information_schema.columns 
        WHERE column_name LIKE '%_ts' 
        AND data_type != 'bigint'
        AND table_schema = 'public'
    " 2>/dev/null | tr -d ' ')
    
    if [ "$NON_BIGINT_TS" -gt 0 ]; then
        echo -e "${YELLOW}⚠ 发现 $NON_BIGINT_TS 个 _ts 字段未使用 BIGINT 类型${NC}"
        WARNINGS=$((WARNINGS + 1))
    else
        echo -e "${GREEN}✓ 所有 _ts 字段都使用 BIGINT 类型${NC}"
    fi
}

check_foreign_keys() {
    echo ""
    echo "检查外键约束..."
    
    # 检查外键命名规范
    INVALID_FK=$(psql -h $DB_HOST -p $DB_PORT -U $DB_USER -d $DB_NAME -t -c "
        SELECT COUNT(*) FROM information_schema.table_constraints tc
        JOIN information_schema.key_column_usage kcu ON tc.constraint_name = kcu.constraint_name
        WHERE tc.constraint_type = 'FOREIGN KEY'
        AND kcu.column_name NOT LIKE '%_id'
        AND tc.table_schema = 'public'
    " 2>/dev/null | tr -d ' ')
    
    if [ "$INVALID_FK" -gt 0 ]; then
        echo -e "${YELLOW}⚠ 发现 $INVALID_FK 个外键字段未使用 _id 后缀${NC}"
        WARNINGS=$((WARNINGS + 1))
    else
        echo -e "${GREEN}✓ 所有外键字段都使用 _id 后缀${NC}"
    fi
}

check_indexes() {
    echo ""
    echo "检查索引..."
    
    # 检查外键是否有索引
    FK_WITHOUT_INDEX=$(psql -h $DB_HOST -p $DB_PORT -U $DB_USER -d $DB_NAME -t -c "
        SELECT COUNT(DISTINCT kcu.column_name) FROM information_schema.table_constraints tc
        JOIN information_schema.key_column_usage kcu ON tc.constraint_name = kcu.constraint_name
        WHERE tc.constraint_type = 'FOREIGN KEY'
        AND tc.table_schema = 'public'
        AND NOT EXISTS (
            SELECT 1 FROM pg_indexes pi 
            WHERE pi.tablename = tc.table_name 
            AND pi.indexdef LIKE '%' || kcu.column_name || '%'
        )
    " 2>/dev/null | tr -d ' ')
    
    if [ "$FK_WITHOUT_INDEX" -gt 0 ]; then
        echo -e "${YELLOW}⚠ 发现 $FK_WITHOUT_INDEX 个外键字段没有索引${NC}"
        WARNINGS=$((WARNINGS + 1))
    else
        echo -e "${GREEN}✓ 所有外键字段都有索引${NC}"
    fi
}

# 主函数
main() {
    echo "开始 Schema 验证..."
    echo ""
    
    check_field_naming
    check_boolean_fields
    check_timestamp_types
    check_foreign_keys
    check_indexes
    
    echo ""
    echo "========================================"
    echo "验证结果"
    echo "========================================"
    echo -e "错误: ${RED}$ERRORS${NC}"
    echo -e "警告: ${YELLOW}$WARNINGS${NC}"
    echo ""
    
    if [ $ERRORS -gt 0 ]; then
        echo -e "${RED}✗ Schema 验证失败，请修复上述错误${NC}"
        exit 1
    elif [ $WARNINGS -gt 0 ]; then
        echo -e "${YELLOW}⚠ Schema 验证通过，但存在警告${NC}"
        exit 0
    else
        echo -e "${GREEN}✓ Schema 验证通过${NC}"
        exit 0
    fi
}

main "$@"
