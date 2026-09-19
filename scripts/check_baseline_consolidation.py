#!/usr/bin/env python3
"""检查 latest consolidated baseline 是否吸收所有增量迁移（防止漏追加）。

背景：build_sqlx_migration_source.py 的 forward-only source 只选 baseline +
extension + V*，依赖「baseline 完整吸收所有增量迁移」的假设。但 baseline 是
手动 consolidate，历史上曾漏吸收 8 个迁移（federation_dead_letter_queue 等），
导致 CI 的 DB 与生产不一致。

本脚本做两件事：
  1. 提取时间戳增量迁移里「新增的表/索引/列」，检查它们是否已在当前 latest
     baseline 中，漏了则报错（退出码 1）。
  2. **重复对象检测**：折叠历史迁移时很容易把「同一对象」折进两份（例如先建普通
     索引、后又加 UNIQUE 约束），结果是每次写入多维护一棵 B-tree，或同一个外键
     列上挂两个约束、其中一个 CASCADE 静默压过另一个。这类重复不会被第 1 项发现
     （对象确实"在" baseline 里），所以单独检查。

用法：
    python3 scripts/check_baseline_consolidation.py
"""

from __future__ import annotations

import pathlib
import re
import sys

MIGRATIONS_DIR = pathlib.Path(__file__).resolve().parent.parent / "migrations"

# 时间戳迁移：^YYYYMMDDHHMMSS_name.sql（排除 baseline/extension/undo）
TS_RE = re.compile(r"^\d{14}_.*\.sql$")

# 扫描面自检（2026-09-19，GATE_INTEGRITY_SWEEP §6 C3）：
# migrations/ 只剩一个 consolidated baseline 时，TS_RE 匹配 0 个文件，下面两组检查
# 都在空集上空转，于是"长期全绿"只是"扫描面为空"的假象。这里显式承认该形态：
#   1. 每个正向 .sql 必须被归类（baseline / extension / V* / TS_RE 命中），
#      命名约定一变就报错，而不是静默跳过；
#   2. TS_RE 命中 0 个时，必须实测证明正向链就是那一个 baseline
#      （与 tests/unit/migration_replayability_guard_tests.rs 的 marker 同款判据）。
CONSOLIDATED_BASELINE_ONLY_MARKER = "consolidated-baseline-only"
BASELINE_RE = re.compile(r"^00000000_unified_schema_v.*\.sql$")
EXTENSION_RE = re.compile(r"^00000001_extensions.*\.sql$")

# 新增表：CREATE TABLE [IF NOT EXISTS] xxx
CREATE_TABLE_RE = re.compile(
    r"CREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?([a-zA-Z_][a-zA-Z0-9_]*)"
)
# 新增索引：CREATE [UNIQUE] INDEX [IF NOT EXISTS] idx_xxx
CREATE_INDEX_RE = re.compile(
    r"CREATE\s+(?:UNIQUE\s+)?INDEX\s+(?:IF\s+NOT\s+EXISTS\s+)?([a-zA-Z_][a-zA-Z0-9_]*)"
)
# 新增列：ALTER TABLE xxx ADD COLUMN [IF NOT EXISTS] yyy
ADD_COLUMN_RE = re.compile(
    r"ALTER\s+TABLE\s+([a-zA-Z_][a-zA-Z0-9_]*)\s+ADD\s+COLUMN\s+(?:IF\s+NOT\s+EXISTS\s+)?([a-zA-Z_][a-zA-Z0-9_]*)"
)

# 提取 baseline 中每个表的全部列名（用于表-列配对判定）
# 仅匹配 `CREATE TABLE IF NOT EXISTS xxx ( ... )` 顶层块内的列定义行
TABLE_COLUMN_RE = re.compile(
    r"CREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?(?P<table>[a-zA-Z_][a-zA-Z0-9_]*)\s*\((?P<body>.*?)\)\s*;",
    re.DOTALL,
)
COLUMN_DEF_RE = re.compile(r"(?:^|\n)\s*(?P<col>[a-zA-Z_][a-zA-Z0-9_]*)\s+(?:[a-zA-Z][a-zA-Z0-9_]*)\b", re.MULTILINE)
# baseline 中以 ALTER TABLE ... ADD COLUMN 形式折入的列
BASELINE_ALTER_ADD_RE = re.compile(
    r"ALTER\s+TABLE\s+(?:IF\s+EXISTS\s+)?([a-zA-Z_][a-zA-Z0-9_]*)\s+ADD\s+COLUMN\s+(?:IF\s+NOT\s+EXISTS\s+)?([a-zA-Z_][a-zA-Z0-9_]*)",
    re.IGNORECASE,
)


def columns_of_table(baseline_text: str, table: str) -> set[str]:
    """Return the set of column names defined for `table` in the baseline text.

    Covers both columns embedded in `CREATE TABLE <table> (...)` and columns
    added later via `ALTER TABLE <table> ADD COLUMN <col>` (the incremental
    fold-in fashion the baseline uses).
    """
    cols: set[str] = set()
    for m in TABLE_COLUMN_RE.finditer(baseline_text):
        if m.group("table") != table:
            continue
        cols |= {c.group("col") for c in COLUMN_DEF_RE.finditer(m.group("body"))}
    for t, c in BASELINE_ALTER_ADD_RE.findall(baseline_text):
        if t == table:
            cols.add(c)
    return cols


def latest_baseline() -> pathlib.Path:
    baselines = sorted(
        p
        for p in MIGRATIONS_DIR.glob("00000000_unified_schema_v*.sql")
        if not p.name.endswith(".undo.sql")
    )
    if not baselines:
        raise SystemExit("no unified schema baseline found")
    return baselines[-1]


def forward_sql_files() -> list[pathlib.Path]:
    """All forward `.sql` files (`.undo.sql` is a rollback script, not forward)."""
    return sorted(
        p for p in MIGRATIONS_DIR.glob("*.sql") if not p.name.endswith(".undo.sql")
    )


def scan_surface_problems(
    baseline: pathlib.Path, ts_files: list[pathlib.Path]
) -> list[str]:
    """Prove the scan surface is non-vacuous before trusting any "no violations".

    The subject of this script is timestamped incremental migrations (`TS_RE`).
    `migrations/` currently holds a single consolidated baseline, so `TS_RE`
    matches nothing and both object loops would pass without evaluating
    anything. The fix is *not* to skip silently but to assert two measured
    facts about the surface:

      * every forward `.sql` is accounted for — the baseline, a known chain
        member (`00000001_extensions*` / `V*`), or a `TS_RE` match. A changed
        naming convention then fails loudly instead of being skipped;
      * when `TS_RE` matches nothing, the forward chain is exactly the single
        consolidated baseline (mirrors
        `tests/unit/migration_replayability_guard_tests.rs`), so the
        consolidated-baseline-only case is an assertion, not an assumption.

    Consequently, as soon as a genuine incremental migration appears the
    original object-absorption checks run unchanged.
    """
    problems: list[str] = []
    forward = forward_sql_files()
    accounted = {p.name for p in ts_files}

    if not forward:
        problems.append("migrations/ 里没有任何正向 .sql 文件（扫描面为空）")
        return problems

    for path in forward:
        if (
            path == baseline
            or BASELINE_RE.match(path.name)
            or EXTENSION_RE.match(path.name)
        ):
            continue
        if path.name.startswith("V") and path.name.endswith(".sql"):
            continue
        if path.name not in accounted:
            problems.append(
                f"{path.name} 既不是 baseline/extension/V*，也不匹配时间戳增量正则 "
                f"{TS_RE.pattern} —— 扫描面漏文件，增量对象吸收检查对它不会生效"
            )

    if not ts_files and forward != [baseline]:
        problems.append(
            "时间戳增量扫描集为空，但正向链不是「单一 consolidated baseline」: "
            f"{[p.name for p in forward]} —— 空集豁免不成立（这是漏扫，不是无违规）"
        )

    return problems


# ── 重复对象检测（2026-09-17 新增）─────────────────────────────────────────────
# 索引定义（含 CONCURRENTLY 与 WHERE 部分谓词）
_INDEX_DEF_RE = re.compile(
    r"CREATE\s+(?P<uniq>UNIQUE\s+)?INDEX(?:\s+CONCURRENTLY)?(?:\s+IF\s+NOT\s+EXISTS)?\s+"
    r"(?P<name>[a-zA-Z_][a-zA-Z0-9_]*)\s+ON\s+(?P<table>[a-zA-Z_][a-zA-Z0-9_]*)\s*"
    r"(?:USING\s+(?P<method>[a-zA-Z_][a-zA-Z0-9_]*)\s*)?\((?P<cols>[^)]*)\)"
    r"(?P<where>\s+WHERE\s+[^;]*)?;",
    re.IGNORECASE | re.DOTALL,
)
# 表级外键（在 CREATE TABLE 体内）与 ALTER TABLE ... ADD CONSTRAINT 形式的外键
_ALTER_FK_RE = re.compile(
    r"ALTER\s+TABLE\s+(?:IF\s+EXISTS\s+)?(?P<table>[a-zA-Z_][a-zA-Z0-9_]*)\s+ADD\s+CONSTRAINT\s+"
    r"(?P<name>[a-zA-Z_][a-zA-Z0-9_]*)\s+FOREIGN\s+KEY\s*\(\s*(?P<col>[a-zA-Z_][a-zA-Z0-9_]*)\s*\)\s*"
    r"REFERENCES\s+(?P<parent>[a-zA-Z_][a-zA-Z0-9_]*)",
    re.IGNORECASE | re.DOTALL,
)
_FK_IN_TABLE_RE = re.compile(
    r"(?:CONSTRAINT\s+(?P<name>[a-zA-Z_][a-zA-Z0-9_]*)\s+)?FOREIGN\s+KEY\s*\(\s*"
    r"(?P<col>[a-zA-Z_][a-zA-Z0-9_]*)\s*\)\s*REFERENCES\s+(?P<parent>[a-zA-Z_][a-zA-Z0-9_]*)",
    re.IGNORECASE | re.DOTALL,
)


def _norm(cols: str) -> str:
    return re.sub(r"\s+", " ", cols).strip().lower()


def duplicate_indexes(text: str) -> dict[tuple, list[str]]:
    """(table, unique, method, cols, where) -> [index names] with len > 1."""
    groups: dict[tuple, list[str]] = {}
    for m in _INDEX_DEF_RE.finditer(text):
        key = (
            m.group("table").lower(),
            bool(m.group("uniq")),
            (m.group("method") or "btree").lower(),
            _norm(m.group("cols")),
            _norm(m.group("where") or ""),
        )
        groups.setdefault(key, []).append(m.group("name"))
    return {k: v for k, v in groups.items() if len(v) > 1}


def duplicate_foreign_keys(text: str) -> dict[tuple, list[str]]:
    """(table, column, referenced_table) -> [constraint names] with len > 1.

    Two FKs on the same column pointing at the same parent is always a bug: one
    of them silently wins for delete/update actions (PostgreSQL fires *all*
    matching actions, so a stray CASCADE defeats an intentional NO ACTION).
    """
    groups: dict[tuple, list[str]] = {}
    for m in TABLE_COLUMN_RE.finditer(text):
        table = m.group("table").lower()
        for fk in _FK_IN_TABLE_RE.finditer(m.group("body")):
            key = (table, fk.group("col").lower(), fk.group("parent").lower())
            groups.setdefault(key, []).append(fk.group("name") or f"{table}.{fk.group('col')}(inline)")
    for m in _ALTER_FK_RE.finditer(text):
        key = (m.group("table").lower(), m.group("col").lower(), m.group("parent").lower())
        groups.setdefault(key, []).append(m.group("name"))
    # Only *distinct* constraint names are a defect: re-stating the same constraint
    # inline and again inside an idempotent `IF NOT EXISTS (SELECT 1 FROM
    # pg_constraint WHERE conname = ...)` block is benign (the ALTER is a no-op).
    # Two different names on the same column both get created, and both delete
    # actions fire — a stray CASCADE silently defeats an intentional NO ACTION.
    return {k: v for k, v in groups.items() if len(set(v)) > 1}


def main() -> int:
    baseline = latest_baseline()
    baseline_text = baseline.read_text(encoding="utf-8", errors="replace")

    missing: list[str] = []
    ts_files = sorted(
        p
        for p in MIGRATIONS_DIR.glob("*.sql")
        if TS_RE.match(p.name) and not p.name.endswith(".undo.sql")
    )

    # 扫描面自检必须先于任何"无违规"结论：空集只有在实测为
    # consolidated-baseline-only 时才允许豁免，否则报错退出。
    surface_problems = scan_surface_problems(baseline, ts_files)
    if surface_problems:
        print("❌ 扫描面自检失败（不是「无违规」，而是「没扫到该扫的东西」）:")
        for problem in surface_problems:
            print(f"   - {problem}")
        print(
            "\n修复：让 TS_RE 覆盖真实的正向迁移命名，或在脚本里显式承认新的链形态；"
            "不得以空扫描面报绿。"
        )
        return 1

    if not ts_files:
        print(
            f"ℹ️  {CONSOLIDATED_BASELINE_ONLY_MARKER}: migrations/ 只有 {baseline.name} 一份正向迁移，"
            "时间戳增量对象吸收检查按空集豁免（已由扫描面自检实测证明，不是漏扫）；"
            "一旦出现任何增量迁移，原检查立即恢复执行。"
        )

    for f in ts_files:
        text = f.read_text(encoding="utf-8", errors="replace")
        for obj in CREATE_TABLE_RE.findall(text):
            if (
                f"CREATE TABLE IF NOT EXISTS {obj}" not in baseline_text
                and obj not in baseline_text
            ):
                missing.append(f"{f.name}: 表 {obj}")
        for obj in CREATE_INDEX_RE.findall(text):
            if obj not in baseline_text:
                missing.append(f"{f.name}: 索引 {obj}")
        for table, col in ADD_COLUMN_RE.findall(text):
            cols = columns_of_table(baseline_text, table)
            if col not in cols:
                missing.append(f"{f.name}: 列 {table}.{col}")

    dup_idx = duplicate_indexes(baseline_text)
    dup_fk = duplicate_foreign_keys(baseline_text)
    if dup_idx or dup_fk:
        print(f"❌ {baseline.name} 存在重复对象（折叠历史迁移时折进了两份）:")
        for (tbl, uniq, method, cols, where), names in sorted(dup_idx.items()):
            kind = "UNIQUE " if uniq else ""
            extra = f" WHERE {where}" if where else ""
            print(f"   - 索引 [{tbl}] {kind}{method} ({cols}){extra}: {', '.join(sorted(names))}")
            print("     同一 (表, 列集, 唯一性, 方法, 谓词) 只需一个；保留约束索引，删除普通索引。")
        for (tbl, col, parent), names in sorted(dup_fk.items()):
            print(f"   - 外键 [{tbl}.{col} → {parent}]: {', '.join(sorted(names))}")
            print("    同一列上的多个外键会让删除动作重复触发（CASCADE 会压过 NO ACTION），只保留一个。")
        print("\n见 docs/audit/DB_REVIEW_2026-09-17.md §1/§5。")
        return 1

    if missing:
        print(
            f"❌ {baseline.name} 漏吸收以下增量迁移对象（会导致 forward-only source 缺失）:"
        )
        for m in missing:
            print(f"   - {m}")
        print(
            "\n修复：把对应迁移内容折入 baseline 尾部（迁移须为幂等 IF NOT EXISTS）。"
        )
        return 1

    print(f"✅ {baseline.name} 已吸收全部 {len(ts_files)} 个增量迁移的对象；无重复索引/外键。")
    return 0


if __name__ == "__main__":
    sys.exit(main())
