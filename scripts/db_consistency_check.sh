#!/bin/bash
#===============================================================================
# 数据库一致性检查脚本
#
# 功能：
#   1. 检查 SQL Schema vs Rust 代码一致性
#   2. 检查字段命名规范
#   3. 检查索引完整性
#
# 使用方法：
#   ./db_consistency_check.sh                    # 检查所有
#   ./db_consistency_check.sh --fields          # 仅检查字段命名
#   ./db_consistency_check.sh --indexes         # 仅检查索引
#   ./db_consistency_check.sh --diff            # 检查差异
#   ./db_consistency_check.sh --help            # 显示帮助
#===============================================================================

set -e

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# 配置
DB_NAME="${DB_NAME:-synapse}"
DB_USER="${DB_USER:-synapse}"
DB_HOST="${DB_HOST:-localhost}"
DB_PORT="${DB_PORT:-5432}"

# 默认检查项
CHECK_FIELDS=true
CHECK_INDEXES=true
CHECK_DIFF=true

#-------------------------------------------------------------------------------
# 帮助信息
#-------------------------------------------------------------------------------
show_help() {
    echo "数据库一致性检查脚本"
    echo ""
    echo "使用方法: $0 [选项]"
    echo ""
    echo "选项:"
    echo "  --fields      仅检查字段命名规范"
    echo "  --indexes     仅检查索引完整性"
    echo "  --diff        仅检查 SQL vs Rust 差异"
    echo "  --all         检查所有 (默认)"
    echo "  --help        显示此帮助信息"
    echo ""
    echo "环境变量:"
    echo "  DB_NAME       数据库名称 (默认: synapse)"
    echo "  DB_USER       数据库用户 (默认: synapse)"
    echo "  DB_HOST       数据库主机 (默认: localhost)"
    echo "  DB_PORT       数据库端口 (默认: 5432)"
    echo ""
    echo "示例:"
    echo "  $0                                    # 检查所有"
    echo "  DB_NAME=synapse $0 --fields           # 检查字段命名"
}

#-------------------------------------------------------------------------------
# 工具函数
#-------------------------------------------------------------------------------
print_header() {
    echo ""
    echo -e "${BLUE}========================================${NC}"
    echo -e "${BLUE}$1${NC}"
    echo -e "${BLUE}========================================${NC}"
}

print_success() {
    echo -e "${GREEN}✓ $1${NC}"
}

print_error() {
    echo -e "${RED}✗ $1${NC}"
}

print_warning() {
    echo -e "${YELLOW}⚠ $1${NC}"
}

print_info() {
    echo -e "${BLUE}ℹ $1${NC}"
}

#-------------------------------------------------------------------------------
# 检查字段命名规范
#-------------------------------------------------------------------------------
check_field_naming() {
    print_header "检查字段命名规范"

    local errors=0

    # 检查 _ts 后缀字段应该是 BIGINT 类型
    echo "检查 _ts 后缀字段类型..."
    local ts_fields=$(psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -t -A -c "
        SELECT table_name || '.' || column_name || ' -> ' || data_type
        FROM information_schema.columns
        WHERE column_name ~ '_ts$'
        AND data_type != 'bigint'
        AND table_schema = 'public';
    " 2>/dev/null || true)

    if [ -n "$ts_fields" ]; then
        print_error "发现非 BIGINT 类型的 _ts 后缀字段:"
        echo "$ts_fields" | while read line; do
            echo "  - $line"
        done
        ((errors++))
    else
        print_success "所有 _ts 后缀字段类型正确"
    fi

    # 检查 _at 后缀字段应该是 BIGINT 类型
    echo "检查 _at 后缀字段类型..."
    local at_fields=$(psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -t -A -c "
        SELECT table_name || '.' || column_name || ' -> ' || data_type
        FROM information_schema.columns
        WHERE column_name ~ '_at$'
        AND data_type != 'bigint'
        AND table_schema = 'public';
    " 2>/dev/null || true)

    if [ -n "$at_fields" ]; then
        print_error "发现非 BIGINT 类型的 _at 后缀字段:"
        echo "$at_fields" | while read line; do
            echo "  - $line"
        done
        ((errors++))
    else
        print_success "所有 _at 后缀字段类型正确"
    fi

    # 检查布尔字段应该有 is_ 或 has_ 前缀
    echo "检查布尔字段命名..."
    local bool_fields=$(psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -t -A -c "
        SELECT table_name || '.' || column_name
        FROM information_schema.columns
        WHERE data_type = 'boolean'
        AND column_name !~ '^is_'
        AND column_name !~ '^has_'
        AND column_name NOT IN ('must_change_password', 'is_one_time_keys_published', 'is_fallback_key_published')
        AND table_schema = 'public';
    " 2>/dev/null || true)

    if [ -n "$bool_fields" ]; then
        print_warning "发现可能不符合规范的布尔字段 (建议优化，非强制):"
        echo "$bool_fields" | while read line; do
            echo "  - $line"
        done
    else
        print_success "布尔字段命名符合规范"
    fi

    # 检查禁止的字段名
    echo "检查禁止的字段名..."
    local forbidden_fields=$(psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -t -A -c "
        SELECT column_name || ' in table ' || table_name
        FROM information_schema.columns
        WHERE column_name IN ('created_at', 'updated_at', 'enabled', 'invalidated',
                              'expires_ts', 'revoked_ts', 'validated_ts', 'last_used_at')
        AND table_schema = 'public';
    " 2>/dev/null || true)

    if [ -n "$forbidden_fields" ]; then
        print_error "发现禁止使用的字段名:"
        echo "$forbidden_fields" | while read line; do
            echo "  - $line"
        done
        ((errors++))
    else
        print_success "未发现禁止的字段名"
    fi

    return $errors
}

#-------------------------------------------------------------------------------
# 检查索引完整性
#-------------------------------------------------------------------------------
check_indexes() {
    print_header "检查索引完整性"

    local errors=0

    # 检查缺失的索引 (根据已知的问题表)
    echo "检查 pushers 表索引..."
    local pushers_index=$(psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -t -A -c "
        SELECT 1 FROM pg_indexes
        WHERE tablename = 'pushers'
        AND indexname = 'idx_pushers_enabled';
    " 2>/dev/null || true)

    if [ -z "$pushers_index" ]; then
        print_warning "pushers 表缺少 idx_pushers_enabled 索引"
        print_info "建议: CREATE INDEX idx_pushers_enabled ON pushers(is_enabled) WHERE is_enabled = TRUE;"
        ((errors++))
    else
        print_success "pushers 表索引完整"
    fi

    echo "检查 space_children 表索引..."
    local space_children_index=$(psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -t -A -c "
        SELECT 1 FROM pg_indexes
        WHERE tablename = 'space_children'
        AND indexname = 'idx_space_children_room';
    " 2>/dev/null || true)

    if [ -z "$space_children_index" ]; then
        print_warning "space_children 表缺少 idx_space_children_room 索引"
        print_info "建议: CREATE INDEX idx_space_children_room ON space_children(room_id);"
        ((errors++))
    else
        print_success "space_children 表索引完整"
    fi

    # 检查未使用的索引
    echo "检查未使用的索引..."
    local unused_indexes=$(psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -t -A -c "
        SELECT indexrelname
        FROM pg_stat_user_indexes
        WHERE idx_scan = 0
        AND schemaname = 'public'
        LIMIT 10;
    " 2>/dev/null || true)

    if [ -n "$unused_indexes" ]; then
        print_warning "发现未使用的索引 (可能需要优化):"
        echo "$unused_indexes" | while read line; do
            echo "  - $line"
        done
    else
        print_success "所有索引都有使用记录"
    fi

    return $errors
}

#-------------------------------------------------------------------------------
# 检查 SQL vs Rust 差异
#-------------------------------------------------------------------------------
check_diff() {
    print_header "检查 SQL vs Rust 代码差异"

    local errors=0

    # 检查 Rust 代码中的表定义是否与 SQL 一致
    echo "检查 database_initializer.rs 中的表定义..."

    local rust_file="src/services/database_initializer.rs"
    if [ ! -f "$rust_file" ]; then
        print_warning "未找到 $rust_file，跳过 Rust 检查"
        return 0
    fi

    # 检查废弃表是否仍在使用
    echo "检查废弃表..."
    local deprecated_tables=("threepids" "reports")

    for table in "${deprecated_tables[@]}"; do
        if grep -q "CREATE TABLE.*$table" "$rust_file" 2>/dev/null; then
            print_warning "发现废弃表仍在 Rust 代码中定义: $table"
            print_info "建议: 移除 $table 表的创建代码，功能已合并到其他表"
            ((errors++))
        fi
    done

    # 检查关键表是否存在
    echo "检查关键表是否存在..."
    local critical_tables=(
        "users"
        "devices"
        "access_tokens"
        "refresh_tokens"
        "user_threepids"
        "rooms"
        "events"
        "room_memberships"
    )

    for table in "${critical_tables[@]}"; do
        local exists=$(psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -t -A -c "
            SELECT 1 FROM pg_tables WHERE tablename = '$table' AND schemaname = 'public';
        " 2>/dev/null || true)

        if [ -z "$exists" ]; then
            print_error "关键表不存在: $table"
            ((errors++))
        else
            print_success "关键表存在: $table"
        fi
    done

    return $errors
}

#-------------------------------------------------------------------------------
# 主函数
#-------------------------------------------------------------------------------
main() {
    # 解析参数
    case "${1:-}" in
        --help|-h)
            show_help
            exit 0
            ;;
        --fields)
            CHECK_DIFF=false
            CHECK_INDEXES=false
            ;;
        --indexes)
            CHECK_FIELDS=false
            CHECK_DIFF=false
            ;;
        --diff)
            CHECK_FIELDS=false
            CHECK_INDEXES=false
            ;;
        --all)
            ;;
        *)
            if [ -n "$1" ]; then
                print_error "未知参数: $1"
                show_help
                exit 1
            fi
            ;;
    esac

    echo ""
    print_info "数据库一致性检查"
    print_info "数据库: $DB_NAME @ $DB_HOST:$DB_PORT"
    echo ""

    local total_errors=0

    if [ "$CHECK_FIELDS" = true ]; then
        if ! check_field_naming; then
            ((total_errors++))
        fi
    fi

    if [ "$CHECK_INDEXES" = true ]; then
        if ! check_indexes; then
            ((total_errors++))
        fi
    fi

    if [ "$CHECK_DIFF" = true ]; then
        if ! check_diff; then
            ((total_errors++))
        fi
    fi

    print_header "检查完成"

    if [ $total_errors -eq 0 ]; then
        print_success "所有检查通过!"
        exit 0
    else
        print_error "发现 $total_errors 个问题，请查看上述报告"
        exit 1
    fi
}

# 运行主函数
main "$@"