#!/usr/bin/env bash
#
# CI gate: enforce `.sqlx/` compile-time query cache consistency.
#
# Migrations to `sqlx::query!` macros (see M-3 in
# `docs/synapse-rust/M3_SQLX_MIGRATION_PLAN.md`) require a checked-in
# `.sqlx/` cache so that `cargo check` (and any other rust-analyzer /
# cargo invocation) can resolve query metadata without a live
# database. The cache must:
#
#   1. Exist (non-empty)
#   2. Pass `cargo check --all-features --locked` in offline mode
#   3. Be in sync with the live schema (via `cargo sqlx prepare --check`)
#
# This script is intentionally cheap: it never opens a DB. The live
# DB check is performed in `drift-detection.yml` after migrations run.
#
# Usage:
#   bash scripts/ci/check_sqlx_offline_cache.sh
#
# Exits 0 on success, 1 on any failure.

set -eu

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SQLX_DIR="${ROOT_DIR}/.sqlx"

cd "${ROOT_DIR}"

# 1) .sqlx/ must exist
if [[ ! -d "${SQLX_DIR}" ]]; then
    echo "check_sqlx_offline_cache: FAIL (.sqlx/ 缓存目录不存在)" >&2
    echo "运行 \`DATABASE_URL=... cargo sqlx prepare --workspace -- --all-features\` 生成。" >&2
    exit 1
fi

# 2) 必须有 query-*.json 缓存文件
query_count=$(find "${SQLX_DIR}" -name 'query-*.json' 2>/dev/null | wc -l | tr -d ' ')
if [[ "${query_count}" -eq 0 ]]; then
    echo "check_sqlx_offline_cache: FAIL (.sqlx/ 没有任何 query 缓存)" >&2
    exit 1
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
