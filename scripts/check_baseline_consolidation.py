#!/usr/bin/env python3
"""检查 latest consolidated baseline 是否吸收所有增量迁移（防止漏追加）。

背景：build_sqlx_migration_source.py 的 forward-only source 只选 baseline +
extension + V*，依赖「baseline 完整吸收所有增量迁移」的假设。但 baseline 是
手动 consolidate，历史上曾漏吸收 8 个迁移（federation_dead_letter_queue 等），
导致 CI 的 DB 与生产不一致。

本脚本提取时间戳增量迁移里「新增的表/索引/列」，检查它们是否已在当前
latest baseline 中，漏了则报错（退出码 1）。

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


def latest_baseline() -> pathlib.Path:
    baselines = sorted(
        p
        for p in MIGRATIONS_DIR.glob("00000000_unified_schema_v*.sql")
        if not p.name.endswith(".undo.sql")
    )
    if not baselines:
        raise SystemExit("no unified schema baseline found")
    return baselines[-1]


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
            if col not in baseline_text:
                missing.append(f"{f.name}: 列 {table}.{col}")

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

    print(f"✅ {baseline.name} 已吸收全部 {len(ts_files)} 个增量迁移的对象。")
    return 0


if __name__ == "__main__":
    sys.exit(main())
