#!/usr/bin/env bash
# 初始化测试库里的一个 schema（public 给 db_tests 直连；模板给共享池 clone）。
#
# 用 psql 按顺序跑 migrations/*.sql（跳过 .undo.sql），原因有二：
#   1. sqlx-cli 会把 `.undo.sql` 误当正向迁移跑；Rust 侧的 run_runtime_migrations
#      才正确跳过它们。
#   2. **sqlx-cli 0.8.x 不认迁移文件首行的 `-- no-transaction` 指令**（2026-09-19
#      用最小探针复现：只含该指令 + 一条 `CREATE INDEX CONCURRENTLY` 的迁移仍然
#      报 "cannot run inside a transaction block"）。baseline 里有 14 处
#      `CREATE INDEX CONCURRENTLY`，所以 `sqlx migrate run` 在当前基线下必然失败。
#      psql 默认 autocommit（不加 `-1`），不存在这个问题。
#
# 用法：
#   bash scripts/init_test_public_schema.sh                       # 重置 public
#   TARGET_SCHEMA=test_template_ci bash scripts/init_test_public_schema.sh
#   TEST_DB_PORT=5433 TEST_DB_NAME=synapse_test bash scripts/init_test_public_schema.sh
#   TEST_DATABASE_URL=postgresql://… bash scripts/init_test_public_schema.sh
#
# `TARGET_SCHEMA=public`（默认）在 `RESET_PUBLIC=1`（默认）时 **DROP + 重建** public
# （db_tests 需要干净基线）；`RESET_PUBLIC=0` 只做幂等 apply。CI 的 seed 走 0：`DROP
# SCHEMA public CASCADE` 会**级联删掉其它 schema 里依赖 public 扩展的对象**（例如
# 模板 schema 上的 `gin_trgm_ops` 索引），2026-09-19 实测过一次，会把"就绪"标记的
# 模板悄悄变成缺索引的半成品。其它目标只 `CREATE SCHEMA IF NOT EXISTS`，并通过
# `PGOPTIONS` 把非限定 DDL 钉进该 schema —— libpq 读 `PGOPTIONS`，而 sqlx 的 rust
# 驱动不读，这正是旧 `prepare_test_db.sh` 里 `?options=-c search_path=…` URL hack 的由来。
#
# 结束时**断言**目标 schema 的表数 ≥ `MIN_TABLES`（默认 100）：旧版只 echo 一个数字，
# 迁移整段没落地也照样 exit 0（`ON_ERROR_STOP=0` + 忽略返回值）。

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
TARGET_SCHEMA="${TARGET_SCHEMA:-public}"
RESET_PUBLIC="${RESET_PUBLIC:-1}"
MIN_TABLES="${MIN_TABLES:-100}"

export PGPASSWORD="$DB_PASSWORD"
# A full URL wins when provided (that is how `scripts/ci/prepare_test_db.sh` and
# `run_local_coverage.sh` pass the target); otherwise assemble from TEST_DB_*.
if [[ -n "${TEST_DATABASE_URL:-}" ]]; then
    PSQL=(psql "$TEST_DATABASE_URL")
else
    PSQL=(psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME")
fi

if [[ "$TARGET_SCHEMA" == "public" ]]; then
    if [[ "$RESET_PUBLIC" == "1" ]]; then
        echo "==> 重置 public schema"
        "${PSQL[@]}" -v ON_ERROR_STOP=1 -c "DROP SCHEMA public CASCADE; CREATE SCHEMA public; GRANT ALL ON SCHEMA public TO $DB_USER; GRANT ALL ON SCHEMA public TO public;" >/dev/null
    else
        echo "==> 幂等 apply 到 public（RESET_PUBLIC=0；不 DROP，避免级联删掉其它 schema 的扩展依赖对象）"
        "${PSQL[@]}" -v ON_ERROR_STOP=1 -c "CREATE SCHEMA IF NOT EXISTS public; GRANT ALL ON SCHEMA public TO $DB_USER; GRANT ALL ON SCHEMA public TO public;" >/dev/null
    fi
else
    echo "==> 准备目标 schema \"$TARGET_SCHEMA\"（不动 public）"
    "${PSQL[@]}" -v ON_ERROR_STOP=1 -c "CREATE SCHEMA IF NOT EXISTS \"$TARGET_SCHEMA\"; GRANT ALL ON SCHEMA \"$TARGET_SCHEMA\" TO $DB_USER;" >/dev/null
fi

echo "==> 按顺序跑正向迁移（跳过 .undo.sql；非限定 DDL 落在 \"$TARGET_SCHEMA\"）"
for f in $(ls migrations/*.sql | grep -v '\.undo\.sql' | sort); do
    PGOPTIONS="-c search_path=$TARGET_SCHEMA,public" "${PSQL[@]}" -v ON_ERROR_STOP=1 -q -f "$f" >/dev/null
    echo "    ok: $f"
done

echo "==> 验证 \"$TARGET_SCHEMA\""
count="$("${PSQL[@]}" -tAc "SELECT count(*) FROM information_schema.tables WHERE table_schema='$TARGET_SCHEMA'")"
echo "    $TARGET_SCHEMA 表数: $count"
if (( count < MIN_TABLES )); then
    echo "::error::$TARGET_SCHEMA 只有 $count 张表（预期 ≥$MIN_TABLES）—— 迁移没有真正落进该 schema" >&2
    exit 1
fi
