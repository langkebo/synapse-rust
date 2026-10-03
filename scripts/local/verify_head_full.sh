#!/usr/bin/env bash
# 分支级全量验证：主树当前 HEAD（干净工作树）的集成全量套件。
#
# 为什么是主树而不是我的 worktree：另一个写者已把在途的 22 个文件提交（`75864fdb5`），
# 主树现在是**干净**的 —— 于是"分支健康度"这个问题第一次有了稳定靶子。我的提交单独的
# storage 全量证据已有（worktree @ b08987df5：1851/1851）。
#
# 前置：scratch 库已复位（4 个 schema）；跑期间**不要**并发做任何 cargo / 数据库操作
# （阶段 1/2 的 TIMEOUT 就是那么来的）。
set -uo pipefail

export PATH="/opt/homebrew/opt/postgresql@15/bin:$PATH"
# 机器相关部分只在这里：库主机/端口/用户走 PG_BASE_URL（默认本机 homebrew postgres）。
PG_BASE_URL="${PG_BASE_URL:-postgresql://synapse@127.0.0.1:5432}"
ROOT=/Users/ljf/Desktop/hu_ts/synapse-rust
SUMMARY=/tmp/head_full_verification_summary.txt
CHECK_LOG=/tmp/head_check.log
SUITE_LOG=/tmp/head_full_integration.log
DB="${PG_BASE_URL}/synapse_router_test"
cd "$ROOT"
: >"$SUMMARY"
log() { printf '[%s] %s\n' "$(date '+%m-%d %H:%M:%S')" "$*" | tee -a "$SUMMARY"; }

COMMIT=$(git rev-parse --short HEAD)
DIRTY=$(git status --porcelain | wc -l | tr -d ' ')
log "target: HEAD=$COMMIT dirty=$DIRTY  (dirty>0 时结论只对该时刻的树成立)"
git log -2 --format='  %h %s' >>"$SUMMARY"

log "step 1/2: cargo check (fails fast if this HEAD does not compile) ..."
SQLX_OFFLINE=true TEST_DATABASE_URL="$DB" TEST_DB_TEMPLATE_SCHEMA=test_template_ci \
    cargo check --workspace --all-targets --all-features --features test-utils --locked \
    >"$CHECK_LOG" 2>&1
check_exit=$?
log "step 1 done: cargo check EXIT=$check_exit"
if [ "$check_exit" -ne 0 ]; then
    grep -E "^error" "$CHECK_LOG" | head -20 >>"$SUMMARY" 2>/dev/null || true
    log "HEAD DOES NOT COMPILE — stopping before the suite"
    log "HEAD VERIFY DONE"
    exit 1
fi

log "step 2/2: full integration suite (1530 tests, 2 threads) on synapse_router_test ..."
TEST_DATABASE_URL="$DB" TEST_DB_TEMPLATE_SCHEMA=test_template_ci SQLX_OFFLINE=true \
    cargo nextest run --profile ci --all-features --test integration --test-threads 2 --no-fail-fast \
    >"$SUITE_LOG" 2>&1
echo "EXIT=$?" >>"$SUITE_LOG"
log "step 2 done: $(grep -E '^ *Summary' "$SUITE_LOG" | tail -1)"
grep -E '^ *(FAIL|TIMEOUT|LEAK) ' "$SUITE_LOG" | sort -u | head -40 >>"$SUMMARY" 2>/dev/null || true
log "HEAD VERIFY DONE"
