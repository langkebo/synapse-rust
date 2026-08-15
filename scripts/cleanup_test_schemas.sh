#!/usr/bin/env bash
# 清理测试库累积的 test_* 隔离 schema（保留 test_template_* 模板）。
#
# 背景：集成测试通过 prepare_shared_test_pool 每次 clone 一个 111 张表的隔离
# schema，用完不 DROP，累积到数千个（catalog 膨胀拖慢 clone 与查询）。清理时
# 单个 DROP SCHEMA ... CASCADE 已接近 max_locks_per_transaction=64 上限，因此
# 必须每个 schema 单独一个事务（不能 DO $$ 循环或单事务批量），否则报
# "out of shared memory"。
#
# 用法：
#   bash scripts/cleanup_test_schemas.sh
#   PGHOST=localhost PGPORT=15432 bash scripts/cleanup_test_schemas.sh

set -uo pipefail

# 连接参数（可用环境变量覆盖）
export PGHOST="${PGHOST:-localhost}"
export PGPORT="${PGPORT:-15432}"
export PGUSER="${PGUSER:-synapse}"
export PGDATABASE="${PGDATABASE:-synapse_test}"
export PGPASSWORD="${PGPASSWORD:-synapse}"

PSQL=(psql -X -q -v ON_ERROR_STOP=0)

echo "==> 查询待清理的 test schema（排除模板）..."
SCHEMAS=$("${PSQL[@]}" -tAc "SELECT schema_name FROM information_schema.schemata WHERE schema_name LIKE 'test_%' AND schema_name NOT LIKE 'test_template%' ORDER BY schema_name;" 2>/dev/null)

TOTAL=$(printf '%s\n' "$SCHEMAS" | grep -c . || true)
echo "    待清理: $TOTAL 个 schema"

if [ "$TOTAL" -eq 0 ]; then
    echo "==> 无残留 schema，无需清理。"
    exit 0
fi

COUNT=0
FAILED=0
while IFS= read -r s; do
    [ -z "$s" ] && continue
    if ! "${PSQL[@]}" -c "DROP SCHEMA \"$s\" CASCADE" >/dev/null 2>&1; then
        echo "    WARN: 清理失败 $s"
        FAILED=$((FAILED + 1))
    fi
    COUNT=$((COUNT + 1))
    if [ $((COUNT % 200)) -eq 0 ]; then
        echo "    进度: $COUNT / $TOTAL"
    fi
done <<< "$SCHEMAS"

echo "==> 清理完成：$COUNT 个（失败 $FAILED 个）"
REMAIN=$("${PSQL[@]}" -tAc "SELECT count(*) FROM information_schema.schemata WHERE schema_name LIKE 'test_%' AND schema_name NOT LIKE 'test_template%';" 2>/dev/null)
echo "    剩余 test schema: $REMAIN"
