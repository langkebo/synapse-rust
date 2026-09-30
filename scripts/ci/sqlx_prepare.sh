#!/usr/bin/env bash
#
# 唯一允许的 `.sqlx` 重生成 / 核对入口（AGENTS.md **R2**）。
#
# ⚠️ **禁止裸 `cargo sqlx prepare`。** 本脚本就是它的替代品。
#
# ── 为什么必须有一层包装（D-77）──────────────────────────────────────────────
#
# `cargo sqlx prepare` 的 destination **就是 `.sqlx/` 本身**，而且**先清空再重写**。
# 它对着 `DATABASE_URL` 指向的库逐条重新 describe SQL；若该库解析到的 schema 里没有
# 基线表，describe 得到的是空结果 —— 缓存会被写坏甚至清空，而 `.cargo/config.toml` 的
# `[env] SQLX_OFFLINE = "true"` 让**所有**构建都依赖这份缓存 ⇒ 一次误跑就是 D-51 同型的
# "整个 CI 编译失败"。
#
# 2026-09-26 的实测事故：共享 `synapse_test.public` 被并发会话清成 **0 表**（D-75），
# 于是 `check_sqlx_cache_fresh.sh --full`（= `prepare --check`）对着它跑出 **1443 个
# E0282/E0277** —— 报错看起来像源码坏了，实际只是"没有表可以 describe"。
#
# ── 三道护栏（全部 fail closed）──────────────────────────────────────────────
#   0. `DATABASE_URL` 必须**显式**给出（脚本刻意没有默认值：默认一个库正是"对着空
#      schema 跑"的成因）。
#   1. 解析到的 schema（`current_schema()`）必须有 ≥ MIN_TABLES 张 BASE TABLE，且含
#      核心基线表 `events` / `rooms` / `users`。满足不了就 fail fast，并说明环境事实
#      （而不是进入几十秒的编译、再吐出上千个误导性错误）。**这一道依赖 `psql`**；
#      环境里没有 `psql`（或沙箱不允许执行）时，可以用一个**已迁移好的库**并设
#      `SQLX_PREPARE_SKIP_DB_CHECK=1` 显式跳过它 —— 之所以能安全跳过，是因为真正的
#      不变量是第 3 道（缩容即回滚），前置检查只是"更快、更早地失败"。
#      `--check` 模式**不允许**跳过（它的语义就是与真库核对）。
#   2. feature 集固定 `--all-features`（R2）：用枚举 feature 会漏掉门控模块，非
#      `--check` 的 prepare 会把它们的条目**剪掉**（D-51 / C6 教训）。
#   3. 写入前先快照 `.sqlx/`；写完后条目数**减少**即打印被删清单、**回滚快照**并失败
#      （除非 `ALLOW_CACHE_SHRINK=1` —— 只有"确实删除了查询"时才允许）。
#
# ── 用法 ─────────────────────────────────────────────────────────────────────
#   # 重生成缓存（唯一允许的写入路径）
#   DATABASE_URL=postgresql://synapse:synapse@localhost:5432/<已迁移库> \
#     bash scripts/ci/sqlx_prepare.sh
#
#   # 只核对（`check_sqlx_cache_fresh.sh --full` 就是委托到这里）
#   DATABASE_URL=… bash scripts/ci/sqlx_prepare.sh --check
#
#   # 确实删除了查询、缓存需要缩容
#   ALLOW_CACHE_SHRINK=1 DATABASE_URL=… bash scripts/ci/sqlx_prepare.sh
#
#   # 环境里没有 psql（或沙箱不允许执行它），但手上有一个**已迁移好**的库：
#   SQLX_PREPARE_SKIP_DB_CHECK=1 DATABASE_URL=… bash scripts/ci/sqlx_prepare.sh
#   # 只跳过 schema 前置检查；快照 + 缩容回滚仍在。psql 在本机 Homebrew 的绝对路径是
#   # /opt/homebrew/opt/postgresql@15/bin/psql（不在 PATH 上时直接用它）。
#   # 注意：`cargo sqlx prepare` 自己只走 Rust 驱动，**不需要 psql**。
#
# 没有"已迁移的库"时，先建一个（不要用共享的 `synapse_test.public` —— 它会被并发
# seed 收敛/重建）：
#   createdb synapse_prepare && \
#   TEST_DATABASE_URL=postgresql://synapse:synapse@localhost:5432/synapse_prepare \
#     RESET_PUBLIC=0 TARGET_SCHEMA=public bash scripts/init_test_public_schema.sh
# 另一个现成的已迁移 schema 是 CI seed 刚重建的模板（会被并发 seed 重建，注意时机）：
#   DATABASE_URL='postgresql://synapse:synapse@localhost:5432/synapse_test?options=-csearch_path%3Dtest_template_ci'
#
# 退出码：0 = 缓存已生成/一致；1 = 前提不满足 / prepare 失败 / 缓存被缩容（已回滚）。
set -euo pipefail

cd "$(dirname "$0")/../.." # repo root (scripts/ci -> repo root)

CACHE_DIR=".sqlx"
MODE="write"
MIN_TABLES="${MIN_TABLES:-100}"

for arg in "$@"; do
    case "$arg" in
        --check) MODE="check" ;;
        -h | --help)
            sed -n '2,59p' "${BASH_SOURCE[0]}"
            exit 0
            ;;
        *)
            echo "ERROR: 未知参数 ${arg}（只支持 --check；本脚本不接受 prepare 的其它旗标）" >&2
            exit 2
            ;;
    esac
done

echo "==> .sqlx 缓存入口（模式: ${MODE}）"

# ── 护栏 0：DATABASE_URL 必须显式给出 ────────────────────────────────────────
if [[ -z "${DATABASE_URL:-}" ]]; then
    cat >&2 <<'MSG'
::error::必须显式给出 DATABASE_URL —— 本脚本刻意没有默认值。
        默认某个库正是"对着空 schema 生成缓存"的成因（D-77：prepare 会先清空 .sqlx 再重写）。
        用法：
          DATABASE_URL=postgresql://synapse:synapse@localhost:5432/<已迁移库> \
            bash scripts/ci/sqlx_prepare.sh
        没有已迁移的库时，按脚本头部注释建一个（不要指向共享的 synapse_test.public）。
MSG
    exit 1
fi

# 护栏 1 是"快速失败"的便利项，真正的不变量是第 3 道（写完缩容即回滚）。因此允许
# 操作者在**没有 psql** 的环境里显式跳过它 —— 前置条件是"库已迁移"这个断言，跳过即由
# 操作者承担；缓存仍受快照/回滚保护。`--check` 不在此列：它的语义就是"与真库核对"，
# 没有 psql 无法核对，必须 fail closed。
SKIP_DB_CHECK="${SQLX_PREPARE_SKIP_DB_CHECK:-0}"
if [[ "$SKIP_DB_CHECK" == "1" && "$MODE" == "check" ]]; then
    echo "::error::SQLX_PREPARE_SKIP_DB_CHECK=1 只允许用于写入模式；--check 必须真连库核对" >&2
    exit 1
fi

if [[ "$SKIP_DB_CHECK" == "1" ]]; then
    echo "WARN: SQLX_PREPARE_SKIP_DB_CHECK=1 —— 跳过 schema 前置检查（操作者断言该库已迁移）。" >&2
    echo "      缩容保护仍然生效：写完若条目数减少会打印被删清单并**回滚**。" >&2
else
    if ! command -v psql >/dev/null 2>&1; then
        cat >&2 <<'MSG'
::error::需要 psql 来核对 DATABASE_URL 指向的 schema 是否已迁移（它不在 PATH 上时可用绝对路径，
         例如本机 Homebrew：/opt/homebrew/opt/postgresql@15/bin/psql）。
         若环境里确实没有 psql（或沙箱不允许执行），两条出路：
           * 用一个**已经迁移好**的库（例如别人备好的），并设
             SQLX_PREPARE_SKIP_DB_CHECK=1 —— 只跳过前置检查，快照/缩容回滚仍在；
           * 或把 psql 的目录加进 PATH 后重跑。
         注意：`cargo sqlx prepare` 自己只走 Rust 驱动，**不需要 psql**；psql 只服务本检查。
MSG
        exit 1
    fi
fi

if ! cargo sqlx --version >/dev/null 2>&1; then
    echo "ERROR: 需要 sqlx-cli（cargo install sqlx-cli --locked --no-default-features --features postgres,rustls）" >&2
    exit 1
fi

# ── 护栏 1：schema 必须真的是迁移后的基线（可用 SKIP 显式跳过，见上）──────────
if [[ "$SKIP_DB_CHECK" != "1" ]]; then
    if ! psql "$DATABASE_URL" -tAc "SELECT 1" >/dev/null 2>&1; then
        echo "::error::无法连接 DATABASE_URL（${DATABASE_URL}）" >&2
        exit 1
    fi

    schema_name="$(psql "$DATABASE_URL" -tAc "SELECT current_schema()")"
    tables="$(psql "$DATABASE_URL" -tAc \
        "SELECT count(*) FROM information_schema.tables WHERE table_schema = current_schema() AND table_type = 'BASE TABLE'")"
    core_missing="$(psql "$DATABASE_URL" -tAc \
        "SELECT count(*) FROM (VALUES ('events'), ('rooms'), ('users')) AS v(t)
          WHERE NOT EXISTS (SELECT 1 FROM information_schema.tables
                            WHERE table_schema = current_schema() AND table_name = v.t)")"

    if [[ "${tables:-0}" -lt "$MIN_TABLES" || "${core_missing:-1}" != "0" ]]; then
        cat >&2 <<MSG
::error::DATABASE_URL 解析到的 schema '${schema_name:-?}' 不满足前置条件：
         BASE TABLE 数 = ${tables:-0}（要求 >= ${MIN_TABLES}），缺失的核心基线表数 = ${core_missing:-?}（要求 0）。

         环境事实（2026-09-26，D-75/D-77）：共享 \`synapse_test.public\` 会被并发会话的
         D-57② 收敛清空（实测 0 表），此时 \`prepare\` 得不到任何列元数据 —— 实测
         \`--full\` 会吐出 **1443 个 E0282/E0277**，看起来像源码坏了。

         ⇒ 换一个**已迁移**的库/schema：
            * 私有 scratch 库：按本脚本头部注释 createdb + init_test_public_schema.sh；
            * CI 刚重建的模板：
              DATABASE_URL='postgresql://…/synapse_test?options=-csearch_path%3Dtest_template_ci'
         本脚本已拒绝执行，未改动 ${CACHE_DIR}/。
MSG
        exit 1
    fi

    echo "OK: 目标 schema '${schema_name}'（${tables} 张 BASE TABLE，核心表齐备）"
fi

PREPARE_ARGS=(--workspace -- --all-features --locked)

# ── --check：只核对（不写缓存）───────────────────────────────────────────────
if [[ "${MODE}" == "check" ]]; then
    echo "==> cargo sqlx prepare --check ${PREPARE_ARGS[*]}"
    cargo sqlx prepare --check "${PREPARE_ARGS[@]}"
    echo "OK: .sqlx 与数据库元数据一致"
    exit 0
fi

# ── 写入模式：快照 → prepare → 缩容保护 ─────────────────────────────────────
count_entries() { find "${CACHE_DIR}" -maxdepth 1 -name 'query-*.json' | wc -l | tr -d ' '; }

before="$(count_entries)"
snapshot="$(mktemp -d)"
cleanup() { rm -rf "$snapshot"; }
trap cleanup EXIT
cp -a "${CACHE_DIR}/." "$snapshot/"

restore_cache() {
    rm -f "${CACHE_DIR}"/query-*.json
    cp -a "${snapshot}/." "${CACHE_DIR}/"
}

echo "==> cargo sqlx prepare ${PREPARE_ARGS[*]}（当前缓存 ${before} 条）"
if ! SQLX_OFFLINE=false cargo sqlx prepare "${PREPARE_ARGS[@]}"; then
    restore_cache
    echo "::error::prepare 失败 —— 已回滚 .sqlx/（${before} 条），缓存未被破坏" >&2
    exit 1
fi

after="$(count_entries)"
if [[ "${after}" -lt "${before}" ]]; then
    removed="$(comm -23 <(cd "$snapshot" && ls query-*.json | LC_ALL=C sort) \
        <(cd "${CACHE_DIR}" && ls query-*.json | LC_ALL=C sort))"
    if [[ "${ALLOW_CACHE_SHRINK:-0}" != "1" ]]; then
        restore_cache
        {
            echo "::error::prepare 之后缓存从 ${before} 条降到 ${after} 条（差 $((before - after)) 条）—— 已回滚。"
            echo "         缓存缩容必须是有意的：确认这些查询确实被删了，再设 ALLOW_CACHE_SHRINK=1 重跑。"
            echo "         被删条目："
            printf '%s\n' "$removed" | sed 's/^/           - /'
        } >&2
        exit 1
    fi
    echo "WARN: 缓存缩容 ${before} → ${after} 条（ALLOW_CACHE_SHRINK=1 已显式允许）；被删条目："
    printf '%s\n' "$removed" | sed 's/^/        - /'
else
    echo "OK: 缓存 ${before} → ${after} 条（未缩容；新增 $((after - before)) 条）"
fi

echo "==> 别忘了把 .sqlx/ 增量与源码一起提交（R2），并用 --compile 复核："
echo "    git status --short .sqlx | head"
echo "    bash scripts/ci/check_sqlx_cache_fresh.sh --compile"
