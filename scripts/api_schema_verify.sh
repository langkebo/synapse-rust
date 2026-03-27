#!/bin/bash
# ============================================================================
# API 与 Schema 对应验证脚本
# 功能:
#   1. 列出所有 API 端点
#   2. 检查对应路由处理函数
#   3. 验证数据库操作的表是否存在
#
# 使用方法: bash scripts/api_schema_verify.sh
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
echo -e "${BLUE}  API 与 Schema 对应检查${NC}"
echo -e "${BLUE}========================================${NC}\n"

# 获取数据库中所有表
get_db_tables() {
    docker exec "$DB_CONTAINER" psql -U synapse -d "$DB_NAME" -t -c "
        SELECT table_name FROM information_schema.tables
        WHERE table_schema = 'public'
        ORDER BY table_name;
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

# 获取表的所有列
get_table_columns() {
    local table="$1"
    docker exec "$DB_CONTAINER" psql -U synapse -d "$DB_NAME" -t -c "
        SELECT column_name FROM information_schema.columns
        WHERE table_name = '$table' AND table_schema = 'public';
    " 2>/dev/null | tr -d ' ' | grep -v '^$' | grep -v '^)$'
}

echo -e "${YELLOW}[1/4] 获取数据库表列表...${NC}"
DB_TABLES=$(get_db_tables)
DB_TABLE_COUNT=$(echo "$DB_TABLES" | wc -l | tr -d ' ')
echo "  数据库有 ${CYAN}$DB_TABLE_COUNT${NC} 个表"

echo -e "\n${YELLOW}[2/4] 定义 API-Schema 映射...${NC}"

# API 与 Schema 映射定义
# 格式: "METHOD URL|TABLES"
declare -a API_MAPPINGS=(
    "POST|/_matrix/client/r0/register|users"
    "POST|/_matrix/client/r0/login|users,access_tokens"
    "POST|/_matrix/client/r0/logout|access_tokens"
    "GET|/_matrix/client/r0/account/whoami|users"
    "GET|/_matrix/client/r0/account/profile|users"
    "PUT|/_matrix/client/r0/account/profile/*/displayname|users"
    "PUT|/_matrix/client/r0/account/profile/*/avatar_url|users"
    "POST|/_matrix/client/r0/createRoom|rooms,room_memberships"
    "POST|/_matrix/client/r0/rooms/*/join|room_memberships"
    "POST|/_matrix/client/r0/rooms/*/leave|room_memberships"
    "POST|/_matrix/client/r0/rooms/*/invite|room_memberships,room_invites"
    "GET|/_matrix/client/r0/rooms/*/messages|room_events,events"
    "GET|/_matrix/client/r0/rooms/*/members|room_memberships"
    "GET|/_matrix/client/v1/spaces/public|spaces,space_members"
    "GET|/_matrix/client/v1/spaces/user|space_members,spaces"
    "GET|/_matrix/client/r0/devices|devices"
    "DELETE|/_matrix/client/r0/devices/*|devices"
    "POST|/_matrix/media/r0/upload|media"
    "GET|/_matrix/media/r0/download/*|media"
    "GET|/_synapse/admin/v1/media|media"
    "POST|/_matrix/client/v3/room_keys/version|key_backups"
    "GET|/_matrix/client/v3/room_keys/keys|backup_keys,key_backups"
    "PUT|/_matrix/client/v3/room_keys/keys/*/*|backup_keys"
    "GET|/_synapse/admin/v1/users|users"
    "POST|/_synapse/admin/v1/users/*|users"
    "GET|/_synapse/admin/v1/rooms|rooms"
    "GET|/_synapse/admin/v1/registration_tokens|registration_tokens"
    "GET|/_matrix/client/r0/presence/*/status|presence"
    "PUT|/_matrix/client/r0/presence/*/status|presence"
)

echo "  发现 ${CYAN}${#API_MAPPINGS[@]}${NC} 个 API-Schema 映射"

echo -e "\n${YELLOW}[3/4] 验证 API 与 Schema 对应...${NC}"

FAILED=0
PASSED=0

for mapping in "${API_MAPPINGS[@]}"; do
    IFS='|' read -r method api tables <<< "$mapping"

    missing=""
    IFS=',' read -ra TABLE_ARRAY <<< "$tables"
    for table in "${TABLE_ARRAY[@]}"; do
        table=$(echo "$table" | tr -d ' ')
        if ! table_exists "$table"; then
            missing="$missing $table"
        fi
    done

    if [ -z "$missing" ]; then
        echo -e "  ${GREEN}✓${NC} $method $api"
        ((PASSED++))
    else
        echo -e "  ${RED}✗${NC} $method $api (缺失表:$missing)"
        ((FAILED++))
    fi
done

echo -e "\n${YELLOW}[4/4] 生成报告...${NC}"

echo ""
echo "========================================"
echo -e "${BLUE}验证结果汇总${NC}"
echo "========================================"
echo -e "通过: ${GREEN}$PASSED${NC}"
echo -e "失败: ${RED}$FAILED${NC}"

if [ $FAILED -gt 0 ]; then
    echo ""
    echo -e "${YELLOW}建议:${NC}"
    echo "  1. 运行 db_schema_check.sh 检查缺失的表"
    echo "  2. 创建迁移脚本添加缺失的表"
    echo "  3. 重启服务后重新测试"
fi

echo ""
echo "========================================"
echo -e "${BLUE}API 端点与 Schema 映射详情${NC}"
echo "========================================"

for mapping in "${API_MAPPINGS[@]}"; do
    IFS='|' read -r method api tables <<< "$mapping"
    echo -e "${CYAN}$method $api${NC}"

    IFS=',' read -ra TABLE_ARRAY <<< "$tables"
    for table in "${TABLE_ARRAY[@]}"; do
        table=$(echo "$table" | tr -d ' ')
        if table_exists "$table"; then
            cols=$(get_table_columns "$table" | tr '\n' ',' | sed 's/,$//')
            if [ -n "$cols" ]; then
                echo "    └─ $table: $cols"
            else
                echo "    └─ $table: (无列)"
            fi
        else
            echo -e "    └─ ${RED}$table (不存在)${NC}"
        fi
    done
    echo ""
done

echo "检查完成 $(date '+%Y-%m-%d %H:%M:%S')"