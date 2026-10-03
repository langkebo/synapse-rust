#!/usr/bin/env bash
# 收尾清理：等「独立 worktree 验证」跑完后，把两个纯 scratch 测试库
# （synapse_router_test / synapse_verify_test）也按 DROP DATABASE + 重新播种的方式
# 复位 —— 夹具每个用例建一个隔离 schema 且从不 DROP，跑一轮 1530 条就会攒下 ~1500 个，
# catalog 膨胀会把后面的用例从 ~2s 拖到 40-55s（实测）。
#
# 用法：bash scripts/local/reset_scratch_test_dbs.sh
set -uo pipefail

export PATH="/opt/homebrew/opt/postgresql@15/bin:$PATH"
# 机器相关部分只在这里：库主机/端口/用户走 PG_BASE_URL（默认本机 homebrew postgres）。
PG_BASE_URL="${PG_BASE_URL:-postgresql://synapse@127.0.0.1:5432}"
ROOT=/Users/ljf/Desktop/hu_ts/synapse-rust
SUMMARY=/tmp/scratch_db_reset_summary.txt
ROOT_URL="${PG_BASE_URL}"
cd "$ROOT"
log() { printf '[%s] %s\n' "$(date '+%m-%d %H:%M:%S')" "$*" | tee -a "$SUMMARY"; }

# 等「独立 worktree 验证」收工。哨兵有两个：脚本走完会写 `VERIFY DONE`；若它在最后一步
# 因故提前退出（例如 `set -u` 撞上全角括号那类毛病），就退化为"进程不在即视为结束"，
# 否则本脚本会永远等一个不会出现的哨兵。
log "waiting for the worktree verification to finish ..."
for _ in $(seq 1 20); do
    grep -q 'VERIFY DONE' /tmp/verify_my_commit_summary.txt 2>/dev/null && break
    pgrep -f 'verify_my_commit.sh$' >/dev/null || break
    sleep 30
done

for db in synapse_router_test synapse_verify_test; do
    before=$(psql "$ROOT_URL/$db" -tAc "select count(*) from information_schema.schemata" 2>/dev/null | tr -d ' ')
    psql "$ROOT_URL/postgres" -v ON_ERROR_STOP=1 -c "DROP DATABASE $db WITH (FORCE)" >>"$SUMMARY" 2>&1
    psql "$ROOT_URL/postgres" -v ON_ERROR_STOP=1 -c "CREATE DATABASE $db" >>"$SUMMARY" 2>&1
    TEST_DATABASE_URL="$ROOT_URL/$db" TEST_DB_TEMPLATE_SCHEMA=test_template_ci \
        bash scripts/ci/prepare_test_db.sh >>"$SUMMARY" 2>&1
    after=$(psql "$ROOT_URL/$db" -tAc "select count(*) from information_schema.schemata" 2>/dev/null | tr -d ' ')
    log "$db: schemas ${before:-?} -> ${after:-?}"
done

log "SCRATCH DB RESET DONE"
