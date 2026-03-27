#!/bin/bash
# 外键完整性检查脚本
# 检查数据库中是否存在孤立的外键记录
# 用法: bash scripts/foreign_key_check.sh

set -e

DB_NAME="${DB_NAME:-synapse}"
DB_USER="${DB_USER:-synapse}"
DB_HOST="${DB_HOST:-localhost}"

echo "=========================================="
echo "  外键完整性检查"
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

check_orphan() {
    local child_table=$1
    local child_fk=$2
    local parent_table=$3
    local parent_pk=${4:-user_id}

    TOTAL_CHECKS=$((TOTAL_CHECKS + 1))

    if ! table_exists "$child_table"; then
        log_warning "$child_table 表不存在 (跳过)"
        return
    fi

    if ! table_exists "$parent_table"; then
        log_warning "$parent_table 表不存在 (跳过)"
        return
    fi

    orphan_count=$(psql -h "$DB_HOST" -U "$DB_USER" -d "$DB_NAME" -t -c "
        SELECT COUNT(*) FROM $child_table c
        WHERE c.$child_fk IS NOT NULL
        AND NOT EXISTS (
            SELECT 1 FROM $parent_table p
            WHERE p.$parent_pk = c.$child_fk
        );
    " 2>/dev/null | xargs)

    if [ -z "$orphan_count" ]; then
        orphan_count=0
    fi

    if [ "$orphan_count" -gt 0 ]; then
        log_error "$child_table.$child_fk 存在 $orphan_count 条孤立记录"

        psql -h "$DB_HOST" -U "$DB_USER" -d "$DB_NAME" -t -c "
            SELECT '$child_table', '$child_fk', $child_fk, COUNT(*)
            FROM $child_table c
            WHERE c.$child_fk IS NOT NULL
            AND NOT EXISTS (
                SELECT 1 FROM $parent_table p
                WHERE p.$parent_pk = c.$child_fk
            )
            GROUP BY $child_fk
            LIMIT 5;
        " 2>/dev/null | sed 's/^ *//;s/ *$//' | grep -v '^$' | while read line; do
            echo "    孤立值: $line"
        done
    else
        log_success "$child_table.$child_fk 无孤立记录 ✓"
        PASSED_CHECKS=$((PASSED_CHECKS + 1))
    fi
}

echo "--- 用户相关外键检查 ---"
echo ""

check_orphan "devices" "user_id" "users" "user_id"
check_orphan "access_tokens" "user_id" "users" "user_id"
check_orphan "refresh_tokens" "user_id" "users" "user_id"
check_orphan "user_threepids" "user_id" "users" "user_id"
check_orphan "presence" "user_id" "users" "user_id"
check_orphan "user_directory" "user_id" "users" "user_id"
check_orphan "blocked_users" "user_id" "users" "user_id"
check_orphan "friends" "user_id" "users" "user_id"
check_orphan "friends" "friend_id" "users" "user_id"
check_orphan "friend_requests" "sender_id" "users" "user_id"
check_orphan "friend_requests" "receiver_id" "users" "user_id"

echo ""
echo "--- 房间相关外键检查 ---"
echo ""

check_orphan "room_memberships" "room_id" "rooms" "room_id"
check_orphan "room_memberships" "user_id" "users" "user_id"
check_orphan "events" "room_id" "rooms" "room_id"
check_orphan "room_summaries" "room_id" "rooms" "room_id"
check_orphan "room_aliases" "room_id" "rooms" "room_id"
check_orphan "room_tags" "room_id" "rooms" "room_id"
check_orphan "room_tags" "user_id" "users" "user_id"
check_orphan "read_markers" "room_id" "rooms" "room_id"
check_orphan "read_markers" "user_id" "users" "user_id"
check_orphan "room_invites" "room_id" "rooms" "room_id"

echo ""
echo "--- 设备相关外键检查 ---"
echo ""

check_orphan "access_tokens" "device_id" "devices" "device_id"
check_orphan "refresh_tokens" "device_id" "devices" "device_id"

echo ""
echo "--- 会员相关外键检查 ---"
echo ""

check_orphan "private_sessions" "user_id_1" "users" "user_id"
check_orphan "private_sessions" "user_id_2" "users" "user_id"
check_orphan "private_messages" "session_id" "private_sessions" "id"

echo ""
echo "--- 外键约束存在性检查 ---"
echo ""

EXPECTED_FKS=(
    "devices:user_id:users:user_id"
    "access_tokens:user_id:users:user_id"
    "refresh_tokens:user_id:users:user_id"
    "user_threepids:user_id:users:user_id"
    "room_memberships:room_id:rooms:room_id"
    "room_memberships:user_id:users:user_id"
    "events:room_id:rooms:room_id"
)

for entry in "${EXPECTED_FKS[@]}"; do
    IFS=':' read -r child_table child_col parent_table parent_col <<< "$entry"

    TOTAL_CHECKS=$((TOTAL_CHECKS + 1))

    fk_exists=$(psql -h "$DB_HOST" -U "$DB_USER" -d "$DB_NAME" -t -c "
        SELECT COUNT(*) FROM information_schema.table_constraints tc
        JOIN information_schema.key_column_usage kcu
            ON tc.constraint_name = kcu.constraint_name
            AND tc.table_schema = kcu.table_schema
        WHERE tc.constraint_type = 'FOREIGN KEY'
        AND tc.table_name = '$child_table'
        AND kcu.column_name = '$child_col'
        AND kcu referenced_table_name = '$parent_table'
        AND kcu referenced_column_name = '$parent_col';
    " 2>/dev/null | xargs)

    if [ "$fk_exists" -gt 0 ]; then
        log_success "外键 $child_table.$child_col -> $parent_table.$parent_col 存在 ✓"
        PASSED_CHECKS=$((PASSED_CHECKS + 1))
    else
        log_warning "外键 $child_table.$child_col -> $parent_table.$parent_col 不存在 (可能使用软删除)"
    fi
done

echo ""
echo "--- 数据统计 ---"
echo ""

log_info "总检查数: $TOTAL_CHECKS"
log_info "通过检查: $PASSED_CHECKS"

if [ $ERRORS -gt 0 ]; then
    log_info "失败检查: $ERRORS"
fi

echo ""
echo "=========================================="
if [ $ERRORS -gt 0 ]; then
    echo -e "  检查完成: ${RED}$ERRORS 个问题${NC}"
    echo "=========================================="
    exit 1
else
    echo -e "  检查完成: ${GREEN}所有外键完整性检查通过${NC}"
    echo "=========================================="
    exit 0
fi