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
count_fmt_diffs() {
    find src synapse-common synapse-cache synapse-storage synapse-e2ee \
         synapse-federation synapse-services benches tests \
         -name '*.rs' -not -path '*/target/*' -print0 2>/dev/null \
        | xargs -0 rustfmt --check --edition 2021 2>&1 \
        | grep -c '^Diff in' || true
}

if ((UPDATE)); then
    current="$(count_fmt_diffs)"
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
    find src synapse-common synapse-cache synapse-storage synapse-e2ee \
         synapse-federation synapse-services benches tests \
         -name '*.rs' -not -path '*/target/*' -print0 2>/dev/null \
        | xargs -0 rustfmt --check --edition 2021 2>&1 | grep '^Diff in' >&2 || true
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
