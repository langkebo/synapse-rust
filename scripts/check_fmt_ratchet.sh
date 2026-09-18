#!/usr/bin/env bash
#
# W7+: fmt debt 棘轮（ratchet）—— 只许减、不许增。
#
# 背景：仓库里累积了 31 处 rustfmt 不合规（friend_room_service、
# sliding_sync_service 等 12 个文件），`cargo fmt --all -- --check`
# 因此长期是红的。长期红的 CI 会被团队忽视，新引入的格式问题也淹没在
# 既有噪声里 —— 这比没有检查更糟。
#
# 棘轮机制：把当前 debt 数量记为 baseline，CI 只比对数字：
#   current > baseline  -> 失败（引入了新格式问题，必须修）
#   current < baseline  -> 失败（debt 减少了，必须收紧棘轮，见下方命令）
#   current == baseline -> 通过
#
# 为什么「减少」也要失败：如果允许通过，baseline 会永远停在初始值，
# 棘轮就名存实亡。强制更新让每一点改善都被固化下来。
#
# 更新 baseline（修好格式问题后执行）：
#   cargo fmt --all
#   ./scripts/check_fmt_ratchet.sh --update
#
# 用法：
#   ./scripts/check_fmt_ratchet.sh            # CI 检查模式
#   ./scripts/check_fmt_ratchet.sh --update   # 重算并写入 baseline
#
set -uo pipefail

cd "$(dirname "$0")/.." || exit 1

BASELINE_FILE="scripts/.fmt-baseline"
UPDATE=0
[[ "${1:-}" == "--update" ]] && UPDATE=1

# The gate is worthless if the tool it measures with is absent: `command not
# found` produces no `Diff in` lines, so the count would be 0 and (with
# baseline 0) the script would print OK. Checked up front AND inside the
# counter, because a rustfmt that exists but crashes has the same signature.
# Regression context: measured 2026-09-19 — the counter was
# `… | xargs -0 rustfmt … | grep -c '^Diff in' || true`, which cannot tell
# "no diffs" from "the formatter never ran".
if ! command -v rustfmt >/dev/null 2>&1; then
    echo "::error::rustfmt is not on PATH — cannot measure fmt debt." >&2
    echo "  This gate must never report OK when its measuring tool is missing." >&2
    exit 1
fi

# 统计 rustfmt 报出的差异处数。
# `grep -c` 在无匹配时退出码为 1，`|| true` 防止误判为脚本失败。
#
# ⚠️ 必须用独立 `rustfmt`，不能用 `cargo fmt --check`。
# `cargo fmt --check` 在检测到差异时**只返回非零退出码，不打印 `Diff in` 块**
# （`Diff in` 是独立 rustfmt 的输出格式）。因此旧实现在 21 个 CI 运行里对 56 个
# 未格式化文件一律报 0，`fmt debt: current=0 baseline=0 / OK` 是假绿。
#
# 实证：对同一个故意未格式化的文件
#   cargo fmt --all -- --check   → Diff 块 0，exit 0   （完全不检测）
#   rustfmt --check --edition 2021 <file> → Diff 块 1，exit 1
#
# 从仓库根目录运行，各文件会自动套用根 `rustfmt.toml`。
# 排除 target/、vendor/ 与 .claude/worktrees（第二份工作树的副本不该计入）。
#
# 失败与"干净"必须可区分：rustfmt 崩溃时输出里没有 `Diff in`，与"0 处差异"
# 长得一模一样。因此这里显式识别工具错误并**向 stdout 输出非数字**，让调用方的
# `^[0-9]+$` 校验失败 —— 绝不返回 0。
fmt_targets() {
    find src synapse-common synapse-cache synapse-storage synapse-e2ee \
         synapse-federation synapse-services synapse-web synapse-test-utils benches tests \
         -name '*.rs' -not -path '*/target/*' -print0 2>/dev/null
}

count_fmt_diffs() {
    local out
    out="$(fmt_targets | xargs -0 rustfmt --check --edition 2021 2>&1)"
    if printf '%s\n' "$out" | grep -qE '^(error|error\[)|^xargs: |rustfmt: .*(not found|No such file)|No such file or directory'; then
        printf '%s\n' "$out" | grep -E '^(error|error\[)|^xargs: |No such file or directory' | head -5 >&2
        echo "rustfmt-failed-to-run"
        return 1
    fi
    printf '%s\n' "$out" | grep -c '^Diff in' || true
}

if ((UPDATE)); then
    current="$(count_fmt_diffs)"
    if ! [[ "$current" =~ ^[0-9]+$ ]]; then
        echo "::error::refusing to write a non-numeric baseline (got: '$current')" >&2
        exit 1
    fi
    printf '%s\n' "$current" >"$BASELINE_FILE"
    echo "fmt baseline updated: $current"
    exit 0
fi

if [[ ! -f "$BASELINE_FILE" ]]; then
    echo "::error::missing baseline file: $BASELINE_FILE" >&2
    echo "  Run './scripts/check_fmt_ratchet.sh --update' to create it." >&2
    exit 1
fi

baseline="$(tr -d '[:space:]' <"$BASELINE_FILE")"
if ! [[ "$baseline" =~ ^[0-9]+$ ]]; then
    echo "::error::baseline is not a number: '$baseline' (file: $BASELINE_FILE)" >&2
    exit 1
fi

current="$(count_fmt_diffs)"
if ! [[ "$current" =~ ^[0-9]+$ ]]; then
    echo "::error::failed to count fmt diffs (got: '$current')" >&2
    exit 1
fi

echo "fmt debt: current=$current baseline=$baseline"

if ((current > baseline)); then
    echo "" >&2
    echo "::error::fmt debt increased: $current > $baseline" >&2
    echo "  New formatting issues were introduced. Fix them with:" >&2
    echo "    cargo fmt --all" >&2
    echo "  (or format only the files you touched:" >&2
    echo "    rustfmt --edition 2021 <file1> <file2> ...)" >&2
    echo "" >&2
    echo "  Offending locations:" >&2
    fmt_targets | xargs -0 rustfmt --check --edition 2021 2>&1 | grep '^Diff in' >&2 || true
    exit 1
fi

if ((current < baseline)); then
    echo "" >&2
    echo "::error::fmt debt decreased: $current < $baseline" >&2
    echo "  Good — the debt shrank, so the ratchet must be tightened." >&2
    echo "  Update the baseline and commit it:" >&2
    echo "    printf '%s\\n' $current > $BASELINE_FILE" >&2
    exit 1
fi

echo "OK: fmt debt at baseline ($current), no regression."
