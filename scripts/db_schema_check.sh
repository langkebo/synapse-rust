#!/bin/bash
# ============================================================================
# 数据库 Schema 验证脚本
# 功能:
#   1. 扫描代码中的 SQL 表引用
#   2. 对比数据库实际 schema
#   3. 报告缺失的表和列
#
# 使用方法: bash scripts/db_schema_check.sh
# ============================================================================

set -e

# Docker 配置
DB_CONTAINER="${DB_CONTAINER:-synapse_db_prod}"
DB_NAME="${DB_NAME:-synapse}"

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}  Synapse-Rust 数据库 Schema 检查${NC}"
echo -e "${BLUE}========================================${NC}\n"

# 获取数据库中所有表
get_db_tables() {
    docker exec "$DB_CONTAINER" psql -U synapse -d "$DB_NAME" -t -c "
        SELECT table_name FROM information_schema.tables
        WHERE table_schema = 'public'
        ORDER BY table_name;
    " 2>/dev/null | tr -d ' ' | grep -v '^$' | grep -v '^)$'
}

# 获取表的列
get_table_columns() {
    local table="$1"
    docker exec "$DB_CONTAINER" psql -U synapse -d "$DB_NAME" -t -c "
        SELECT column_name FROM information_schema.columns
        WHERE table_name = '$table'
        ORDER BY ordinal_position;
    " 2>/dev/null | tr -d ' ' | grep -v '^$' | grep -v '^)$'
}

# 检查表是否存在
table_exists() {
    local table="$1"
    docker exec "$DB_CONTAINER" psql -U synapse -d "$DB_NAME" -t -c "
        SELECT 1 FROM information_schema.tables
        WHERE table_name = '$table' AND table_schema = 'public';
    " 2>/dev/null | grep -q "1"
}

# 检查列是否存在
column_exists() {
    local table="$1"
    local column="$2"
    docker exec "$DB_CONTAINER" psql -U synapse -d "$DB_NAME" -t -c "
        SELECT 1 FROM information_schema.columns
        WHERE table_name = '$table' AND column_name = '$column' AND table_schema = 'public';
    " 2>/dev/null | grep -q "1"
}

echo -e "${YELLOW}[1/5] 扫描代码中的 SQL 表引用...${NC}"

# 扫描 Rust 代码中的表引用
CODE_TABLES=$(grep -rhoE "FROM [a-z_]+|JOIN [a-z_]+|INTO [a-z_]+|TABLE [a-z_]+" src/ --include="*.rs" 2>/dev/null \
    | sed 's/FROM //g; s/JOIN //g; s/INTO //g; s/TABLE //g' \
    | tr ' ' '\n' \
    | grep -v '^$' \
    | sort -u)

echo "  发现 ${CYAN}$(echo "$CODE_TABLES" | wc -l)${NC} 个表引用"

echo -e "\n${YELLOW}[2/5] 获取数据库中的表...${NC}"
DB_TABLES=$(get_db_tables)
DB_TABLE_COUNT=$(echo "$DB_TABLES" | wc -l | tr -d ' ')
echo "  数据库有 ${CYAN}$DB_TABLE_COUNT${NC} 个表"

echo -e "\n${YELLOW}[3/5] 对比表结构...${NC}"

MISSING_TABLES=""
for table in $CODE_TABLES; do
    if ! table_exists "$table"; then
        MISSING_TABLES="$MISSING_TABLES $table"
        echo -e "  ${RED}✗${NC} $table (不存在)"
    fi
done

echo -e "\n${YELLOW}[4/5] 检查核心表...${NC}"

# 核心表清单
CORE_TABLES=(
    "users"
    "devices"
    "access_tokens"
    "refresh_tokens"
    "rooms"
    "room_memberships"
    "room_events"
    "events"
    "spaces"
    "space_members"
    "space_summaries"
    "media"
    "key_backups"
    "backup_keys"
    "registration_tokens"
    "olm_sessions"
    "olm_accounts"
    "user_threepids"
    "event_receipts"
    "invites"
)

CORE_MISSING=""
for table in "${CORE_TABLES[@]}"; do
    if table_exists "$table"; then
        echo -e "  ${GREEN}✓${NC} $table"
    else
        echo -e "  ${RED}✗${NC} $table (缺失)"
        CORE_MISSING="$CORE_MISSING $table"
    fi
done

echo -e "\n${YELLOW}[5/5] 生成报告...${NC}"

echo ""
echo "========================================"
echo -e "${BLUE}检查结果汇总${NC}"
echo "========================================"

if [ -z "$MISSING_TABLES" ] && [ -z "$CORE_MISSING" ]; then
    echo -e "${GREEN}✅ 所有检查通过!${NC}"
    echo ""
    echo "  - 代码引用的表都存在"
    echo "  - 核心表完整"
else
    if [ -n "$MISSING_TABLES" ]; then
        echo -e "\n${RED}❌ 代码引用但数据库缺失的表:${NC}"
        for table in $MISSING_TABLES; do
            echo -e "    - ${RED}$table${NC}"
        done
    fi

    if [ -n "$CORE_MISSING" ]; then
        echo -e "\n${RED}❌ 缺失的核心表:${NC}"
        for table in $CORE_MISSING; do
            echo -e "    - ${RED}$table${NC}"
        done
    fi

    echo ""
    echo -e "${YELLOW}建议:${NC}"
    echo "  运行以下命令创建缺失的表:"
    echo "  docker exec -i $DB_CONTAINER psql -U synapse -d $DB_NAME < migrations/YYYYMMDD_missing_tables.sql"
fi

echo ""
echo "========================================"
echo -e "${BLUE}详细表列表${NC}"
echo "========================================"
echo "$DB_TABLES" | while read table; do
    [ -z "$table" ] && continue
    COLS=$(get_table_columns "$table" | tr '\n' ',' | sed 's/,$//')
    echo -e "${CYAN}$table${NC}: $COLS"
done

echo ""
echo "检查完成 $(date '+%Y-%m-%d %H:%M:%S')"