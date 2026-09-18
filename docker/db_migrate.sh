#!/bin/bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
MIGRATIONS_DIR="${MIGRATIONS_DIR:-$PROJECT_ROOT/migrations}"

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
    # If the operator already supplied DATABASE_URL on the command line,
    # treat it as the source of truth: parse it back into the individual
    # DB_* variables so docker container detection, password masking, and
    # the psql admin URL all stay consistent. Without this, sourcing .env
    # can silently re-introduce default credentials and route migrations to
    # the wrong instance.
    local user_supplied_database_url="${DATABASE_URL:-}"
    # 暴露给护栏：只有"调用方显式给了目标"才允许宿主 psql 打 loopback（见
    # `host_psql_target_is_implicit_loopback`）。这里是唯一能区分
    # "用户指定" 与 ".env 兜底" 的位置 —— 一旦 source 了 .env，两者就分不开了。
    #
    # DB_HOST 同样要记：`.github/workflows/db-migration-gate.yml` 与
    # `drift-detection.yml` 只给 DB_HOST/DB_PORT/DB_NAME/DB_USER/DB_PASSWORD，
    # 不给 DATABASE_URL，只认后者会把这两条 CI 判死。
    local user_supplied_db_host="${DB_HOST:-}"
    CALLER_SUPPLIED_DATABASE_URL="$user_supplied_database_url"
    CALLER_SUPPLIED_DB_HOST="$user_supplied_db_host"
    export CALLER_SUPPLIED_DATABASE_URL
    export CALLER_SUPPLIED_DB_HOST

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

    if [ -n "$user_supplied_database_url" ]; then
        DATABASE_URL="$user_supplied_database_url"
        log_info "使用调用方提供的 DATABASE_URL（覆盖 .env 默认值）"

        # Parse postgres[ql]://user:pass@host:port/dbname into discrete fields.
        local stripped="${DATABASE_URL#*://}"
        local creds_and_host="${stripped%%/*}"
        local dbname="${stripped#*/}"
        dbname="${dbname%%\?*}"
        local creds=""
        local hostport="$creds_and_host"
        if [[ "$creds_and_host" == *@* ]]; then
            creds="${creds_and_host%@*}"
            hostport="${creds_and_host##*@}"
        fi
        local host="${hostport%%:*}"
        local port=""
        if [[ "$hostport" == *:* ]]; then
            port="${hostport##*:}"
        fi

        if [ -n "$host" ]; then export DB_HOST="$host"; fi
        if [ -n "$port" ]; then export DB_PORT="$port"; fi
        if [ -n "$dbname" ]; then export DB_NAME="$dbname"; fi
        if [ -n "$creds" ]; then
            export DB_USER="${creds%%:*}"
            if [[ "$creds" == *:* ]]; then
                export DB_PASSWORD="${creds#*:}"
            fi
        fi
    fi

    export DB_HOST="${DB_HOST:-localhost}"
    export DB_PORT="${DB_PORT:-5432}"
    export DB_NAME="${DB_NAME:-synapse}"
    export DB_USER="${DB_USER:-synapse}"
    if [ -z "$DB_PASSWORD" ]; then
        log_error "DB_PASSWORD must be set explicitly (no insecure default). Example: DB_PASSWORD=$(head -c 16 /dev/urandom | base64 | tr -d '=/+' | head -c 24)"
        exit 1
    fi

    # Reject identifiers that cannot be safely interpolated into "CREATE DATABASE \"$DB_NAME\""
    # or appear in table/column lookups. Postgres unquoted identifiers are [A-Za-z_][A-Za-z0-9_]*.
    if ! [[ "$DB_NAME" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]]; then
        log_error "DB_NAME 含非法字符，仅允许 [A-Za-z_][A-Za-z0-9_]*: $DB_NAME"
        exit 1
    fi
    if ! [[ "$DB_USER" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]]; then
        log_error "DB_USER 含非法字符，仅允许 [A-Za-z_][A-Za-z0-9_]*: $DB_USER"
        exit 1
    fi
    if ! [[ "$DB_PORT" =~ ^[0-9]+$ ]]; then
        log_error "DB_PORT 必须为数字: $DB_PORT"
        exit 1
    fi
    export DATABASE_URL="${DATABASE_URL:-postgres://$DB_USER:$DB_PASSWORD@$DB_HOST:$DB_PORT/$DB_NAME}"
    export POSTGRES_ADMIN_URL="${POSTGRES_ADMIN_URL:-postgres://$DB_USER:$DB_PASSWORD@$DB_HOST:$DB_PORT/postgres}"
    export DB_CONTAINER="${DB_CONTAINER:-${COMPOSE_PROJECT_NAME:-synapse}-postgres}"

    detect_psql_backend
}

PSQL_USE_DOCKER=0
PSQL_DOCKER_CONTAINER=""

# 目标是否为 loopback（宿主本机）。
is_loopback_db_host() {
    case "$DB_HOST" in
        localhost | 127.0.0.1 | ::1 | 0.0.0.0) return 0 ;;
        *) return 1 ;;
    esac
}

# 是否运行在容器内（容器内调用时 loopback 就是"本容器/同网络"，语义不同）。
inside_container() {
    [ -f /.dockerenv ] && return 0
    grep -qaE 'docker|containerd|kubepods' /proc/1/cgroup 2>/dev/null && return 0
    return 1
}

# H-14 护栏的判据集合，供 detect_psql_backend 与自测复用。
#
# 拒绝条件（全部满足才拒绝）：
#   1. 后端是**宿主 psql**（不是 docker exec）
#   2. 目标是 loopback
#   3. 调用方**没有显式给出目标** —— 既没给 DATABASE_URL，也没给 DB_HOST，
#      即走的是本脚本从 .env 兜底出来的默认值
#      （docker/.env 里就是 DB_USER=synapse / DB_NAME=synapse / localhost:5432）
#   4. 不在容器内
# 除非显式放行 SYNAPSE_DB_MIGRATE_ALLOW_HOST_PSQL=1。
#
# 判据 3 是关键：H-14 的复现形态正是"裸跑 `bash docker/db_migrate.sh validate`"，
# 此时没有任何一处代码说明"这一刀该落在哪" —— 而兜底值恰好指向宿主自己的 PG。
# 反过来，显式给出目标的调用方一律放行：CI 的 mutation-testing / dev-test-setup
# 用 DATABASE_URL，CI 的 db-migration-gate / drift-detection 用 DB_HOST。
host_psql_target_is_implicit_loopback() {
    [ "$PSQL_USE_DOCKER" -eq 0 ] || return 1
    is_loopback_db_host || return 1
    inside_container && return 1
    [ -z "${CALLER_SUPPLIED_DATABASE_URL:-}" ] || return 1
    [ -z "${CALLER_SUPPLIED_DB_HOST:-}" ] || return 1
    return 0
}

detect_psql_backend() {
    if command -v psql >/dev/null 2>&1; then
        PSQL_USE_DOCKER=0
        PSQL_DOCKER_CONTAINER=""

        # ── 护栏：别把命令打在"另一个 PostgreSQL"上 ──────────────────────────
        #
        # dev 栈与 deploy 栈都**不把 5432 发布到宿主**（应用在容器网络内直连
        # `db:5432` / `postgres:5432`）。所以从宿主执行本脚本时，`localhost:5432`
        # 永远不是 compose 栈的数据库，而是宿主自己那一台。
        #
        # 2026-09-15 实测（复现步骤：裸跑 `bash docker/db_migrate.sh validate`）：
        # 宿主 5432 是本机 Homebrew PostgreSQL，脚本在它上面
        # **建了库**（"[INFO] 数据库不存在，尝试创建: synapse"）之后才报"表缺失" ——
        # 一条只读语气的命令改动了错误的实例（PROJECT_ACTUAL_ISSUES §6 H-14）。
        if host_psql_target_is_implicit_loopback; then
            if [ "${SYNAPSE_DB_MIGRATE_ALLOW_HOST_PSQL:-0}" != "1" ]; then
                log_error "拒绝执行：本次要用宿主 psql 连 ${DB_HOST}:${DB_PORT}，而目标是从 docker/.env 兜底出来的默认值。"
                log_error "两个 compose 栈都不把 5432 发布到宿主，因此 $DB_HOST:$DB_PORT 上的是**另一个**"
                log_error "PostgreSQL 实例（宿主自装的那台）。继续执行会在那个实例上建库 / 迁移，"
                log_error "而它可能根本不是你想要的库。"
                log_error "请改用其中之一："
                log_error "  * 显式指定目标： DATABASE_URL=postgres://user:pass@host:port/dbname bash docker/db_migrate.sh <command>"
                log_error "  * 或分项给出： DB_HOST=<host> DB_PORT=<port> DB_NAME=<db> DB_USER=<user> DB_PASSWORD=<pass> bash docker/db_migrate.sh <command>"
                log_error "  * 或直接连 compose 的库： docker exec -i <db-container> psql -U ${DB_USER} -d ${DB_NAME} ..."
                log_error "确实就想打宿主实例时显式放行："
                log_error "  SYNAPSE_DB_MIGRATE_ALLOW_HOST_PSQL=1 bash docker/db_migrate.sh <command>"
                return 1
            fi
            log_warning "SYNAPSE_DB_MIGRATE_ALLOW_HOST_PSQL=1：放行宿主 psql 打 $DB_HOST:$DB_PORT"
        fi

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

# 在任何写操作之前把"这一刀落在哪台服务器上"打出来。
# 事后排查"命令打错实例"时，日志里有这一行就够了。
log_target_banner() {
    local backend
    if [ "$PSQL_USE_DOCKER" -eq 1 ]; then
        backend="docker exec $PSQL_DOCKER_CONTAINER (容器内 psql)"
    else
        backend="宿主 psql"
    fi
    log_info "迁移目标: ${DB_USER}@${DB_HOST}:${DB_PORT}/${DB_NAME}  [backend: ${backend}]"
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
    psql_db -v ON_ERROR_STOP=1 -v tname="$table_name" -tA <<'SQL' 2>/dev/null | grep -q '^t$'
SELECT EXISTS (
    SELECT 1
    FROM information_schema.tables
    WHERE table_schema = 'public' AND table_name = :'tname'
);
SQL
}

schema_migrations_has_column() {
    local column_name="$1"
    psql_db -v ON_ERROR_STOP=1 -v cname="$column_name" -tA <<'SQL' 2>/dev/null | grep -q '^t$'
SELECT EXISTS (
    SELECT 1
    FROM information_schema.columns
    WHERE table_schema = 'public'
      AND table_name = 'schema_migrations'
      AND column_name = :'cname'
);
SQL
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
    is_success BOOLEAN NOT NULL DEFAULT TRUE,
    description TEXT,
    executed_at BIGINT DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    CONSTRAINT uq_schema_migrations_version UNIQUE (version)
);
ALTER TABLE schema_migrations ADD COLUMN IF NOT EXISTS name TEXT;
ALTER TABLE schema_migrations ADD COLUMN IF NOT EXISTS checksum TEXT;
ALTER TABLE schema_migrations ADD COLUMN IF NOT EXISTS applied_ts BIGINT;
ALTER TABLE schema_migrations ADD COLUMN IF NOT EXISTS execution_time_ms BIGINT;
ALTER TABLE schema_migrations ADD COLUMN IF NOT EXISTS is_success BOOLEAN NOT NULL DEFAULT TRUE;
ALTER TABLE schema_migrations ADD COLUMN IF NOT EXISTS description TEXT;
ALTER TABLE schema_migrations ADD COLUMN IF NOT EXISTS executed_at BIGINT DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT;
CREATE UNIQUE INDEX IF NOT EXISTS idx_schema_migrations_version ON schema_migrations(version);
SQL
}

latest_baseline_file() {
    find "$MIGRATIONS_DIR" -maxdepth 1 -type f -name '00000000_unified_schema_v*.sql' ! -name '*.undo.sql' | sort | tail -n 1
}

# 除当前选中的基线（`apply_pending_migrations` 已按路径跳过）外，任何
# `00000000_unified_schema_v*.sql` 都不能再作为"增量迁移"执行：它们的
# `CREATE TABLE IF NOT EXISTS` 会对最新基线已建的表空转，而 `CREATE INDEX`
# 可能在已被改名/删除的列上失败；更严重的是会被写进 schema_migrations，
# 让本地机器（磁盘可能残留历史基线）与 CI 全新检出的 schema 彻底分叉。
# 判据唯一实现在本文件：部署侧 `docker/deploy/scripts/container-migrate.sh` 已收敛为
# 只 exec 本脚本的薄包装（commit 8efe7b77），不再自带 is_baseline_file()。
is_superseded_by_latest_baseline() {
    local filename="$1"

    case "$filename" in
        00000000_unified_schema_v*.sql) return 0 ;;
    esac

    return 1
}

# Content checksum of a migration file.
#
# `schema_migrations.checksum` used to store `md5(filename)` — a constant per file,
# so it could never tell that a baseline had been *edited*. That matters because this
# repo's convention is to fold schema changes directly into
# `00000000_unified_schema_v12.sql`; with a filename hash the runner saw "same
# version, already applied" and silently skipped every such change (a deployed
# database could not be upgraded by `migrate` at all).
#
# `md5sum` (coreutils) and `md5` (BSD/macOS) differ in flags, so try both.
file_content_checksum() {
    local file="$1"
    if command -v md5sum >/dev/null 2>&1; then
        md5sum "$file" | awk '{print $1}'
    elif command -v md5 >/dev/null 2>&1; then
        md5 -q "$file"
    else
        cksum "$file" | awk '{print $1}'
    fi
}

# Checksum recorded for `version`, or empty when the row/column is absent.
recorded_migration_checksum() {
    local version="$1"
    psql_db -v ON_ERROR_STOP=1 -v ver="$version" -tA <<'SQL' 2>/dev/null | tr -d '[:space:]'
SELECT COALESCE(checksum, '') FROM schema_migrations WHERE version = :'ver';
SQL
}

record_migration() {
    local version="$1"
    local filename="$2"
    local duration_ms="$3"
    local success="$4"
    local checksum="${5:-}"

    if ! [[ "$duration_ms" =~ ^[0-9]+$ ]]; then duration_ms=0; fi
    if [ "$success" != "TRUE" ] && [ "$success" != "FALSE" ]; then success="FALSE"; fi

    psql_db -v ON_ERROR_STOP=1 \
        -v ver="$version" \
        -v fname="$filename" \
        -v dur="$duration_ms" \
        -v ok="$success" \
        -v cksum="$checksum" \
        <<'SQL' >/dev/null
INSERT INTO schema_migrations (version, name, checksum, applied_ts, execution_time_ms, is_success, description, executed_at)
VALUES (
    :'ver',
    :'fname',
    NULLIF(:'cksum', ''),
    EXTRACT(EPOCH FROM NOW()) * 1000,
    :'dur'::BIGINT,
    :'ok'::BOOLEAN,
    :'fname',
    (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT
)
ON CONFLICT (version) DO UPDATE SET
    name = EXCLUDED.name,
    checksum = EXCLUDED.checksum,
    applied_ts = EXCLUDED.applied_ts,
    execution_time_ms = EXCLUDED.execution_time_ms,
    is_success = EXCLUDED.is_success,
    description = EXCLUDED.description,
    executed_at = EXCLUDED.executed_at;
SQL
}

is_migration_applied() {
    local version="$1"
    psql_db -v ON_ERROR_STOP=1 -v ver="$version" -tA <<'SQL' 2>/dev/null | grep -q '^t$'
SELECT COALESCE(bool_and(is_success), FALSE) FROM schema_migrations WHERE version = :'ver';
SQL
}

apply_sql_file() {
    local file="$1"
    local tolerant="${2:-false}"
    local filename
    filename="$(basename "$file")"
    local version="${filename%.sql}"
    local started_at
    started_at="$(now_ms)"
    local checksum
    checksum="$(file_content_checksum "$file")"

    log_info "应用迁移: $filename"

    if [ "$tolerant" = "true" ]; then
        psql_db <"$file" >/dev/null 2>&1 || true
        local finished_at
        finished_at="$(now_ms)"
        record_migration "$version" "$filename" "$((finished_at - started_at))" TRUE "$checksum"
        log_success "迁移完成 (容错模式): $filename"
        return 0
    fi

    if psql_db -v ON_ERROR_STOP=1 <"$file" >/dev/null; then
        local finished_at
        finished_at="$(now_ms)"
        record_migration "$version" "$filename" "$((finished_at - started_at))" TRUE "$checksum"
        log_success "迁移完成: $filename"
        return 0
    fi

    local finished_at
    finished_at="$(now_ms)"
    psql_db -c "ABORT;" >/dev/null 2>&1 || true
    record_migration "$version" "$filename" "$((finished_at - started_at))" FALSE "$checksum" || true
    log_error "迁移失败: $filename"
    return 1
}

init_database() {
    log_info "初始化数据库..."
    ensure_schema_migrations_table

    local baseline_file
    baseline_file="$(latest_baseline_file)"
    if [ -z "$baseline_file" ]; then
        log_error "找不到统一基线脚本"
        return 1
    fi

    local baseline_name
    baseline_name="$(basename "$baseline_file")"
    local baseline_version="${baseline_name%.sql}"

    local baseline_checksum
    baseline_checksum="$(file_content_checksum "$baseline_file")"

    if is_migration_applied "$baseline_version"; then
        local recorded_checksum
        recorded_checksum="$(recorded_migration_checksum "$baseline_version")"
        if [ "$recorded_checksum" = "$baseline_checksum" ]; then
            log_info "基线迁移已记录且内容未变: $baseline_name"
            return 0
        fi
        # 本仓库约定：schema 变更直接折入基线（没有时间戳增量文件），所以内容变化
        # 就是"有待应用的变更"。基线是幂等的（只有 IF NOT EXISTS / DROP IF EXISTS
        # 与一次幂等的去重 DELETE），因此以容错模式重放。
        log_warning "基线内容已变化，重放基线以应用变更: $baseline_name (记录: ${recorded_checksum:-<空>} / 当前: $baseline_checksum)"
        apply_sql_file "$baseline_file" "true"
        return 0
    fi

    if table_exists "users"; then
        log_info "检测到现有业务表，使用容错模式应用基线迁移"
        apply_sql_file "$baseline_file" "true"
    else
        apply_sql_file "$baseline_file"
    fi
}

get_current_version() {
    psql_db -tAc "SELECT COALESCE(version, '') FROM schema_migrations ORDER BY COALESCE(applied_ts, 0) DESC, version DESC LIMIT 1" 2>/dev/null | tr -d '[:space:]'
}

list_applied_migrations() {
    ensure_schema_migrations_table
    log_info "已应用的迁移:"
    psql_db -c "
        SELECT version, COALESCE(name, description, version) AS name, is_success, applied_ts
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
    local baseline_name=""
    if [ -n "$baseline_file" ]; then
        baseline_name="$(basename "$baseline_file")"
    fi
    local pending=0

    local migration_list
    migration_list="$(mktemp "${TMPDIR:-/tmp}/db_migrate.XXXXXX")"
    find "$MIGRATIONS_DIR" -maxdepth 1 -type f -name '*.sql' ! -name '*.undo.sql' | sort >"$migration_list"

    while IFS= read -r file; do
        local filename
        filename="$(basename "$file")"
        local version="${filename%.sql}"

        if [ -n "$baseline_file" ] && [ "$file" = "$baseline_file" ]; then
            continue
        fi

        if is_superseded_by_latest_baseline "$filename"; then
            log_info "跳过历史基线（已被 ${baseline_name} 取代）: $filename"
            continue
        fi

        if is_migration_applied "$version"; then
            continue
        fi

        pending=$((pending + 1))
        apply_sql_file "$file"
    done <"$migration_list"

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
        "background_updates"
        "room_retention_policies"
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
    stale_count="$(psql_db -v ON_ERROR_STOP=1 -tAc "
        SELECT COUNT(*)
        FROM pg_stat_activity
        WHERE state = 'idle in transaction'
          AND query_start < NOW() - INTERVAL '5 minutes'
          AND datname = current_database()
    " 2>/dev/null | tr -d '[:space:]' || true)"

    if [ -n "${stale_count:-}" ] && [[ "$stale_count" =~ ^[0-9]+$ ]] && [ "$stale_count" -gt 0 ]; then
        log_warning "发现 $stale_count 个空闲事务连接，正在清理..."
        psql_db -c "
            SELECT pg_terminate_backend(pid)
            FROM pg_stat_activity
            WHERE state = 'idle in transaction'
              AND query_start < NOW() - INTERVAL '5 minutes'
              AND datname = current_database()
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
            log_target_banner
            check_db_connection
            cleanup_stale_connections
            init_database
            ;;
        migrate)
            log_target_banner
            check_db_connection
            cleanup_stale_connections
            apply_pending_migrations
            ;;
        status)
            log_target_banner
            check_db_connection
            list_applied_migrations
            ;;
        validate)
            log_target_banner
            check_db_connection
            validate_schema
            ;;
        help | --help | -h)
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
