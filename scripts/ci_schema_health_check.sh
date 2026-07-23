#!/bin/bash
# CI: Database schema health check gate
#
# 强制门禁：构建 schema_health_check 二进制 → 启动临时 DB（如未提供 DATABASE_URL）→
# 跑 v8 基线迁移 → 运行 schema 健康检查。
#
# 退出码：
#   0 - schema 健康
#   非 0 - schema 漂移或执行错误
#
# 环境变量：
#   DATABASE_URL  - 直接使用提供的连接（CI 期望预置）
#   SKIP_DB_SETUP - 设为 1 跳过 DB 启动与迁移（仅当 DB 已准备就绪时）
#   POSTGRES_USER / POSTGRES_PASSWORD / POSTGRES_DB / POSTGRES_PORT - 自建 DB 时使用
#
# 用法：
#   bash scripts/ci_schema_health_check.sh          # 完整流程
#   DATABASE_URL=... bash scripts/ci_schema_health_check.sh  # 直接连已存在的 DB
#   SKIP_DB_SETUP=1 bash scripts/ci_schema_health_check.sh   # 跳过启动与迁移
#
# CI 集成示例 (.github/workflows):
#   - name: Schema health check
#     run: bash scripts/ci_schema_health_check.sh

set -eEuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

POSTGRES_USER="${POSTGRES_USER:-synapse}"
POSTGRES_PASSWORD="${POSTGRES_PASSWORD:-synapse}"
POSTGRES_DB="${POSTGRES_DB:-synapse_ci}"
POSTGRES_PORT="${POSTGRES_PORT:-5432}"
POSTGRES_HOST="${POSTGRES_HOST:-127.0.0.1}"
TEMP_DB_STARTED=0
PG_BIN=""

log() {
    printf '[ci-schema-health] %s\n' "$*"
}

err() {
    printf '[ci-schema-health][ERROR] %s\n' "$*" >&2
}

cleanup() {
    if [ "$TEMP_DB_STARTED" = "1" ] && [ -n "$PG_BIN" ]; then
        log "停止临时 PostgreSQL"
        "$PG_BIN/pg_ctl" -D "$PGDATA" stop -m fast >/dev/null 2>&1 || true
    fi
}

trap cleanup EXIT

# 已存在 DATABASE_URL：仅跑健康检查
if [ -n "${DATABASE_URL:-}" ]; then
    log "使用现有 DATABASE_URL 跑 schema 健康检查"
    log "DATABASE_URL=$DATABASE_URL"
    if [ "${SKIP_BINARY_BUILD:-0}" = "1" ]; then
        log "SKIP_BINARY_BUILD=1，跳过 cargo build"
    else
        log "构建 schema_health_check 二进制"
        cargo build --bin schema_health_check --locked
    fi
    exec cargo run --quiet --bin schema_health_check --locked
fi

# SKIP_DB_SETUP：要求外部预置 DATABASE_URL
if [ "${SKIP_DB_SETUP:-0}" = "1" ]; then
    err "SKIP_DB_SETUP=1 但未提供 DATABASE_URL"
    exit 2
fi

# 自建临时 PostgreSQL
log "未提供 DATABASE_URL，将启动临时 PostgreSQL"
PGDATA="$(mktemp -d -t synapse_pgdata.XXXXXX)"
log "PGDATA=$PGDATA"

PG_BIN="$(command -v pg_ctl || true)"
if [ -z "$PG_BIN" ]; then
    PG_BIN_DIR="$(brew --prefix postgresql@16 2>/dev/null)/bin || true"
    if [ -d "$PG_BIN_DIR" ]; then
        export PATH="$PG_BIN_DIR:$PATH"
        PG_BIN="$PG_BIN_DIR/pg_ctl"
    fi
fi
if [ -z "$PG_BIN" ] || [ ! -x "$PG_BIN" ]; then
    err "找不到 pg_ctl，请安装 postgresql@16 或设置 DATABASE_URL"
    exit 2
fi

INIT_BIN="$(dirname "$PG_BIN")/initdb"
PG_ISREADY_BIN="$(dirname "$PG_BIN")/pg_isready"
PSQL_BIN="$(dirname "$PG_BIN")/psql"

log "初始化 PostgreSQL 数据目录"
"$INIT_BIN" -D "$PGDATA" --auth=trust --username="$POSTGRES_USER" >/dev/null

# 配置 listen & 数据目录
cat >>"$PGDATA/postgresql.conf" <<EOF
port = $POSTGRES_PORT
listen_addresses = '127.0.0.1'
unix_socket_directories = '$PGDATA'
EOF

log "启动 PostgreSQL"
"$PG_BIN" -D "$PGDATA" -l "$PGDATA/logfile" start
TEMP_DB_STARTED=1

# 等待就绪
for i in 1 2 3 4 5 6 7 8 9 10; do
    if "$PG_ISREADY_BIN" -h "$POSTGRES_HOST" -p "$POSTGRES_PORT" >/dev/null 2>&1; then
        log "PostgreSQL 就绪"
        break
    fi
    sleep 1
done

log "创建测试数据库: $POSTGRES_DB"
"$PSQL_BIN" -h "$POSTGRES_HOST" -p "$POSTGRES_PORT" -U "$POSTGRES_USER" -d postgres \
    -c "CREATE DATABASE $POSTGRES_DB;" >/dev/null 2>&1 || true

export DATABASE_URL="postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@${POSTGRES_HOST}:${POSTGRES_PORT}/${POSTGRES_DB}"
log "DATABASE_URL=$DATABASE_URL"

# 跑 v8 基线
log "应用 v8 基线迁移"
V8_FILE="$ROOT_DIR/migrations/00000000_unified_schema_v8.sql"
EXT_FILE="$ROOT_DIR/migrations/00000001_extensions_v8.sql"
if [ ! -f "$V8_FILE" ]; then
    err "找不到 v8 基线: $V8_FILE"
    exit 2
fi
"$PSQL_BIN" "$DATABASE_URL" -v ON_ERROR_STOP=1 -f "$V8_FILE" >/dev/null
if [ -f "$EXT_FILE" ]; then
    "$PSQL_BIN" "$DATABASE_URL" -v ON_ERROR_STOP=1 -f "$EXT_FILE" >/dev/null
fi

# 构建并跑健康检查
log "构建 schema_health_check 二进制"
cargo build --bin schema_health_check --locked

log "运行 schema_health_check"
cargo run --quiet --bin schema_health_check --locked
