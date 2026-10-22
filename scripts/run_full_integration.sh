#!/usr/bin/env bash
#
# 全量集成套件（**唯一权威跑法**）。
#
# 为什么要有这个脚本：这套跑法的三个关键参数，2026-10-22 那次手工执行时错了两个，
# 代价是白跑一轮 90 分钟 + 一轮 75 分钟的假红排查：
#
#   1. `--all-features` —— 少它会让 3 条用例假红（`snapshot_login_flows_v3` 等快照里带着
#      `m.login.cas` / `m.login.sso`，精简 feature 集下那些路由根本没注册）。
#   2. `--no-fail-fast` —— 少它时一条失败会把后面 1500 条**直接掐掉**，日志看着"跑完了"。
#   3. `-j 2` —— 8 路并行时 `CREATE DATABASE … TEMPLATE` 要对模板 222 张表各加一把锁，
#      PostgreSQL 锁表被打爆（`out of shared memory`），失败与代码无关但会污染结论。
#
# 另外两个坑也一并封在这里：
#   * **`TEST_DB_TEMPLATE_SCHEMA` 必须 unset**：某些 shell 会话会带着 `=public`，
#     而 `converge_public_schema.sh` 的回退链会取它 ⇒ 参考集变成 public 自身
#     ⇒ `sqlx_cache_tooling_guard_tests::converge_*` 两条假红。
#   * **数据库没准备好就拒绝跑**：`setup_fresh_test_app()` 拿不到库时用例会 `return`，
#     nextest 把它记为 PASS —— 不拦的话这个脚本会给你一个"全绿"的空结果。
#
# 用法：
#   bash scripts/run_full_integration.sh                      # 全量（约 90 min）
#   JOBS=4 bash scripts/run_full_integration.sh               # 改并发（机器强可试 4）
#   bash scripts/run_full_integration.sh -E 'test(api_admin_)'      # 透传给 nextest（别加 `--`）
#   bash scripts/run_full_integration.sh --classify /tmp/xxx.log    # 只复核历史日志
#
# 退出码：0 全绿 / 1 有真实失败 / 2 只有基础设施失败（PG 锁表等，结果不采信）/
#         3 前置检查未通过（库不可达或没有基线表，没跑）
#
set -euo pipefail

cd "$(dirname "$0")/.." # repo root

JOBS="${JOBS:-2}"
LOG_DIR="${LOG_DIR:-/tmp}"
FEATURES_ARGS=(--test integration --all-features --no-fail-fast -j "$JOBS")

# shellcheck source=lib/db_credentials.sh
source scripts/lib/db_credentials.sh

# ── 分类：把 nextest 的失败拆成"基础设施"与"真失败" ─────────────────────────
#
# 按**去重后的用例名**计数，不要按行数：nextest 会把同一条失败打印两遍
# （进度流一次 + 结尾的 failures 汇总一次），按行数会把 7 条报成 14 条。
# 每条基础设施失败的特征是紧跟在 panic 位置之后出现 `out of shared memory`，
# 所以用"用例名 + 后续 3 行"配对，而不是在整份日志里数关键字。
classify() {
    local log="$1"
    local total=0 infra=0 real name
    local fail_names

    # ⚠️ `set -o pipefail` + `grep` 无匹配（退出 1）会让这行**赋值**返回 1，
    # 于是 `set -e` 让整个脚本静默退出——2026-10-22 实测：全绿时脚本直接消失、
    # 一行都不打印。所有"可能没有匹配"的 grep 都必须 `|| true`。
    fail_names="$(grep -E '^ +FAIL \[' "$log" | sed 's/.*integration //' | LC_ALL=C sort -u || true)"

    if [[ -n "$fail_names" ]]; then
        total="$(printf '%s\n' "$fail_names" | grep -c . || true)"
        total="${total:-0}"
        while IFS= read -r name; do
            [[ -z "$name" ]] && continue
            if grep -A 3 -F "thread '${name}'" "$log" | grep -q 'out of shared memory'; then
                infra=$((infra + 1))
            fi
        done <<<"$fail_names"
    fi
    real=$((total - infra))

    {
        echo
        echo "──────── 失败分类 ────────"
        printf '失败总数 : %s（去重后）\n' "$total"
        printf '基础设施 : %s（「out of shared memory」 ⇒ 并发克隆把 PG 锁表打爆，调小 JOBS）\n' "$infra"
        printf '真失败   : %s\n' "$real"
    } >&2

    if [[ "$total" -gt 0 ]]; then
        {
            echo
            echo "全部失败用例："
            printf '%s\n' "$fail_names"
        } >&2
    fi

    if [[ "$real" -gt 0 ]]; then
        echo "⇒ 有 ${real} 条真实失败，结果不能采信。" >&2
        return 1
    fi
    if [[ "$infra" -gt 0 ]]; then
        echo "⇒ 只有基础设施失败，重跑（JOBS=1）即可，不要把它们当成代码问题。" >&2
        return 2
    fi
    echo "⇒ 全绿。" >&2
    return 0
}

# ── 只复核历史日志 ──────────────────────────────────────────────────────────
if [[ "${1:-}" == "--classify" ]]; then
    log="${2:?--classify 需要一个日志路径}"
    [[ -f "$log" ]] || {
        echo "找不到日志：$log" >&2
        exit 3
    }
    classify "$log"
    exit $?
fi

# ── 前置：库必须真的可用，且带着基线表 ──────────────────────────────────────
#
# 这两条不是形式主义：集成用例在"拿不到库"时会**静默 return**，nextest 记 PASS。
# 少了这一步，本脚本会在库挂掉时给你一份 1637 条全绿的假报告。
export TEST_DATABASE_URL="${TEST_DATABASE_URL:-postgresql://synapse:$(db_password)@localhost:5432/synapse_test}"
unset TEST_DB_TEMPLATE_SCHEMA TEMPLATE_SCHEMA

# 先分清"没有 psql"和"连不上库"：两者都被 `psql … >/dev/null 2>&1` 吞成同一句
# "cannot connect"，会把人引去查数据库，而真因是 Homebrew keg-only 没进 PATH。
if ! command -v psql >/dev/null 2>&1; then
    echo "::error::PATH 里没有 psql，无法做前置检查。" >&2
    echo "         Homebrew keg-only 安装需要先把 bin 加进 PATH：" >&2
    echo "           export PATH=\"/opt/homebrew/opt/postgresql@15/bin:\$PATH\"   # Apple Silicon" >&2
    echo "           export PATH=\"/usr/local/opt/postgresql@15/bin:\$PATH\"      # Intel" >&2
    exit 3
fi

# 打印目标库时**去掉凭据**：`postgresql://user:pass@host:port/db` ⇒ `host:port/db`。
# 错误信息会进日志/终端/CI 输出，把口令印出来是最容易被忽略的泄露面。
db_target() { printf '%s' "${TEST_DATABASE_URL##*@}"; }

if ! psql "$TEST_DATABASE_URL" -tAc "SELECT 1" >/dev/null 2>&1; then
    echo "::error::连不上测试库：$(db_target)" >&2
    echo "         先起库并跑迁移（scripts/dev-test-setup.sh up / scripts/ci/prepare_test_db.sh）" >&2
    exit 3
fi

baseline_tables="$(psql "$TEST_DATABASE_URL" -tAc \
    "SELECT count(*) FROM information_schema.tables WHERE table_schema='public' AND table_type='BASE TABLE'" || echo 0)"
if [[ "${baseline_tables:-0}" -lt 100 ]]; then
    echo "::error::public 只有 ${baseline_tables:-0} 张表（< 100）—— 这个库没跑过迁移，用例会静默跳过并记 PASS。" >&2
    echo "         当前目标：$(db_target)" >&2
    echo "         集成测试要的是跑过 v12 基线的 synapse_test。若 shell 里导出了" >&2
    echo "         TEST_DB_PORT / TEST_DB_HOST（例如 scripts/dev-test-setup.sh 的 55432 集群），" >&2
    echo "         db_credentials.sh 会认它们 ⇒ 请 unset，或显式给 TEST_DATABASE_URL。" >&2
    exit 3
fi

mkdir -p "$LOG_DIR"
LOG="$LOG_DIR/full-integration-$(date +%Y%m%d-%H%M%S).log"

{
    echo "==> 全量集成套件"
    echo "    库      : $(db_target)"
    echo "    public  : ${baseline_tables} 张表"
    echo "    参数    : ${FEATURES_ARGS[*]}"
    echo "    日志    : $LOG"
} | tee "$LOG"

# ── 执行 ────────────────────────────────────────────────────────────────────
#
# 透传参数**不能**加 `--`：`cargo nextest` 会把 `--` 之后的当成"给测试二进制"的参数，
# 于是 `-E` 报 `arguments are unsupported`（退出 96）。这里接受两种写法
# （带不带 `--` 都行），并统一直接追加到 nextest 的命令行。
passthrough=("$@")
if [[ "${passthrough[0]:-}" == "--" ]]; then
    passthrough=("${passthrough[@]:1}")
fi

{
    echo "    附加参数: ${passthrough[*]:-<无>}"
} | tee -a "$LOG"

set +e
SQLX_OFFLINE="${SQLX_OFFLINE:-true}" cargo nextest run "${FEATURES_ARGS[@]}" "${passthrough[@]}" 2>&1 | tee -a "$LOG"
nextest_status="${PIPESTATUS[0]}"
set -e

if [[ "$nextest_status" -ne 0 ]] && ! grep -qE '^ +FAIL \[' "$LOG"; then
    # 没有 FAIL 行却不是 0：编译失败 / 参数写错 / nextest 自身出错。
    # 分类器只会看 FAIL 行，这时若不管就会报"全绿"——这是最危险的假绿。
    echo "::error::nextest 退出码 ${nextest_status} 但没有失败用例 ⇒" >&2
    echo "         多为编译失败、参数写错（如把 nextest 的「-E」放在「--」之后）或运行环境问题。" >&2
    echo "         见 $LOG" >&2
    exit 1
fi

classify "$LOG"
