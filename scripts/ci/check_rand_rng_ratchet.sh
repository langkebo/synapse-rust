#!/usr/bin/env bash
#
# 棘轮：`rand::rng()` 的**新增**用法（RUSTSEC-2026-0097 的纵深防御）。
#
# 政策来源：`.cargo/audit.toml` 的 RUSTSEC-2026-0097 注释 +
# docs/security/ci-security-grading.md。
#
# 2026-09-21 复核（详见 `.cargo/audit.toml`）：advisory 的 `patched` 区间是
#   `>= 0.10.1` / `>= 0.9.3, < 0.10.0` / `>= 0.8.6, < 0.9.0`，
# 而 Cargo.lock 是 rand 0.8.7 与 0.9.5 —— **都在已修复区间内**，所以那条
# `RUSTSEC-2026-0097` ignore 已删除（留着会在 rand 被降级时静默放行）。
# 本棘轮作为纵深防御保留：CI 不允许**新增** `rand::rng()` 用法。
#
# ⚠️ 原来 CI 里那一步是**绝对禁令**（`if git grep -n "rand::rng()" -- '*.rs'; then … exit 1`），
# 而树上本来就有 47 处存量 —— 它**永远不可能绿**。由于 Security Audit job 长期在更早的
# advisory-db 步骤就失败，这道禁令从未被执行过；2026-09-20 第一次真跑才暴露（§14.14.1）。
# 政策说的是"禁止**新增**"，所以这里按仓库既有棘轮范式实现（同 fmt debt / missing_docs /
# trait count / sqlx ratio / coverage_baseline）：
#
#   current > baseline → FAIL（有人新增了 rand::rng()）
#   current < baseline → FAIL（要求收紧 baseline，防止棘轮悄悄变松）
#
# 用法：
#   bash scripts/ci/check_rand_rng_ratchet.sh            # CI 检查模式
#   bash scripts/ci/check_rand_rng_ratchet.sh --update   # 重算并写入 baseline
#
# 扫描面与旧步骤一致（`git grep "rand::rng()" -- '*.rs'`，按**行**计数），因此 baseline
# 与历史判据可比；唯一差别是排除守卫文件本身。
#
# 为什么要排除：`tests/unit/ci_test_scope_tests.rs` 里的守卫必须写出被禁模式（文档注释、
# 断言消息、以及它自己那条 `git grep` 命令），否则无法自证能变红。若把守卫文件算进去，
# 8 处自指命中会把实测值从 47 抬到 55，baseline 随之失真 —— 那不是"新增用法"，是扫描面
# 把自己也照了进去。守卫文件是 CI 口径测试，不会承载生产用法。
set -euo pipefail

cd "$(dirname "$0")/../.."

BASELINE_FILE="scripts/ci/rand_rng_baseline"

# 必须与 `tests/unit/ci_test_scope_tests.rs::rand_rng_step_is_a_ratchet_with_an_honest_baseline`
# 使用**同一个**排除路径，否则守卫算出的实测值与这里不一致，会假红。
EXCLUDE_GUARD=':(exclude)tests/unit/ci_test_scope_tests.rs'

count_occurrences() {
    git grep -n "rand::rng()" -- '*.rs' "$EXCLUDE_GUARD" 2>/dev/null | wc -l | tr -d ' '
}

if [[ "${1:-}" == "--update" ]]; then
    current="$(count_occurrences)"
    printf '%s\n' "$current" >"$BASELINE_FILE"
    echo "rand::rng() baseline updated: $current"
    exit 0
fi

if [[ ! -f "$BASELINE_FILE" ]]; then
    echo "::error::missing baseline file: $BASELINE_FILE" >&2
    echo "  Run: bash scripts/ci/check_rand_rng_ratchet.sh --update" >&2
    exit 1
fi

baseline="$(tr -d '[:space:]' <"$BASELINE_FILE")"
if ! [[ "$baseline" =~ ^[0-9]+$ ]]; then
    echo "::error::baseline 必须是单个整数: $BASELINE_FILE ('$baseline')" >&2
    exit 1
fi

current="$(count_occurrences)"

if ((current > baseline)); then
    echo "::error::rand::rng() 用法增加了: $current > $baseline" >&2
    git grep -n "rand::rng()" -- '*.rs' "$EXCLUDE_GUARD" >&2 || true
    echo "" >&2
    echo "  政策（.cargo/audit.toml，RUSTSEC-2026-0097）：没有上游修复，禁止**新增**" >&2
    echo "  \`rand::rng()\` 用法。若确有正当理由，请先更新 .cargo/audit.toml 的裁定并" >&2
    echo "  调整 baseline（--update），不要把这步改回绝对禁令。" >&2
    exit 1
fi

if ((current < baseline)); then
    echo "::error::rand::rng() 用法减少了: $current < $baseline —— 好事，请收紧 baseline:" >&2
    echo "    bash scripts/ci/check_rand_rng_ratchet.sh --update" >&2
    exit 1
fi

echo "OK: rand::rng() 用法数量与 baseline 一致（${current}）。"
