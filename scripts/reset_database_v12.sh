#!/usr/bin/env bash
# 测试/开发库重置脚本（重部署窗口专用）。
#
# 只做「清空 public schema」这一件事，然后**把迁移应用委托给唯一入口**
# `docker/db_migrate.sh migrate`。此前它自带一个 `find migrations/*.sql` 的应用
# 循环 —— 那是第二份迁移实现，并含两条指向已删除文件的死分支
#（`00000000_unified_schema_v10.sql`、`00000001_extensions_v10.sql`）。原文件名
# `init_v11_database.sh` 也已名不副实（v11 基线早已被 v12 取代），故改名。
#
# 用法：
#   bash scripts/reset_database_v12.sh
#   bash scripts/reset_database_v12.sh --keep-existing
#   TEST_DB_PORT=5432 bash scripts/reset_database_v12.sh

set -euo pipefail

cd "$(dirname "$0")/.."

# === 参数解析 ===
KEEP_EXISTING=0
if [[ "${1:-}" == "--keep-existing" ]]; then
    KEEP_EXISTING=1
fi

# === 环境变量 ===
DB_HOST="${TEST_DB_HOST:-localhost}"
DB_PORT="${TEST_DB_PORT:-5432}"
DB_USER="${TEST_DB_USER:-synapse}"
DB_PASSWORD="${TEST_DB_PASSWORD:-synapse}"
DB_NAME="${TEST_DB_NAME:-synapse_test}"

export PGPASSWORD="$DB_PASSWORD"
PSQL=(psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME")

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
MIGRATIONS_DIR="$PROJECT_ROOT/migrations"

echo "==> [$DB_NAME@$DB_HOST:$DB_PORT] v12 重置脚本"
echo "    keep_existing=$KEEP_EXISTING"

# === 前置检查 ===
if ! "${PSQL[@]}" -c "SELECT 1" >/dev/null 2>&1; then
    echo "ERROR: 无法连接数据库 ${DB_NAME}，请确保 PostgreSQL 运行中"
    echo "  命令: PGPASSWORD=$DB_PASSWORD psql -h $DB_HOST -p $DB_PORT -U $DB_USER -d $DB_NAME"
    exit 1
fi

# === 创建数据库（如不存在）===
if ! "${PSQL[@]}" -c "SELECT 1" >/dev/null 2>&1; then
    echo "==> 创建数据库: $DB_NAME"
    PGPASSWORD="$DB_PASSWORD" psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d postgres \
        -c "CREATE DATABASE \"$DB_NAME\";" >/dev/null 2>&1 || true
fi

# === 查找 v11 baseline ===
CURRENT_BASELINE="$MIGRATIONS_DIR/00000000_unified_schema_v12.sql"
if [[ ! -f "$CURRENT_BASELINE" ]]; then
    echo "ERROR: v12 baseline 不存在: $CURRENT_BASELINE"
    echo "  请先运行 ticket 01-v11-baseline-scaffold"
    exit 1
fi

# === 重置数据库 ===
if [[ "$KEEP_EXISTING" == "1" ]]; then
    echo "==> 跳过 DROP（--keep-existing 模式）"
else
    echo "==> 重置 public schema（DROP CASCADE）"
    "${PSQL[@]}" -c "DROP SCHEMA public CASCADE; CREATE SCHEMA public; GRANT ALL ON SCHEMA public TO $DB_USER; GRANT ALL ON SCHEMA public TO public;" >/dev/null
fi

# === 应用迁移（唯一入口：docker/db_migrate.sh） ===
# H-14 护栏要求调用方显式给出目标，这里用 DATABASE_URL 显式传递。
echo "==> 应用迁移（docker/db_migrate.sh migrate）"
DATABASE_URL="postgresql://${DB_USER}:${DB_PASSWORD}@${DB_HOST}:${DB_PORT}/${DB_NAME}" \
    bash "$PROJECT_ROOT/docker/db_migrate.sh" migrate

# === 验证 ===
echo "==> 验证 schema"
TABLE_COUNT=""
INDEX_COUNT=""
TRIGGER_COUNT=""
# 用 || true 确保 psql 失败时变量不为空
TABLE_COUNT="$("${PSQL[@]}" -tAc "SELECT count(*) FROM information_schema.tables WHERE table_schema='public' AND table_type='BASE TABLE';" 2>/dev/null | grep -E '^[0-9]+$' | head -1)" || true
echo "    public 表数: ${TABLE_COUNT:-未读取}"

if [[ -z "$TABLE_COUNT" ]] || [[ "$TABLE_COUNT" -eq 0 ]]; then
    echo "ERROR: 表数为 0 或读取失败，schema 应用可能失败"
    exit 1
fi

INDEX_COUNT="$("${PSQL[@]}" -tAc "SELECT count(*) FROM pg_indexes WHERE schemaname='public';" 2>/dev/null | grep -E '^[0-9]+$' | head -1)" || true
echo "    public 索引数: ${INDEX_COUNT:-未读取}"

TRIGGER_COUNT="$("${PSQL[@]}" -tAc "SELECT count(*) FROM pg_trigger WHERE NOT tgisinternal;" 2>/dev/null | grep -E '^[0-9]+$' | head -1)" || true
echo "    触发器数（非内部）: ${TRIGGER_COUNT:-未读取}"

# === 删除脚手架测试表（如存在）===
if [[ "$KEEP_EXISTING" != "1" ]]; then
    "${PSQL[@]}" -c "DROP TABLE IF EXISTS _v12_scaffold_check;" >/dev/null 2>&1 || true
    echo "    (已清理 _v12_scaffold_check 标记表)"
fi

echo ""
echo "✅ v12 重置完成！表数: ${TABLE_COUNT}，索引数: ${INDEX_COUNT}"
