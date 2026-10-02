#!/usr/bin/env bash
# 提交前把「会阻断 CI 的格式 / 拼写 / 契约」门禁一次跑全。
#
# 存在理由（2026-10-02 实测两次事故，都不是"改错了"而是"跑漏了"）：
#   * `416ece3c8` 漏跑 `cargo fmt` ⇒ Format Governance 红；
#   * `cf845cb35` 漏跑 `ruff format` ⇒ 同一条车道红。
# 单跑某一个门的习惯不可靠，故把 CI 的同名入口按改动集一次跑完。
#
# 用法：
#   bash scripts/quality/preflight.sh            # 快档（默认）
#   bash scripts/quality/preflight.sh --clippy   # 追加两档 clippy（约 7 分钟）
# 退出码：0 = 全部通过；非 0 = 有失败项（明细已在 stdout）。
set -uo pipefail
cd "$(git rev-parse --show-toplevel)"
export npm_config_cache="${npm_config_cache:-/tmp/npm-cache}"

fail=0
step() { printf '\n== %s ==\n' "$1"; }
run() { # run <描述> <命令...>
    local desc="$1"; shift
    if "$@" >/tmp/preflight_step.log 2>&1; then
        echo "OK   $desc"
    else
        echo "FAIL $desc"
        tail -15 /tmp/preflight_step.log
        fail=1
    fi
}

# 改动集 = 已暂存 ∪ 未暂存（pre-commit 时改动已暂存，故两者都要看）
# 未跟踪的新文件也必须进改动集：`git diff` 看不到它们，而"新增一个 .py/.md 忘了格式化"
# 正是本脚本要防的场景之一。
changed=$( { git diff --name-only; git diff --cached --name-only; git ls-files --others --exclude-standard; } | sort -u )
md=$(printf '%s\n' "$changed" | grep -E '\.md$' | grep -v '/archive/' || true)
py=$(printf '%s\n' "$changed" | grep -E '\.py$' || true)
rs=$(printf '%s\n' "$changed" | grep -E '\.rs$' || true)

step "Rust 格式化"
run "cargo fmt --all" cargo fmt --all
run "./scripts/check_fmt_ratchet.sh" ./scripts/check_fmt_ratchet.sh

if [ -n "$py" ] || [ -n "$rs" ] || [ -n "$(printf '%s\n' "$changed" | grep -E '\.(json|ya?ml|sh)$' || true)" ]; then
    step "Format Governance 同款入口（ruff / shfmt / yamllint / check-json / format_audit）"
    run "scripts/quality/format_check.sh" bash scripts/quality/format_check.sh
fi

step "文档：逐文件 aspell + markdownlint"
for f in $md; do
    [ -f "$f" ] || continue
    run "aspell $f" bash scripts/check_doc_spelling.sh "$f"
done
if [ -n "$md" ]; then
    # shellcheck disable=SC2086
    run "markdownlint" npx --yes markdownlint-cli@0.49.1 -c .markdownlint.json $md
fi

step "契约 gate"
if [ -z "$(printf '%s\n' "$changed" | grep -E 'ROUTE_CONTRACT|ledger_annotations|extract_registered|derived_route|ledger_export|route-table\.json|route_ledger' || true)" ]; then
    run "check_route_contract.sh" bash scripts/contract/check_route_contract.sh
else
    echo "SKIP 本次改动了契约链文件 ⇒ 该 gate 在提交前构造性红（它把重生成的 ROUTE_CONTRACT.md 与 HEAD 比），"
    echo "     改在提交后复核（参见 docs/audit/OPTIMIZATION_EXECUTION_PLAN_2026-09-15.md §8.3 L-6）"
fi

if [ "${1:-}" = "--clippy" ]; then
    step "clippy（两档，CI 同款）"
    run "clippy (test-utils)" env SQLX_OFFLINE=true cargo clippy --workspace --all-targets --features test-utils --locked -- -D warnings
    run "clippy (all-features)" env SQLX_OFFLINE=true cargo clippy --workspace --all-targets --features test-utils --all-features --locked -- -D warnings
fi

printf '\n== 结论 ==\n'
if [ "$fail" -eq 0 ]; then
    echo "全部通过（跑的是 CI 同名门禁）"
else
    echo "有失败项 —— 修完再提交"
fi
exit "$fail"
