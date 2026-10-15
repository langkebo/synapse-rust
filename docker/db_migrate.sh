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
    # 除 DATABASE_URL 外，**分项给出的 DB_\* 也属于"调用方显式指定"**，必须记下来：
    # source `.env` 会把它们一并覆盖（见下方恢复逻辑）。2026-10-15 实测事故：
    # 只传 `DB_NAME=<临时库>` 时被 `docker/.env` 覆盖成 `synapse`
    # ⇒ "给临时库跑迁移"实际打到了**正在服务中的应用库**上。
    local user_supplied_db_name="${DB_NAME:-}"
    local user_supplied_db_user="${DB_USER:-}"
    local user_supplied_db_port="${DB_PORT:-}"
    local user_supplied_db_password="${DB_PASSWORD:-}"
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
    else
        # 没有 DATABASE_URL 时，把调用方显式给出的分项 DB_* 恢复回来 ——
        # 上面 `set -a; . "$env_file"` 已经把它们覆盖成 `.env` 的值。
        # 优先级：DATABASE_URL（若给）> 调用方 DB_* > `.env` > 内置默认。
        # 判据（为什么这条对 CI 零影响）：`docker/.env` 在 `.gitignore` 里
        # （`.gitignore:15`）⇒ CI 全新检出根本没有这个文件，也就谈不上覆盖。
        if [ -n "$user_supplied_db_host" ]; then export DB_HOST="$user_supplied_db_host"; fi
        if [ -n "$user_supplied_db_port" ]; then export DB_PORT="$user_supplied_db_port"; fi
        if [ -n "$user_supplied_db_name" ]; then export DB_NAME="$user_supplied_db_name"; fi
        if [ -n "$user_supplied_db_user" ]; then export DB_USER="$user_supplied_db_user"; fi
        if [ -n "$user_supplied_db_password" ]; then export DB_PASSWORD="$user_supplied_db_password"; fi
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

        # ── 护栏：隐式目标不猜 ────────────────────────────────────────────────
        #
        # ⚠️ 2026-10-15「环境收敛」后的前提已变：**容器 5432 已发布到宿主回环**
        # （两个 compose 都加了 `ports: ["127.0.0.1:5432:5432"]`），所以
        # `localhost:5432` **现在通常就是本项目库**；宿主自装的两台 Homebrew PG 已全部停用。
        # 这条护栏**仍然保留**，但理由从"那个端口上是**别人的** PG"变成了
        # **"隐式目标本身就不该被猜"** —— 由 `docker/.env` 兜底出来的目标无法说明
        # "这一刀该落在哪"，而 DDL 打错实例的代价是**建库/改表**（见下面的 H-14 事故）。
        #
        # 2026-09-15 实测（复现步骤：裸跑 `bash docker/db_migrate.sh validate`）：
        # 当时宿主 5432 是本机 Homebrew PostgreSQL，脚本在它上面
        # **建了库**（"[INFO] 数据库不存在，尝试创建: synapse"）之后才报"表缺失" ——
        # 一条只读语气的命令改动了错误的实例（PROJECT_ACTUAL_ISSUES §6 H-14）。
        if host_psql_target_is_implicit_loopback; then
            if [ "${SYNAPSE_DB_MIGRATE_ALLOW_HOST_PSQL:-0}" != "1" ]; then
                log_error "拒绝执行：本次要用宿主 psql 连 ${DB_HOST}:${DB_PORT}，而目标是从 docker/.env 兜底出来的默认值。"
                log_error "DDL 打错实例的代价是建库/改表，所以本脚本不接受**猜出来**的目标。"
                log_error "（2026-10-15 起 $DB_HOST:$DB_PORT 上通常已是**本项目**库 —— 容器 5432 发布到回环；"
                log_error "  但「通常是本项目库」仍不足以让脚本替你决定。）"
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

# 迁移 advisory lock 的**取锁语句**（B4.2，2026-10-15）。
#
# key 与 Rust 侧**同一表达式**：`synapse-services/src/database_initializer/mod.rs`
# 的 `migration_lock_key()` 用的是
#     hashtext(COALESCE(current_database() || ':' || current_schema(), current_database() || ':'))::bigint
# 这里照抄同一条表达式，而**不是**各自硬编码一个魔数 —— 魔数会漂移，表达式不会。
# 两处都留了互相指认的注释。
#
# `COALESCE` 不是装饰：`current_schema()` 在 `search_path` 为空时返回 NULL（D-69），
# 少了它 `hashtext(NULL)` 也是 NULL ⇒ 取锁语句本身会失败。
migration_lock_statement() {
    echo "SELECT pg_advisory_lock(hashtext(COALESCE(current_database() || ':' || current_schema(), current_database() || ':'))::bigint);"
}

# ---------------------------------------------------------------------------
# 迁移锁的**持有会话**（B4.2，2026-10-15）
# ---------------------------------------------------------------------------
# 为什么必须是"一个长驻会话"：advisory lock 是**会话级**的，而一次 `migrate` 会开
# 好几个 psql 会话（记账建表 → 应用迁移文件 → 写账本）。只在"应用文件"那次会话里
# 取锁覆盖不到记账阶段 —— 实测并发两次 `migrate` 时，后到的那个会在
# `ensure_schema_migrations_table` 上撞目录竞态并失败：
#     ERROR: duplicate key value violates unique constraint "pg_type_typname_nsp_index"
# ⇒ 必须让**一个会话**在整次命令期间持锁。
#
# 机制：后台 psql 从 FIFO 读语句（按住不放，读到 EOF 才退出 ⇒ 锁一直握着），
# 取到锁后打印哨兵；主流程等到哨兵出现才继续（这同时给出了"阻塞等待"的语义）。
# 结束时关闭写端 ⇒ psql 收到 EOF ⇒ 会话结束 ⇒ 锁自动释放。
#
# ⚠️ 用**固定 fd**（`exec 9>`）而不是 `exec {fd}>`：macOS 自带 `/bin/bash` 是
#    **3.2**，不支持动态 fd 语法；本脚本的 shebang 就是 `#!/bin/bash`。
MIGRATION_LOCK_FD=9
MIGRATION_LOCK_DIR=""
MIGRATION_LOCK_HOLDER_PID=""

acquire_migration_lock() {
    MIGRATION_LOCK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/synapse-miglock.XXXXXX")"
    local in_fifo="$MIGRATION_LOCK_DIR/in"
    local out_file="$MIGRATION_LOCK_DIR/out"
    : >"$out_file"
    mkfifo "$in_fifo"

    # 读者先起（FIFO 的写端 open 会阻塞到有读者为止）
    psql_db -q -A -t -f - <"$in_fifo" >>"$out_file" 2>&1 &
    MIGRATION_LOCK_HOLDER_PID=$!

    eval "exec $MIGRATION_LOCK_FD>\"$in_fifo\""

    {
        migration_lock_statement
        printf "SELECT 'SYNAPSE_MIGRATION_LOCK_ACQUIRED';\n"
    } >&"$MIGRATION_LOCK_FD"

    local waited=0
    while ! grep -q 'SYNAPSE_MIGRATION_LOCK_ACQUIRED' "$out_file" 2>/dev/null; do
        if ! kill -0 "$MIGRATION_LOCK_HOLDER_PID" 2>/dev/null; then
            log_error "迁移锁持有会话异常退出，未取得锁:"
            sed 's/^/    /' "$out_file" >&2
            release_migration_lock
            return 1
        fi
        # 已有持有者时，上面的取锁语句会让本会话**阻塞等待**，这里就一直等哨兵。
        sleep 0.2 2>/dev/null || sleep 1
        waited=$((waited + 1))
        if [ "$waited" -gt 900 ]; then
            log_error "等待迁移 advisory lock 超时（约 3 分钟）；可能有卡死的迁移会话"
            release_migration_lock
            return 1
        fi
    done
    log_success "已获取迁移 advisory lock（持有会话 pid=${MIGRATION_LOCK_HOLDER_PID}，与 Rust 侧同一 key）"
    return 0
}

release_migration_lock() {
    if [ -n "$MIGRATION_LOCK_DIR" ]; then
        # 关写端 ⇒ 持有会话读到 EOF ⇒ 退出 ⇒ 锁释放
        eval "exec $MIGRATION_LOCK_FD>&-" 2>/dev/null || true
        if [ -n "$MIGRATION_LOCK_HOLDER_PID" ]; then
            wait "$MIGRATION_LOCK_HOLDER_PID" 2>/dev/null || true
        fi
        rm -rf "$MIGRATION_LOCK_DIR"
        MIGRATION_LOCK_DIR=""
        MIGRATION_LOCK_HOLDER_PID=""
    fi
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
    id BIGSERIAL,
    version TEXT NOT NULL,
    name TEXT,
    checksum TEXT,
    applied_ts BIGINT,
    execution_time_ms BIGINT,
    is_success BOOLEAN NOT NULL DEFAULT TRUE,
    description TEXT,
    executed_at BIGINT DEFAULT ((EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT),
    -- ⚠️ 必须与 canonical 基线 `migrations/00000000_unified_schema_v12.sql` 的
    -- `schema_migrations` 定义一致：**命名式 PK**。此前写内联 `PRIMARY KEY` ⇒ 实际名字是
    -- PostgreSQL 自动生成的 `schema_migrations_pkey`，与基线声明的 `pk_schema_migrations`
    -- 不一致（且引导总是先建表 ⇒ 基线的命名永不生效）。老库由基线的幂等 RENAME 收敛。
    CONSTRAINT pk_schema_migrations PRIMARY KEY (id),
    CONSTRAINT uq_schema_migrations_version UNIQUE (version)
);
ALTER TABLE schema_migrations ADD COLUMN IF NOT EXISTS name TEXT;
ALTER TABLE schema_migrations ADD COLUMN IF NOT EXISTS checksum TEXT;
ALTER TABLE schema_migrations ADD COLUMN IF NOT EXISTS applied_ts BIGINT;
ALTER TABLE schema_migrations ADD COLUMN IF NOT EXISTS execution_time_ms BIGINT;
ALTER TABLE schema_migrations ADD COLUMN IF NOT EXISTS is_success BOOLEAN NOT NULL DEFAULT TRUE;
ALTER TABLE schema_migrations ADD COLUMN IF NOT EXISTS description TEXT;
ALTER TABLE schema_migrations ADD COLUMN IF NOT EXISTS executed_at BIGINT DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT;
-- ⚠️ 刻意**不建** `idx_schema_migrations_version`（2026-10-15，方案 B5.4）：
-- 它与上面的 `uq_schema_migrations_version`（UNIQUE(version)）**完全重复** ——
-- 同一列的又一棵 btree，只带来双倍写放大与双倍空间，不提供任何新能力。
-- 更糟的是本处曾写 `CREATE UNIQUE INDEX`、而 `docker/deploy/scripts/init-db.sql` 与
-- Rust `database_initializer` 写的是非唯一 ⇒ `IF NOT EXISTS` 下**谁先跑谁赢**，
-- 同一版本号在不同机器上得到不同语义。三方已统一为"不建"，老库由基线的
-- `DROP INDEX IF EXISTS` 收敛。
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
    local finished_at
    local checksum
    checksum="$(file_content_checksum "$file")"
    local err_file
    err_file="$(mktemp)"
    local psql_rc=0

    log_info "应用迁移: $filename"
    # 锁由**整次命令的长驻持有会话**持有（见 acquire_migration_lock）：这里刻意
    # **不再**单独取锁 —— 同一把会话级锁若在本会话再取一次，会与持有者互相等待
    # （持有者要等命令结束才释放）⇒ 自死锁。

    # ------------------------------------------------------------------
    # 迁移锁（B4.2）：在**同一个 psql 会话**里先取会话级 advisory lock，再跑文件内容。
    #   * 会话结束（正常或报错）即自动释放 ⇒ 不需要显式 unlock，也就没有
    #     "unlock 失败反而把一次成功的迁移变成错误"这种边角。
    #   * ⚠️ 不可把 `pg_advisory_lock` 放到**另一次** psql 调用里：会话级锁会随那次
    #     调用结束立刻释放，等于没锁 —— 这是 advisory lock 在 shell 里最常见的误用。
    #   * 锁的粒度是"每个被应用的迁移文件"。本仓库**没有时间戳增量文件**（schema 变更
    #     直接折入唯一基线），所以实际上就等于"整次 migrate 全程持锁"。
    # ------------------------------------------------------------------
    if [ "$tolerant" = "true" ]; then
        # 容错模式：逐语句跑、个别语句失败不中断（基线含 `CREATE INDEX CONCURRENTLY`
        # 等不能在事务块里执行的语句）。
        #
        # ⚠️ 这里**不能靠 psql 退出码判断成败**（2026-10-15 实测）：不带
        #    `ON_ERROR_STOP` 时，即使语句报错 psql 也返回 **0** ——
        #    含 `SELECT 1/0` 的脚本 `exit=0`，加 `ON_ERROR_STOP=1` 才 `exit=3`。
        #    所以判据只能是 **stderr 里的 `ERROR:` 行**。
        #
        # 判据为何可靠：基线**确实幂等** —— 实测把整份基线重放到已迁移库，
        #    `ERROR` 行数 **0**（`psql_db` 用 `client_min_messages=warning`，
        #    NOTICE 已被压掉）。因此"重放出现 ERROR" ⇒ 真有问题，
        #    而不是"预期内的重复对象"。
        psql_db <"$file" >/dev/null 2>"$err_file" || psql_rc=$?

        local error_count=0
        error_count="$(grep -c '^ERROR' "$err_file" 2>/dev/null || true)"
        error_count="${error_count:-0}"

        finished_at="$(now_ms)"
        if [ "$psql_rc" -eq 0 ] && [ "$error_count" -eq 0 ]; then
            record_migration "$version" "$filename" "$((finished_at - started_at))" TRUE "$checksum"
            log_success "迁移完成 (容错模式): $filename"
            rm -f "$err_file"
            return 0
        fi

        # 记实话：**不把半迁移记成成功**。否则叠加"checksum 相同即 early return"，
        # 半迁移会被永久固化成一个"已应用"的版本（见
        # docs/audit/DB_OPTIMIZATION_PLAN_2026-10-15.md §5 B4.1）。
        record_migration "$version" "$filename" "$((finished_at - started_at))" FALSE "$checksum" || true
        log_error "容错模式仍有语句失败: $filename（psql 退出码=$psql_rc，ERROR 行数=$error_count）"
        if [ "$error_count" -gt 0 ]; then
            log_error "失败语句（最多 10 条）:"
            grep '^ERROR' "$err_file" | head -10 | sed 's/^/    /' >&2
        fi
        log_error "已记为 is_success=FALSE。调用方（如 docker/entrypoint.sh 的"
        log_error "STOP_ON_MIGRATION_FAILURE）据此决定是否阻断启动。"
        rm -f "$err_file"
        return 1
    fi

    if psql_db -v ON_ERROR_STOP=1 <"$file" >/dev/null 2>"$err_file"; then
        finished_at="$(now_ms)"
        record_migration "$version" "$filename" "$((finished_at - started_at))" TRUE "$checksum"
        log_success "迁移完成: $filename"
        rm -f "$err_file"
        return 0
    fi

    finished_at="$(now_ms)"
    psql_db -c "ABORT;" >/dev/null 2>&1 || true
    record_migration "$version" "$filename" "$((finished_at - started_at))" FALSE "$checksum" || true
    log_error "迁移失败: $filename"
    head -20 "$err_file" 2>/dev/null | sed 's/^/    /' >&2
    rm -f "$err_file"
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
        # ⚠️ 必须传播失败：容错模式现在会如实报告"仍有语句失败"（B4.1）。
        # 这里若照旧 `return 0`，失败就被吞掉，而 checksum 已被记为"当前值" ⇒
        # 下一次跑会 early return，半迁移永远修不回来。
        apply_sql_file "$baseline_file" "true" || return 1
        return 0
    fi

    if table_exists "users"; then
        log_info "检测到现有业务表，使用容错模式应用基线迁移"
        # 注意：这里刻意不吞返回值 —— `if/else` 的退出码取自最后执行的命令，
        # 因此 `init_database` 会把 apply_sql_file 的失败如实传给调用方。
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
            # 写操作：整次命令持迁移锁（B4.2）。trap 保证任何退出路径都会释放
            # （脚本在子进程里运行，不会影响调用方的 trap）。
            trap 'release_migration_lock' EXIT INT TERM
            acquire_migration_lock || exit 1
            init_database
            ;;
        migrate)
            log_target_banner
            check_db_connection
            cleanup_stale_connections
            trap 'release_migration_lock' EXIT INT TERM
            acquire_migration_lock || exit 1
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
