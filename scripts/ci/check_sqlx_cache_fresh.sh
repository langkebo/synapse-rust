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
#
#   --compile（无需数据库，需要一次 cargo check）
#       断言 `SQLX_OFFLINE=true cargo check --all-targets` 通过 —— 这是"缓存完整"
#       的**权威**证明：SQL 文本变化会产生新哈希，缺条目即编译失败。
#
#   --full（需要 `cargo sqlx` 与 `DATABASE_URL`）
#       执行 `cargo sqlx prepare --check --workspace`：与数据库核对每条 `query!`
#       后判定缓存是否需要变化。**只在有已迁移数据库的环境跑**（本地、或
#       backend-validation 这类带 postgres service 的 job）。
#
# 说明：`SQLX_OFFLINE=true cargo check --all-targets` 本身也能抓出"缺条目"
# （SQL 文本变化 → 查询哈希变化 → 离线元数据缺失 → 编译失败）。本脚本的价值在于
# ① 在 CI 里用**零成本**的 --static 抓住"缓存被删/未跟踪/为空"；
# ② 用 --full 抓住"缓存里存在但已过期"（离线编译发现不了的反方向）。
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
        -h|--help)
            sed -n '2,32p' "${BASH_SOURCE[0]}"
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

    # 注意：**不能**用"静态宏调用数 <= 缓存条目数"作为判据 —— 多处调用写同一条
    # SQL 文本时只生成一个哈希条目，调用点天然可能多于条目。缓存是否**完整**由
    # `SQLX_OFFLINE=true` 构建来证明（SQL 文本变化 → 哈希变化 → 缺条目 → 编译失败），
    # 见 `--compile` 模式；本模式只守住"缓存存在/非空/被跟踪"这个零成本不变量。
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
    if ! cargo sqlx --version >/dev/null 2>&1; then
        echo "ERROR: --full 需要 sqlx-cli（cargo install sqlx-cli --locked --no-default-features --features postgres,rustls）" >&2
        exit 1
    fi
    if [[ -z "${DATABASE_URL:-}" ]]; then
        echo "ERROR: --full 需要指向**已迁移**数据库的 DATABASE_URL" >&2
        exit 1
    fi
    echo "==> cargo sqlx prepare --check --workspace（需要数据库）"
    cargo sqlx prepare --check --workspace
    echo "OK: .sqlx 与数据库元数据一致"
fi

echo ".sqlx cache: OK"
