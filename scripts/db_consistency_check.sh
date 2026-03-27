#!/bin/bash
# 数据库一致性检查脚本
# 比较 SQL 迁移文件与 Rust 运行时结构体的一致性
# 用法: bash scripts/db_consistency_check.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

DB_NAME="${DB_NAME:-synapse}"
DB_USER="${DB_USER:-synapse}"
DB_HOST="${DB_HOST:-localhost}"

echo "=========================================="
echo "  数据库一致性检查"
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

check_sql_table_exists() {
    local table=$1
    psql -h "$DB_HOST" -U "$DB_USER" -d "$DB_NAME" -t -c "
        SELECT COUNT(*) FROM information_schema.tables
        WHERE table_schema = 'public' AND table_name = '$table';
    " 2>/dev/null | xargs | grep -q "^1$"
}

get_sql_columns() {
    local table=$1
    psql -h "$DB_HOST" -U "$DB_USER" -d "$DB_NAME" -t -c "
        SELECT column_name FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = '$table'
        ORDER BY ordinal_position;
    " 2>/dev/null | sed 's/^ *//;s/ *$//' | grep -v '^$'
}

check_rust_struct_exists() {
    local struct=$1
    grep -q "pub struct $struct" "$PROJECT_DIR/src/storage/models/"*.rs 2>/dev/null
}

get_rust_struct_fields() {
    local struct=$1
    local model_file=""
    for f in "$PROJECT_DIR/src/storage/models/"*.rs; do
        if grep -q "pub struct $struct" "$f"; then
            model_file="$f"
            break
        fi
    done

    if [ -z "$model_file" ]; then
        return 1
    fi

    awk -v struct="$struct" '
        /^pub struct '"$struct"'/ { in_struct=1; next }
        /^}/ { if (in_struct) exit }
        in_struct && /^[[:space:]]+pub [[:word:]]+:/ {
            sub(/^[[:space:]]+pub /, "")
            sub(/:.*$/, "")
            gsub(/[[:space:]]/, "", $0)
            print $0
        }
    ' "$model_file"
}

echo "--- SQL 表 vs Rust 结构体 映射检查 ---"
echo ""

declare -A SQL_TO_RUST=(
    ["users"]="User"
    ["devices"]="Device"
    ["access_tokens"]="AccessToken"
    ["refresh_tokens"]="RefreshToken"
    ["token_blacklist"]="TokenBlacklistEntry"
    ["user_threepids"]="UserThreepid"
    ["rooms"]="Room"
    ["room_memberships"]="RoomMembership"
    ["events"]="Event"
    ["room_summaries"]="RoomSummary"
    ["room_directory"]="RoomDirectory"
    ["room_aliases"]="RoomAlias"
    ["room_tags"]="RoomTag"
    ["read_markers"]="ReadMarker"
    ["thread_roots"]="ThreadRoot"
    ["thread_statistics"]="ThreadStatistics"
    ["room_parents"]="RoomParent"
    ["space_children"]="SpaceChild"
    ["room_invites"]="RoomInvite"
    ["presence"]="Presence"
    ["user_directory"]="UserDirectory"
    ["blocked_users"]="BlockedUser"
    ["friend_requests"]="FriendRequest"
    ["friend_categories"]="FriendCategory"
    ["friends"]="Friend"
    ["private_sessions"]="PrivateSession"
    ["private_messages"]="PrivateMessage"
)

MISMATCHES=0

for sql_table in "${!SQL_TO_RUST[@]}"; do
    rust_struct="${SQL_TO_RUST[$sql_table]}"

    if ! check_sql_table_exists "$sql_table"; then
        log_warning "SQL 表 '$sql_table' 不存在 (跳过)"
        continue
    fi

    if ! check_rust_struct_exists "$rust_struct"; then
        log_warning "Rust 结构体 '$rust_struct' 不存在 (跳过)"
        continue
    fi

    echo "检查: $sql_table -> $rust_struct"

    sql_columns=$(get_sql_columns "$sql_table" | sort)
    rust_fields=$(get_rust_struct_fields "$rust_struct" | sort)

    if [ -z "$sql_columns" ]; then
        log_error "无法获取 SQL 表 '$sql_table' 的字段"
        continue
    fi

    if [ -z "$rust_fields" ]; then
        log_error "无法获取 Rust 结构体 '$rust_struct' 的字段"
        continue
    fi

    sql_field_count=$(echo "$sql_columns" | wc -l | xargs)
    rust_field_count=$(echo "$rust_fields" | wc -l | xargs)

    if [ "$sql_field_count" != "$rust_field_count" ]; then
        log_error "字段数量不匹配: SQL=$sql_field_count, Rust=$rust_field_count"
        MISMATCHES=$((MISMATCHES + 1))
    fi

    for field in $rust_fields; do
        if ! echo "$sql_columns" | grep -qx "$field"; then
            log_error "  Rust 字段 '$field' 在 SQL 表 '$sql_table' 中不存在"
            MISMATCHES=$((MISMATCHES + 1))
        fi
    done

    for field in $sql_columns; do
        if ! echo "$rust_fields" | grep -qx "$field"; then
            log_error "  SQL 字段 '$field' 在 Rust 结构体 '$rust_struct' 中不存在"
            MISMATCHES=$((MISMATCHES + 1))
        fi
    done

    if [ $? -eq 0 ] && [ "$sql_field_count" = "$rust_field_count" ]; then
        log_success "  字段数量一致: $sql_field_count"
    fi
done

echo ""
echo "--- 字段类型一致性抽查 ---"
echo ""

SPECIAL_FIELDS=(
    "users:password_expires_at:Option<i64>"
    "user_threepids:validated_at:Option<i64>"
    "user_threepids:added_ts:i64"
    "refresh_tokens:last_used_ts:Option<i64>"
    "refresh_tokens:expires_at:Option<i64>"
    "refresh_tokens:created_ts:i64"
    "access_tokens:created_ts:i64"
    "access_tokens:expires_at:Option<i64>"
    "devices:created_ts:i64"
    "devices:last_seen_ts:Option<i64>"
    "rooms:created_ts:i64"
    "rooms:last_activity_ts:Option<i64>"
    "room_memberships:joined_ts:Option<i64>"
    "room_memberships:invited_ts:Option<i64>"
)

for entry in "${SPECIAL_FIELDS[@]}"; do
    IFS=':' read -r table field expected_type <<< "$entry"

    if ! check_sql_table_exists "$table"; then
        continue
    fi

    column_type=$(psql -h "$DB_HOST" -U "$DB_USER" -d "$DB_NAME" -t -c "
        SELECT data_type FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = '$table' AND column_name = '$field';
    " 2>/dev/null | sed 's/^ *//;s/ *$//')

    case "$expected_type" in
        "i64")
            if [[ "$column_type" == "bigint" ]]; then
                log_success "$table.$field: BIGINT ✓"
            else
                log_error "$table.$field: 类型不匹配 (期望 BIGINT, 实际 $column_type)"
            fi
            ;;
        "Option<i64>")
            if [[ "$column_type" == "character varying" ]] || [[ "$column_type" == "text" ]] || [[ "$column_type" == "bigint" ]]; then
                nullable=$(psql -h "$DB_HOST" -U "$DB_USER" -d "$DB_NAME" -t -c "
                    SELECT is_nullable FROM information_schema.columns
                    WHERE table_schema = 'public' AND table_name = '$table' AND column_name = '$field';
                " 2>/dev/null | sed 's/^ *//;s/ *$//')
                if [[ "$nullable" == "YES" ]]; then
                    log_success "$table.$field: BIGINT (nullable) ✓"
                else
                    log_error "$table.$field: 期望可空 (nullable), 实际 NOT NULL"
                fi
            else
                log_error "$table.$field: 类型不匹配 (期望 BIGINT, 实际 $column_type)"
            fi
            ;;
    esac
done

echo ""
echo "=========================================="
if [ $ERRORS -gt 0 ] || [ $MISMATCHES -gt 0 ]; then
    TOTAL=$((ERRORS + MISMATCHES))
    echo -e "  检查完成: ${RED}$TOTAL 个问题${NC}"
    echo "=========================================="
    exit 1
else
    echo -e "  检查完成: ${GREEN}所有检查通过${NC}"
    echo "=========================================="
    exit 0
fi