#!/usr/bin/env bash
#
# Advisory gate: verify checked-in `.sqlx/` cache consistency when the repo
# actually maintains one.
#
# Historically M-3 required a checked-in `.sqlx/` cache so that
# `cargo check` (and rust-analyzer) could resolve query metadata
# without a live database. The current repository baseline no longer
# requires `.sqlx/` to be populated: primary validation runs against a
# DB-enabled schema gate, while `.sqlx/` is treated as an optional
# accelerator for offline builds. When `.sqlx/` exists and contains
# query metadata, this script still validates the offline path.
# Expected behavior:
#
#   1. If `.sqlx/` is absent or empty, report SKIP and exit 0
#   2. If `.sqlx/` is populated, pass `cargo check --all-features --locked`
#      in offline mode
#
# This script is intentionally cheap: it never opens a DB. Live-schema
# validation is handled by DB-enabled compile / migration gates.
#
# Usage:
#   bash scripts/ci/check_sqlx_offline_cache.sh
#
# Exits 0 on success or intentional skip, 1 on validation failure.

set -eu

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SQLX_DIR="${ROOT_DIR}/.sqlx"

cd "${ROOT_DIR}"

# 1) `.sqlx/` is optional in the current baseline.
if [[ ! -d "${SQLX_DIR}" ]]; then
    echo "check_sqlx_offline_cache: SKIP (.sqlx/ 缓存目录不存在；当前仓库使用 live-schema 校验基线)"
    exit 0
fi

# 2) 若目录为空，也视为当前策略下的可接受状态。
query_count=$(find "${SQLX_DIR}" -name 'query-*.json' 2>/dev/null | wc -l | tr -d ' ')
if [[ "${query_count}" -eq 0 ]]; then
    echo "check_sqlx_offline_cache: SKIP (.sqlx/ 目录为空；当前仓库未维护离线 query 缓存基线)"
    exit 0
fi
echo "check_sqlx_offline_cache: .sqlx/ 包含 ${query_count} 个 query 缓存"

# 3) SQLX_OFFLINE 模式下编译期校验
echo "check_sqlx_offline_cache: 运行 SQLX_OFFLINE=true cargo check ..."
if ! SQLX_OFFLINE=true cargo check --all-features --locked --quiet 2>/dev/null; then
    echo "check_sqlx_offline_cache: FAIL (SQLX_OFFLINE=true cargo check 失败)" >&2
    echo "可能原因：新增了 query! 宏但未运行 \`cargo sqlx prepare\` 更新 .sqlx/ 缓存。" >&2
    exit 1
fi

echo "check_sqlx_offline_cache: OK"
exit 0
