#!/usr/bin/env bash
# 初始化测试库的 public schema（db_tests 直连 public，需已迁移的表）。
#
# storage 的 51 个 db_tests 文件用 test_pool() 直连 TEST_DATABASE_URL 的 public schema
# （不设 search_path、不走隔离 schema），因此跑覆盖率前必须先把 public 迁移到最新。
# 用 psql 按顺序跑 migrations/*.sql（跳过 .undo.sql）—— sqlx-cli 的 `sqlx migrate run`
# 会把 .undo.sql 误当正向迁移跑，Rust 侧 run_runtime_migrations 才正确跳过 .undo.sql。
#
# 用法：
#   bash scripts/init_test_public_schema.sh
#   TEST_DB_PORT=5433 TEST_DB_NAME=synapse_test bash scripts/init_test_public_schema.sh

set -euo pipefail

cd "$(dirname "$0")/.."

DB_HOST="${TEST_DB_HOST:-localhost}"
# 5432 = CI 导出的端口、dev compose override 发布的端口
# （`${DB_EXPOSE_PORT:-5432}:5432`）、以及本地 Homebrew PostgreSQL。
# 全套 harness 必须一致，见 tests/unit/test_db_url_convention_tests.rs。
DB_PORT="${TEST_DB_PORT:-5432}"
DB_USER="${TEST_DB_USER:-synapse}"
DB_PASSWORD="${TEST_DB_PASSWORD:-synapse}"
DB_NAME="${TEST_DB_NAME:-synapse_test}"

export PGPASSWORD="$DB_PASSWORD"
PSQL=(psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME")

echo "==> 重置 public schema"
"${PSQL[@]}" -c "DROP SCHEMA public CASCADE; CREATE SCHEMA public; GRANT ALL ON SCHEMA public TO $DB_USER; GRANT ALL ON SCHEMA public TO public;" >/dev/null

echo "==> 按顺序跑正向迁移（跳过 .undo.sql）"
for f in $(ls migrations/*.sql | grep -v '\.undo\.sql' | sort); do
    "${PSQL[@]}" -v ON_ERROR_STOP=0 -q -f "$f" >/dev/null 2>&1 || echo "  (非致命) $f"
done

echo "==> 验证"
"${PSQL[@]}" -tAc "SELECT 'public 表数: ' || count(*) FROM information_schema.tables WHERE table_schema='public';"
