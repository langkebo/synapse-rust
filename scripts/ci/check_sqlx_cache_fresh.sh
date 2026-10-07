#!/usr/bin/env bash
#
# CI/local gate: `.sqlx` 离线缓存必须与源码一致。
#
# ── 为什么需要 ────────────────────────────────────────────────────────────────
#
# CI 的部分 job 用 `SQLX_OFFLINE: "true"` 构建（见 `.github/workflows/ci.yml`）。
# 一旦有人新增/修改 `query!` / `query_as!` / `query_scalar!` / `query_file!` 而
# 没有把对应的 `.sqlx/` 元数据一起提交，这些 job 会在 `no cached data` 上失败——
# 报错发生在**远端 CI**，而不是提交时。本脚本把这件事提前到本地/Repo Sanity。
#
# ── 两种模式 ─────────────────────────────────────────────────────────────────
#
#   --static（默认，无需数据库、无编译）
#       断言 `.sqlx/` 存在、非空、且被 git 跟踪（缓存是**版本控制产物**，
#       不是构建缓存）。这是 CI 中可安全执行的部分。
#       并做**内容新鲜度断言**（COMPAT-06）：`.sqlx/query-<hash>.json` 的
#       `<hash>` = `sha256(源码中该查询的 SQL 字面量)`，逐条把源码字面量映射成
#       "应有的缓存文件名"，缺失即"改了 SQL 却没重生成缓存"。非常量实参
#       （`concat!` / `format!` / 变量）静态无法求值，一律跳过、绝不误报。
#
#   --compile（无需数据库，需要一次 cargo check）
#       断言 `SQLX_OFFLINE=true cargo check --all-targets` 通过 —— 这是"缓存完整"
#       的**权威**证明：SQL 文本变化会产生新哈希，缺条目即编译失败。
#
#   --full（需要 `cargo sqlx` 与已迁移的 DATABASE_URL）
#       `exec bash scripts/ci/sqlx_prepare.sh --check` 的别名 —— 前置检查、feature 集
#       与失败语义都只有一份实现（铁律 2）。**本仓日常不用它**：只用 `--static` +
#       `--compile`（见下方环境事实）。CI 也从不调用它（`ci.yml` 只跑默认的 `--static`）。
#
# ⚠️ 2026-09-26 环境事实（D-75/D-77）：共享 `synapse_test.public` 会被并发会话的
#   D-57② 收敛清空（实测 0 表）。此时 `--full` 不是"报缓存过期"，而是**上千个误导性
#   编译错误**（实测 1443 个 E0282/E0277，看起来像源码坏了）—— 因为 `prepare --check`
#   要对着真库逐条 describe，而没有表就没有列元数据。⇒ 本仓**禁止**把 `--full` 排进
#   CI 或任务清单；`sqlx_prepare.sh` 现在会先做前置检查、不满足即 fail fast。
#   `.sqlx` 的完整性由 `--compile` 权威证明；写入唯一入口是 `scripts/ci/sqlx_prepare.sh`。
#
# 说明：`SQLX_OFFLINE=true cargo check --all-targets` 本身也能抓出"缺条目"
# （SQL 文本变化 → 查询哈希变化 → 离线元数据缺失 → 编译失败）。本脚本的价值在于
# ① 在 CI 里用**零成本**的 --static 抓住"缓存被删/未跟踪/为空/**陈旧**"；
# ② 用 --compile 抓住"缺条目/条目对不上"（离线编译发现不了的反方向需要 --full，
#   而 --full 需要一个已迁移的库，本环境默认不满足）。
#
# Exits 0 on success, 1 on any violation.

set -eu

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

CACHE_DIR=".sqlx"
MODE="static"
for arg in "$@"; do
    case "${arg}" in
        --static) MODE="static" ;;
        --full) MODE="full" ;;
        --compile) MODE="compile" ;;
        -h | --help)
            sed -n '2,45p' "${BASH_SOURCE[0]}"
            exit 0
            ;;
        *)
            echo "ERROR: 未知参数 ${arg}（支持 --static / --full / --compile）" >&2
            exit 2
            ;;
    esac
done

echo "==> .sqlx 离线缓存新鲜度（模式: ${MODE}）"

FAILURES=0
fail() {
    echo "FAIL: $*" >&2
    FAILURES=$((FAILURES + 1))
}

# ---------------------------------------------------------------------------
# 1) 静态不变量：缓存必须存在、非空、被 git 跟踪
# ---------------------------------------------------------------------------
if [[ ! -d "${CACHE_DIR}" ]]; then
    fail "缺少 ${CACHE_DIR}/：CI 的 SQLX_OFFLINE=true job 会在 no cached data 上失败"
    echo "      修复：cargo sqlx prepare --workspace" >&2
else
    count=$(find "${CACHE_DIR}" -maxdepth 1 -name 'query-*.json' | wc -l | tr -d ' ')
    if [[ "${count}" -eq 0 ]]; then
        fail "${CACHE_DIR}/ 存在但没有 query-*.json 条目"
    else
        echo "OK: ${CACHE_DIR}/ 含 ${count} 条查询元数据"
    fi

    tracked=$(git ls-files "${CACHE_DIR}" | wc -l | tr -d ' ')
    if [[ "${tracked}" -eq 0 ]]; then
        fail "${CACHE_DIR}/ 未被 git 跟踪（缓存是版本控制产物，不是构建缓存）"
        echo "      提示：检查 .gitignore 是否把 .sqlx/ 排除了。" >&2
    else
        echo "OK: ${CACHE_DIR}/ 已被 git 跟踪（${tracked} 个文件）"
    fi

    # 内容新鲜度断言（COMPAT-06）：源码里的每个静态查询字面量都必须在 `.sqlx/`
    # 找到对应哈希条目（`query-<sha256(SQL)>.json`）。缺失 = 改了 SQL 却没重生成
    # 缓存 → CI 的 SQLX_OFFLINE job 会在"no cached data"上失败；本断言把它提前到
    # 本地/Repo Sanity。缓存**完整性**仍由 `--compile`（SQLX_OFFLINE 构建）权威证明：
    # 本模式只做"字面量 → 文件名"的零成本静态映射，非常量实参跳过，绝不误报。
    #
    # 仅在 --static 模式运行：CI 的缺口正是"只跑 --static"（COMPAT-06）。--full 已由
    # `sqlx_prepare.sh --check` 对着真库做权威内容核对，--compile 已由离线构建证明
    # 条目完整，两者都不需要、也不该重复这条零成本静态断言。
    if [[ "${MODE}" == "static" ]]; then
        if command -v python3 >/dev/null 2>&1; then
            if ! python3 scripts/ci/sqlx_query_census.py \
                --check-cache-fresh "${ROOT_DIR}"; then
                fail "源码查询指纹与 ${CACHE_DIR}/ 缓存不一致（缓存陈旧/未重新生成，见上）"
            fi
        else
            echo "注意：未找到 python3，跳过 .sqlx 内容新鲜度断言（存在性检查已通过）" >&2
        fi
    fi

    # 注意：**不能**用"静态宏调用数 <= 缓存条目数"作为判据 —— 多处调用写同一条
    # SQL 文本时只生成一个哈希条目，调用点天然可能多于条目。上面的指纹断言是按
    # **SQL 文本**去重后比对，天然规避了这一点。
fi

if [[ "${FAILURES}" -gt 0 ]]; then
    echo ".sqlx cache: FAIL (${FAILURES} 项)" >&2
    exit 1
fi

# ---------------------------------------------------------------------------
# 2) 可选全量模式：与数据库核对（需要 sqlx-cli + DATABASE_URL）
# ---------------------------------------------------------------------------
if [[ "${MODE}" == "compile" ]]; then
    echo "==> SQLX_OFFLINE=true cargo check --workspace --all-targets（证明缓存完整）"
    SQLX_OFFLINE=true cargo check --workspace --features test-utils --all-features --all-targets --locked
    echo "OK: 离线构建通过，缓存覆盖全部已编译的 query! 调用点"
fi

if [[ "${MODE}" == "full" ]]; then
    # 与写入模式共用同一套前置检查与 feature 集：护栏只有一份实现（铁律 2 / D-77）。
    # `-h` 的 sed 范围（2,32p）覆盖本行上方的模式说明。
    exec bash scripts/ci/sqlx_prepare.sh --check
fi

echo ".sqlx cache: OK"
