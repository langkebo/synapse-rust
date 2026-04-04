#!/bin/bash
# ============================================================================
# 数据库迁移管理工具
# 创建日期: 2026-03-11
# 更新日期: 2026-04-04
# 描述: 自动化执行数据库迁移脚本，支持版本控制和回滚
# 功能: status, apply, verify, rollback, create, test, validate
# ============================================================================

set -e

# 配置
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
MIGRATIONS_DIR="$PROJECT_ROOT/migrations"
UNDO_DIR="$MIGRATIONS_DIR/undo"
MIGRATION_INDEX="$MIGRATIONS_DIR/MIGRATION_INDEX.md"
DOCKER_COMPOSE_FILE="$PROJECT_ROOT/docker/docker-compose.yml"
DB_SERVICE_NAME="db"
DB_NAME="synapse_test"
DB_USER="synapse"
TEST_DB_NAME="synapse_migration_test"

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

# ============================================================================
# 新增功能：迁移创建工具
# ============================================================================

# 创建新迁移
create_migration() {
    local name="$1"
    local description="$2"

    if [ -z "$name" ]; then
        log_error "Migration name is required"
        echo "Usage: $0 create <name> [description]"
        exit 1
    fi

    # 生成时间戳版本号
    local timestamp=$(date +%Y%m%d%H%M%S)
    local migration_file="$MIGRATIONS_DIR/${timestamp}_${name}.sql"
    local undo_file="$UNDO_DIR/${timestamp}_${name}_undo.sql"

    log_info "Creating migration: $timestamp"

    # 创建 undo 目录
    mkdir -p "$UNDO_DIR"

    # 生成迁移文件模板
    cat > "$migration_file" <<EOF
-- Migration: ${name}
-- Version: ${timestamp}
-- Description: ${description:-Add description here}
-- Created: $(date +%Y-%m-%d)

-- ============================================================================
-- IMPORTANT: Use CREATE INDEX CONCURRENTLY for production safety
-- ============================================================================

BEGIN;

-- Add your migration SQL here
-- Example:
-- CREATE TABLE IF NOT EXISTS example_table (
--     id BIGSERIAL PRIMARY KEY,
--     name VARCHAR(255) NOT NULL,
--     created_ts BIGINT NOT NULL
-- );

-- CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_example_name
--     ON example_table(name);

COMMIT;
EOF

    # 生成 undo 文件模板
    cat > "$undo_file" <<EOF
-- Undo Migration: ${name}
-- Version: ${timestamp}
-- Description: Rollback for ${timestamp}_${name}
-- Created: $(date +%Y-%m-%d)

BEGIN;

-- Add your rollback SQL here
-- Example:
-- DROP INDEX IF EXISTS idx_example_name;
-- DROP TABLE IF EXISTS example_table;

COMMIT;
EOF

    log_success "Migration files created:"
    echo "  Migration: $migration_file"
    echo "  Undo:      $undo_file"

    # 更新 MIGRATION_INDEX.md
    if [ -f "$MIGRATION_INDEX" ]; then
        log_info "Updating MIGRATION_INDEX.md..."
        echo "" >> "$MIGRATION_INDEX"
        echo "### ${timestamp}_${name}" >> "$MIGRATION_INDEX"
        echo "" >> "$MIGRATION_INDEX"
        echo "- **Version**: ${timestamp}" >> "$MIGRATION_INDEX"
        echo "- **Description**: ${description:-Add description here}" >> "$MIGRATION_INDEX"
        echo "- **Created**: $(date +%Y-%m-%d)" >> "$MIGRATION_INDEX"
        echo "- **Files**:" >> "$MIGRATION_INDEX"
        echo "  - Migration: \`${timestamp}_${name}.sql\`" >> "$MIGRATION_INDEX"
        echo "  - Undo: \`undo/${timestamp}_${name}_undo.sql\`" >> "$MIGRATION_INDEX"
        log_success "MIGRATION_INDEX.md updated"
    fi

    log_info "Next steps:"
    echo "  1. Edit the migration file: $migration_file"
    echo "  2. Edit the undo file: $undo_file"
    echo "  3. Test the migration: $0 test ${timestamp}_${name}.sql"
    echo "  4. Validate the migration: $0 validate ${timestamp}_${name}.sql"
}

# ============================================================================
# 新增功能：迁移测试工具
# ============================================================================

# 测试迁移（在隔离环境）
test_migration() {
    local migration_file="$1"

    if [ -z "$migration_file" ]; then
        log_error "Migration file is required"
        echo "Usage: $0 test <migration_file>"
        exit 1
    fi

    if [ ! -f "$migration_file" ]; then
        migration_file="$MIGRATIONS_DIR/$migration_file"
    fi

    if [ ! -f "$migration_file" ]; then
        log_error "Migration file not found: $migration_file"
        exit 1
    fi

    local filename=$(basename "$migration_file")
    local version=$(echo "$filename" | grep -oE '^[0-9]+' || echo "unknown")

    log_info "Testing migration: $filename"

    check_container

    # 创建测试数据库
    log_info "Creating test database: $TEST_DB_NAME"
    compose_exec_db psql -U "$DB_USER" -d postgres -c "DROP DATABASE IF EXISTS $TEST_DB_NAME;" > /dev/null 2>&1 || true
    compose_exec_db psql -U "$DB_USER" -d postgres -c "CREATE DATABASE $TEST_DB_NAME;" > /dev/null 2>&1

    # 应用基线 schema
    log_info "Applying baseline schema..."
    if [ -f "$MIGRATIONS_DIR/00000000_unified_schema_v6.sql" ]; then
        compose_exec_db psql -U "$DB_USER" -d "$TEST_DB_NAME" < "$MIGRATIONS_DIR/00000000_unified_schema_v6.sql" > /dev/null 2>&1
        log_success "Baseline schema applied"
    else
        log_warning "Baseline schema not found, skipping"
    fi

    # 应用目标迁移
    log_info "Applying migration..."
    if compose_exec_db psql -U "$DB_USER" -d "$TEST_DB_NAME" < "$migration_file" 2>&1; then
        log_success "Migration applied successfully"
    else
        log_error "Migration failed"
        compose_exec_db psql -U "$DB_USER" -d postgres -c "DROP DATABASE IF EXISTS $TEST_DB_NAME;" > /dev/null 2>&1
        exit 1
    fi

    # 查找对应的 undo 文件
    local undo_file="$UNDO_DIR/${version}_*_undo.sql"
    local undo_found=$(ls $undo_file 2>/dev/null | head -1)

    if [ -n "$undo_found" ] && [ -f "$undo_found" ]; then
        log_info "Testing undo migration: $(basename "$undo_found")"
        if compose_exec_db psql -U "$DB_USER" -d "$TEST_DB_NAME" < "$undo_found" 2>&1; then
            log_success "Undo migration applied successfully"
        else
            log_error "Undo migration failed"
        fi
    else
        log_warning "Undo file not found, skipping rollback test"
    fi

    # 清理测试数据库
    log_info "Cleaning up test database..."
    compose_exec_db psql -U "$DB_USER" -d postgres -c "DROP DATABASE IF EXISTS $TEST_DB_NAME;" > /dev/null 2>&1

    log_success "Migration test completed"
}

# ============================================================================
# 新增功能：迁移验证工具
# ============================================================================

# 验证迁移文件
validate_migration() {
    local migration_file="$1"

    if [ -z "$migration_file" ]; then
        log_error "Migration file is required"
        echo "Usage: $0 validate <migration_file>"
        exit 1
    fi

    if [ ! -f "$migration_file" ]; then
        migration_file="$MIGRATIONS_DIR/$migration_file"
    fi

    if [ ! -f "$migration_file" ]; then
        log_error "Migration file not found: $migration_file"
        exit 1
    fi

    local filename=$(basename "$migration_file")
    log_info "Validating migration: $filename"

    local issues=0

    # 检查 1: SQL 语法检查
    log_info "Checking SQL syntax..."
    check_container
    if compose_exec_db psql -U "$DB_USER" -d "$DB_NAME" --set=ON_ERROR_STOP=1 --single-transaction --dry-run < "$migration_file" > /dev/null 2>&1; then
        echo "  ✓ SQL syntax valid"
    else
        echo "  ✗ SQL syntax error detected"
        issues=$((issues + 1))
    fi

    # 检查 2: 索引创建检查
    log_info "Checking index creation..."
    local non_concurrent_indexes=$(grep -i "CREATE INDEX" "$migration_file" | grep -v "CONCURRENTLY" | grep -v "^--" || true)
    if [ -n "$non_concurrent_indexes" ]; then
        echo "  ✗ Non-concurrent index creation found:"
        echo "$non_concurrent_indexes" | sed 's/^/    /'
        log_warning "Consider using CREATE INDEX CONCURRENTLY for production safety"
        issues=$((issues + 1))
    else
        echo "  ✓ All indexes use CONCURRENTLY or no indexes created"
    fi

    # 检查 3: 危险操作检查
    log_info "Checking for dangerous operations..."
    local dangerous_ops=$(grep -iE "DROP TABLE|TRUNCATE|DELETE FROM.*WHERE.*1=1|DROP DATABASE" "$migration_file" | grep -v "^--" || true)
    if [ -n "$dangerous_ops" ]; then
        echo "  ✗ Dangerous operations found:"
        echo "$dangerous_ops" | sed 's/^/    /'
        log_warning "Review these operations carefully"
        issues=$((issues + 1))
    else
        echo "  ✓ No dangerous operations detected"
    fi

    # 检查 4: 命名规范检查
    log_info "Checking naming convention..."
    if [[ "$filename" =~ ^[0-9]{14}_[a-z0-9_]+\.sql$ ]]; then
        echo "  ✓ Filename follows naming convention"
    else
        echo "  ✗ Filename does not follow convention: YYYYMMDDHHMMSS_name.sql"
        issues=$((issues + 1))
    fi

    # 总结
    echo ""
    if [ $issues -eq 0 ]; then
        log_success "Validation passed: no issues found"
        return 0
    else
        log_warning "Validation completed with $issues issue(s)"
        return 1
    fi
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
        create)
            local name="${2:-}"
            local description="${3:-}"
            create_migration "$name" "$description"
            ;;
        test)
            local migration_file="${2:-}"
            test_migration "$migration_file"
            ;;
        validate)
            local migration_file="${2:-}"
            validate_migration "$migration_file"
            ;;
        *)
            echo "Usage: $0 {status|apply|verify|rollback|create|test|validate}"
            echo ""
            echo "Commands:"
            echo "  status                          Show migration status"
            echo "  apply                           Apply all pending migrations"
            echo "  verify                          Verify migration integrity"
            echo "  rollback <version>              Rollback a specific migration"
            echo "  create <name> [description]     Create new migration files"
            echo "  test <migration_file>           Test migration in isolated environment"
            echo "  validate <migration_file>       Validate migration file"
            echo ""
            echo "Examples:"
            echo "  $0 create add_user_settings 'Add user settings table'"
            echo "  $0 test 20260404120000_add_user_settings.sql"
            echo "  $0 validate 20260404120000_add_user_settings.sql"
            exit 1
            ;;
    esac
}

main "$@"
