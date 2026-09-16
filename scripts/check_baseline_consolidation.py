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
