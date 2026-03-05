#!/bin/bash
# ============================================================================
# synapse-rust 数据库迁移管理脚本
# 版本: 1.0.0
# 创建日期: 2026-03-01
# ============================================================================

set -e

# 配置
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
MIGRATIONS_DIR="$PROJECT_ROOT/migrations"
ENV_FILE="$PROJECT_ROOT/.env"

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# 日志函数
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# 加载环境变量
load_env() {
    if [ -f "$ENV_FILE" ]; then
        log_info "加载环境变量: $ENV_FILE"
        export $(grep -v '^#' "$ENV_FILE" | xargs)
    fi
    
    export DATABASE_URL="${DATABASE_URL:-postgres://synapse:synapse@localhost:5432/synapse}"

    local parsed_user=$(echo "$DATABASE_URL" | sed -n 's|^postgres://\([^:]*\):.*$|\1|p')
    local parsed_password=$(echo "$DATABASE_URL" | sed -n 's|^postgres://[^:]*:\([^@]*\)@.*$|\1|p')
    local parsed_host=$(echo "$DATABASE_URL" | sed -n 's|^postgres://[^@]*@\([^:/]*\).*$|\1|p')
    local parsed_port=$(echo "$DATABASE_URL" | sed -n 's|^postgres://[^@]*@[^:/]*:\([0-9]*\)/.*$|\1|p')
    local parsed_db=$(echo "$DATABASE_URL" | sed -n 's|.*/\([^?]*\).*$|\1|p')

    export DB_HOST="${DB_HOST:-${parsed_host:-localhost}}"
    export DB_PORT="${DB_PORT:-${parsed_port:-5432}}"
    export DB_NAME="${DB_NAME:-${parsed_db:-synapse}}"
    export DB_USER="${DB_USER:-${parsed_user:-synapse}}"
    export DB_PASSWORD="${DB_PASSWORD:-${parsed_password:-synapse}}"
}

# 检查数据库连接
check_db_connection() {
    log_info "检查数据库连接..."
    if psql "postgres://$DB_USER:$DB_PASSWORD@$DB_HOST:$DB_PORT/$DB_NAME" -c "SELECT 1" > /dev/null 2>&1; then
        log_success "数据库连接成功"
        return 0
    else
        log_error "无法连接到数据库"
        return 1
    fi
}

# 获取当前版本
get_current_version() {
    local order_column="applied_ts"
    if ! schema_migrations_has_column "applied_ts"; then
        order_column="executed_at"
    fi
    local version=$(psql "postgres://$DB_USER:$DB_PASSWORD@$DB_HOST:$DB_PORT/$DB_NAME" -t -c "
        SELECT version FROM schema_migrations ORDER BY ${order_column} DESC LIMIT 1
    " 2>/dev/null | tr -d ' ')
    echo "$version"
}

schema_migrations_has_column() {
    local column_name="$1"
    psql "postgres://$DB_USER:$DB_PASSWORD@$DB_HOST:$DB_PORT/$DB_NAME" -t -c "
        SELECT EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_schema = 'public'
              AND table_name = 'schema_migrations'
              AND column_name = '$column_name'
        )
    " 2>/dev/null | grep -q t
}

# 列出已应用的迁移
list_applied_migrations() {
    log_info "已应用的迁移:"
    if schema_migrations_has_column "name" && schema_migrations_has_column "applied_ts"; then
        psql "postgres://$DB_USER:$DB_PASSWORD@$DB_HOST:$DB_PORT/$DB_NAME" -c "
            SELECT version, name, applied_ts
            FROM schema_migrations
            ORDER BY applied_ts DESC
        " 2>/dev/null
    elif schema_migrations_has_column "executed_at"; then
        psql "postgres://$DB_USER:$DB_PASSWORD@$DB_HOST:$DB_PORT/$DB_NAME" -c "
            SELECT version, COALESCE(description, '') AS name, executed_at AS applied_ts
            FROM schema_migrations
            ORDER BY executed_at DESC
        " 2>/dev/null
    else
        psql "postgres://$DB_USER:$DB_PASSWORD@$DB_HOST:$DB_PORT/$DB_NAME" -c "
            SELECT version FROM schema_migrations ORDER BY version DESC
        " 2>/dev/null
    fi
}

# 初始化数据库
init_database() {
    log_info "初始化数据库..."
    
    # 检查数据库是否存在
    if ! psql "postgres://$DB_USER:$DB_PASSWORD@$DB_HOST:$DB_PORT/$DB_NAME" -c "SELECT 1" > /dev/null 2>&1; then
        log_info "创建数据库: $DB_NAME"
        psql "postgres://$DB_USER:$DB_PASSWORD@$DB_HOST:$DB_PORT/postgres" -c "CREATE DATABASE $DB_NAME;" 2>/dev/null || true
    fi
    
    # 执行统一架构脚本
    local schema_file="$MIGRATIONS_DIR/00000000_unified_schema_v5.sql"
    if [ ! -f "$schema_file" ]; then
        schema_file="$MIGRATIONS_DIR/00000000_unified_schema_v4.sql"
    fi
    if [ -f "$schema_file" ]; then
        log_info "执行统一架构脚本: $schema_file"
        psql "postgres://$DB_USER:$DB_PASSWORD@$DB_HOST:$DB_PORT/$DB_NAME" -f "$schema_file"
        log_success "数据库初始化完成"
    else
        log_error "找不到统一架构脚本: $schema_file"
        return 1
    fi
}

# 执行单个迁移
apply_migration() {
    local migration_file="$1"
    local filename=$(basename "$migration_file")
    local version=$(echo "$filename" | sed 's/\.sql$//')
    
    log_info "应用迁移: $filename"
    
    local start_time=$(date +%s%3N)
    
    if psql "postgres://$DB_USER:$DB_PASSWORD@$DB_HOST:$DB_PORT/$DB_NAME" -f "$migration_file"; then
        local end_time=$(date +%s%3N)
        local duration=$((end_time - start_time))
        
        if schema_migrations_has_column "name" && schema_migrations_has_column "applied_ts" && schema_migrations_has_column "execution_time_ms"; then
            psql "postgres://$DB_USER:$DB_PASSWORD@$DB_HOST:$DB_PORT/$DB_NAME" -c "
                INSERT INTO schema_migrations (version, name, applied_ts, execution_time_ms)
                VALUES ('$version', '$filename', EXTRACT(EPOCH FROM NOW()) * 1000, $duration)
                ON CONFLICT (version) DO NOTHING
            " 2>/dev/null
        elif schema_migrations_has_column "executed_at" && schema_migrations_has_column "success"; then
            psql "postgres://$DB_USER:$DB_PASSWORD@$DB_HOST:$DB_PORT/$DB_NAME" -c "
                INSERT INTO schema_migrations (version, checksum, executed_at, success, error_message, description)
                VALUES ('$version', md5('$filename'), NOW(), TRUE, NULL, '$filename')
                ON CONFLICT (version) DO NOTHING
            " 2>/dev/null
        else
            psql "postgres://$DB_USER:$DB_PASSWORD@$DB_HOST:$DB_PORT/$DB_NAME" -c "
                INSERT INTO schema_migrations (version)
                VALUES ('$version')
                ON CONFLICT (version) DO NOTHING
            " 2>/dev/null
        fi
        
        log_success "迁移完成: $filename (${duration}ms)"
        return 0
    else
        log_error "迁移失败: $filename"
        return 1
    fi
}

# 执行所有待处理的迁移
apply_pending_migrations() {
    log_info "检查待处理的迁移..."
    
    local current_version=$(get_current_version)
    log_info "当前版本: $current_version"
    
    local pending=0
    for file in $(ls "$MIGRATIONS_DIR"/*.sql 2>/dev/null | sort); do
        local filename=$(basename "$file")
        local version=$(echo "$filename" | sed 's/\.sql$//')
        
        # 跳过已应用的迁移
        if psql "postgres://$DB_USER:$DB_PASSWORD@$DB_HOST:$DB_PORT/$DB_NAME" -t -c "
            SELECT 1 FROM schema_migrations WHERE version = '$version'
        " 2>/dev/null | grep -q 1; then
            continue
        fi
        
        # 跳过统一架构脚本（已在初始化时执行）
        if [[ "$filename" == "00000000_unified_schema_v4.sql" || "$filename" == "00000000_unified_schema_v5.sql" ]]; then
            continue
        fi
        
        pending=$((pending + 1))
        apply_migration "$file" || return 1
    done
    
    if [ $pending -eq 0 ]; then
        log_success "没有待处理的迁移"
    else
        log_success "已应用 $pending 个迁移"
    fi
}

# 回滚到指定版本
rollback_to_version() {
    local target_version="$1"
    log_warning "回滚到版本: $target_version"
    log_warning "注意: 此操作不可逆！"
    
    read -p "确认回滚? (y/N): " confirm
    if [ "$confirm" != "y" ] && [ "$confirm" != "Y" ]; then
        log_info "取消回滚"
        return 0
    fi
    
    # 这里需要实现回滚逻辑
    log_warning "回滚功能需要配合回滚脚本使用"
}

# 验证数据库架构
validate_schema() {
    log_info "验证数据库架构..."
    
    local errors=0
    
    # 检查必要的表
    local required_tables=(
        "users"
        "devices"
        "access_tokens"
        "refresh_tokens"
        "rooms"
        "events"
        "device_keys"
        "schema_migrations"
    )
    
    for table in "${required_tables[@]}"; do
        if psql "postgres://$DB_USER:$DB_PASSWORD@$DB_HOST:$DB_PORT/$DB_NAME" -t -c "
            SELECT EXISTS (
                SELECT FROM information_schema.tables 
                WHERE table_name = '$table'
            )
        " 2>/dev/null | grep -q t; then
            log_success "表存在: $table"
        else
            log_error "表缺失: $table"
            errors=$((errors + 1))
        fi
    done
    
    # 检查字段命名规范
    log_info "检查字段命名规范..."
    
    # 检查 created_at 字段（应为 created_ts）
    local created_at_count=$(psql "postgres://$DB_USER:$DB_PASSWORD@$DB_HOST:$DB_PORT/$DB_NAME" -t -c "
        SELECT COUNT(*) FROM information_schema.columns 
        WHERE column_name = 'created_at'
    " 2>/dev/null | tr -d ' ')
    
    if [ "$created_at_count" -gt 0 ]; then
        log_warning "发现 $created_at_count 个 created_at 字段（建议使用 created_ts）"
    fi
    
    # 检查 updated_at 字段（应为 updated_ts）
    local updated_at_count=$(psql "postgres://$DB_USER:$DB_PASSWORD@$DB_HOST:$DB_PORT/$DB_NAME" -t -c "
        SELECT COUNT(*) FROM information_schema.columns 
        WHERE column_name = 'updated_at'
    " 2>/dev/null | tr -d ' ')
    
    if [ "$updated_at_count" -gt 0 ]; then
        log_warning "发现 $updated_at_count 个 updated_at 字段（建议使用 updated_ts）"
    fi
    
    if [ $errors -eq 0 ]; then
        log_success "数据库架构验证通过"
        return 0
    else
        log_error "数据库架构验证失败，发现 $errors 个错误"
        return 1
    fi
}

# 显示帮助信息
show_help() {
    echo "synapse-rust 数据库迁移管理工具"
    echo ""
    echo "用法: $0 <命令> [参数]"
    echo ""
    echo "命令:"
    echo "  init        初始化数据库（执行统一架构脚本）"
    echo "  migrate     执行所有待处理的迁移"
    echo "  status      显示当前迁移状态"
    echo "  validate    验证数据库架构"
    echo "  rollback    回滚到指定版本（需要回滚脚本）"
    echo "  help        显示此帮助信息"
    echo ""
    echo "环境变量:"
    echo "  DATABASE_URL    数据库连接字符串"
    echo "  DB_HOST         数据库主机 (默认: localhost)"
    echo "  DB_PORT         数据库端口 (默认: 5432)"
    echo "  DB_NAME         数据库名称 (默认: synapse)"
    echo "  DB_USER         数据库用户 (默认: synapse)"
    echo "  DB_PASSWORD     数据库密码 (默认: synapse)"
}

# 主函数
main() {
    local command="${1:-help}"
    
    load_env
    
    case "$command" in
        init)
            check_db_connection && init_database
            ;;
        migrate)
            check_db_connection && apply_pending_migrations
            ;;
        status)
            check_db_connection && list_applied_migrations
            ;;
        validate)
            check_db_connection && validate_schema
            ;;
        rollback)
            check_db_connection && rollback_to_version "${2:-}"
            ;;
        help|--help|-h)
            show_help
            ;;
        *)
            log_error "未知命令: $command"
            show_help
            exit 1
            ;;
    esac
}

main "$@"
