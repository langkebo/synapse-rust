#!/usr/bin/env bash
# 调优测试库 PostgreSQL，支撑集成测试高并发（方案 C：提高并发）。
#
# 背景：集成测试通过 prepare_shared_test_pool 克隆隔离 schema，其 clone DO 块
# 需在单个事务内 CREATE TABLE LIKE 111 张表 + 重建全部索引 + 序列 + 视图，
# 峰值占用 ~1000 把锁。默认 max_locks_per_transaction=64（× max_connections=100
# = 6400 把锁）时，6+ 并发克隆即触发 "out of shared memory"（SQLSTATE 53200）。
# 提高到 256（25600 把锁）后 6~8 并发稳定（storage 516 测试 6 并发全绿）。
#
# 用法（幂等，可重复执行）：
#   bash scripts/tune_test_db.sh
#   TEST_DB_CONTAINER=synapse-test-db bash scripts/tune_test_db.sh
#
# 无 docker（裸机 Postgres）时设 TEST_DB_CONTAINER="" 跳过自动重启，手动
# `pg_ctl restart` 或 `SELECT pg_reload_conf()` 前需重启 postmaster。

set -uo pipefail

export PGHOST="${PGHOST:-localhost}"
export PGPORT="${PGPORT:-5432}"
export PGUSER="${PGUSER:-synapse}"
export PGDATABASE="${PGDATABASE:-synapse_test}"
export PGPASSWORD="${PGPASSWORD:-synapse}"

TARGET_LOCKS="${TEST_DB_MAX_LOCKS_PER_TXN:-256}"
CONTAINER="${TEST_DB_CONTAINER-synapse-test-db}"

PSQL=(psql -X -q -v ON_ERROR_STOP=1)

echo "==> 设置 max_locks_per_transaction / max_pred_locks_per_transaction = ${TARGET_LOCKS}"
"${PSQL[@]}" -c "ALTER SYSTEM SET max_locks_per_transaction = ${TARGET_LOCKS};" >/dev/null
"${PSQL[@]}" -c "ALTER SYSTEM SET max_pred_locks_per_transaction = ${TARGET_LOCKS};" >/dev/null
echo "    ALTER SYSTEM 已写入（postgresql.auto.conf）"

CURRENT=$("${PSQL[@]}" -tAc "SHOW max_locks_per_transaction;")
if [ "$CURRENT" = "$TARGET_LOCKS" ]; then
    echo "==> 已生效（当前 max_locks_per_transaction=${CURRENT}），无需重启。"
    exit 0
fi

echo "==> 当前生效值 ${CURRENT}（需重启 postmaster 才生效，目标 ${TARGET_LOCKS}）"
if [ -n "$CONTAINER" ] && command -v docker >/dev/null 2>&1 && docker ps --format '{{.Names}}' | grep -qx "$CONTAINER"; then
    echo "==> 重启容器 $CONTAINER ..."
    docker restart "$CONTAINER" >/dev/null
    # 等待 postmaster 就绪
    for _ in $(seq 1 20); do
        sleep 1
        if docker exec "$CONTAINER" pg_isready -U "$PGUSER" -d "$PGDATABASE" >/dev/null 2>&1; then
            break
        fi
    done
    echo "==> 重启后 max_locks_per_transaction=$("${PSQL[@]}" -tAc "SHOW max_locks_per_transaction;")"
else
    echo "==> 未检测到容器 ${CONTAINER}（或已设 TEST_DB_CONTAINER=\"\"），请手动重启 Postgres："
    echo "    docker restart $CONTAINER   # 或  pg_ctl restart"
fi
