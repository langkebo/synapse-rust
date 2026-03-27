#!/bin/bash
# 字段命名规范检查脚本
# 验证字段命名符合 DATABASE_FIELD_STANDARDS.md 规范
# 用法: bash scripts/field_naming_check.sh

set -e

DB_NAME="${DB_NAME:-synapse}"
DB_USER="${DB_USER:-synapse}"
DB_HOST="${DB_HOST:-localhost}"

echo "=========================================="
echo "  字段命名规范检查"
echo "=========================================="
echo ""

ERRORS=0

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

echo "--- 检查禁止使用的字段名 ---"
echo ""

FORBIDDEN_COLUMNS=(
    "created_at:应使用 created_ts"
    "updated_at:应使用 updated_ts"
    "expires_ts:可选过期时间应使用 expires_at"
    "revoked_ts:可选撤销时间应使用 revoked_at"
    "validated_ts:验证时间应使用 validated_at"
    "invalidated:布尔字段应使用 is_revoked"
    "invalidated_ts:应使用 revoked_at"
    "enabled:布尔字段应使用 is_enabled"
    "last_used_at:活跃时间应使用 last_used_ts"
)

for entry in "${FORBIDDEN_COLUMNS[@]}"; do
    IFS=':' read -r column reason <<< "$entry"

    count=$(psql -h "$DB_HOST" -U "$DB_USER" -d "$DB_NAME" -t -c "
        SELECT COUNT(*) FROM information_schema.columns
        WHERE table_schema = 'public' AND column_name = '$column';
    " 2>/dev/null | xargs)

    if [ "$count" -gt 0 ]; then
        log_error "禁止字段 '$column' (found $count) - $reason"

        psql -h "$DB_HOST" -U "$DB_USER" -d "$DB_NAME" -t -c "
            SELECT table_name FROM information_schema.columns
            WHERE table_schema = 'public' AND column_name = '$column';
        " 2>/dev/null | sed 's/^ *//;s/ *$//' | grep -v '^$' | while read table; do
            echo "    - 表: $table"
        done
    else
        log_success "禁止字段 '$column': 未发现 ✓"
    fi
done

echo ""
echo "--- 检查必须使用 _ts 后缀的时间戳字段 ---"
echo ""

TS_SUFFIX_FIELDS=(
    "users:created_ts:NOT NULL"
    "users:updated_ts:NULLABLE"
    "users:password_changed_ts:NULLABLE"
    "devices:created_ts:NOT NULL"
    "devices:first_seen_ts:NOT NULL"
    "devices:last_seen_ts:NULLABLE"
    "access_tokens:created_ts:NOT NULL"
    "access_tokens:last_used_ts:NULLABLE"
    "refresh_tokens:created_ts:NOT NULL"
    "refresh_tokens:last_used_ts:NULLABLE"
    "rooms:created_ts:NOT NULL"
    "rooms:last_activity_ts:NULLABLE"
    "room_memberships:joined_ts:NULLABLE"
    "room_memberships:invited_ts:NULLABLE"
    "room_memberships:left_ts:NULLABLE"
    "room_memberships:banned_ts:NULLABLE"
    "room_memberships:updated_ts:NULLABLE"
    "events:origin_server_ts:NOT NULL"
    "events:redacted_at:NULLABLE"
    "events:processed_at:NULLABLE"
    "presence:created_ts:NOT NULL"
    "presence:updated_ts:NOT NULL"
    "presence:last_active_ts:NOT NULL"
    "user_directory:created_ts:NOT NULL"
    "user_directory:updated_ts:NULLABLE"
    "room_directory:added_ts:NOT NULL"
    "blocked_users:created_ts:NOT NULL"
    "friends:created_ts:NOT NULL"
    "friend_requests:created_ts:NOT NULL"
    "friend_requests:updated_ts:NULLABLE"
    "friend_categories:created_ts:NOT NULL"
    "private_sessions:created_ts:NOT NULL"
    "private_sessions:last_activity_ts:NOT NULL"
    "private_sessions:updated_ts:NULLABLE"
    "private_messages:created_ts:NOT NULL"
    "private_messages:read_ts:NULLABLE"
    "private_messages:deleted_at:NULLABLE"
)

for entry in "${TS_SUFFIX_FIELDS[@]}"; do
    IFS=':' read -r table column nullable <<< "$entry"

    exists=$(psql -h "$DB_HOST" -U "$DB_USER" -d "$DB_NAME" -t -c "
        SELECT COUNT(*) FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = '$table' AND column_name = '$column';
    " 2>/dev/null | xargs)

    if [ "$exists" -eq 0 ]; then
        log_warning "字段 $table.$column 不存在 (跳过)"
        continue
    fi

    if [[ "$column" == *_ts ]]; then
        log_success "$table.$column: 正确使用 _ts 后缀 ✓"
    else
        log_error "$table.$column: 应使用 _ts 后缀"
    fi
done

echo ""
echo "--- 检查必须使用 _at 后缀的可选时间戳字段 ---"
echo ""

AT_SUFFIX_FIELDS=(
    "users:password_expires_at:NULLABLE"
    "users:locked_until:NULLABLE"
    "refresh_tokens:expires_at:NULLABLE"
    "access_tokens:expires_at:NULLABLE"
    "user_threepids:validated_at:NULLABLE"
    "user_threepids:verification_expires_at:NULLABLE"
    "refresh_tokens:revoked_at:NULLABLE"
    "registration_tokens:expires_at:NULLABLE"
    "registration_tokens:last_used_ts:NOT NULL"
    "room_invites:accepted_at:NULLABLE"
    "room_invites:expires_at:NULLABLE"
    "thread_roots:last_reply_ts:NULLABLE"
    "thread_roots:updated_ts:NULLABLE"
    "thread_statistics:last_reply_at:NULLABLE"
    "thread_statistics:updated_ts:NULLABLE"
)

for entry in "${AT_SUFFIX_FIELDS[@]}"; do
    IFS=':' read -r table column nullable <<< "$entry"

    exists=$(psql -h "$DB_HOST" -U "$DB_USER" -d "$DB_NAME" -t -c "
        SELECT COUNT(*) FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = '$table' AND column_name = '$column';
    " 2>/dev/null | xargs)

    if [ "$exists" -eq 0 ]; then
        log_warning "字段 $table.$column 不存在 (跳过)"
        continue
    fi

    if [[ "$column" == *_at ]]; then
        log_success "$table.$column: 正确使用 _at 后缀 ✓"
    else
        log_error "$table.$column: 应使用 _at 后缀"
    fi
done

echo ""
echo "--- 检查布尔字段命名规范 (is_/has_ 前缀) ---"
echo ""

EXPECTED_BOOL_PREFIX=(
    "users:is_admin"
    "users:is_guest"
    "users:is_shadow_banned"
    "users:is_deactivated"
    "users:is_password_change_required"
    "users:must_change_password"
    "devices:is_restored"
    "dehydrated_devices:is_restored"
    "access_tokens:is_revoked"
    "refresh_tokens:is_revoked"
    "token_blacklist:is_revoked"
    "openid_tokens:is_valid"
    "refresh_token_families:is_compromised"
    "refresh_token_usages:is_success"
    "room_memberships:is_banned"
    "room_directory:is_public"
    "room_directory:is_searchable"
    "rooms:is_public"
    "rooms:is_federated"
    "rooms:has_guest_access"
    "room_summaries:is_world_readable"
    "room_summaries:can_guest_join"
    "room_summaries:is_federated"
    "user_threepids:is_verified"
    "blocked_users:is_blocked"
    "room_invites:is_accepted"
    "private_messages:is_read"
    "private_messages:read_by_receiver"
    "private_messages:is_deleted"
    "private_messages:is_edited"
    "thread_roots:is_fetched"
    "space_children:is_suggested"
    "room_parents:is_suggested"
)

for entry in "${EXPECTED_BOOL_PREFIX[@]}"; do
    IFS=':' read -r table column <<< "$entry"

    exists=$(psql -h "$DB_HOST" -U "$DB_USER" -d "$DB_NAME" -t -c "
        SELECT COUNT(*) FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = '$table' AND column_name = '$column';
    " 2>/dev/null | xargs)

    if [ "$exists" -eq 0 ]; then
        continue
    fi

    if [[ "$column" == is_* ]] || [[ "$column" == has_* ]]; then
        log_success "$table.$column: 正确使用布尔前缀 ✓"
    else
        log_error "$table.$column: 布尔字段应使用 is_/has_ 前缀"
    fi
done

echo ""
echo "--- 检查缺少 is_ 前缀的布尔字段 ---"
echo ""

psql -h "$DB_HOST" -U "$DB_USER" -d "$DB_NAME" -t -c "
    SELECT table_name, column_name FROM information_schema.columns
    WHERE table_schema = 'public'
    AND data_type = 'boolean'
    AND column_name NOT LIKE 'is_%'
    AND column_name NOT LIKE 'has_%'
    AND column_name NOT IN ('must_change_password')
    ORDER BY table_name, column_name;
" 2>/dev/null | sed 's/^ *//;s/ *$//' | grep -v '^$' | while read line; do
    if [ -n "$line" ]; then
        log_warning "布尔字段可能缺少前缀: $line"
    fi
done

echo ""
echo "=========================================="
if [ $ERRORS -gt 0 ]; then
    echo -e "  检查完成: ${RED}$ERRORS 个问题${NC}"
    echo "=========================================="
    exit 1
else
    echo -e "  检查完成: ${GREEN}所有检查通过${NC}"
    echo "=========================================="
    exit 0
fi