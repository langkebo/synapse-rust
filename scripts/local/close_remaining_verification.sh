#!/usr/bin/env bash
# 顺序收掉剩下的验证缺口（一次只跑一个 DB 重度阶段，避免互相拖成超时）。
#
#   1) 等 in-flight 的「夹具迁移」全量套件跑完
#   2) tip 的集成全量套件
#   3) storage 全量 lib（此前从未跑过的那批 DB 用例），在**新库**上跑
#   4) 清理旧测试库 synapse_merge_test 的残留 schema
#
# 用法：bash scripts/local/close_remaining_verification.sh
set -uo pipefail

export PATH="/opt/homebrew/opt/postgresql@15/bin:$PATH"
ROOT=/Users/ljf/Desktop/hu_ts/synapse-rust
SUMMARY=/tmp/remaining_verification_summary.txt
MIGRATED_LOG=/tmp/full_integration_migrated.log
TIP_LOG=/tmp/full_integration_tip.log
STORAGE_LOG=/tmp/storage_lib_full.log
CLEANUP_LOG=/tmp/schema_cleanup.log
: >"$SUMMARY"
cd "$ROOT"

log() { printf '[%s] %s\n' "$(date '+%m-%d %H:%M:%S')" "$*" | tee -a "$SUMMARY"; }
# nextest indents FAIL/TIMEOUT by 5 spaces; keep the pattern whitespace-tolerant so the
# counter can never silently stay 0 (that bug previously made a red run look all-green).
fails_of() { grep -cE '^[[:space:]]+FAIL\b|^[[:space:]]+TIMEOUT\b' "$1" 2>/dev/null || true; }
# nextest prints "Summary [ 1.2s] ..."; cargo test prints "test result: ok. ...". Accept both.
sum_of() { grep -E '^ *Summary|^test result:' "$1" 2>/dev/null | tail -1; }

log "stage 1/4: waiting for the in-flight harness-migration suite to finish ..."
for _ in $(seq 1 1200); do
    grep -q '^FULL_EXIT=' "$MIGRATED_LOG" 2>/dev/null && break
    sleep 30
done
log "stage 1 done: $(sum_of "$MIGRATED_LOG") ; FAIL/TIMEOUT=$(fails_of "$MIGRATED_LOG")"
grep -E '^[[:space:]]+FAIL\b|^[[:space:]]+TIMEOUT\b' "$MIGRATED_LOG" | head -50 >>"$SUMMARY" 2>/dev/null || true

export TEST_DB_TEMPLATE_SCHEMA=test_template_ci
export SQLX_OFFLINE=true

# Use `cargo test` (NOT nextest) for the local full-suite run: nextest forks one process per
# test, so synapse-test-utils' in-process SCHEMA_POOL can never be reused and every test pays a
# full ~255-table schema clone. `cargo test` runs the whole integration binary in one process, so
# the pool engages (first N clone, the rest TRUNCATE-and-reuse). Measured: ~0.26s/test vs 40-88s.
log "stage 2/4: tip integration suite ($(git rev-parse --short HEAD)) on synapse_router_test ..."
TEST_DATABASE_URL="postgresql://synapse@127.0.0.1:5432/synapse_router_test" \
    cargo test --all-features --test integration --no-fail-fast -- --test-threads 4 \
    >"$TIP_LOG" 2>&1
echo "EXIT=$?" >>"$TIP_LOG"
log "stage 2 done: $(sum_of "$TIP_LOG") ; FAIL/TIMEOUT=$(fails_of "$TIP_LOG")"
grep -E '^[[:space:]]+FAIL\b|^[[:space:]]+TIMEOUT\b' "$TIP_LOG" | head -50 >>"$SUMMARY" 2>/dev/null || true

log "stage 3/4: full synapse-storage lib on a fresh database ..."
if ! psql "postgresql://synapse@127.0.0.1:5432/postgres" -tAc \
    "select 1 from pg_database where datname='synapse_storage_test'" | grep -q 1; then
    psql "postgresql://synapse@127.0.0.1:5432/postgres" -c "CREATE DATABASE synapse_storage_test" >>"$SUMMARY" 2>&1
fi
TEST_DATABASE_URL="postgresql://synapse@127.0.0.1:5432/synapse_storage_test" \
    bash scripts/ci/prepare_test_db.sh >>"$SUMMARY" 2>&1
TEST_DATABASE_URL="postgresql://synapse@127.0.0.1:5432/synapse_storage_test" \
    cargo test -p synapse-storage --lib --all-features --no-fail-fast -- --test-threads 4 \
    >"$STORAGE_LOG" 2>&1
echo "EXIT=$?" >>"$STORAGE_LOG"
log "stage 3 done: $(sum_of "$STORAGE_LOG") ; FAIL/TIMEOUT=$(fails_of "$STORAGE_LOG")"
grep -E '^[[:space:]]+FAIL\b|^[[:space:]]+TIMEOUT\b' "$STORAGE_LOG" | head -50 >>"$SUMMARY" 2>/dev/null || true

log "stage 4/4: cleaning leftover schemas in synapse_merge_test ..."
before=$(psql "postgresql://synapse@127.0.0.1:5432/synapse_merge_test" -tAc \
    "select count(*) from information_schema.schemata" 2>/dev/null | tr -d ' ')
DATABASE_URL="postgresql://synapse@127.0.0.1:5432/synapse_merge_test" \
    bash scripts/cleanup_test_schemas.sh --apply >"$CLEANUP_LOG" 2>&1
after=$(psql "postgresql://synapse@127.0.0.1:5432/synapse_merge_test" -tAc \
    "select count(*) from information_schema.schemata" 2>/dev/null | tr -d ' ')
log "stage 4 done: schemas ${before:-?} -> ${after:-?}（日志 $CLEANUP_LOG）"

log "ALL STAGES DONE"
