#!/bin/bash
# ============================================================================
# Synapse Rust Database Migration Script
# ============================================================================
# 功能:
#   1. 检查数据库连接
#   2. 检查已执行的迁移版本
#   3. 按顺序执行未应用的迁移
#   4. 记录执行结果
#
# 环境变量:
#   DATABASE_URL - 数据库连接字符串
# ============================================================================

set -e

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() {
    echo -e "${BLUE}[INFO]${NC} $(date '+%Y-%m-%d %H:%M:%S') - $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $(date '+%Y-%m-%d %H:%M:%S') - $1"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $(date '+%Y-%m-%d %H:%M:%S') - $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $(date '+%Y-%m-%d %H:%M:%S') - $1"
}

# 配置
MIGRATION_DIR="${MIGRATION_DIR:-/app/migrations}"
MIGRATION_TABLE="schema_migrations"

# 解析数据库连接信息
parse_database_url() {
    if [ -z "$DATABASE_URL" ]; then
        log_error "DATABASE_URL is not set"
        exit 1
    fi

    export PGHOST=$(echo "$DATABASE_URL" | sed -n 's/.*@\([^:]*\):.*/\1/p')
    export PGPORT=$(echo "$DATABASE_URL" | sed -n 's/.*:\([0-9]*\)\/.*/\1/p')
    export PGDATABASE=$(echo "$DATABASE_URL" | sed -n 's/.*\/\([^?]*\).*/\1/p')
    export PGUSER=$(echo "$DATABASE_URL" | sed -n 's/.*\/\/\([^:]*\):.*/\1/p')
    export PGPASSWORD=$(echo "$DATABASE_URL" | sed -n 's/.*:\/\/[^:]*:\([^@]*\)@.*/\1/p')
}

# 检查数据库连接
check_database_connection() {
    log_info "Checking database connection..."

    local max_attempts=10
    local attempt=1

    while [ $attempt -le $max_attempts ]; do
        if psql -c "SELECT 1" > /dev/null 2>&1; then
            log_success "Database connection established"
            return 0
        fi

        log_info "Attempt $attempt/$max_attempts: Waiting for database..."
        sleep 2
        attempt=$((attempt + 1))
    done

    log_error "Failed to connect to database after $max_attempts attempts"
    exit 1
}

# 确保迁移版本表存在
ensure_migration_table() {
    log_info "Ensuring migration table exists..."

    psql <<EOF
CREATE TABLE IF NOT EXISTS ${MIGRATION_TABLE} (
    version VARCHAR(255) PRIMARY KEY,
    checksum VARCHAR(64),
    execution_time_ms BIGINT,
    success BOOLEAN NOT NULL DEFAULT TRUE,
    executed_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    error_message TEXT,
    description TEXT
);
EOF

    if [ $? -eq 0 ]; then
        log_success "Migration table ready"
    else
        log_error "Failed to create migration table"
        exit 1
    fi
}

# 获取已执行的迁移版本
get_executed_migrations() {
    psql -t -A -c "SELECT version FROM ${MIGRATION_TABLE} WHERE success = true ORDER BY version"
}

# 计算文件校验和
get_file_checksum() {
    local file="$1"
    sha256sum "$file" 2>/dev/null | cut -d' ' -f1
}

# 执行单个迁移文件
execute_migration() {
    local migration_file="$1"
    local version=$(basename "$migration_file" .sql)
    local start_time=$(date +%s%3N)
    local checksum=$(get_file_checksum "$migration_file")

    log_info "Executing migration: $version"

    local description=$(head -5 "$migration_file" | grep -i "description:" | cut -d':' -f2- | xargs || echo "")

    local error_message=""
    local success=false

    if psql -v ON_ERROR_STOP=1 -f "$migration_file" > /tmp/migration_output.log 2>&1; then
        success=true
        log_success "Migration $version executed successfully"
    else
        error_message=$(cat /tmp/migration_output.log)
        log_error "Migration $version failed: $error_message"
    fi

    local end_time=$(date +%s%3N)
    local execution_time=$((end_time - start_time))

    psql <<EOF
INSERT INTO ${MIGRATION_TABLE} (version, checksum, execution_time_ms, success, error_message, description)
VALUES ('$version', '$checksum', $execution_time, $success, \$\$${error_message}\$\$, \$\$${description}\$\$)
ON CONFLICT (version) DO UPDATE SET
    checksum = '$checksum',
    execution_time_ms = $execution_time,
    success = $success,
    executed_at = NOW(),
    error_message = \$\$${error_message}\$\$,
    description = \$\$${description}\$\$;
EOF

    if [ "$success" = false ]; then
        return 1
    fi

    return 0
}

# 执行统一初始化脚本
execute_unified_schema() {
    local unified_file="${MIGRATION_DIR}/00000000_unified_schema_v2.sql"

    if [ ! -f "$unified_file" ]; then
        unified_file="${MIGRATION_DIR}/00000000_unified_schema.sql"
    fi

    if [ -f "$unified_file" ]; then
        local version=$(basename "$unified_file" .sql)

        local executed=$(psql -t -A -c "SELECT COUNT(*) FROM ${MIGRATION_TABLE} WHERE version = '$version' AND success = true")

        if [ "$executed" -eq 0 ]; then
            log_info "Executing unified schema initialization: $version"
            execute_migration "$unified_file" || return 1
        else
            log_info "Unified schema already applied: $version"
        fi
    fi

    return 0
}

# 执行增量迁移
execute_incremental_migrations() {
    log_info "Checking for incremental migrations..."

    local executed_migrations=$(get_executed_migrations)
    local migration_count=0
    local failed_count=0

    for migration_file in $(ls -1 "${MIGRATION_DIR}"/*.sql 2>/dev/null | sort); do
        local version=$(basename "$migration_file" .sql)

        if echo "$executed_migrations" | grep -q "^${version}$"; then
            log_info "Skipping already applied migration: $version"
            continue
        fi

        if [[ "$version" == "00000000_unified_schema"* ]]; then
            continue
        fi

        migration_count=$((migration_count + 1))

        if ! execute_migration "$migration_file"; then
            failed_count=$((failed_count + 1))
            log_error "Migration failed: $version"

            if [ "${STOP_ON_MIGRATION_FAILURE:-true}" = "true" ]; then
                log_error "Stopping due to migration failure"
                return 1
            fi
        fi
    done

    if [ $migration_count -eq 0 ]; then
        log_info "No new migrations to apply"
    else
        log_success "Applied $migration_count migrations, $failed_count failures"
    fi

    return 0
}

# 显示迁移状态
show_migration_status() {
    log_info "Migration Status:"
    echo ""

    psql <<EOF
SELECT
    version,
    success,
    executed_at,
    execution_time_ms || 'ms' as duration,
    LEFT(error_message, 50) as error_preview
FROM ${MIGRATION_TABLE}
ORDER BY executed_at DESC
LIMIT 20;
EOF

    echo ""

    local total=$(psql -t -A -c "SELECT COUNT(*) FROM ${MIGRATION_TABLE}")
    local successful=$(psql -t -A -c "SELECT COUNT(*) FROM ${MIGRATION_TABLE} WHERE success = true")
    local failed=$(psql -t -A -c "SELECT COUNT(*) FROM ${MIGRATION_TABLE} WHERE success = false")

    echo "Total migrations: $total"
    echo "Successful: $successful"
    echo "Failed: $failed"
}

# 主函数
main() {
    log_info "Starting database migration process..."

    parse_database_url

    check_database_connection

    ensure_migration_table

    execute_unified_schema || {
        log_error "Unified schema initialization failed"
        exit 1
    }

    execute_incremental_migrations || {
        log_error "Incremental migrations failed"
        show_migration_status
        exit 1
    }

    show_migration_status

    log_success "Database migration process completed successfully"
}

main "$@"
