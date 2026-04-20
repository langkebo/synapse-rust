#!/bin/bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
MIGRATIONS_DIR="$PROJECT_ROOT/migrations"

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

load_env() {
    local env_file=""
    for candidate in "$SCRIPT_DIR/.env" "$PWD/.env" "$PROJECT_ROOT/.env"; do
        if [ -f "$candidate" ]; then
            env_file="$candidate"
            break
        fi
    done

    if [ -n "$env_file" ]; then
        log_info "加载环境变量: $env_file"
        set -a
        . "$env_file"
        set +a
    fi

    export DB_HOST="${DB_HOST:-localhost}"
    export DB_PORT="${DB_PORT:-5432}"
    export DB_NAME="${DB_NAME:-synapse}"
    export DB_USER="${DB_USER:-synapse}"
    export DB_PASSWORD="${DB_PASSWORD:-synapse}"
    export DATABASE_URL="${DATABASE_URL:-postgres://$DB_USER:$DB_PASSWORD@$DB_HOST:$DB_PORT/$DB_NAME}"
    export POSTGRES_ADMIN_URL="${POSTGRES_ADMIN_URL:-postgres://$DB_USER:$DB_PASSWORD@$DB_HOST:$DB_PORT/postgres}"
    export DB_CONTAINER="${DB_CONTAINER:-${COMPOSE_PROJECT_NAME:-synapse}-postgres}"

    detect_psql_backend
}

PSQL_USE_DOCKER=0
PSQL_DOCKER_CONTAINER=""

detect_psql_backend() {
    if command -v psql >/dev/null 2>&1; then
        PSQL_USE_DOCKER=0
        PSQL_DOCKER_CONTAINER=""
        return 0
    fi

    if command -v docker >/dev/null 2>&1 && docker ps --format '{{.Names}}' | grep -qx "$DB_CONTAINER"; then
        PSQL_USE_DOCKER=1
        PSQL_DOCKER_CONTAINER="$DB_CONTAINER"
        log_warning "未检测到本机 psql，回退为容器内 psql: $PSQL_DOCKER_CONTAINER"
        return 0
    fi

    log_error "未找到可用的 psql；请安装本机 psql 或启动数据库容器: $DB_CONTAINER"
    return 1
}

psql_exec() {
    local database_name="$1"
    shift

    if [ "$PSQL_USE_DOCKER" -eq 1 ]; then
        docker exec \
            -i \
            -e PGOPTIONS='-c client_min_messages=warning' \
            -e PGPASSWORD="$DB_PASSWORD" \
            "$PSQL_DOCKER_CONTAINER" \
            psql \
            -h localhost \
            -p 5432 \
            -U "$DB_USER" \
            -d "$database_name" \
            "$@"
        return $?
    fi

    local target_url="$DATABASE_URL"
    if [ "$database_name" = "postgres" ]; then
        target_url="$POSTGRES_ADMIN_URL"
    fi

    PGOPTIONS='-c client_min_messages=warning' psql "$target_url" "$@"
}

psql_db() {
    psql_exec "$DB_NAME" "$@"
}

psql_admin() {
    psql_exec "postgres" "$@"
}

now_ms() {
    local ts
    ts="$(date +%s%3N 2>/dev/null || true)"
    if [[ "$ts" =~ ^[0-9]+$ ]]; then
        echo "$ts"
        return 0
    fi
    python3 - <<'PY'
import time
print(int(time.time() * 1000))
PY
}

table_exists() {
    local table_name="$1"
    psql_db -tAc "SELECT EXISTS (
        SELECT 1
        FROM information_schema.tables
        WHERE table_schema = 'public' AND table_name = '$table_name'
    )" 2>/dev/null | grep -q '^t$'
}

schema_migrations_has_column() {
    local column_name="$1"
    psql_db -tAc "SELECT EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'schema_migrations'
          AND column_name = '$column_name'
    )" 2>/dev/null | grep -q '^t$'
}

ensure_database_exists() {
    if psql_db -c "SELECT 1" >/dev/null 2>&1; then
        return 0
    fi

    log_info "数据库不存在，尝试创建: $DB_NAME"
    psql_admin -v ON_ERROR_STOP=1 -c "CREATE DATABASE \"$DB_NAME\";" >/dev/null 2>&1 || true
    psql_db -c "SELECT 1" >/dev/null 2>&1
}

check_db_connection() {
    log_info "检查数据库连接..."
    ensure_database_exists
    psql_db -c "SELECT 1" >/dev/null 2>&1
    log_success "数据库连接成功"
}

ensure_schema_migrations_table() {
    psql_db -v ON_ERROR_STOP=1 <<'SQL' >/dev/null
CREATE TABLE IF NOT EXISTS schema_migrations (
    id BIGSERIAL PRIMARY KEY,
    version TEXT NOT NULL,
    name TEXT,
    checksum TEXT,
    applied_ts BIGINT,
    execution_time_ms BIGINT,
    success BOOLEAN NOT NULL DEFAULT TRUE,
    description TEXT,
    executed_at TIMESTAMPTZ DEFAULT NOW(),
    CONSTRAINT uq_schema_migrations_version UNIQUE (version)
);
ALTER TABLE schema_migrations ADD COLUMN IF NOT EXISTS name TEXT;
ALTER TABLE schema_migrations ADD COLUMN IF NOT EXISTS checksum TEXT;
ALTER TABLE schema_migrations ADD COLUMN IF NOT EXISTS applied_ts BIGINT;
ALTER TABLE schema_migrations ADD COLUMN IF NOT EXISTS execution_time_ms BIGINT;
ALTER TABLE schema_migrations ADD COLUMN IF NOT EXISTS success BOOLEAN NOT NULL DEFAULT TRUE;
ALTER TABLE schema_migrations ADD COLUMN IF NOT EXISTS description TEXT;
ALTER TABLE schema_migrations ADD COLUMN IF NOT EXISTS executed_at TIMESTAMPTZ DEFAULT NOW();
CREATE UNIQUE INDEX IF NOT EXISTS idx_schema_migrations_version ON schema_migrations(version);
SQL
}

latest_baseline_file() {
    find "$MIGRATIONS_DIR" -maxdepth 1 -type f -name '00000000_unified_schema_v*.sql' ! -name '*.undo.sql' | sort | tail -n 1
}

record_migration() {
    local version="$1"
    local filename="$2"
    local duration_ms="$3"
    local success="$4"

    psql_db -v ON_ERROR_STOP=1 -c "
        INSERT INTO schema_migrations (version, name, checksum, applied_ts, execution_time_ms, success, description, executed_at)
        VALUES (
            '$version',
            '$filename',
            md5('$filename'),
            EXTRACT(EPOCH FROM NOW()) * 1000,
            $duration_ms,
            $success,
            '$filename',
            NOW()
        )
        ON CONFLICT (version) DO UPDATE SET
            name = EXCLUDED.name,
            checksum = EXCLUDED.checksum,
            applied_ts = EXCLUDED.applied_ts,
            execution_time_ms = EXCLUDED.execution_time_ms,
            success = EXCLUDED.success,
            description = EXCLUDED.description,
            executed_at = EXCLUDED.executed_at
    " >/dev/null
}

is_migration_applied() {
    local version="$1"
    psql_db -tAc "SELECT COALESCE(bool_and(success), FALSE) FROM schema_migrations WHERE version = '$version'" 2>/dev/null | grep -q '^t$'
}

apply_sql_file() {
    local file="$1"
    local filename
    filename="$(basename "$file")"
    local version="${filename%.sql}"
    local started_at
    started_at="$(now_ms)"

    log_info "应用迁移: $filename"

    # 通过 STDIN 执行，兼容“本机无 psql 时回退到容器内 psql”的路径差异。
    if psql_db -v ON_ERROR_STOP=1 < "$file" >/dev/null; then
        local finished_at
        finished_at="$(now_ms)"
        record_migration "$version" "$filename" "$((finished_at - started_at))" TRUE
        log_success "迁移完成: $filename"
        return 0
    fi

    local finished_at
    finished_at="$(now_ms)"
    psql_db -c "ABORT;" >/dev/null 2>&1 || true
    record_migration "$version" "$filename" "$((finished_at - started_at))" FALSE || true
    log_error "迁移失败: $filename"
    return 1
}

init_database() {
    log_info "初始化数据库..."
    ensure_schema_migrations_table

    if table_exists "users"; then
        log_info "检测到现有业务表，跳过基线初始化"
        return 0
    fi

    local baseline_file
    baseline_file="$(latest_baseline_file)"
    if [ -z "$baseline_file" ]; then
        log_error "找不到统一基线脚本"
        return 1
    fi

    local baseline_name
    baseline_name="$(basename "$baseline_file")"
    local baseline_version="${baseline_name%.sql}"

    if is_migration_applied "$baseline_version"; then
        log_info "基线迁移已记录: $baseline_name"
        return 0
    fi

    apply_sql_file "$baseline_file"
}

get_current_version() {
    psql_db -tAc "SELECT COALESCE(version, '') FROM schema_migrations ORDER BY COALESCE(applied_ts, 0) DESC, version DESC LIMIT 1" 2>/dev/null | tr -d '[:space:]'
}

list_applied_migrations() {
    ensure_schema_migrations_table
    log_info "已应用的迁移:"
    psql_db -c "
        SELECT version, COALESCE(name, description, version) AS name, success, applied_ts
        FROM schema_migrations
        ORDER BY COALESCE(applied_ts, 0) DESC, version DESC
    "
}

apply_pending_migrations() {
    log_info "检查待处理的迁移..."
    ensure_schema_migrations_table
    init_database

    local current_version
    current_version="$(get_current_version)"
    log_info "当前版本: ${current_version:-<empty>}"

    local baseline_file
    baseline_file="$(latest_baseline_file)"
    local pending=0

    local migration_list
    migration_list="$(mktemp "${TMPDIR:-/tmp}/db_migrate.XXXXXX")"
    find "$MIGRATIONS_DIR" -maxdepth 1 -type f -name '*.sql' ! -name '*.undo.sql' | sort > "$migration_list"

    while IFS= read -r file; do
        local filename
        filename="$(basename "$file")"
        local version="${filename%.sql}"

        if [ -n "$baseline_file" ] && [ "$file" = "$baseline_file" ]; then
            continue
        fi

        if is_migration_applied "$version"; then
            continue
        fi

        pending=$((pending + 1))
        apply_sql_file "$file"
    done < "$migration_list"

    rm -f "$migration_list"

    if [ "$pending" -eq 0 ]; then
        log_success "没有待处理的迁移"
    else
        log_success "已应用 $pending 个迁移"
    fi
}

validate_schema() {
    log_info "验证数据库架构..."
    ensure_schema_migrations_table

    local required_tables=(
        "users"
        "devices"
        "access_tokens"
        "refresh_tokens"
        "rooms"
        "events"
        "event_relations"
        "rate_limits"
        "server_notices"
        "user_notification_settings"
        "widgets"
        "secure_key_backups"
        "secure_backup_session_keys"
        "schema_migrations"
    )
    local errors=0

    for table in "${required_tables[@]}"; do
        if table_exists "$table"; then
            log_success "表存在: $table"
        else
            log_error "表缺失: $table"
            errors=$((errors + 1))
        fi
    done

    local created_at_count
    created_at_count="$(psql_db -tAc "SELECT COUNT(*) FROM information_schema.columns WHERE table_schema = 'public' AND column_name = 'created_at'" 2>/dev/null | tr -d '[:space:]')"
    if [ "${created_at_count:-0}" -gt 0 ]; then
        log_info "发现 $created_at_count 个历史 created_at 字段，保留兼容性提醒"
    fi

    local updated_at_count
    updated_at_count="$(psql_db -tAc "SELECT COUNT(*) FROM information_schema.columns WHERE table_schema = 'public' AND column_name = 'updated_at'" 2>/dev/null | tr -d '[:space:]')"
    if [ "${updated_at_count:-0}" -gt 0 ]; then
        log_info "发现 $updated_at_count 个历史 updated_at 字段，保留兼容性提醒"
    fi

    if [ "$errors" -gt 0 ]; then
        log_error "数据库架构验证失败，发现 $errors 个错误"
        return 1
    fi

    log_success "数据库架构验证通过"
}

cleanup_stale_connections() {
    log_info "检查并清理异常连接..."
    local stale_count
    stale_count="$(psql_db -tAc "
        SELECT COUNT(*)
        FROM pg_stat_activity
        WHERE state = 'idle in transaction'
          AND query_start < NOW() - INTERVAL '5 minutes'
          AND datname = '$DB_NAME'
    " 2>/dev/null | tr -d '[:space:]')"

    if [ -n "${stale_count:-}" ] && [ "$stale_count" -gt 0 ]; then
        log_warning "发现 $stale_count 个空闲事务连接，正在清理..."
        psql_db -c "
            SELECT pg_terminate_backend(pid)
            FROM pg_stat_activity
            WHERE state = 'idle in transaction'
              AND query_start < NOW() - INTERVAL '5 minutes'
              AND datname = '$DB_NAME'
        " >/dev/null 2>&1 || true
        log_success "异常连接已清理"
    else
        log_info "没有发现异常连接"
    fi
}

show_help() {
    echo "synapse-rust 数据库迁移管理工具"
    echo
    echo "用法: $0 <命令>"
    echo
    echo "命令:"
    echo "  init"
    echo "  migrate"
    echo "  status"
    echo "  validate"
    echo "  help"
}

main() {
    local command="${1:-help}"

    load_env

    case "$command" in
        init)
            check_db_connection
            cleanup_stale_connections
            init_database
            ;;
        migrate)
            check_db_connection
            cleanup_stale_connections
            apply_pending_migrations
            ;;
        status)
            check_db_connection
            list_applied_migrations
            ;;
        validate)
            check_db_connection
            validate_schema
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
