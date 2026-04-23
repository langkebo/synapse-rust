#!/bin/sh
# =============================================================================
# container-migrate.sh — Database migration runner
# =============================================================================
# Supports selective extension migrations via ENABLED_EXTENSIONS env var.
# Core migrations are always applied. Extension migrations are skipped
# unless their feature is listed in ENABLED_EXTENSIONS.
#
# Examples:
#   ENABLED_EXTENSIONS=all              — apply everything (default)
#   ENABLED_EXTENSIONS=none             — core only, skip all extensions
#   ENABLED_EXTENSIONS=openclaw-routes,friends  — only named extensions
# =============================================================================

set -eu

MIGRATIONS_DIR="${MIGRATIONS_DIR:-/migrations}"
DB_HOST="${DB_HOST:-postgres}"
DB_PORT="${DB_PORT:-5432}"
DB_NAME="${DB_NAME:-synapse}"
DB_USER="${DB_USER:-postgres}"
DB_PASSWORD="${DB_PASSWORD:-}"
ENABLED_EXTENSIONS="${ENABLED_EXTENSIONS:-all}"
EXTENSION_MAP="${MIGRATIONS_DIR}/extension_map.conf"

if [ -n "$DB_PASSWORD" ]; then
    export PGPASSWORD="$DB_PASSWORD"
fi

log() {
    level="$1"
    shift
    printf '[%s] %s\n' "$level" "$*"
}

psql_db() {
    psql -v ON_ERROR_STOP=1 -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" "$@"
}

psql_admin() {
    psql -v ON_ERROR_STOP=1 -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d postgres "$@"
}

ensure_database_exists() {
    if psql_db -c "SELECT 1" >/dev/null 2>&1; then
        return 0
    fi

    log INFO "数据库不存在，尝试创建: $DB_NAME"
    psql_admin -c "CREATE DATABASE \"$DB_NAME\";" >/dev/null 2>&1 || true
    psql_db -c "SELECT 1" >/dev/null 2>&1
}

ensure_schema_migrations_table() {
    psql_db >/dev/null <<'SQL'
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

table_exists() {
    table_name="$1"
    psql_db -tAc "SELECT EXISTS (
        SELECT 1
        FROM information_schema.tables
        WHERE table_schema = 'public' AND table_name = '$table_name'
    )" 2>/dev/null | grep -q '^t$'
}

is_migration_applied() {
    version="$1"
    psql_db -tAc "SELECT COALESCE(bool_and(success), FALSE) FROM schema_migrations WHERE version = '$version'" 2>/dev/null | grep -q '^t$'
}

# ---------------------------------------------------------------------------
# Extension filtering
# ---------------------------------------------------------------------------

# Check if a migration file is an extension migration and whether it should
# be applied given the current ENABLED_EXTENSIONS setting.
should_apply_migration() {
    filename="$(basename "$1")"

    # Always apply if extensions are not filtered
    if [ "$ENABLED_EXTENSIONS" = "all" ]; then
        return 0
    fi

    # If no extension map exists, apply everything
    if [ ! -f "$EXTENSION_MAP" ]; then
        return 0
    fi

    # Look up the file in the extension map
    required_feature=""
    while IFS= read -r line; do
        # Skip comments and empty lines
        case "$line" in
            \#*|"") continue ;;
        esac
        map_file="${line%%=*}"
        map_feature="${line#*=}"
        if [ "$map_file" = "$filename" ]; then
            required_feature="$map_feature"
            break
        fi
    done < "$EXTENSION_MAP"

    # Not in the map — it's a core migration, always apply
    if [ -z "$required_feature" ]; then
        return 0
    fi

    # In the map — check if the feature is enabled
    if [ "$ENABLED_EXTENSIONS" = "none" ]; then
        log INFO "跳过扩展迁移 (feature=$required_feature): $filename"
        return 1
    fi

    # Check comma-separated list
    case ",$ENABLED_EXTENSIONS," in
        *",$required_feature,"*)
            return 0
            ;;
    esac

    log INFO "跳过扩展迁移 (feature=$required_feature 未启用): $filename"
    return 1
}

apply_sql_file() {
    file="$1"
    filename="$(basename "$file")"
    version="${filename%.sql}"
    started_at="$(date +%s)"

    log INFO "应用迁移: $filename"
    if psql_db < "$file" >/dev/null; then
        finished_at="$(date +%s)"
        duration_ms=$(( (finished_at - started_at) * 1000 ))
        psql_db -c "
            INSERT INTO schema_migrations (version, name, checksum, applied_ts, execution_time_ms, success, description, executed_at)
            VALUES (
                '$version',
                '$filename',
                md5('$filename'),
                EXTRACT(EPOCH FROM NOW()) * 1000,
                $duration_ms,
                TRUE,
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
        log INFO "迁移完成: $filename"
        return 0
    fi

    finished_at="$(date +%s)"
    duration_ms=$(( (finished_at - started_at) * 1000 ))
    psql_db -c "ABORT;" >/dev/null 2>&1 || true
    psql_db -c "
        INSERT INTO schema_migrations (version, name, checksum, applied_ts, execution_time_ms, success, description, executed_at)
        VALUES (
            '$version',
            '$filename',
            md5('$filename'),
            EXTRACT(EPOCH FROM NOW()) * 1000,
            $duration_ms,
            FALSE,
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
    " >/dev/null || true
    log ERROR "迁移失败: $filename"
    return 1
}

init_database() {
    ensure_schema_migrations_table

    if table_exists "users"; then
        log INFO "检测到现有业务表，跳过基线初始化"
        return 0
    fi

    baseline_file="$(latest_baseline_file)"
    if [ -z "$baseline_file" ]; then
        log ERROR "找不到统一基线脚本"
        return 1
    fi

    baseline_name="$(basename "$baseline_file")"
    baseline_version="${baseline_name%.sql}"
    if is_migration_applied "$baseline_version"; then
        log INFO "基线迁移已记录: $baseline_name"
        return 0
    fi

    apply_sql_file "$baseline_file"
}

apply_pending_migrations() {
    ensure_database_exists
    ensure_schema_migrations_table
    init_database

    log INFO "扩展模式: ENABLED_EXTENSIONS=$ENABLED_EXTENSIONS"

    skipped=0
    applied=0

    find "$MIGRATIONS_DIR" -maxdepth 1 -type f -name '*.sql' ! -name '*.undo.sql' ! -name '*.conf' | sort | while IFS= read -r file; do
        baseline_file="$(latest_baseline_file)"
        if [ -n "$baseline_file" ] && [ "$file" = "$baseline_file" ]; then
            continue
        fi

        version="$(basename "$file" .sql)"
        if is_migration_applied "$version"; then
            continue
        fi

        if ! should_apply_migration "$file"; then
            skipped=$((skipped + 1))
            continue
        fi

        apply_sql_file "$file"
        applied=$((applied + 1))
    done

    log INFO "迁移完成: applied=$applied, skipped=$skipped (extension filtered)"
}

validate_schema() {
    ensure_database_exists
    ensure_schema_migrations_table

    missing=0
    # Core tables — always required
    for table in users devices access_tokens refresh_tokens rooms events event_relations rate_limits schema_migrations; do
        if table_exists "$table"; then
            log INFO "表存在: $table"
        else
            log ERROR "表缺失: $table"
            missing=$((missing + 1))
        fi
    done

    # Extension tables — only validate if their feature is enabled
    if [ "$ENABLED_EXTENSIONS" = "all" ] || echo ",$ENABLED_EXTENSIONS," | grep -q ",widgets,"; then
        for table in widgets; do
            if table_exists "$table"; then
                log INFO "表存在 (widgets): $table"
            else
                log ERROR "表缺失 (widgets): $table"
                missing=$((missing + 1))
            fi
        done
    fi

    if [ "$ENABLED_EXTENSIONS" = "all" ] || echo ",$ENABLED_EXTENSIONS," | grep -q ",server-notifications,"; then
        for table in server_notifications user_notification_status; do
            if table_exists "$table"; then
                log INFO "表存在 (server-notifications): $table"
            else
                log ERROR "表缺失 (server-notifications): $table"
                missing=$((missing + 1))
            fi
        done
    fi

    if [ "$ENABLED_EXTENSIONS" = "all" ] || echo ",$ENABLED_EXTENSIONS," | grep -q ",openclaw-routes,"; then
        for table in openclaw_connections ai_conversations ai_connections; do
            if table_exists "$table"; then
                log INFO "表存在 (openclaw): $table"
            else
                log ERROR "表缺失 (openclaw): $table"
                missing=$((missing + 1))
            fi
        done
    fi

    if [ "$missing" -gt 0 ]; then
        log ERROR "数据库架构验证失败，缺失 $missing 个表"
        return 1
    fi

    log INFO "数据库架构验证通过"
}

list_applied_migrations() {
    ensure_database_exists
    ensure_schema_migrations_table
    psql_db -c "
        SELECT version, COALESCE(name, description, version) AS name, success, applied_ts
        FROM schema_migrations
        ORDER BY COALESCE(applied_ts, 0) DESC, version DESC
    "
}

show_help() {
    cat <<'EOF'
用法: container-migrate.sh <命令>

命令:
  migrate    应用待执行的迁移（根据 ENABLED_EXTENSIONS 过滤扩展迁移）
  validate   验证数据库 schema 完整性
  status     显示已执行的迁移记录

环境变量:
  ENABLED_EXTENSIONS   控制扩展迁移范围（默认: all）
    all                应用所有迁移（默认行为）
    none               仅应用核心迁移，跳过所有扩展
    <feature,...>      逗号分隔的功能列表，如: openclaw-routes,friends

  可用功能: openclaw-routes, friends, voice-extended, saml-sso, cas-sso,
           beacons, voip-tracking, widgets, server-notifications,
           burn-after-read, privacy-ext, external-services
EOF
}

main() {
    command="${1:-migrate}"

    case "$command" in
        migrate)
            apply_pending_migrations
            ;;
        validate)
            validate_schema
            ;;
        status)
            list_applied_migrations
            ;;
        help|--help|-h)
            show_help
            ;;
        *)
            log ERROR "未知命令: $command"
            show_help
            exit 1
            ;;
    esac
}

main "$@"
