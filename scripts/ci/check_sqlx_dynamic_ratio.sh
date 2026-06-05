#!/usr/bin/env bash
#
# CI gate: track the ratio of dynamic to compile-time-checked SQL
# queries (M-3 in `docs/synapse-rust/M3_SQLX_MIGRATION_PLAN.md`).
#
# The 2026-06-03 comprehensive audit reported that 99.6% of all
# `sqlx::query` / `sqlx::query_as` call sites in the project are
# dynamic, which means schema drift is only caught at runtime. The
# long-term target is to flip the ratio so that ≥ 70% of call sites
# use the compile-time-checked `query!` / `query_as!` macros.
#
# This script computes the current ratio, prints it for visibility,
# and fails the build if the ratio exceeds 0.30 (i.e. dynamic > 30%).
# The threshold can be tightened per batch via the
# `SQLX_DYNAMIC_RATIO_MAX` environment variable.
#
# Usage:
#   bash scripts/ci/check_sqlx_dynamic_ratio.sh
#   SQLX_DYNAMIC_RATIO_MAX=0.20 bash scripts/ci/check_sqlx_dynamic_ratio.sh
#
# Exits 0 on success, 1 on threshold violation.

set -eu

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
MAX_RATIO="${SQLX_DYNAMIC_RATIO_MAX:-0.30}"

cd "${ROOT_DIR}"

# Count dynamic queries: sqlx::query( ... ) and sqlx::query_as::<...>( ...)
# We deliberately exclude macro invocations (sqlx::query!, sqlx::query_as!).
# The grep below is intentionally conservative: it under-counts macros
# that have been split across lines, but matches the structure used in
# the codebase today (compact, one-line call).
dynamic=$(grep -rE --include='*.rs' \
    -e 'sqlx::query\(' \
    -e 'sqlx::query_as\(' \
    -e 'sqlx::query_scalar\(' \
    src/ 2>/dev/null | wc -l | tr -d ' ')

# Count static queries: query!, query_as!, query_scalar!, query_file!
static=$(grep -rE --include='*.rs' \
    -e 'sqlx::query!' \
    -e 'sqlx::query_as!' \
    -e 'sqlx::query_scalar!' \
    -e 'sqlx::query_file!' \
    src/ 2>/dev/null | wc -l | tr -d ' ')

total=$((dynamic + static))
if [[ "${total}" -eq 0 ]]; then
    echo "check_sqlx_dynamic_ratio: 没有发现任何 sqlx 查询"
    exit 0
fi

ratio=$(awk -v d="${dynamic}" -v t="${total}" 'BEGIN { printf "%.4f", d / t }')

echo "check_sqlx_dynamic_ratio: dynamic=${dynamic} static=${static} total=${total} ratio=${ratio} (max=${MAX_RATIO})"

# Threshold check
if awk -v r="${ratio}" -v m="${MAX_RATIO}" 'BEGIN { exit !(r > m) }'; then
    echo "check_sqlx_dynamic_ratio: FAIL (dynamic ratio ${ratio} 超过阈值 ${MAX_RATIO})" >&2
    echo "请参考 docs/synapse-rust/M3_SQLX_MIGRATION_PLAN.md 把动态查询迁移为 query! 宏" >&2
    exit 1
fi

echo "check_sqlx_dynamic_ratio: OK"
exit 0
