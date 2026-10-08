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

# `npm_config_cache` 可能被环境预设成一个**当前用户不可写**的目录：实测
# `npm_config_cache=$HOME/.npm` 而该目录含 root 属主的 `_cacache` 文件 ⇒ `npx` 以
# EPERM 退出、markdownlint 步报 FAIL。那看起来像"文档格式红了"，实际是环境问题，
# 而且会拦住**所有人**的提交（2026-10-02 实测）。
#
# 探针打在 npm 自己会写的位置（`<_cacache>/tmp`），而不是缓存目录本身 —— 目录本身
# 可写、内层被 root 占有时，只探外层会误判为"可写"。
npm_cache_probe="${npm_config_cache:-/tmp/npm-cache}"
if ! mkdir -p "$npm_cache_probe/_cacache/tmp" 2>/dev/null ||
    ! (: >"$npm_cache_probe/_cacache/tmp/.preflight-write-probe") 2>/dev/null; then
    npm_cache_probe="$(git rev-parse --show-toplevel)/target/tmp/npm-cache"
    mkdir -p "$npm_cache_probe/_cacache/tmp" 2>/dev/null || true
    echo "NOTE npm_config_cache 原值不可写，本步改用 $npm_cache_probe"
fi
rm -f "$npm_cache_probe/_cacache/tmp/.preflight-write-probe" 2>/dev/null || true
export npm_config_cache="$npm_cache_probe"

fail=0
step() { printf '\n== %s ==\n' "$1"; }
run() { # run <描述> <命令...>
    local desc="$1"
    shift
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
#
# ⚠️ `-c core.quotePath=false` 不可省：该配置默认 true，非 ASCII 路径会被 Git 输出成
# `"docs/\346\226\207..."`（带引号 + 八进制转义），下面 `grep -E '\.md$'` 之类的
# **后缀**匹配于是全部落空 —— 实测 2026-10-08：本仓的核心方案文档
# `docs/前缀命名空间治理方案-2026-10-08.md`（中文名）被改了多轮，
# markdownlint / aspell / doc_credibility 三道文档守卫**每一次都静默 SKIP**，
# 而该文档当时真实带着 18 处 markdownlint 违规（AGENTS.md 铁律 8 的
# "门禁长期全绿 ⇒ 先怀疑它没在工作"，这里是"门禁根本没跑"）。
changed=$({
    git -c core.quotePath=false diff --name-only
    git -c core.quotePath=false diff --cached --name-only
    git -c core.quotePath=false ls-files --others --exclude-standard
} | sort -u)
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

# 文档可信度守卫：`docs/synapse-rust-vs-synapse-comparison.md` 的计数必须等于
# `ROUTE_CONTRACT.md`，引用的路径必须存在，五个历史章节必须带 §18 指针。
#
# 存在理由（2026-10-02 实测）：该守卫**早就存在**，但从未进提交前脚本；L-6 把
# `ROUTE_CONTRACT.md` 更新到 1,154 后，对比报告三处仍写 1,152，守卫长期**判红**
# 而无人察觉 —— "守卫存在"不等于"守卫在跑"（AGENTS.md 铁律 8 推论）。
# 只在文档或契约链变动时运行：守卫读的两个文件都没变时它的结论不可能变。
step "文档可信度守卫（计数 / 路径 / 历史章节指针）"
if [ -n "$md" ] || [ -n "$(printf '%s\n' "$changed" | grep -E 'ROUTE_CONTRACT|route_ledger|ledger_export|derived_route' || true)" ]; then
    run "doc_credibility_guard_tests" env SQLX_OFFLINE=true cargo nextest run --test unit --features test-utils -E 'test(doc_credibility_guard_tests)'
else
    echo "SKIP 本次未改动 .md / 契约链 ⇒ 守卫读的文件未变"
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
