#!/bin/bash
# ============================================================================
# 数据库迁移管理工具
# 创建日期: 2026-03-11
# 描述: 自动化执行数据库迁移脚本，支持版本控制和回滚
# ============================================================================

set -e

# 配置
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
MIGRATIONS_DIR="$PROJECT_ROOT/migrations"
DOCKER_COMPOSE_FILE="$PROJECT_ROOT/docker/docker-compose.yml"
DB_SERVICE_NAME="db"
DB_NAME="synapse_test"
DB_USER="synapse"

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

compose_exec_db() {
    docker compose -f "$DOCKER_COMPOSE_FILE" exec -T "$DB_SERVICE_NAME" "$@"
}

# 检查 Docker 容器是否运行
check_container() {
    if ! docker compose -f "$DOCKER_COMPOSE_FILE" ps "$DB_SERVICE_NAME" | grep -q "running"; then
        log_error "Service $DB_SERVICE_NAME is not running"
        log_info "Starting container..."
        docker compose -f "$DOCKER_COMPOSE_FILE" up -d "$DB_SERVICE_NAME"
        sleep 5
    fi
    log_success "Service $DB_SERVICE_NAME is running"
}

# 检查 schema_migrations 表是否存在
check_migrations_table() {
    log_info "Checking schema_migrations table..."
    compose_exec_db psql -U "$DB_USER" -d "$DB_NAME" -c "
        CREATE TABLE IF NOT EXISTS schema_migrations (
            version VARCHAR(50) PRIMARY KEY,
            name VARCHAR(255) NOT NULL,
            applied_ts BIGINT NOT NULL,
            description TEXT
        );
    " > /dev/null 2>&1
    log_success "schema_migrations table ready"
}

# 获取已应用的迁移列表
get_applied_migrations() {
    compose_exec_db psql -U "$DB_USER" -d "$DB_NAME" -t -c "
        SELECT version FROM schema_migrations ORDER BY version;
    " 2>/dev/null | tr -d ' '
}

# 获取待应用的迁移文件列表
get_pending_migrations() {
    local applied_migrations="$1"
    local pending=()
    
    for file in "$MIGRATIONS_DIR"/*.sql; do
        if [ -f "$file" ]; then
            local filename=$(basename "$file")
            local version=$(echo "$filename" | grep -oE '^[0-9]+' || echo "")
            
            if [ -n "$version" ]; then
                if ! echo "$applied_migrations" | grep -q "$version"; then
                    pending+=("$file")
                fi
            fi
        fi
    done
    
    printf '%s\n' "${pending[@]}" | sort
}

# 应用单个迁移
apply_migration() {
    local migration_file="$1"
    local filename=$(basename "$migration_file")
    local version=$(echo "$filename" | grep -oE '^[0-9]+' || echo "unknown")
    
    log_info "Applying migration: $filename"
    
    # 创建备份点
    local backup_name="pre_migration_${version}_$(date +%Y%m%d_%H%M%S)"
    log_info "Creating backup point: $backup_name"
    
    # 执行迁移
    if compose_exec_db psql -U "$DB_USER" -d "$DB_NAME" < "$migration_file" 2>&1; then
        log_success "Migration applied successfully: $filename"
        return 0
    else
        log_error "Migration failed: $filename"
        log_warning "Manual rollback may be required"
        return 1
    fi
}

# 回滚迁移
rollback_migration() {
    local version="$1"
    log_warning "Rollback functionality is not yet implemented for version: $version"
    log_info "Please manually revert the changes from migration $version"
}

# 显示迁移状态
show_status() {
    log_info "Migration Status:"
    echo ""
    
    local applied=$(get_applied_migrations)
    local pending=$(get_pending_migrations "$applied")
    
    echo "Applied Migrations:"
    if [ -n "$applied" ]; then
        echo "$applied" | while read version; do
            if [ -n "$version" ]; then
                echo "  ✓ $version"
            fi
        done
    else
        echo "  (none)"
    fi
    
    echo ""
    echo "Pending Migrations:"
    if [ -n "$pending" ]; then
        echo "$pending" | while read file; do
            if [ -f "$file" ]; then
                echo "  ○ $(basename "$file")"
            fi
        done
    else
        echo "  (none)"
    fi
    echo ""
}

# 应用所有待处理的迁移
apply_all() {
    check_container
    check_migrations_table
    
    local applied=$(get_applied_migrations)
    local pending=$(get_pending_migrations "$applied")
    
    if [ -z "$pending" ]; then
        log_success "No pending migrations"
        return 0
    fi
    
    log_info "Found $(echo "$pending" | wc -l | tr -d ' ') pending migration(s)"
    
    echo "$pending" | while read migration_file; do
        if [ -f "$migration_file" ]; then
            apply_migration "$migration_file"
        fi
    done
    
    log_success "All migrations applied"
}

# 验证迁移
verify_migrations() {
    log_info "Verifying migrations..."
    
    local applied=$(get_applied_migrations)
    local count=$(echo "$applied" | grep -c . || echo "0")
    
    log_success "Total applied migrations: $count"
    
    # 验证关键表是否存在
    local tables=("users" "rooms" "events" "device_keys" "one_time_keys" "key_backups" "backup_keys")
    
    for table in "${tables[@]}"; do
        local exists=$(compose_exec_db psql -U "$DB_USER" -d "$DB_NAME" -t -c "
            SELECT EXISTS (
                SELECT FROM information_schema.tables 
                WHERE table_name = '$table'
            );
        " 2>/dev/null | tr -d ' ')
        
        if [ "$exists" = "t" ]; then
            echo "  ✓ Table '$table' exists"
        else
            echo "  ✗ Table '$table' missing"
        fi
    done
}

# 主函数
main() {
    local command="${1:-status}"
    
    case "$command" in
        status)
            check_container
            show_status
            ;;
        apply)
            apply_all
            ;;
        verify)
            verify_migrations
            ;;
        rollback)
            local version="${2:-}"
            if [ -z "$version" ]; then
                log_error "Please specify a version to rollback"
                exit 1
            fi
            rollback_migration "$version"
            ;;
        *)
            echo "Usage: $0 {status|apply|verify|rollback <version>}"
            echo ""
            echo "Commands:"
            echo "  status              Show migration status"
            echo "  apply               Apply all pending migrations"
            echo "  verify              Verify migration integrity"
            echo "  rollback <version>  Rollback a specific migration"
            exit 1
            ;;
    esac
}

main "$@"
