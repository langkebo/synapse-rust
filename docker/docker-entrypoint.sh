#!/bin/bash
# ============================================================================
# Synapse Rust Docker Entrypoint Script
# ============================================================================
# 功能:
#   1. 等待数据库就绪
#   2. 执行数据库迁移
#   3. 启动主应用
#
# 环境变量:
#   DATABASE_URL      - 数据库连接字符串
#   RUN_MIGRATIONS    - 是否执行迁移 (默认: true)
#   MIGRATION_TIMEOUT - 迁移超时时间(秒) (默认: 300)
# ============================================================================

set -e

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

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

# 解析数据库连接信息
parse_database_url() {
    if [ -z "$DATABASE_URL" ]; then
        log_error "DATABASE_URL is not set"
        exit 1
    fi

    DB_HOST=$(echo "$DATABASE_URL" | sed -n 's/.*@\([^:]*\):.*/\1/p')
    DB_PORT=$(echo "$DATABASE_URL" | sed -n 's/.*:\([0-9]*\)\/.*/\1/p')
    DB_NAME=$(echo "$DATABASE_URL" | sed -n 's/.*\/\([^?]*\).*/\1/p')
    DB_USER=$(echo "$DATABASE_URL" | sed -n 's/.*\/\/\([^:]*\):.*/\1/p')

    log_info "Database: ${DB_USER}@${DB_HOST}:${DB_PORT}/${DB_NAME}"
}

# 等待数据库就绪
wait_for_database() {
    local max_attempts=${DB_WAIT_ATTEMPTS:-30}
    local wait_interval=${DB_WAIT_INTERVAL:-2}
    local attempt=1

    log_info "Waiting for database to be ready..."

    while [ $attempt -le $max_attempts ]; do
        if pg_isready -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" > /dev/null 2>&1; then
            log_success "Database is ready!"
            return 0
        fi

        log_info "Attempt $attempt/$max_attempts: Database not ready yet, waiting ${wait_interval}s..."
        sleep $wait_interval
        attempt=$((attempt + 1))
    done

    log_error "Database connection timeout after $max_attempts attempts"
    exit 1
}

# 执行数据库迁移
run_migrations() {
    if [ "${RUN_MIGRATIONS:-true}" != "true" ]; then
        log_info "Skipping migrations (RUN_MIGRATIONS=false)"
        return 0
    fi

    log_info "Starting database migrations..."

    local migration_dir="/app/migrations"
    local migration_log="/app/logs/migrations.log"

    mkdir -p "$(dirname "$migration_log")"

    if [ ! -d "$migration_dir" ]; then
        log_warning "Migration directory not found: $migration_dir"
        return 0
    fi

    if [ ! -f "/app/scripts/run-migrations.sh" ]; then
        log_warning "Migration script not found: /app/scripts/run-migrations.sh"
        return 0
    fi

    chmod +x /app/scripts/run-migrations.sh

    local timeout=${MIGRATION_TIMEOUT:-300}
    log_info "Executing migrations with timeout ${timeout}s..."

    if timeout "$timeout" /app/scripts/run-migrations.sh 2>&1 | tee -a "$migration_log"; then
        log_success "Database migrations completed successfully"
    else
        local exit_code=$?
        if [ $exit_code -eq 124 ]; then
            log_error "Migration timeout after ${timeout}s"
        else
            log_error "Migration failed with exit code: $exit_code"
        fi
        return $exit_code
    fi
}

# 验证数据库架构
verify_schema() {
    if [ "${VERIFY_SCHEMA:-true}" != "true" ]; then
        log_info "Skipping schema verification (VERIFY_SCHEMA=false)"
        return 0
    fi

    log_info "Verifying database schema..."

    if [ ! -f "/app/scripts/verify-schema.sh" ]; then
        log_warning "Schema verification script not found"
        return 0
    fi

    chmod +x /app/scripts/verify-schema.sh

    if /app/scripts/verify-schema.sh; then
        log_success "Database schema verification passed"
    else
        log_warning "Database schema verification reported issues (non-fatal)"
    fi
}

# 生成密钥（如果需要）
generate_keys_if_needed() {
    local keys_dir="/app/data/keys"

    if [ ! -d "$keys_dir" ]; then
        mkdir -p "$keys_dir"
        chmod 700 "$keys_dir"
    fi

    if [ ! -f "$keys_dir/signing.key" ]; then
        log_info "Generating federation signing key..."
        if command -v generate_test_keypair &> /dev/null; then
            generate_test_keypair > "$keys_dir/signing.key"
            chmod 600 "$keys_dir/signing.key"
            log_success "Federation signing key generated"
        else
            log_warning "generate_test_keypair not found, skipping key generation"
        fi
    fi
}

# 验证配置
validate_config() {
    log_info "Validating configuration..."

    if [ -z "$SERVER_NAME" ]; then
        log_error "SERVER_NAME is not set"
        exit 1
    fi

    if [ ! -f "$SYNAPSE_CONFIG_PATH" ]; then
        log_warning "Config file not found: $SYNAPSE_CONFIG_PATH"
    fi

    log_success "Configuration validated"
}

# 显示启动信息
print_startup_info() {
    echo ""
    echo "========================================"
    echo "  Synapse Rust Matrix Homeserver"
    echo "========================================"
    echo "  Server Name:  ${SERVER_NAME}"
    echo "  Database:     ${DB_HOST}:${DB_PORT}/${DB_NAME}"
    echo "  Config:       ${SYNAPSE_CONFIG_PATH}"
    echo "  Log Level:    ${RUST_LOG:-info}"
    echo "  Timezone:     ${TZ:-UTC}"
    echo "========================================"
    echo ""
}

# 主函数
main() {
    log_info "Starting Synapse Rust entrypoint..."

    parse_database_url

    wait_for_database

    run_migrations

    verify_schema

    generate_keys_if_needed

    validate_config

    print_startup_info

    log_info "Starting Synapse Rust application..."

    exec "$@"
}

main "$@"
