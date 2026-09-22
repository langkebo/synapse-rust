#!/usr/bin/env bash
#
# CI gate: SQLx 动态/静态查询**棘轮**（ratchet）
#
# ── 背景与本次修复 ────────────────────────────────────────────────────────────
#
# 本脚本此前存在三处缺陷，且从未被任何测试真正执行过：
#
# 1. 扫描范围错误 —— 只 `grep ... src/`（根 crate）。SQL 调用绝大多数在
#    workspace crate：实测 `src/` 仅 25 处，而 `synapse-storage/src/` 有 1,123 处。
#    报告数字基于约 1.7% 的样本，不可信。
# 2. 阈值不可达 —— 硬编码 `max=0.30`（动态占比 ≤30%），而实测动态占比约 96%。
#    脚本自述引用的 2026-06-03 审计就记录过 99.6% 动态。一个永远失败的门禁
#    等于没有门禁；它也因此从未被接入 CI。
# 3. 死引用 —— 失败信息指向不存在的 `docs/synapse-rust/M3_SQLX_MIGRATION_PLAN.md`。
#
# 现改为**棘轮（ratchet）**语义，与 `scripts/check_fmt_ratchet.sh` 同型：
#   * 动态调用数 **不得增加**（回归即失败）
#   * 静态调用数 **不得减少**（迁移成果不得丢失）
# 基线固化在 `scripts/ci/sqlx_dynamic_ratio_baseline`，随迁移进度手动下调。
#
# 这样门禁是"不会变坏"而不是"必须先重写 ~1,400 处调用"，因此可以**接入 CI 阻塞**。
#
# ── 环境 ─────────────────────────────────────────────────────────────────────
#   SQLX_DYNAMIC_MAX_BASELINE  覆盖允许的动态调用上限（用于自测/收紧）
#   SQLX_STATIC_MIN_BASELINE   覆盖要求的静态调用下限
#   SQLX_DYNAMIC_RATIO_MAX     兼容旧接口：额外的比率上限（默认不启用）
#
# Exits 0 on success, 1 on ratchet violation or missing baseline.
#
# 注意：计数包含 crate 内 `#[cfg(test)]` 内联测试模块（如
# `synapse-storage/src/event/db_tests.rs`）。它们由 `cargo test` 编译，
# 同样存在 schema 漂移风险，故一并计入；这一点在基线文件中已注明。

set -eu

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

BASELINE_FILE="scripts/ci/sqlx_dynamic_ratio_baseline"

# 扫描范围：根 crate + 全部 workspace crate 成员（见 Cargo.toml [workspace].members）。
# 排除：
#   .claude/worktrees  —— 旧仓库副本，会让计数随本地 worktree 状态漂移
#   target             —— 构建产物
SCAN_DIRS=(
    "src"
    "synapse-common/src"
    "synapse-cache/src"
    "synapse-storage/src"
    "synapse-e2ee/src"
    "synapse-federation/src"
    "synapse-services/src"
    "synapse-web/src"
    "synapse-test-utils/src"
)

EXCLUDES=(
    "--exclude-dir=target"
    "--exclude-dir=.claude"
    "--exclude-dir=.git"
)

echo "==> SQLx 动态/静态查询棘轮"
echo "    扫描范围: ${SCAN_DIRS[*]}"

# ---------------------------------------------------------------------------
# 1) 基线
# ---------------------------------------------------------------------------
if [[ ! -f "${BASELINE_FILE}" ]]; then
    echo "ERROR: 找不到棘轮基线文件 ${BASELINE_FILE}" >&2
    echo "       基线缺失时无法判定回归；请先创建（见本脚本头部说明）。" >&2
    exit 1
fi

# shellcheck disable=SC1090
. "${BASELINE_FILE}"

: "${BASELINE_DYNAMIC:?基线文件必须定义 BASELINE_DYNAMIC}"
: "${BASELINE_STATIC:?基线文件必须定义 BASELINE_STATIC}"

MAX_DYNAMIC="${SQLX_DYNAMIC_MAX_BASELINE:-${BASELINE_DYNAMIC}}"
MIN_STATIC="${SQLX_STATIC_MIN_BASELINE:-${BASELINE_STATIC}}"

# ---------------------------------------------------------------------------
# 2) 计数
# ---------------------------------------------------------------------------
# 动态调用：sqlx::query( / query_as( / query_scalar( / query_as::< / query_scalar::< （非宏）
# 静态调用：sqlx::query! / query_as! / query_scalar! / query_file!（编译期校验）
# ⚠️ 排除：doc comments (!) 行（它们包含 `sqlx::query` 提及但非实际调用）
count_matches() {
    local pattern="$1"
    local total=0
    local dir
    for dir in "${SCAN_DIRS[@]}"; do
        [[ -d "${dir}" ]] || continue
        # 过滤 doc comments：grep -v 排除以 //! /// 开头的行
        local n
        n=$(grep -rE --exclude-dir=target --exclude-dir=.claude --exclude-dir=.git -e "${pattern}" "${dir}" 2>/dev/null |
            grep -vE ':.*//!|:.*///' | wc -l | tr -d ' ') || true
        total=$((total + n))
    done
    echo "${total}"
}

# 融合 turbofish 形式：query_as::<(_) query_scalar::<(_)
dynamic=$(count_matches 'sqlx::query(_as|_scalar)?([<(]|::<)')
static=$(count_matches 'sqlx::query(_as|_scalar|_file)?!')

total=$((dynamic + static))
if [[ "${total}" -eq 0 ]]; then
    echo "ERROR: 扫描到 0 处 sqlx 调用，扫描范围配置可能已失效" >&2
    exit 1
fi

ratio=$(awk -v d="${dynamic}" -v t="${total}" 'BEGIN { printf "%.4f", d / t }')

echo "check_sqlx_dynamic_ratio: dynamic=${dynamic} static=${static} total=${total} ratio=${ratio}"
echo "check_sqlx_dynamic_ratio: 棘轮基线 dynamic<=${MAX_DYNAMIC} static>=${MIN_STATIC}"

FAILURES=0

# ---------------------------------------------------------------------------
# 3) 棘轮判定：动态不得增加
# ---------------------------------------------------------------------------
if [[ "${dynamic}" -gt "${MAX_DYNAMIC}" ]]; then
    delta=$((dynamic - MAX_DYNAMIC))
    echo "FAIL: 动态 SQL 调用增加 ${delta} 处（${MAX_DYNAMIC} → ${dynamic}）" >&2
    echo "      请改用编译期校验的 query! / query_as! 宏；" >&2
    echo "      若确属无法静态化的动态 SQL（可变 WHERE 拼接等），" >&2
    echo "      请显式下调/调整 ${BASELINE_FILE} 并说明理由。" >&2
    FAILURES=$((FAILURES + 1))
else
    echo "OK: 动态 SQL 未增加（${dynamic} <= ${MAX_DYNAMIC}）"
fi

# ---------------------------------------------------------------------------
# 4) 棘轮判定：静态不得减少
# ---------------------------------------------------------------------------
if [[ "${static}" -lt "${MIN_STATIC}" ]]; then
    delta=$((MIN_STATIC - static))
    echo "FAIL: 静态 SQL 调用减少 ${delta} 处（${MIN_STATIC} → ${static}）" >&2
    echo "      编译期校验的查询被改回了动态查询，或迁移成果丢失。" >&2
    FAILURES=$((FAILURES + 1))
else
    echo "OK: 静态 SQL 未减少（${static} >= ${MIN_STATIC}）"
fi

# ---------------------------------------------------------------------------
# 5) 可选的比率上限（兼容旧接口，默认不启用）
# ---------------------------------------------------------------------------
if [[ -n "${SQLX_DYNAMIC_RATIO_MAX:-}" ]]; then
    if awk -v r="${ratio}" -v m="${SQLX_DYNAMIC_RATIO_MAX}" 'BEGIN { exit !(r > m) }'; then
        echo "FAIL: ratio ${ratio} 超过 SQLX_DYNAMIC_RATIO_MAX=${SQLX_DYNAMIC_RATIO_MAX}" >&2
        FAILURES=$((FAILURES + 1))
    else
        echo "OK: ratio ${ratio} <= ${SQLX_DYNAMIC_RATIO_MAX}"
    fi
fi

if [[ "${FAILURES}" -gt 0 ]]; then
    echo "check_sqlx_dynamic_ratio: FAIL (${FAILURES} 项棘轮违规)" >&2
    exit 1
fi

echo "check_sqlx_dynamic_ratio: OK"
exit 0
