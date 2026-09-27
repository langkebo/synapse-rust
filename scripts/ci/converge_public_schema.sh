#!/usr/bin/env bash
# D-57②: converge the shared `public` schema onto the migration baseline.
#
# ## Why
#
# `public` is built by `scripts/init_test_public_schema.sh` with `RESET_PUBLIC=0`: the
# baseline is a `CREATE TABLE IF NOT EXISTS` merge script and the apply is idempotent,
# so anything *deleted from the baseline* stays behind in a long-lived database. Those
# leftovers make "the table/column exists" assertions pass against a schema the
# migrations no longer describe (D-57, docs/audit/SQLX_STATICIZATION_PLAN_2026-09-23.md
# §7.2), and they silently pollute the shared `public` that `connect_shared_test_pool`
# hands to storage `db_tests`.
#
# `DROP SCHEMA public CASCADE` is **not** an option: it also removes objects in *other*
# schemas that depend on public's extensions (measured 2026-09-19: the `gin_trgm_ops`
# indexes on the isolation templates), silently degrading a template that still carries
# its ready marker. So this script drops **only the objects that are not in the
# baseline**, one by one, with `CASCADE` limited to those objects' dependents.
#
# ## How the expected object set is derived (no hard-coded list to drift)
#
# `prepare_test_db.sh` rebuilds `test_template_ci` from the *same* migration set in the
# step right before this one. That schema is therefore an exact, freshly-built copy of
# the baseline's object set — so it is used as the reference. No list is maintained here,
# and a baseline change needs no edit in this file.
#
# ## Safety rails (all fail closed)
#
# 1. The reference schema must exist and hold >= MIN_TABLES tables. Without this, an
#    empty/stale reference would make "everything in public is extra" true and the
#    script would drop the whole baseline.
# 2. The target database name must contain "test" (same rule as the DB-name guard in
#    `src/test_utils.rs`). Override only deliberately, with
#    `CONVERGE_ALLOW_NON_TEST_DB=1` — never in CI.
# 3. Extension-owned objects in `public` are never candidates (they are not the
#    baseline's to manage).
# 4. Views/materialized views are dropped before sequences and tables, so ordinary
#    dependency order is respected; `IF EXISTS` keeps a CASCADE-removed dependent from
#    turning into an error.
# 5. After the apply, the script re-runs the diff and **fails** if anything is left.
#
# ── 2026-09-26：TOCTOU 事故与新增栏杆 6–8（D-75）─────────────────────────────
#
# 事故：`synapse_test.public` 曾被清成 **0 表**（本该有 ~220 张；`prepare_test_db.sh`
# 的 [4/4] 步也断言 >=200）。成因是**本脚本与 `prepare_test_db.sh` 第 [2/4] 步的竞态**：
# 那一步 `DROP SCHEMA test_template_ci CASCADE` 之后要花数十秒重建参考 schema，
# 而本脚本的删除清单是在 rail 1 之后**又一次**现场求值 `$DIFF_SQL`（在 apply 的 heredoc 里）
# ⇒ 参考集此刻为空/半成品时，public 的**全部**对象都被判为"多余"并逐个 DROP。
# 事后不变量（rail 5）当时也用**同一个已塌掉的参考集**求值：`extra=0 / missing=0`
# 两边同时退化成 0 ⇒ 恒过，事故被静默放行（日志里只有一行 `public tables=0 / 参考=220`）。
#
# 修法（结构性，不靠"操作时小心"）：
#   6. **冻结删除清单**：`$DIFF_SQL` 只在 rail 1 之后求值一次，清单落盘后由 apply
#      从该文件消费（`\copy` + `\gexec`），apply 期间**不再**咨询参考 schema。
#   7. **参考稳定性复检 + 大规模删除栏杆**：apply **前**重新测量参考 schema，若表数
#      低于 rail 1 的测量值（= 参考正在被并发重建/已消失）⇒ 拒绝执行、一个对象都不删；
#      另外，若清单执行后 public 的存活对象数会**少于参考对象数** ⇒ 拒绝（可用
#      `CONVERGE_ALLOW_MASS_DELETE=1` 显式放行，仅用于刻意清空的场景）。
#   8. **非空不变量**：收敛后除了双向 diff 为 0，还要求参考 schema 仍 >= rail 1 的
#      表数、且 public 的 BASE TABLE 数 >= MIN_TABLES —— 否则"参考消失"会让 rail 5
#      重新退化成恒真。
#
# 用法：
#   bash scripts/ci/converge_public_schema.sh                # 收敛（CI seed 用的默认）
#   CONVERGE_MODE=report bash scripts/ci/converge_public_schema.sh   # 只打印将删对象（dry-run）
#   TEMPLATE_SCHEMA=other_template bash scripts/ci/converge_public_schema.sh
#   TEST_DATABASE_URL=postgresql://… bash scripts/ci/converge_public_schema.sh
#
# 退出码：0 = 已收敛（或 report 模式跑完）；1 = 安全栏杆触发 / 收敛后仍有残留。
set -euo pipefail

cd "$(dirname "$0")/../.." # repo root (scripts/ci -> repo root)

export TEST_DATABASE_URL="${TEST_DATABASE_URL:-postgresql://synapse:synapse@localhost:5432/synapse_test}"
TEMPLATE_SCHEMA="${TEMPLATE_SCHEMA:-${TEST_DB_TEMPLATE_SCHEMA:-test_template_ci}}"
MODE="${CONVERGE_MODE:-apply}"
MIN_TABLES="${MIN_TABLES:-100}"

case "$MODE" in
    apply | report) ;;
    *)
        echo "::error::CONVERGE_MODE must be 'apply' or 'report' (got '$MODE')" >&2
        exit 1
        ;;
esac

if ! psql "$TEST_DATABASE_URL" -tAc "SELECT 1" >/dev/null 2>&1; then
    echo "::error::cannot connect to $TEST_DATABASE_URL" >&2
    exit 1
fi

# ── 安全栏杆 2：库名必须含 "test" ────────────────────────────────────────────
db_name="$(psql "$TEST_DATABASE_URL" -tAc "SELECT current_database()")"
if [[ "$db_name" != *test* && "${CONVERGE_ALLOW_NON_TEST_DB:-0}" != "1" ]]; then
    echo "::error::refusing to converge '$db_name' (database name must contain 'test';" >&2
    echo "          set CONVERGE_ALLOW_NON_TEST_DB=1 only for a deliberate non-test run)" >&2
    exit 1
fi

# ── 安全栏杆 1：参考 schema 必须真的存在且非空 ───────────────────────────────
reference_tables="$(psql "$TEST_DATABASE_URL" -tAc \
    "SELECT count(*) FROM information_schema.tables WHERE table_schema='${TEMPLATE_SCHEMA}' AND table_type='BASE TABLE'")"
if [[ "${reference_tables:-0}" -lt "$MIN_TABLES" ]]; then
    echo "::error::reference schema '${TEMPLATE_SCHEMA}' has ${reference_tables:-0} tables (< ${MIN_TABLES});" >&2
    echo "          refusing to converge — an empty reference would mark the whole baseline as 'extra'" >&2
    exit 1
fi

# 差异查询：`public` 里不属于基线（= 参考 schema）的对象。
#   * relkind r/p = table, v = view, m = materialized view, S = sequence
#   * 扩展拥有的对象（pg_depend.deptype='e'）永不作为候选
read -r -d '' DIFF_SQL <<SQL || true
WITH object_kind AS (
    SELECT c.oid,
           c.relname AS name,
           CASE c.relkind
               WHEN 'r' THEN 'TABLE'
               WHEN 'p' THEN 'TABLE'
               WHEN 'v' THEN 'VIEW'
               WHEN 'm' THEN 'MATERIALIZED VIEW'
               WHEN 'S' THEN 'SEQUENCE'
           END AS kind,
           n.nspname AS schema_name
    FROM pg_class c
    JOIN pg_namespace n ON n.oid = c.relnamespace
    WHERE c.relkind IN ('r', 'p', 'v', 'm', 'S')
      AND NOT EXISTS (SELECT 1 FROM pg_depend d WHERE d.objid = c.oid AND d.deptype = 'e')
),
public_objects AS (SELECT name, kind FROM object_kind WHERE schema_name = 'public'),
expected_objects AS (SELECT name, kind FROM object_kind WHERE schema_name = '${TEMPLATE_SCHEMA}')
SELECT kind, name FROM public_objects
EXCEPT
SELECT kind, name FROM expected_objects
SQL

diff_rows="$(psql "$TEST_DATABASE_URL" -tA -F$'\t' -c "$DIFF_SQL")"
extra_count=0
if [[ -n "$diff_rows" ]]; then
    extra_count="$(printf '%s\n' "$diff_rows" | wc -l | tr -d ' ')"
fi

echo "==> D-57② public 收敛：参考 schema='${TEMPLATE_SCHEMA}'（${reference_tables} tables），库='${db_name}'，模式=${MODE}"
if [[ "$extra_count" -eq 0 ]]; then
    echo "    public 已在基线上：无多余对象（0 处待删）"
else
    echo "    public 有 ${extra_count} 个不属于基线的对象："
    printf '%s\n' "$diff_rows" | while IFS=$'\t' read -r kind name; do
        echo "      - ${kind} public.${name}"
    done
fi

# ── 安全栏杆 7a（D-75）：参考 schema 必须与 rail 1 时**一样**在场 ────────────
# `prepare_test_db.sh` 的 [2/4] 步先 `DROP SCHEMA … CASCADE` 再重建参考 schema；
# 并发时 rail 1 看到的是 220 张表、apply 时可能已经是 0 张 —— 那会让"public 全是多余的"
# 成立并清空 public。这里在**任何删除之前**复测一次，参考只许长大、不许缩小。
if [[ "$MODE" == "apply" && "$extra_count" -gt 0 ]]; then
    reference_tables_now="$(psql "$TEST_DATABASE_URL" -tAc \
        "SELECT count(*) FROM information_schema.tables WHERE table_schema='${TEMPLATE_SCHEMA}' AND table_type='BASE TABLE'")"
    if [[ "${reference_tables_now:-0}" -lt "$reference_tables" ]]; then
        echo "::error::参考 schema '${TEMPLATE_SCHEMA}' 在测量后缩小了（${reference_tables} → ${reference_tables_now} 张表）——" >&2
        echo "         它正在被并发重建（prepare_test_db.sh 的 [2/4] 步会先 DROP SCHEMA）;" >&2
        echo "         此时执行删除会把 public 的基线对象当成'多余对象'清空。" >&2
        echo "         已拒绝执行、未删除任何对象；请等 seed 完成后再跑本脚本。" >&2
        exit 1
    fi
    if [[ "${reference_tables_now:-0}" -lt "$MIN_TABLES" ]]; then
        echo "::error::参考 schema '${TEMPLATE_SCHEMA}' 只剩 ${reference_tables_now} 张表（< ${MIN_TABLES}）—— 拒绝执行" >&2
        exit 1
    fi
fi

# ── 安全栏杆 7b（D-75）：不许把 public 削到比参考集还小 ──────────────────────
# 参考集**就是**目标对象集，所以正确的收敛应让 public 的存活对象数 ≥ 参考对象数。
# 低于它意味着删除清单本身是错的（典型：清单是在参考集为空时求值的）。
if [[ "$MODE" == "apply" && "$extra_count" -gt 0 && "${CONVERGE_ALLOW_MASS_DELETE:-0}" != "1" ]]; then
    public_objects_total="$(psql "$TEST_DATABASE_URL" -tAc \
        "SELECT count(*) FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
         WHERE n.nspname = 'public' AND c.relkind IN ('r','p','v','m','S')
           AND NOT EXISTS (SELECT 1 FROM pg_depend d WHERE d.objid = c.oid AND d.deptype = 'e')")"
    reference_objects="$(psql "$TEST_DATABASE_URL" -tAc \
        "SELECT count(*) FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
         WHERE n.nspname = '${TEMPLATE_SCHEMA}' AND c.relkind IN ('r','p','v','m','S')
           AND NOT EXISTS (SELECT 1 FROM pg_depend d WHERE d.objid = c.oid AND d.deptype = 'e')")"
    survivors=$((${public_objects_total:-0} - extra_count))
    if [[ "$survivors" -lt "${reference_objects:-0}" ]]; then
        echo "::error::删除清单会把 public 从 ${public_objects_total} 个对象削到 ${survivors} 个，" >&2
        echo "         而参考集 '${TEMPLATE_SCHEMA}' 有 ${reference_objects} 个 —— 这不是收敛，是清空。" >&2
        echo "         已拒绝执行、未删除任何对象。若确实要刻意清空，设 CONVERGE_ALLOW_MASS_DELETE=1。" >&2
        exit 1
    fi
fi

if [[ "$MODE" == "report" || "$extra_count" -eq 0 ]]; then
    [[ "$MODE" == "report" ]] && echo "==> report 模式：未删除任何对象"
else
    # ── 执行删除：视图 → 物化视图 → 序列 → 表（普通依赖顺序；IF EXISTS 容忍级联）──
    echo "==> 删除 ${extra_count} 个多余对象（CASCADE 仅作用于它们自身的依赖者）"
    # ⚠️ D-75：清单在 rail 1 之后**只求值一次**并落盘，apply 从文件消费
    #   （`\copy` 进临时表）—— 绝不在 apply 期间重新咨询参考 schema。
    #   `format('%I')` 负责标识符转义；`\gexec` 逐条执行生成的 DROP。
    frozen_list="$(mktemp)"
    trap 'rm -f "$frozen_list"' EXIT
    printf '%s\n' "$diff_rows" >"$frozen_list"
    psql "$TEST_DATABASE_URL" -v ON_ERROR_STOP=1 -q <<SQL
\set QUIET off
CREATE TEMP TABLE converge_extra (kind text, name text);
\copy converge_extra FROM '$frozen_list' WITH (FORMAT text)
SELECT format('DROP %s IF EXISTS %I.%I CASCADE;', kind, 'public', name)
FROM converge_extra
ORDER BY CASE kind WHEN 'VIEW' THEN 1 WHEN 'MATERIALIZED VIEW' THEN 2 WHEN 'SEQUENCE' THEN 3 ELSE 4 END
\gexec
SQL
fi

# ── 安全栏杆 5：收敛后复算，必须双向一致 ─────────────────────────────────────
read -r -d '' INVARIANT_SQL <<SQL || true
WITH object_kind AS (
    SELECT c.oid,
           c.relname AS name,
           CASE c.relkind
               WHEN 'r' THEN 'TABLE'
               WHEN 'p' THEN 'TABLE'
               WHEN 'v' THEN 'VIEW'
               WHEN 'm' THEN 'MATERIALIZED VIEW'
               WHEN 'S' THEN 'SEQUENCE'
           END AS kind,
           n.nspname AS schema_name
    FROM pg_class c
    JOIN pg_namespace n ON n.oid = c.relnamespace
    WHERE c.relkind IN ('r', 'p', 'v', 'm', 'S')
      AND NOT EXISTS (SELECT 1 FROM pg_depend d WHERE d.objid = c.oid AND d.deptype = 'e')
),
public_objects AS (SELECT name, kind FROM object_kind WHERE schema_name = 'public'),
expected_objects AS (SELECT name, kind FROM object_kind WHERE schema_name = '${TEMPLATE_SCHEMA}')
SELECT
    (SELECT count(*) FROM (SELECT * FROM public_objects EXCEPT SELECT * FROM expected_objects) AS extra),
    (SELECT count(*) FROM (SELECT * FROM expected_objects EXCEPT SELECT * FROM public_objects) AS missing)
SQL

read -r extra_after missing_after <<<"$(psql "$TEST_DATABASE_URL" -tA -F' ' -c "$INVARIANT_SQL")"
public_tables="$(psql "$TEST_DATABASE_URL" -tAc \
    "SELECT count(*) FROM information_schema.tables WHERE table_schema='public' AND table_type='BASE TABLE'")"
# D-75 栏杆 8：双向 diff 为 0 只有在**参考集仍在场**时才有意义 —— 参考被并发 DROP 后
# `expected_objects` 为空，extra 与 missing 会同时退化成 0（本次事故正是这样被放行的）。
reference_tables_after="$(psql "$TEST_DATABASE_URL" -tAc \
    "SELECT count(*) FROM information_schema.tables WHERE table_schema='${TEMPLATE_SCHEMA}' AND table_type='BASE TABLE'")"
echo "==> 收敛后：public 多余=${extra_after:-?} 缺失=${missing_after:-?}；public tables=${public_tables} / 参考=${reference_tables_after:-?}（rail 1 时 ${reference_tables}）"

if [[ "$MODE" == "apply" ]]; then
    if [[ "${reference_tables_after:-0}" -lt "$reference_tables" ]]; then
        echo "::error::参考 schema '${TEMPLATE_SCHEMA}' 在收敛期间缩小了（${reference_tables} → ${reference_tables_after:-0} 张表）⇒" >&2
        echo "         本次 extra/missing 判定不可信（参考消失会让两者同时退化成 0）。" >&2
        exit 1
    fi
    if [[ "${extra_after:-1}" != "0" ]]; then
        echo "::error::convergence incomplete: ${extra_after} object(s) still not in the baseline" >&2
        exit 1
    fi
    if [[ "${missing_after:-1}" != "0" ]]; then
        echo "::error::public is missing ${missing_after} baseline object(s) — the apply step did not land" >&2
        exit 1
    fi
    if [[ "${public_tables:-0}" -lt "$MIN_TABLES" ]]; then
        echo "::error::public 收敛后只剩 ${public_tables} 张表（< ${MIN_TABLES}）—— 这不是收敛" >&2
        exit 1
    fi
fi

echo "==> D-57② OK"
