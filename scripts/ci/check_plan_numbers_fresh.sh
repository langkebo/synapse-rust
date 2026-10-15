#!/usr/bin/env bash
#
# 「文档数字随访」—— SQLx 静态化方案文档里引用的**棘轮数字**必须与基线/实测一致。
#
# ── 为什么存在 ───────────────────────────────────────────────────────────────
# `docs/audit/SQLX_STATICIZATION_PLAN_2026-09-23.md` 的 §8.7 曾写下
# "静态化轴的十条判据全部满足"。2026-10-15 复核发现其中第 ③ 条（四道棘轮全 OK）
# **当时是红的**：棘轮的口径缺陷把测试区的新增少报了 6 处（报 758→763，真实 758→769），
# 而文档照旧声称全绿 —— 一份"看起来是结论、实际是过期读数"的文档比没有更危险。
#
# 分工（两道合起来才等于"文档说的话现实也成立"）：
#   * **现实 == 基线** 由 `sqlx_ratio_gate_tests`（unit 车道）保证；
#   * **文档 == 基线** 由本脚本保证（连同 `.sqlx` 条数与磁盘实数比对）。
#
# ── 判据 ────────────────────────────────────────────────────────────────────
#   1. 文档里 `<!-- sqlx-plan-numbers:start --> … :end -->` 块内的每个 `KEY=VALUE`
#      都必须能在"真源"里找到同名项且**数值相等**；
#   2. 块里出现、真源里没有的键 ⇒ FAIL（防"文档自造一个数字"）；
#   3. 真源里有、块里漏掉的**受管键** ⇒ FAIL（防"新增棘轮后忘了写进文档"）。
#
# 真源：`scripts/ci/sqlx_dynamic_ratio_baseline`（`BASELINE_*` 四个键）
#       与 `.sqlx/query-*.json` 的磁盘计数（`SQLX_CACHE_ENTRIES`）。
#
# ── 用法 ────────────────────────────────────────────────────────────────────
#   bash scripts/ci/check_plan_numbers_fresh.sh
#   bash scripts/ci/check_plan_numbers_fresh.sh --doc <md> --baseline <file>   # 自测/变异用
#
set -uo pipefail

ROOT="$(git rev-parse --show-toplevel)"
DOC="$ROOT/docs/audit/SQLX_STATICIZATION_PLAN_2026-09-23.md"
BASELINE="$ROOT/scripts/ci/sqlx_dynamic_ratio_baseline"

while [ $# -gt 0 ]; do
    case "$1" in
        --doc)
            DOC="$2"
            shift 2
            ;;
        --baseline)
            BASELINE="$2"
            shift 2
            ;;
        -h | --help)
            sed -n '2,30p' "$0"
            exit 0
            ;;
        *)
            echo "未知参数：$1" >&2
            exit 2
            ;;
    esac
done

# 受管键：新增棘轮时应同时加进这里，否则"漏写进文档"不会被判红。
MANAGED_KEYS=(
    BASELINE_DYNAMIC_PRODUCTION
    BASELINE_DYNAMIC_TEST_INFRA
    BASELINE_STATIC
    BASELINE_QUERY_BUILDER
)

if [ ! -f "$DOC" ]; then
    echo "FAIL 找不到文档：$DOC" >&2
    exit 1
fi
if [ ! -f "$BASELINE" ]; then
    echo "FAIL 找不到基线：$BASELINE" >&2
    exit 1
fi

doc_block() {
    sed -n '/<!-- sqlx-plan-numbers:start -->/,/<!-- sqlx-plan-numbers:end -->/p' "$DOC"
}

if ! doc_block | grep -q 'sqlx-plan-numbers:start'; then
    echo "FAIL 文档里找不到 <!-- sqlx-plan-numbers:start --> 标记块：$DOC" >&2
    echo "     该块是「文档数字随访」的锚点；删掉它会让本门禁静默失效。" >&2
    exit 1
fi

doc_value() {
    doc_block | grep -E "^${1}=" | head -1 | cut -d= -f2- | tr -d '[:space:]'
}

truth_value() {
    case "$1" in
        SQLX_CACHE_ENTRIES)
            find "$ROOT/.sqlx" -maxdepth 1 -name 'query-*.json' 2>/dev/null | wc -l | tr -d '[:space:]'
            ;;
        *)
            grep -E "^${1}=" "$BASELINE" | head -1 | cut -d= -f2- | tr -d '[:space:]'
            ;;
    esac
}

failed=0
reported=0

# 受管键 + 文档块里出现的其它键（后者用于判据 2：文档自造数字）
doc_keys="$(doc_block | grep -oE '^[A-Z_][A-Z0-9_]*=' | tr -d '=' | sort -u)"
all_keys="$(printf '%s\n' "${MANAGED_KEYS[@]}" | cat - <(printf '%s\n' "$doc_keys") | sort -u)"

for key in $all_keys; do
    [ -n "$key" ] || continue
    got="$(doc_value "$key")"
    want="$(truth_value "$key")"

    if [ -z "$got" ]; then
        echo "FAIL 文档块漏写了受管键 \`${key}\`（真源当前值 ${want:-<无>}）"
        failed=1
        continue
    fi
    if [ -z "$want" ]; then
        echo "FAIL 文档块里的 \`${key}=${got}\` 在真源里找不到同名项（真源：基线文件 + .sqlx 磁盘计数）"
        failed=1
        continue
    fi
    reported=$((reported + 1))
    if [ "$got" != "$want" ]; then
        echo "FAIL \`${key}\` 文档写 ${got} / 真源现算 ${want} —— 改基线（或改 .sqlx 条数）时必须同步文档块"
        failed=1
    fi
done

if [ "$reported" -eq 0 ]; then
    echo "FAIL 一个受管键都没核对到 —— 文档块可能已被清空，本门禁处于假绿状态" >&2
    exit 1
fi

if [ "$failed" -ne 0 ]; then
    exit 1
fi

echo "OK 文档数字随访通过：${reported} 个键与真源一致（棘轮基线 + .sqlx 条数）"
