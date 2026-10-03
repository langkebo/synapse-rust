#!/usr/bin/env bash
# 在**独立 worktree**里验证我自己的提交（不受并发写者影响）。
#
# 背景：主工作树里另一个写者正在改 22 个文件（含删 `synapse-services/src/room/api_trait.rs`），
# 而且已在我的提交之上提交了 `dd03508a9`。主树里跑 storage 全量 lib 会构建那棵脏树，
# 结果无法归因到我的提交。因此这里 `git worktree add --detach` 到我的提交，独立 target/、
# 独立数据库，跑：
#   1) storage 全量 lib（1851 条，此前从未跑过）
#   2) 阶段 1 里被标记 TIMEOUT / LEAK 的两条用例隔离重跑（判定是否环境争用）
#   3) 旧库 synapse_merge_test 的残留 schema 清理
#
# 用法：bash .worktrees/verify_b08987df5.sh   （脚本自身放在主树 scripts/local/ 下）
set -uo pipefail

export PATH="/opt/homebrew/opt/postgresql@15/bin:$PATH"
ROOT=/Users/ljf/Desktop/hu_ts/synapse-rust
COMMIT=b08987df5
WT="$ROOT/.worktrees/verify-$COMMIT"
SUMMARY=/tmp/verify_my_commit_summary.txt
TIP_LOG=/tmp/full_integration_tip.log
STORAGE_LOG=/tmp/storage_lib_verify.log
ISOLATED_LOG=/tmp/flagged_tests_verify.log
CLEANUP_LOG=/tmp/schema_cleanup.log
VERIFY_DB="postgresql://synapse@127.0.0.1:5432/synapse_verify_test"
: >"$SUMMARY"

log() { printf '[%s] %s\n' "$(date '+%m-%d %H:%M:%S')" "$*" | tee -a "$SUMMARY"; }
sum_of() { grep -E '^ *Summary' "$1" 2>/dev/null | tail -1; }

# ── 1) 先等在跑的 tip 套件结束（避免重复制造阶段 1 那次争用）───────────────
log "step 1/4: waiting for the in-flight tip integration suite to finish ..."
for _ in $(seq 1 240); do
    if grep -qE '^ *Summary' "$TIP_LOG" 2>/dev/null &&
        ! pgrep -f 'cargo-nextest nextest run --profile ci --all-features --test integration' >/dev/null; then
        break
    fi
    sleep 30
done
log "step 1 done: $(sum_of "$TIP_LOG")"
grep -E '^ *(FAIL|TIMEOUT|LEAK) ' "$TIP_LOG" | head -20 >>"$SUMMARY" 2>/dev/null || true

# ── 2) 隔离 worktree（钉在我的提交上）───────────────────────────────────────
log "step 2/4: creating detached worktree at $COMMIT ..."
if [ ! -d "$WT" ]; then
    git -C "$ROOT" worktree add --detach "$WT" "$COMMIT" >>"$SUMMARY" 2>&1
fi
cd "$WT"

log "step 3/4: full synapse-storage lib on a fresh database ($VERIFY_DB) ..."
if ! psql "postgresql://synapse@127.0.0.1:5432/postgres" -tAc \
    "select 1 from pg_database where datname='synapse_verify_test'" | grep -q 1; then
    psql "postgresql://synapse@127.0.0.1:5432/postgres" -c "CREATE DATABASE synapse_verify_test" >>"$SUMMARY" 2>&1
fi
TEST_DATABASE_URL="$VERIFY_DB" TEST_DB_TEMPLATE_SCHEMA=test_template_ci \
    bash scripts/ci/prepare_test_db.sh >>"$SUMMARY" 2>&1
TEST_DATABASE_URL="$VERIFY_DB" TEST_DB_TEMPLATE_SCHEMA=test_template_ci SQLX_OFFLINE=true \
    cargo nextest run -p synapse-storage --lib --all-features --test-threads 4 --no-fail-fast \
    >"$STORAGE_LOG" 2>&1
echo "EXIT=$?" >>"$STORAGE_LOG"
log "step 3 done: $(sum_of "$STORAGE_LOG")"
grep -E '^ *(FAIL|TIMEOUT|LEAK) ' "$STORAGE_LOG" | head -40 >>"$SUMMARY" 2>/dev/null || true

# ── 3) 阶段 1 的两条可疑用例隔离重跑（1 线程，无并发）──────────────────────
log "step 4/4: re-running the TIMEOUT / LEAK cases in isolation ..."
TEST_DATABASE_URL="$VERIFY_DB" TEST_DB_TEMPLATE_SCHEMA=test_template_ci SQLX_OFFLINE=true \
    cargo nextest run --profile ci --all-features --test integration --test-threads 1 --no-fail-fast \
    -E 'test(api_placeholder_contract_p1p2_tests::test_get_state_event_empty_key_returns_raw_content_only) | test(admin_registration_service_tests_migrated::test_compute_hmac_with_user_type)' \
    >"$ISOLATED_LOG" 2>&1
echo "EXIT=$?" >>"$ISOLATED_LOG"
log "step 4 done: $(sum_of "$ISOLATED_LOG")"
grep -E '^ *(FAIL|TIMEOUT|LEAK) ' "$ISOLATED_LOG" | head -10 >>"$SUMMARY" 2>/dev/null || true

# ── 4) 旧库残留 schema 清理 ────────────────────────────────────────────────
before=$(psql "postgresql://synapse@127.0.0.1:5432/synapse_merge_test" -tAc \
    "select count(*) from information_schema.schemata" 2>/dev/null | tr -d ' ')
DATABASE_URL="postgresql://synapse@127.0.0.1:5432/synapse_merge_test" \
    bash scripts/cleanup_test_schemas.sh --apply >"$CLEANUP_LOG" 2>&1
after=$(psql "postgresql://synapse@127.0.0.1:5432/synapse_merge_test" -tAc \
    "select count(*) from information_schema.schemata" 2>/dev/null | tr -d ' ')
log "cleanup done: schemas ${before:-?} -> ${after:-?}（${CLEANUP_LOG}）"

log "VERIFY DONE"
