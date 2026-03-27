#!/bin/bash
# 索引存在性检查脚本
# 验证数据库中必要索引的存在
# 用法: bash scripts/index_check.sh

set -e

DB_NAME="${DB_NAME:-synapse}"
DB_USER="${DB_USER:-synapse}"
DB_HOST="${DB_HOST:-localhost}"

echo "=========================================="
echo "  索引存在性检查"
echo "=========================================="
echo ""

ERRORS=0
TOTAL_CHECKS=0
PASSED_CHECKS=0

# 颜色定义
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[PASS]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[FAIL]${NC} $1"
    ERRORS=$((ERRORS + 1))
}

table_exists() {
    local table=$1
    psql -h "$DB_HOST" -U "$DB_USER" -d "$DB_NAME" -t -c "
        SELECT COUNT(*) FROM information_schema.tables
        WHERE table_schema = 'public' AND table_name = '$table';
    " 2>/dev/null | xargs | grep -q "^1$"
}

check_index() {
    local table=$1
    local index_name=$2
    local columns=$3
    local index_type=${4:-BTREE}

    TOTAL_CHECKS=$((TOTAL_CHECKS + 1))

    if ! table_exists "$table"; then
        log_warning "$table 表不存在 (跳过索引检查: $index_name)"
        return
    fi

    index_exists=$(psql -h "$DB_HOST" -U "$DB_USER" -d "$DB_NAME" -t -c "
        SELECT COUNT(*) FROM pg_indexes
        WHERE schemaname = 'public'
        AND tablename = '$table'
        AND indexname = '$index_name';
    " 2>/dev/null | xargs)

    if [ "$index_exists" -gt 0 ]; then
        log_success "$table.$index_name ($columns) 存在 ✓"
        PASSED_CHECKS=$((PASSED_CHECKS + 1))
    else
        log_error "$table.$index_name ($columns) 缺失"
        ERRORS=$((ERRORS + 1))
    fi
}

check_index_pattern() {
    local table=$1
    local pattern=$2
    local description=$3

    TOTAL_CHECKS=$((TOTAL_CHECKS + 1))

    if ! table_exists "$table"; then
        log_warning "$table 表不存在 (跳过)"
        return
    fi

    matching_indexes=$(psql -h "$DB_HOST" -U "$DB_USER" -d "$DB_NAME" -t -c "
        SELECT COUNT(*) FROM pg_indexes
        WHERE schemaname = 'public'
        AND tablename = '$table'
        AND indexname LIKE '$pattern';
    " 2>/dev/null | xargs)

    if [ "$matching_indexes" -gt 0 ]; then
        log_success "$table: $description ($matching_indexes 个匹配) ✓"
        PASSED_CHECKS=$((PASSED_CHECKS + 1))
    else
        log_error "$table: $description 缺失"
        ERRORS=$((ERRORS + 1))
    fi
}

echo "--- 用户表索引检查 ---"
echo ""

if table_exists "users"; then
    check_index "users" "idx_users_email" "email"
    check_index "users" "idx_users_is_admin" "is_admin"
    check_index_pattern "users" "idx_users_username%" "username 唯一索引"
else
    log_warning "users 表不存在"
fi

echo ""
echo "--- 设备表索引检查 ---"
echo ""

if table_exists "devices"; then
    check_index "devices" "idx_devices_user_id" "user_id"
    check_index_pattern "devices" "idx_devices_%" "设备相关索引"
else
    log_warning "devices 表不存在"
fi

echo ""
echo "--- Token 表索引检查 ---"
echo ""

if table_exists "access_tokens"; then
    check_index "access_tokens" "idx_access_tokens_user_id" "user_id"
    check_index_pattern "access_tokens" "idx_access_tokens_%" "访问令牌索引"
else
    log_warning "access_tokens 表不存在"
fi

if table_exists "refresh_tokens"; then
    check_index "refresh_tokens" "idx_refresh_tokens_user_id" "user_id"
    check_index_pattern "refresh_tokens" "idx_refresh_tokens_%" "刷新令牌索引"
else
    log_warning "refresh_tokens 表不存在"
fi

echo ""
echo "--- 房间表索引检查 ---"
echo ""

if table_exists "rooms"; then
    check_index_pattern "rooms" "idx_rooms_%" "房间相关索引"
else
    log_warning "rooms 表不存在"
fi

echo ""
echo "--- 房间会员表索引检查 ---"
echo ""

if table_exists "room_memberships"; then
    check_index "room_memberships" "idx_room_memberships_room" "room_id"
    check_index "room_memberships" "idx_room_memberships_user" "user_id"
    check_index "room_memberships" "idx_room_memberships_membership" "membership"
    check_index_pattern "room_memberships" "idx_room_memberships_%" "会员相关索引"
else
    log_warning "room_memberships 表不存在"
fi

echo ""
echo "--- 事件表索引检查 ---"
echo ""

if table_exists "events"; then
    check_index "events" "idx_events_room_id" "room_id"
    check_index "events" "idx_events_sender" "sender"
    check_index "events" "idx_events_type" "event_type"
    check_index_pattern "events" "idx_events_%" "事件相关索引"
else
    log_warning "events 表不存在"
fi

echo ""
echo "--- 其他表索引检查 ---"
echo ""

if table_exists "user_threepids"; then
    check_index "user_threepids" "idx_user_threepids_user" "user_id"
fi

if table_exists "presence"; then
    check_index_pattern "presence" "idx_presence_%" "存在状态索引"
fi

if table_exists "room_directory"; then
    check_index_pattern "room_directory" "idx_room_directory_%" "房间目录索引"
fi

echo ""
echo "--- 索引统计 ---"
echo ""

TOTAL_INDEXES=$(psql -h "$DB_HOST" -U "$DB_USER" -d "$DB_NAME" -t -c "
    SELECT COUNT(*) FROM pg_indexes WHERE schemaname = 'public';
" 2>/dev/null | xargs)

TABLE_COUNT=$(psql -h "$DB_HOST" -U "$DB_USER" -d "$DB_NAME" -t -c "
    SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public' AND table_type = 'BASE TABLE';
" 2>/dev/null | xargs)

log_info "总表数: $TABLE_COUNT"
log_info "总索引数: $TOTAL_INDEXES"

if [ "$TABLE_COUNT" -gt 0 ]; then
    AVG_INDEXES=$((TOTAL_INDEXES / TABLE_COUNT))
    log_info "平均每表索引数: $AVG_INDEXES"
fi

echo ""
echo "=========================================="
if [ $ERRORS -gt 0 ]; then
    echo -e "  检查完成: ${RED}$ERRORS 个缺失索引${NC}"
    echo "=========================================="
    exit 1
else
    echo -e "  检查完成: ${GREEN}所有必需索引存在${NC}"
    echo "=========================================="
    exit 0
fi