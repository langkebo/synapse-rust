#!/usr/bin/env bash
#
# B2 棘轮：missing_docs 增量门禁。
#
# 仓库现状（2026-09-20）：7 个 library crate 用 crate 级 `#![deny(missing_docs)]`
# 自我把关，`synapse-web` / `synapse-test-utils` 的 lib 与根包的 6 个二进制入口
# 没有 crate 级属性 —— 二进制入口曾贡献全部 6 条存量 debt，补齐 `//!` 后
# workspace debt 已归零（baseline=0）。
#
# 棘轮策略：
#   - 存量归零：任何一条新 debt 都会立刻让 CI 变红
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
    # 这里曾经回退到一个**开发者本机的绝对路径**
    # （/Users/ljf/.workbuddy/binaries/python/versions/3.13.12/bin/python3）。
    # 那是个无法在 CI 上成立的字面量：一旦 python3 缺失，脚本会去执行别的机器
    # 上的解释器（或者直接 command not found 而报出与真实原因无关的错误）。
    # 缺解释器是环境问题，必须显式报出来。
    echo "::error::python3 not found on PATH; set PYTHON=<interpreter> to override" >&2
    exit 2
fi

if [[ ! -f "scripts/check_missing_docs_ratchet.py" ]]; then
    echo "::error::scripts/check_missing_docs_ratchet.py not found" >&2
    exit 1
fi

exec "$PY" scripts/check_missing_docs_ratchet.py "$@"
