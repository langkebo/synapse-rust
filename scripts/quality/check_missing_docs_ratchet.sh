#!/usr/bin/env bash
# scripts/quality/check_missing_docs_ratchet.sh
#
# B-3.1-a: 文档缺失 ratchet 度量脚本
#
# 思路：跑 `cargo doc --no-deps --workspace --all-features`，grep "warning: missing
# documentation" 的行数，与 `.workbuddy/memory/missing-docs-baseline.txt` 中存
# 的上次值比较。当前值 > baseline → 退出码 1（ratchet 触发，禁增）；当前值
# ≤ baseline → 退出码 0（OK 或进步）。
#
# 这是 B-3.1 文档门禁从 `#![warn(missing_docs)]` 切到 `#![deny(missing_docs)]`
# 的 expand 阶段：先建立度量，让团队习惯每次提交不引入新缺文档；然后在
# `ticket 05 (B-3.1-b)` 把所有现存缺文档补完，再把 `lib.rs:4` 的 `B2-TODO`
# 注释切到 `#![deny(missing_docs)]`。
#
# 第一次跑会生成 baseline（如果不存在）—— 这是 expand 起点。
#
# 本地增量跑：可加 `--lib <crate_name>` 参数走 `cargo doc --no-deps -p <crate>`
# 只测单个 crate（CI 跑全量）。
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BASELINE_PATH="${ROOT_DIR}/.workbuddy/memory/missing-docs-baseline.txt"
mkdir -p "$(dirname "$BASELINE_PATH")"

cd "$ROOT_DIR"

# Parse optional flags
DOC_TARGET="--workspace --all-features"
DOC_DESC="workspace (--all-features)"
while [[ $# -gt 0 ]]; do
    case "$1" in
        --lib)
            DOC_TARGET="-p synapse-rust --all-features"
            DOC_DESC="synapse-rust root crate (--all-features)"
            shift
            ;;
        -p)
            DOC_TARGET="-p $2 --all-features"
            DOC_DESC="single crate: $2 (--all-features)"
            shift 2
            ;;
        *)
            echo "Unknown arg: $1" >&2
            echo "Usage: $0 [--lib | -p <crate_name>]" >&2
            exit 2
            ;;
    esac
done

# 1) 跑 cargo doc，捕获 stderr（rustdoc 警告走 stderr）
#    `|| true` 防止 doc 失败（非 missing_docs 类）中断脚本。
echo "Running: cargo doc --no-deps ${DOC_TARGET}"
echo "Target: ${DOC_DESC}"

DOC_OUT="$(cargo doc --no-deps ${DOC_TARGET} 2>&1 || true)"

# 2) 数 missing docs warning 行数
CURRENT_COUNT="$(printf '%s\n' "$DOC_OUT" | grep -c "warning: missing documentation" || true)"

echo "Current missing-docs count: ${CURRENT_COUNT}"

# 3) 读 baseline（如果不存在 → 生成并提示用户 commit）
if [[ ! -f "$BASELINE_PATH" ]]; then
    echo "No baseline found at ${BASELINE_PATH}."
    echo "Writing initial baseline: ${CURRENT_COUNT}"
    echo "${CURRENT_COUNT}" >"$BASELINE_PATH"
    echo ""
    echo "  NEXT STEPS:"
    echo "    1. Review the count above — does it match expectations?"
    echo "    2. git add ${BASELINE_PATH}"
    echo "    3. git commit -m 'docs(ratchet): initial missing-docs baseline = ${CURRENT_COUNT}'"
    echo ""
    echo "  Subsequent runs will ratchet against this number."
    exit 0
fi

BASELINE_COUNT="$(cat "$BASELINE_PATH" | tr -d '[:space:]')"

# 校验 baseline 是整数
if ! [[ "$BASELINE_COUNT" =~ ^[0-9]+$ ]]; then
    echo "ERROR: baseline file is not a non-negative integer: '${BASELINE_COUNT}'" >&2
    echo "  Path: ${BASELINE_PATH}" >&2
    echo "  Fix: overwrite with the correct count, e.g.: echo 0 > ${BASELINE_PATH}" >&2
    exit 2
fi

echo "Baseline: ${BASELINE_COUNT}"

# 4) ratchet: 当前 > 上次 → 退出码 1；当前 ≤ 上次 → 退出码 0
if [[ "$CURRENT_COUNT" -gt "$BASELINE_COUNT" ]]; then
    echo ""
    echo "  ✗ REGRESSION: missing docs regressed ${BASELINE_COUNT} → ${CURRENT_COUNT} (+$((CURRENT_COUNT - BASELINE_COUNT)))"
    echo ""
    echo "  Add doc comments to the items listed above, then re-run."
    echo "  To update baseline (only after intentional cleanup):"
    echo "    echo ${CURRENT_COUNT} > ${BASELINE_PATH}"
    exit 1
elif [[ "$CURRENT_COUNT" -eq "$BASELINE_COUNT" ]]; then
    echo "  ✓ missing docs: ${CURRENT_COUNT} (no regression)"
    if [[ "$CURRENT_COUNT" -eq 0 ]]; then
        echo "  ✓ READY for #![deny(missing_docs)] in lib.rs (ticket 05)"
    fi
    exit 0
else
    # CURRENT_COUNT < BASELINE_COUNT — progress!
    DELTA=$((BASELINE_COUNT - CURRENT_COUNT))
    echo "  ✓ PROGRESS: missing docs ${BASELINE_COUNT} → ${CURRENT_COUNT} (-${DELTA})"
    if [[ "$CURRENT_COUNT" -eq 0 ]]; then
        echo "  ✓ READY for #![deny(missing_docs)] in lib.rs (ticket 05)"
    else
        echo "  Updating baseline → ${CURRENT_COUNT}"
        echo "${CURRENT_COUNT}" >"$BASELINE_PATH"
    fi
    exit 0
fi
