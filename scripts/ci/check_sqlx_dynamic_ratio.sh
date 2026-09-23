#!/usr/bin/env bash
#
# CI gate: SQLx 动态/静态查询**棘轮**（ratchet）—— 生产 / #[cfg(test)] 双基线
#
# ── 历史与本次修复 ────────────────────────────────────────────────────────────
#
# 本脚本此前存在三处缺陷，且从未被任何测试真正执行过：
#
# 1. 扫描范围错误 —— 只 `grep ... src/`（根 crate）。SQL 调用绝大多数在
#    workspace crate：实测 `src/` 仅 25 处，而 `synapse-storage/src/` 有 1,123 处。
# 2. 阈值不可达 —— 硬编码 `max=0.30`，而实测动态占比约 96%。一个永远失败的门禁
#    等于没有门禁。**已改为棘轮语义**：动态不得增加、静态不得减少。
# 3. 死引用 —— 失败信息曾指向不存在的 `docs/synapse-rust/M3_SQLX_MIGRATION_PLAN.md`。
#    该引用已移除；本注释保留说明（见 `tests/unit/sqlx_ratio_gate_tests.rs`）。
#
# ── 2026-09-23：计数改为分区口径（本脚本仍是唯一入口）────────────────────────
#
# 旧计数器 `grep … | wc -l` 有三个已登记缺陷：① 不看注释（散文里的
# `sqlx::query(` 被算作调用，历史 +1 即此类）；② 按行计数（同一行两处只算一处）；
# ③ **不分区**，把 `#[cfg(test)]` 内联夹具与生产代码混在一个数字里，2,151 处里
# 哪些是真实**生产**债务不可见。
#
# 现计数委托给 `scripts/ci/sqlx_query_census.py`：词法剥离注释/字符串后按**出现
# 次数**计数，并按源码区域把结果分成 `dynamic_production` / `dynamic_test`。
# 棘轮因此有两条动态基线与一条静态基线：
#   * dynamic_production 不得增加（真实生产债务，**应当单调降**）
#   * dynamic_test       不得增加（测试基础设施；确需增加须在基线文件写理由）
#   * static             不得减少（编译期校验成果不得回退）
#
# 语义与 `scripts/check_fmt_ratchet.sh` 同型：这是"不会变坏"的门禁，不是
# "必须先重写全部调用"的门禁。取得进展后请**同时**下调 dynamic、上调 static。
#
# ── 环境 ─────────────────────────────────────────────────────────────────────
#   SQLX_DYNAMIC_PRODUCTION_MAX  覆盖生产动态上限（自测/收紧）
#   SQLX_DYNAMIC_TEST_MAX        覆盖测试动态上限（自测/收紧）
#   SQLX_DYNAMIC_MAX_BASELINE    兼容旧接口：额外限制**动态总数**（自测用）
#   SQLX_STATIC_MIN_BASELINE     覆盖静态下限（自测/收紧）
#   SQLX_DYNAMIC_RATIO_MAX       兼容旧接口：额外的比率上限（默认不启用）
#
# Exits 0 on success, 1 on ratchet violation or missing baseline.
#
# ⚠️ 计数**包含** crate 内 `#[cfg(test)]` 内联测试模块，以及由
# `#[cfg(test)] mod x;` 声明的整份测试文件（如 `event/db_tests.rs`）；
# 独立测试目录 `tests/` 与 `artifacts/` 不在扫描范围内。
# 扫描显式排除 `target/` 与 `.claude/`（旧仓库副本会污染计数）。

set -eu

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

BASELINE_FILE="scripts/ci/sqlx_dynamic_ratio_baseline"
CENSUS="scripts/ci/sqlx_query_census.py"

echo "==> SQLx 动态/静态查询棘轮（生产 / #[cfg(test)] 分区）"

if [[ ! -f "${BASELINE_FILE}" ]]; then
    echo "ERROR: 找不到棘轮基线文件 ${BASELINE_FILE}" >&2
    echo "       基线缺失时无法判定回归；请先创建（见本脚本头部说明）。" >&2
    exit 1
fi
if [[ ! -f "${CENSUS}" ]]; then
    echo "ERROR: 找不到普查脚本 ${CENSUS}" >&2
    exit 1
fi

# shellcheck disable=SC1090
. "${BASELINE_FILE}"

: "${BASELINE_DYNAMIC_PRODUCTION:?基线文件必须定义 BASELINE_DYNAMIC_PRODUCTION}"
: "${BASELINE_DYNAMIC_TEST_INFRA:?基线文件必须定义 BASELINE_DYNAMIC_TEST_INFRA}"
: "${BASELINE_STATIC:?基线文件必须定义 BASELINE_STATIC}"

MAX_PRODUCTION="${SQLX_DYNAMIC_PRODUCTION_MAX:-${BASELINE_DYNAMIC_PRODUCTION}}"
MAX_TEST="${SQLX_DYNAMIC_TEST_MAX:-${BASELINE_DYNAMIC_TEST_INFRA}}"
MIN_STATIC="${SQLX_STATIC_MIN_BASELINE:-${BASELINE_STATIC}}"

# ---------------------------------------------------------------------------
# 计数（幂等、无副作用；--json 便于测试与后续工具消费）
# ---------------------------------------------------------------------------
CENSUS_JSON="$(python3 "${CENSUS}" --json)"
metric() {
    printf '%s' "${CENSUS_JSON}" | python3 -c "import json,sys; print(json.load(sys.stdin)['$1'])"
}

dynamic_production="$(metric dynamic_production)"
dynamic_test="$(metric dynamic_test)"
static="$(metric static)"
dynamic="$(metric dynamic)"
total="$(metric total)"
ratio="$(metric ratio)"
query_builder="$(metric query_builder)"

echo "check_sqlx_dynamic_ratio: dynamic=${dynamic} static=${static} total=${total} ratio=${ratio} query_builder=${query_builder}"
echo "check_sqlx_dynamic_ratio: 分区 production=${dynamic_production} test=${dynamic_test}"
echo "check_sqlx_dynamic_ratio: 棘轮基线 production<=${MAX_PRODUCTION} test<=${MAX_TEST} static>=${MIN_STATIC}"

FAILURES=0
fail() {
    echo "FAIL: $*" >&2
    FAILURES=$((FAILURES + 1))
}

# ---------------------------------------------------------------------------
# 棘轮判定：生产动态不得增加
# ---------------------------------------------------------------------------
if [[ "${dynamic_production}" -gt "${MAX_PRODUCTION}" ]]; then
    fail "生产动态 SQL 增加 $((dynamic_production - MAX_PRODUCTION)) 处（${MAX_PRODUCTION} → ${dynamic_production}）"
    echo "      请改用编译期校验的 query! / query_as! / query_scalar! 宏；" >&2
    echo "      若确属无法静态化的动态 SQL（DDL / 动态标识符 / 动态 IN 列表），" >&2
    echo "      请按 scripts/ci/sqlx_dynamic_ratio_baseline 的体例下调/调整基线并说明理由。" >&2
else
    echo "OK: 生产动态 SQL 未增加（${dynamic_production} <= ${MAX_PRODUCTION}）"
fi

# ---------------------------------------------------------------------------
# 棘轮判定：测试基础设施动态不得增加
# ---------------------------------------------------------------------------
if [[ "${dynamic_test}" -gt "${MAX_TEST}" ]]; then
    fail "测试基础设施动态 SQL 增加 $((dynamic_test - MAX_TEST)) 处（${MAX_TEST} → ${dynamic_test}）"
    echo "      \`#[cfg(test)]\` 内的宏不进 \`cargo sqlx prepare\`（\`--all-targets\` 会 E0432），" >&2
    echo "      测试夹具确需动态 SQL 时，请在基线文件写清来源与收紧方向。" >&2
else
    echo "OK: 测试基础设施动态 SQL 未增加（${dynamic_test} <= ${MAX_TEST}）"
fi

# ---------------------------------------------------------------------------
# 棘轮判定：静态不得减少
# ---------------------------------------------------------------------------
if [[ "${static}" -lt "${MIN_STATIC}" ]]; then
    fail "静态 SQL 调用减少 $((MIN_STATIC - static)) 处（${MIN_STATIC} → ${static}）"
    echo "      编译期校验的查询被改回了动态查询，或迁移成果丢失。" >&2
else
    echo "OK: 静态 SQL 未减少（${static} >= ${MIN_STATIC}）"
fi

# ---------------------------------------------------------------------------
# 兼容旧接口：额外的动态总数上限（自测用）
# ---------------------------------------------------------------------------
if [[ -n "${SQLX_DYNAMIC_MAX_BASELINE:-}" && "${dynamic}" -gt "${SQLX_DYNAMIC_MAX_BASELINE}" ]]; then
    fail "动态 SQL 总数 ${dynamic} 超过 SQLX_DYNAMIC_MAX_BASELINE=${SQLX_DYNAMIC_MAX_BASELINE}"
fi

# ---------------------------------------------------------------------------
# 兼容旧接口：可选的比率上限（默认不启用）
# ---------------------------------------------------------------------------
if [[ -n "${SQLX_DYNAMIC_RATIO_MAX:-}" ]]; then
    if awk -v r="${ratio}" -v m="${SQLX_DYNAMIC_RATIO_MAX}" 'BEGIN { exit !(r > m) }'; then
        fail "ratio ${ratio} 超过 SQLX_DYNAMIC_RATIO_MAX=${SQLX_DYNAMIC_RATIO_MAX}"
    else
        echo "OK: ratio ${ratio} <= ${SQLX_DYNAMIC_RATIO_MAX}"
    fi
fi

if [[ "${FAILURES}" -gt 0 ]]; then
    echo "check_sqlx_dynamic_ratio: FAIL (${FAILURES} 项棘轮违规)" >&2
    exit 1
fi

echo "check_sqlx_dynamic_ratio: OK"
