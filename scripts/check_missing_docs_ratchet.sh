#!/usr/bin/env bash
#
# B2 棘轮：missing_docs 增量门禁。
#
# 仓库现状：7 个 crate 都开了 crate 级 `#![allow(missing_docs)]`，累积了
# 几百个 warning。如果直接打开 `-W missing_docs`，CI 会立刻红一长串——
# 长红 CI 等于无 CI，新引入的问题会淹没在既有噪声里。
#
# 棘轮策略：
#   - 存量不动（保持 crate 级 `allow`）
#   - 只卡**增量** pub 项：每个 PR 新增的 pub 必须有 `///` doc
#   - workspace 整体 debt 用 baseline 文件记录，减少时 CI 失败，
#     强制收紧 baseline（防止 ratchet 形同虚设）
#
# 用法：
#   ./scripts/check_missing_docs_ratchet.sh            # CI 检查模式
#   ./scripts/check_missing_docs_ratchet.sh --update  # 重算并写入 baseline
#
# 依赖：
#   - python3 (>=3.10)
#   - cargo clippy + SQLX_OFFLINE=true 可跑
#
set -uo pipefail

cd "$(dirname "$0")/.." || exit 1

PY="${PYTHON:-python3}"
if ! command -v "$PY" >/dev/null 2>&1; then
    PY="/Users/ljf/.workbuddy/binaries/python/versions/3.13.12/bin/python3"
fi

if [[ ! -f "scripts/check_missing_docs_ratchet.py" ]]; then
    echo "::error::scripts/check_missing_docs_ratchet.py not found" >&2
    exit 1
fi

exec "$PY" scripts/check_missing_docs_ratchet.py "$@"