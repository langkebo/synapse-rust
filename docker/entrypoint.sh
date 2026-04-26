#!/bin/bash
set -euo pipefail

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() {
    echo -e "${BLUE}[INFO]${NC} $*"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $*"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $*"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $*" >&2
}

# 等待数据库就绪
wait_for_db() {
    local max_attempts="${DB_WAIT_ATTEMPTS:-30}"
    local interval="${DB_WAIT_INTERVAL:-2}"
    local attempt=0

    log_info "Waiting for database to be ready..."

    while [ $attempt -lt $max_attempts ]; do
        if pg_isready -h "${DB_HOST:-db}" -p "${DB_PORT:-5432}" -U "${DB_USER:-synapse}" >/dev/null 2>&1; then
            log_success "Database is ready"
            return 0
        fi

        attempt=$((attempt + 1))
        log_info "Attempt $attempt/$max_attempts: Database not ready, waiting ${interval}s..."
        sleep "$interval"
    done

    log_error "Database failed to become ready after $max_attempts attempts"
    return 1
}

# 运行数据库迁移
run_migrations() {
    if [ "${RUN_MIGRATIONS:-true}" != "true" ]; then
        log_info "Database migrations disabled (RUN_MIGRATIONS != true)"
        return 0
    fi

    log_info "Running database migrations..."

    export DB_HOST="${DB_HOST:-db}"
    export DB_PORT="${DB_PORT:-5432}"
    export DB_NAME="${DB_NAME:-synapse}"
    export DB_USER="${DB_USER:-synapse}"
    export DB_PASSWORD="${DB_PASSWORD}"
    export DATABASE_URL="${DATABASE_URL:-postgres://$DB_USER:$DB_PASSWORD@$DB_HOST:$DB_PORT/$DB_NAME}"

    if [ -f "/app/scripts/db_migrate.sh" ]; then
        if timeout "${MIGRATION_TIMEOUT:-300}" /app/scripts/db_migrate.sh migrate; then
            log_success "Database migrations completed successfully"
            return 0
        else
            log_error "Database migrations failed"
            if [ "${STOP_ON_MIGRATION_FAILURE:-true}" = "true" ]; then
                log_error "Stopping container due to migration failure"
                exit 1
            fi
            return 1
        fi
    else
        log_warning "Migration script not found at /app/scripts/db_migrate.sh"
        return 0
    fi
}

# 验证数据库架构
verify_schema() {
    if [ "${VERIFY_SCHEMA:-true}" != "true" ]; then
        log_info "Schema verification disabled (VERIFY_SCHEMA != true)"
        return 0
    fi

    log_info "Verifying database schema..."

    if [ -f "/app/scripts/db_migrate.sh" ]; then
        if /app/scripts/db_migrate.sh validate; then
            log_success "Schema verification passed"
            return 0
        else
            log_warning "Schema verification failed (non-fatal)"
            return 0
        fi
    else
        log_warning "Migration script not found, skipping schema verification"
        return 0
    fi
}

# 主流程
main() {
    log_info "Starting Synapse Rust entrypoint..."

    # 等待数据库
    if ! wait_for_db; then
        log_error "Failed to connect to database"
        exit 1
    fi

    # 运行迁移
    if ! run_migrations; then
        log_warning "Migrations encountered issues, but continuing..."
    fi

    # 验证架构
    verify_schema

    log_success "Initialization complete, starting application..."

    # 启动应用程序
    exec "$@"
}

main "$@"
