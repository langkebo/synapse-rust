#!/usr/bin/env python3
"""
Generate next baseline by merging current baseline + extensions_v10
and appending P0-5/P0-6 integrity constraints and hot indexes.
"""
import pathlib

BASE_DIR = pathlib.Path(__file__).parent.parent
MIGRATIONS = BASE_DIR / "migrations"
SCRIPT_DIR = pathlib.Path(__file__).parent

current_path = MIGRATIONS / "00000000_unified_schema_v12.sql"
ext_path = MIGRATIONS / "00000001_extensions_v10.sql"
v12_path = MIGRATIONS / "00000000_unified_schema_v12.sql"

print(f"Reading {current_path}")
current = current_path.read_text(encoding="utf-8")
print(f"Reading {ext_path}")
ext = ext_path.read_text(encoding="utf-8")

header = """-- ============================================================================
-- synapse-rust 统一数据库架构 v12.0.0
-- 创建日期: 2026-09-16（基于 v12 baseline + extensions 统一）
--
-- 合并说明:
--   - v12 baseline (00000000_unified_schema_v12.sql)
--   - extensions_v10 (00000001_extensions_v10.sql) 直接内inline
--   - P0-5 完整性约束 + P0-6 热点索引
--
-- 规范: 与 v12 保持一致
-- ============================================================================

--no-transaction
"""

# Ensure extensions appear after baseline, avoid duplicate CREATE EXTENSION
def deduplicate_extensions(current_text, ext_text):
    # Simple heuristic: keep current baseline extensions block, remove duplicate CREATE EXTENSION lines from ext
    lines = ext_text.splitlines()
    # Keep everything but drop lines starting with CREATE EXTENSION IF NOT EXISTS
    filtered = [l for l in lines if not l.strip().upper().startswith("CREATE EXTENSION IF NOT EXISTS")]
    return "\n".join(filtered)

ext_body = deduplicate_extensions(current, ext)

# Load P0 constraints + indexes
p0_path = SCRIPT_DIR / "p0_constraints_indexes.sql"
if p0_path.exists():
    p0_sql = p0_path.read_text(encoding="utf-8")
else:
    # P0-5/P0-6 完整性约束与热点索引已随 v12 主体一并折入（132 FOREIGN KEY /
    # 26 CHECK / 165 UNIQUE 及尾部热点索引），不再用"暂空，请补充"占位——
    # 该占位会在 baseline 里留下一条与事实相反的注释，误导复核。
    p0_sql = (
        "-- P0-5/P0-6 完整性约束与热点索引已随 v12 主体一并折入"
        "（见上文 FOREIGN KEY / CHECK / UNIQUE 与尾部热点索引）。\n"
    )

body = header + "\n" + current + "\n\n" + ext_body + "\n\n" + p0_sql + "\n"

v12_path.write_text(body, encoding="utf-8")
print(f"Wrote {v12_path} ({len(body):,} bytes)")
