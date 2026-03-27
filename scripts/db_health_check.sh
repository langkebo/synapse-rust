#!/bin/bash
# ============================================================================
# 数据库 Schema 健康检查脚本
# 功能: 对比代码中的 SQL 查询与实际数据库 schema，报告缺失的表/列
# ============================================================================

set -e

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Docker 配置
DB_CONTAINER="${DB_CONTAINER:-synapse_db_prod}"
DB_NAME="${DB_NAME:-synapse}"

echo -e "${BLUE}=== Synapse-Rust 数据库 Schema 健康检查 ===${NC}\n"

# 获取数据库中所有表
get_tables() {
    docker exec "$DB_CONTAINER" psql -U synapse -d "$DB_NAME" -t -c "
        SELECT table_name FROM information_schema.tables
        WHERE table_schema = 'public'
        ORDER BY table_name;
    " 2>/dev/null | tr -d ' ' | grep -v '^$'
}

# 获取表的列
get_columns() {
    local table="$1"
    docker exec "$DB_CONTAINER" psql -U synapse -d "$DB_NAME" -t -c "
        SELECT column_name FROM information_schema.columns
        WHERE table_name = '$table'
        ORDER BY ordinal_position;
    " 2>/dev/null | tr -d ' ' | grep -v '^$'
}

echo -e "${YELLOW}[1/4] 扫描代码中的 SQL 查询...${NC}"

# 扫描 Rust 代码中的表引用
CODE_TABLES=$(grep -roh "FROM [a-z_]*" src/ --include="*.rs" 2>/dev/null | sort -u | sed 's/FROM //' | grep -v '^$' | sort -u)
CODE_TABLES="$CODE_TABLES $(grep -roh "JOIN [a-z_]*" src/ --include="*.rs" 2>/dev/null | sort -u | sed 's/JOIN //' | grep -v '^$' | sort -u)"
CODE_TABLES="$CODE_TABLES $(grep -roh "INTO [a-z_]*" src/ --include="*.rs" 2>/dev/null | sort -u | sed 's/INTO //' | grep -v '^$' | sort -u)"
CODE_TABLES="$CODE_TABLES $(grep -roh "TABLE [a-z_]*" src/ --include="*.rs" 2>/dev/null | sort -u | sed 's/TABLE //' | grep -v '^$' | sort -u)"

# 去重
CODE_TABLES=$(echo "$CODE_TABLES" | tr ' ' '\n' | sort -u | grep -v '^$')

echo -e "${YELLOW}[2/4] 获取数据库中的表...${NC}"
DB_TABLES=$(get_tables)
DB_TABLES_COUNT=$(echo "$DB_TABLES" | wc -l)

echo -e "${YELLOW}[3/4] 对比表结构...${NC}"

# 报告缺失的表
MISSING_TABLES=""
for table in $CODE_TABLES; do
    if ! echo "$DB_TABLES" | grep -q "^${table}$"; then
        MISSING_TABLES="$MISSING_TABLES $table"
    fi
done

echo -e "${YELLOW}[4/4] 生成报告...${NC}\n"

# 输出报告
echo "=========================================="
echo -e "${BLUE}数据库表数量: $DB_TABLES_COUNT${NC}"
echo "=========================================="

if [ -z "$MISSING_TABLES" ]; then
    echo -e "${GREEN}✅ 所有代码引用的表都存在${NC}"
else
    echo -e "${RED}❌ 缺失以下表:${NC}"
    for table in $MISSING_TABLES; do
        echo -e "  - ${RED}$table${NC}"
    done
    echo ""
fi

# 检查关键表是否存在及其列
echo ""
echo "=========================================="
echo -e "${BLUE}关键表结构检查${NC}"
echo "=========================================="

CHECK_TABLES="spaces space_members space_summaries users devices access_tokens refresh_tokens room_memberships room_events events key_backups backup_keys registration_tokens media"

for table in $CHECK_TABLES; do
    if echo "$DB_TABLES" | grep -q "^${table}$"; then
        echo -e "${GREEN}✅ $table 存在${NC}"
        COLUMNS=$(get_columns "$table")
        echo "   列: $(echo $COLUMNS | tr '\n' ' ' | sed 's/ $//')"
    else
        echo -e "${RED}❌ $table 不存在${NC}"
    fi
done

echo ""
echo "=========================================="
echo -e "${BLUE}建议${NC}"
echo "=========================================="

if [ -z "$MISSING_TABLES" ]; then
    echo -e "${GREEN}Schema 检查通过，无需迁移${NC}"
else
    echo "请执行以下 SQL 创建缺失的表:"
    for table in $MISSING_TABLES; do
        echo "-- TODO: 创建 $table 表"
    done
fi

echo ""
echo "检查完成 $(date '+%Y-%m-%d %H:%M:%S')"