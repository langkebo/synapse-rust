#!/usr/bin/env bash
# 清理测试库累积的隔离 schema（catalog 膨胀治理）。
#
# ── 背景 ─────────────────────────────────────────────────────────────────────
# 测试夹具每次运行都 CREATE SCHEMA 且从不 DROP，本地库累积到 **23,662** 个
# 残留 schema（2026-09-12 实测：test_* 22,543、media_test_* 1,033、
# synapse_test_* 48、陈旧 test_template_v2_* 38）。catalog 膨胀到
# `pg_database_size()` 都会超时的程度，直接拖慢 clone 与所有测试。
#
# 必须每个 schema 单独一个事务（不能 DO $$ 循环或单事务批量）：单个
# DROP SCHEMA ... CASCADE 已接近 max_locks_per_transaction=64 上限，
# 批量会报 "out of shared memory"。
#
# ── 清理范围（全部保留 live 模板）────────────────────────────────────────────
#   test_*                        —— 隔离/克隆 schema
#   media_test_*                  —— synapse-services/src/media/mod.rs 自建
#   synapse_test_*                —— 旧模板 + 就绪标记
#   test_template_v<N>_<hex>      —— 旧指纹模板：**仅删陈旧指纹**
#   test_isolation_template_<hex> —— 现共享模板家族
#                                    （synapse-common/src/test_isolation.rs）
#
# 两个模板家族都只删「不在 keep 集合里」的成员，live 模板由标记文件决定。
# 漏掉 `test_isolation_template_*` 会在 --apply 下删掉 LIVE 共享模板：克隆的
# 列默认值引用模板的序列（`LIKE ... INCLUDING ALL` 复制的是 DEFAULT 表达式），
# 模板被 CASCADE 删除会把所有并发克隆的默认值一起级联掉。
#
# 旧版本只处理 `test_%` 且**无条件保留所有 `test_template%`**，因此另外 3 个
# 家族永远清不掉；同时它的默认连接指向 localhost:15432/synapse_test（Docker
# 端口），在"非同款环境"里会静默连到**另一个数据库**，看起来像"没有残留"。
#
# ── 用法 ─────────────────────────────────────────────────────────────────────
#   bash scripts/cleanup_test_schemas.sh                 # 预演（默认，不改动任何东西）
#   bash scripts/cleanup_test_schemas.sh --apply         # 实际执行
#   DATABASE_URL=postgres://... bash scripts/cleanup_test_schemas.sh --apply
#   bash scripts/cleanup_test_schemas.sh --keep-template test_template_v2_abc...
#   bash scripts/cleanup_test_schemas.sh --keep-template test_isolation_template_abc123...
#   bash scripts/cleanup_test_schemas.sh --keep-all-templates --apply
#
# 连接优先级：DATABASE_URL > TEST_DATABASE_URL > PG* 环境变量 > 默认值。
# 默认值刻意保留 Docker 端口，但**预演会打印实际连接目标**，执行前请核对。
#
# ── §9 兜底机制（CI 无文件系统标记文件时的 live 模板保护）────────────────────
# 脚本按优先级认定 live 模板：
#   1. 标记文件 `synapse_test_template_ready_<schema>`（test_utils.rs 写入）
#   2. 环境变量 `TEST_DB_TEMPLATE_SCHEMA`（CI seed 钉住的名字，如 test_template_ci）
#   3. 两者都缺 → 降级为「保留全部模板家族、仅删克隆 schema」（同 --keep-all-templates）
# 认定为 live 的模板通过 `AND nspname NOT IN (...)` 硬排除，绝不会被 CASCADE 误删。

set -uo pipefail

APPLY=0
KEEP_ALL_TEMPLATES=0
EXTRA_KEEP=()
while [ $# -gt 0 ]; do
    case "$1" in
        --apply) APPLY=1 ;;
        --dry-run) APPLY=0 ;;
        --keep-all-templates) KEEP_ALL_TEMPLATES=1 ;;
        --keep-template)
            shift
            [ $# -gt 0 ] || { echo "ERROR: --keep-template 需要一个 schema 名" >&2; exit 2; }
            EXTRA_KEEP+=("$1")
            ;;
        -h|--help)
            sed -n '2,48p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *) echo "ERROR: 未知参数 $1（见 --help）" >&2; exit 2 ;;
    esac
    shift
done

# ── 解析连接：DATABASE_URL/TEST_DATABASE_URL 优先，回落 PG* ──────────────────
DB_URL="${DATABASE_URL:-${TEST_DATABASE_URL:-}}"
if [ -n "$DB_URL" ]; then
    # psql 直接吃 URL；同时让后续只读查询复用同一目标
    PSQL=(psql -X -q -v ON_ERROR_STOP=0 -d "$DB_URL")
    TARGET_DESC="$DB_URL"
else
    export PGHOST="${PGHOST:-localhost}"
    export PGPORT="${PGPORT:-15432}"
    export PGUSER="${PGUSER:-synapse}"
    export PGDATABASE="${PGDATABASE:-synapse_test}"
    export PGPASSWORD="${PGPASSWORD:-synapse}"
    PSQL=(psql -X -q -v ON_ERROR_STOP=0)
    TARGET_DESC="host=$PGHOST port=$PGPORT user=$PGUSER db=$PGDATABASE"
fi

echo "==> 连接目标: $TARGET_DESC"
if ! "${PSQL[@]}" -tAc "SELECT 1" >/dev/null 2>&1; then
    echo "ERROR: 无法连接上述目标。请设置 DATABASE_URL（或 PG* 变量）指向测试库。" >&2
    exit 1
fi
CURRENT_DB=$("${PSQL[@]}" -tAc "SELECT current_database()" 2>/dev/null)
CURRENT_HOST=$("${PSQL[@]}" -tAc "SELECT inet_server_addr()::text || ':' || inet_server_port()" 2>/dev/null)
echo "    实际连到: db=$CURRENT_DB server=$CURRENT_HOST"

# ── 决定要保留的 live 模板 ───────────────────────────────────────────────────
# 标记目录与 src/test_utils.rs::template_marker_dir() 一致。
# 允许用 SYNAPSE_TEMPLATE_MARKER_DIR 覆盖：本地自定义 CARGO_TARGET_DIR 时，
# 默认的 $CARGO_TARGET_TMPDIR / <repo>/target/tmp 都可能指不到标记文件，
# 导致这里找不到任何 live 模板而中止（§9 的实测痛点）。CI 里则应指向
# 跑测试时实际生成标记的 target 目录，或直接依赖下面的 env / 反查兜底。
MARKER_DIR="${SYNAPSE_TEMPLATE_MARKER_DIR:-${CARGO_TARGET_TMPDIR:-$(cd "$(dirname "$0")/.." && pwd)/target/tmp}/synapse_test_templates}"
KEEP_TEMPLATES=()
if [ "$KEEP_ALL_TEMPLATES" -eq 1 ]; then
    echo "==> --keep-all-templates：保留全部模板 schema"
elif [ -d "$MARKER_DIR" ]; then
    # 标记文件名形如 synapse_test_template_ready_<schema>
    while IFS= read -r m; do
        [ -z "$m" ] && continue
        KEEP_TEMPLATES+=("${m##*synapse_test_template_ready_}")
    done < <(find "$MARKER_DIR" -maxdepth 1 -name 'synapse_test_template_ready_*' -type f 2>/dev/null | sort)
    [ "${#KEEP_TEMPLATES[@]}" -gt 0 ] && \
        echo "==> 由标记文件认定的 live 模板: ${#KEEP_TEMPLATES[@]} 个 ${KEEP_TEMPLATES[*]:-}"
fi
for extra in "${EXTRA_KEEP[@]:-}"; do
    [ -n "$extra" ] && KEEP_TEMPLATES+=("$extra")
done

# TEST_DB_TEMPLATE_SCHEMA 是一个**独立**的保护项（通常来自 CI seed 步骤），
# 必须始终加入 keep 集合——即使标记文件已认定了其他模板。这样能防止 CI 里的
# 纯 shell 种子模板（无 Rust 标记）被误删。
if [ -n "${TEST_DB_TEMPLATE_SCHEMA:-}" ]; then
    HAS_TS=0
    for t in "${KEEP_TEMPLATES[@]:-}"; do
        [ "$t" = "$TEST_DB_TEMPLATE_SCHEMA" ] && HAS_TS=1 && break
    done
    if [ "$HAS_TS" -eq 0 ]; then
        KEEP_TEMPLATES+=("$TEST_DB_TEMPLATE_SCHEMA")
        echo "==> 附加 TEST_DB_TEMPLATE_SCHEMA：$TEST_DB_TEMPLATE_SCHEMA"
    fi
fi

# keep 集合仍为空（无标记文件、无 EXTRA_KEEP、且未设 TEST_DB_TEMPLATE_SCHEMA）时，
# 保守降级为"保留全部模板家族、只删克隆 schema"（等价于 KEEP_ALL_TEMPLATES=1），
# 打印告警而非中止——避免"本地换 target 目录 / CI 无标记"就彻底罢工（§9 痛点）。
#   —— 降级**必须**切到 KEEP_ALL_TEMPLATES=1，不能保留空的 keep 集合：否则
#      KEEP_SQL 退化为 ""，硬排除子句被省略，指纹模板家族会全落入正则分支被删。
if [ "$KEEP_ALL_TEMPLATES" -eq 0 ] && [ "${#KEEP_TEMPLATES[@]}" -eq 0 ]; then
    KEEP_ALL_TEMPLATES=1
    echo "WARN: 找不到任何 live 模板标记（${MARKER_DIR}），且未设 TEST_DB_TEMPLATE_SCHEMA。" >&2
    echo "      为避免误删仍在使用的模板，降级为「保留全部模板家族、仅清理克隆 schema」。" >&2
    echo "      若要连陈旧模板一起删，请显式加 --keep-all-templates（见 --help）。" >&2
fi

# ── 构造候选列表 ─────────────────────────────────────────────────────────────
# KEEP_SQL is the literal list of schema names to protect. It must include every
# name in KEEP_TEMPLATES (marker files + TEST_DB_TEMPLATE_SCHEMA fallback +
# EXTRA_KEEP), because those are hard exclusions — see the CANDIDATE_SQL note
# below about why the family-regex predicate alone is NOT enough.
KEEP_SQL=""
if [ "$KEEP_ALL_TEMPLATES" -eq 0 ] || [ "${#KEEP_TEMPLATES[@]}" -gt 0 ]; then
    for t in "${KEEP_TEMPLATES[@]:-}"; do
        [ -n "$t" ] && KEEP_SQL="${KEEP_SQL}${KEEP_SQL:+,}'$t'"
    done
fi

# 模板家族仅匹配指纹形态，避免误伤任意命名的模板：
#   test_template_v<N>_<hex>        —— 旧 storage 家族
#   test_isolation_template_<hex>   —— 现 shared 模块家族
# TEMPLATE_PREDICATE 只负责「指纹模板家族」的 keep 判定：命中家族且不在 keep 集合
# 才删，不命中家族的（普通克隆）一律是候选。
if [ "$KEEP_ALL_TEMPLATES" -eq 1 ]; then
    TEMPLATE_PREDICATE="(nspname !~ '^test_template_v[0-9]+_[0-9a-f]{16}\$' AND nspname !~ '^test_isolation_template_[0-9a-f]{16}\$')"
else
    if [ -n "$KEEP_SQL" ]; then
        TEMPLATE_PREDICATE="((nspname !~ '^test_template_v[0-9]+_[0-9a-f]{16}\$' AND nspname !~ '^test_isolation_template_[0-9a-f]{16}\$') OR nspname NOT IN ($KEEP_SQL))"
    else
        TEMPLATE_PREDICATE="(nspname !~ '^test_template_v[0-9]+_[0-9a-f]{16}\$' AND nspname !~ '^test_isolation_template_[0-9a-f]{16}\$')"
    fi
fi

# ⚠️ HARD EXCLUSION — this is the §9 fix.
# `test_template_ci`（CI seed 步骤钉住的模板，见 §1）匹配 `test\_%`，但**不**匹配任何
# 指纹家族正则，所以对 TEMPLATE_PREDICATE 的第一个 OR 分支恒为真 → 它会成为删除候选，
# 而 keep 集合（含 TEST_DB_TEMPLATE_SCHEMA）在旧逻辑里根本拦不住它，CASCADE 会把整个
# 套件依赖的共享模板删掉。因此 keep 名单必须作为 `AND nspname NOT IN (...)` 的硬排除，
# 叠加在最外层，而不是只塞进那个会被家族正则 OR 短路掉的谓词里。
KEEP_EXCLUDE=""
if [ -n "$KEEP_SQL" ]; then
    KEEP_EXCLUDE="AND nspname NOT IN ($KEEP_SQL)"
fi

CANDIDATE_SQL="
SELECT nspname FROM pg_namespace
WHERE (
        nspname LIKE 'test\_%'
     OR nspname LIKE 'media\_test\_%'
     OR nspname LIKE 'synapse\_test\_%'
      )
  AND $TEMPLATE_PREDICATE
  $KEEP_EXCLUDE
  AND nspname NOT IN ('public','information_schema')
ORDER BY nspname;"

SCHEMAS=$("${PSQL[@]}" -tAc "$CANDIDATE_SQL" 2>/dev/null)
TOTAL=$(printf '%s\n' "$SCHEMAS" | grep -c . || true)
echo "==> 待清理: $TOTAL 个 schema"

if [ "$TOTAL" -eq 0 ]; then
    echo "==> 无残留 schema，无需清理。"
    exit 0
fi

if [ "$APPLY" -eq 0 ]; then
    echo "==> 预演模式（未做任何改动）。抽样前 10 个："
    printf '%s\n' "$SCHEMAS" | head -10 | sed 's/^/    /'
    echo "    确认无误后加 --apply 实际执行。"
    exit 0
fi

# ── 执行 ─────────────────────────────────────────────────────────────────────
COUNT=0
FAILED=0
FIRST_ERROR=""
# Consecutive `out of shared memory` failures. This is not a transient error: a
# single DROP SCHEMA takes one lock per cascaded object, so a schema with more
# objects than `max_locks_per_transaction` can NEVER be dropped this way, and
# each attempt just burns ~12s. Bail out with an actionable message instead of
# grinding through the whole list (observed on 2026-09-12: 25 template schemas
# at 1,197 objects each, every one failing after 12s).
LOCK_FAILURES=0
while IFS= read -r s; do
    [ -z "$s" ] && continue
    # 每个 schema 单独一个事务；失败时**保留 stderr**（旧版本 2>/dev/null 把
    # 失败原因全丢了，只剩一个 WARN）。
    if ! ERR=$("${PSQL[@]}" -c "DROP SCHEMA \"$s\" CASCADE" 2>&1 >/dev/null); then
        FAILED=$((FAILED + 1))
        if [ -z "$FIRST_ERROR" ]; then
            FIRST_ERROR="$s: $ERR"
        fi
        [ "$FAILED" -le 5 ] && echo "    WARN: 清理失败 $s — $ERR"
        case "$ERR" in
            *"out of shared memory"*|*max_locks_per_transaction*)
                LOCK_FAILURES=$((LOCK_FAILURES + 1))
                if [ "$LOCK_FAILURES" -ge 3 ]; then
                    echo "" >&2
                    echo "ERROR: 连续 ${LOCK_FAILURES} 个 schema 因锁表耗尽而无法 DROP：" >&2
                    echo "       ERROR: out of shared memory ... increase max_locks_per_transaction" >&2
                    echo "" >&2
                    echo "       单个 DROP SCHEMA ... CASCADE 会对被级联的**每个对象**各持一把锁。" >&2
                    echo "       对象数超过 max_locks_per_transaction（本机 256）的 schema 用这种方式" >&2
                    echo "       永远删不掉，每次只是白等十几秒。" >&2
                    echo "" >&2
                    echo "       推荐改用重建测试库（秒级，且天然绕过锁限制）：" >&2
                    echo "         DROP DATABASE <db>;  CREATE DATABASE <db>;  # 然后重放 migrations/" >&2
                    echo "       或提高 max_locks_per_transaction 后重启 PostgreSQL。" >&2
                    echo "       详见 docs/audit/P5_test_schema_accumulation_2026-09-12.md §4.2。" >&2
                    break
                fi
                ;;
            *)
                LOCK_FAILURES=0
                ;;
        esac
    else
        LOCK_FAILURES=0
    fi
    COUNT=$((COUNT + 1))
    if [ $((COUNT % 500)) -eq 0 ]; then
        echo "    进度: $COUNT / ${TOTAL}（失败 ${FAILED}）"
    fi
done <<<"$SCHEMAS"

echo "==> 清理完成：尝试 $COUNT 个（失败 $FAILED 个）"
if [ -n "$FIRST_ERROR" ]; then
    echo "    首个失败原因: $FIRST_ERROR"
fi
REMAIN=$("${PSQL[@]}" -tAc "$CANDIDATE_SQL" 2>/dev/null | grep -c . || true)
echo "    剩余残留 schema: $REMAIN"
[ "$FAILED" -eq 0 ] || exit 1
