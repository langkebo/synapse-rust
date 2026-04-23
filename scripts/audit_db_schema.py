#!/usr/bin/env python3
"""Extract all table definitions from migration SQL files and compare with Rust structs."""

import re
import os
import glob
import json

MIGRATIONS_DIR = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "migrations")
ACTIVE_PATTERNS = [
    "00000000_unified_schema_v6.sql",
    "00000001_extensions_*.sql",
    "202604*.sql",
]


def extract_tables_from_migrations():
    """Parse all CREATE TABLE and ALTER TABLE ADD COLUMN from active migrations."""
    tables = {}
    all_files = []
    for pattern in ACTIVE_PATTERNS:
        all_files.extend(glob.glob(os.path.join(MIGRATIONS_DIR, pattern)))
    all_files = sorted(set(f for f in all_files if ".undo." not in f))

    for fpath in all_files:
        fname = os.path.basename(fpath)
        with open(fpath) as f:
            content = f.read()

        # CREATE TABLE IF NOT EXISTS name ( ... );
        for m in re.finditer(
            r"CREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?(\w+)\s*\((.*?)\);",
            content,
            re.DOTALL | re.IGNORECASE,
        ):
            tname = m.group(1)
            body = m.group(2)
            cols = _parse_columns(body)
            if tname not in tables:
                tables[tname] = {"columns": cols, "source": fname}
            else:
                tables[tname]["columns"].update(cols)
                tables[tname]["source"] += "+" + fname

        # ALTER TABLE name ADD COLUMN [IF NOT EXISTS] colname type ...
        for m in re.finditer(
            r"ALTER\s+TABLE\s+(\w+)\s+ADD\s+COLUMN\s+(?:IF\s+NOT\s+EXISTS\s+)?\"?(\w+)\"?\s+(.+?)(?:;|$)",
            content,
            re.IGNORECASE,
        ):
            tname = m.group(1)
            cname = m.group(2)
            cdef = m.group(3).strip().rstrip(";")
            if tname in tables and cname not in tables[tname]["columns"]:
                tables[tname]["columns"][cname] = cdef
                tables[tname]["source"] += "+ALTER(" + fname + ")"

    return tables


def _parse_columns(body):
    """Extract column definitions from CREATE TABLE body."""
    columns = {}
    skip_prefixes = (
        "CONSTRAINT",
        "PRIMARY KEY",
        "FOREIGN KEY",
        "UNIQUE",
        "CHECK",
        "EXCLUDE",
    )
    for line in body.split("\n"):
        line = line.strip().rstrip(",")
        if not line or line.startswith("--"):
            continue
        upper = line.upper()
        if any(upper.startswith(p) for p in skip_prefixes):
            continue
        cm = re.match(r'^"?(\w+)"?\s+(.+)$', line)
        if cm:
            cname = cm.group(1)
            cdef = cm.group(2).strip()
            columns[cname] = cdef
    return columns


def extract_rust_structs():
    """Find all #[derive(FromRow)] structs with their fields."""
    project_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    rust_dir = os.path.join(project_root, "src")
    structs = {}
    for root, dirs, files in os.walk(rust_dir):
        if "__pycache__" in root or "target" in root:
            continue
        for fname in files:
            if not fname.endswith(".rs"):
                continue
            fpath = os.path.join(root, fname)
            rel_path = os.path.relpath(fpath, project_root)
            try:
                with open(fpath) as f:
                    content = f.read()
            except Exception:
                continue

            # Find struct blocks preceded by FromRow derive
            pattern = r"#\[derive[^\]]*FromRow[^\]]*\]\s*(?:(?:#\[sqlx\([^\]]*\)\]|#[^\n]+)\s*)*pub\s+struct\s+(\w+)\s*(?:<[^>]+>)?\s*\{([^}]+)\}"
            for m in re.finditer(pattern, content, re.DOTALL):
                sname = m.group(1)
                body = m.group(2)
                fields = {}
                rename_map = {}
                skip_fields = set()

                # Collect sqlx(rename) and sqlx(skip) attributes before the struct body
                block_start = m.start()
                preceding = content[block_start:m.start()]

                # Parse each field, tracking preceding attribute lines
                pending_rename = None
                pending_skip = False
                for field_line in body.split("\n"):
                    fl = field_line.strip().rstrip(",")
                    if not fl or fl.startswith("//"):
                        # Check if this is an attribute line
                        attr_fl = fl.strip()
                        if attr_fl.startswith("#["):
                            rename_m = re.search(r'#\[\s*sqlx\(rename\s*=\s*"([^"]+)"\s*\)\]', attr_fl)
                            skip_m = re.search(r'#\[\s*sqlx\(skip\)\]', attr_fl)
                            if rename_m:
                                pending_rename = rename_m.group(1)
                            if skip_m:
                                pending_skip = True
                        continue

                    # Check for sqlx attribute on this field (same line)
                    rename_m = re.search(r'#\[\s*sqlx\(rename\s*=\s*"([^"]+)"\s*\)\]', fl)
                    skip_m = re.search(r'#\[\s*sqlx\(skip\)\]', fl)

                    if skip_m or pending_skip:
                        fm = re.search(r'pub\s+(\w+)\s*:', fl)
                        if fm:
                            skip_fields.add(fm.group(1))
                        pending_skip = False
                        pending_rename = None
                        continue

                    if rename_m:
                        db_name = rename_m.group(1)
                    elif pending_rename:
                        db_name = pending_rename
                    else:
                        db_name = None

                    fm = re.match(r'pub\s+(\w+)\s*:\s*(.+)', fl)
                    if fm:
                        fname_field = fm.group(1)
                        ftype = fm.group(2).strip()
                        fields[fname_field] = ftype
                        if db_name:
                            rename_map[fname_field] = db_name

                    pending_rename = None

                structs[sname] = {
                    "fields": fields,
                    "renames": rename_map,
                    "skips": skip_fields,
                    "file": rel_path,
                }

    # Now extract struct-to-table mapping from query_as calls
    struct_table_map, alias_maps = _extract_struct_table_mapping(rust_dir, structs)
    for sname, tname in struct_table_map.items():
        if sname in structs:
            structs[sname]["table"] = tname
    for sname, aliases in alias_maps.items():
        if sname in structs:
            structs[sname]["sql_aliases"] = aliases

    return structs


def _extract_struct_table_mapping(rust_dir, structs):
    """Extract which SQL table each struct is used with by finding query_as patterns.
    Also extracts SQL AS alias mappings (e.g., 'sender as user_id' means struct field user_id maps to SQL column sender).
    """
    mapping = {}
    alias_maps = {}
    for root, dirs, files in os.walk(rust_dir):
        if "__pycache__" in root or "target" in root:
            continue
        for fname in files:
            if not fname.endswith(".rs"):
                continue
            fpath = os.path.join(root, fname)
            try:
                with open(fpath) as f:
                    content = f.read()
            except Exception:
                continue

            for m in re.finditer(
                r'(?:sqlx::)?query_as\s*::\s*<\s*_\s*,\s*(\w+)\s*>\s*\(\s*(?:r#")?(.*?)(?:"#|"\s*\))',
                content,
                re.DOTALL,
            ):
                sname = m.group(1)
                sql_text = m.group(2)
                if sname not in structs:
                    continue
                from_m = re.search(r'\bFROM\s+(\w+)', sql_text, re.IGNORECASE)
                if from_m:
                    tname = from_m.group(1)
                    if sname not in mapping:
                        mapping[sname] = tname

                # Extract AS aliases: "column_name AS alias_name" or "expr as alias_name"
                aliases = {}
                for alias_m in re.finditer(
                    r'(?:COALESCE\([^)]+\)|[\w.]+)\s+[Aa][Ss]\s+(\w+)',
                    sql_text,
                ):
                    alias_name = alias_m.group(1)
                    # Find the source column
                    full_expr = alias_m.group(0)
                    # Simple case: "column_name AS alias"
                    simple_col = re.match(r'(\w+)\s+[Aa][Ss]\s+', full_expr)
                    if simple_col:
                        source_col = simple_col.group(1)
                        if source_col.upper() not in ('COALESCE', 'NULLIF', 'CAST', 'CASE', 'EXTRACT', 'COUNT', 'SUM', 'AVG', 'MAX', 'MIN'):
                            aliases[alias_name] = source_col
                    else:
                        # Complex expression like COALESCE(col, default) AS alias
                        coal_m = re.search(r'COALESCE\(\s*(\w+)', full_expr, re.IGNORECASE)
                        if coal_m:
                            aliases[alias_name] = coal_m.group(1)

                if aliases and sname not in alias_maps:
                    alias_maps[sname] = aliases

    return mapping, alias_maps


def rust_type_to_sql_type(rust_type):
    """Map Rust type to approximate SQL type for comparison."""
    rt = rust_type.strip()
    if "String" in rt:
        return "TEXT"
    if "i64" in rt or "i32" in rt or "i16" in rt:
        return "BIGINT"
    if "bool" in rt:
        return "BOOLEAN"
    if "f64" in rt or "f32" in rt:
        return "DOUBLE PRECISION"
    if "DateTime" in rt or "chrono" in rt.lower():
        return "TIMESTAMPTZ"
    if "Uuid" in rt or "uuid" in rt.lower():
        return "UUID"
    if "serde_json::Value" in rt or "json" in rt.lower():
        return "JSONB"
    if "Option<" in rt:
        inner = re.sub(r'Option<(.*)>', r'\1', rt)
        return rust_type_to_sql_type(inner) + "_NULLABLE"
    return rt


def infer_table_from_struct(struct_name, sql_tables):
    """Try to map a Rust struct name to a SQL table name."""
    # Common naming patterns: User -> users, RoomMembership -> room_memberships
    candidates = [
        struct_name.lower() + "s",
        struct_name.lower().replace("_", ""),
        struct_name.lower(),
        re.sub(r'(?<!^)(?=[A-Z])', '_', struct_name).lower(),
        re.sub(r'(?<!^)(?=[A-Z])', '_', struct_name).lower() + "s",
    ]
    for c in candidates:
        if c in sql_tables:
            return c
    return None


def compare_schema_vs_code(sql_tables, rust_structs):
    """Compare SQL schema definitions with Rust FromRow structs and generate diff report."""
    issues = {
        "critical": [],
        "high": [],
        "medium": [],
        "low": [],
        "info": [],
    }

    matched = []
    sql_orphan = set(sql_tables.keys())
    rust_orphan = set(rust_structs.keys())

    for sname, sinfo in rust_structs.items():
        # Priority 1: Use explicit table mapping from query_as
        if "table" in sinfo and sinfo["table"] in sql_tables:
            tname = sinfo["table"]
        else:
            tname = infer_table_from_struct(sname, sql_tables)
        if not tname:
            rust_orphan.discard(sname)
            issues["info"].append({
                "type": "NO_TABLE_MATCH",
                "struct": sname,
                "file": sinfo["file"],
                "detail": f"Struct '{sname}' has no obvious matching SQL table"
            })
            continue

        rust_orphan.discard(sname)
        sql_orphan.discard(tname)
        tdef = sql_tables[tname]
        sql_cols = tdef["columns"]
        rust_fields = sinfo["fields"]
        renames = sinfo["renames"]
        skips = sinfo["skips"]

        matched.append((tname, sname))

        # Build effective DB column name -> Rust field mapping
        # Consider: 1) sqlx(rename) attributes, 2) SQL AS aliases
        sql_aliases = sinfo.get("sql_aliases", {})
        db_to_rust = {}
        for fname, ftype in rust_fields.items():
            # Priority: sqlx(rename) > sql_aliases > field name
            if fname in renames:
                db_name = renames[fname]
            elif fname in sql_aliases:
                db_name = sql_aliases[fname]
            else:
                db_name = fname
            db_to_rust[db_name] = (fname, ftype)

        # Check: SQL columns that have no matching Rust field
        for cname, cdef in sorted(sql_cols.items()):
            if cname not in db_to_rust and cname not in [r[0] for r in renames.values()] and cname not in skips:
                c_upper = cdef.upper()
                is_nullable = "NOT NULL" not in c_upper or "DEFAULT" in c_upper
                has_default = "DEFAULT" in c_upper

                if "NOT NULL" in c_upper and not has_default:
                    issues["high"].append({
                        "type": "SQL_COLUMN_WITHOUT_RUST_FIELD",
                        "table": tname,
                        "struct": sname,
                        "column": cname,
                        "col_def": cdef.strip(),
                        "detail": f"SQL column '{cname}' ({cdef.strip()}) has no matching Rust field in {sname}"
                    })
                else:
                    issues["low"].append({
                        "type": "SQL_COLUMN_UNUSED_BY_RUST",
                        "table": tname,
                        "struct": sname,
                        "column": cname,
                        "col_def": cdef.strip(),
                        "detail": f"SQL column '{cname}' not mapped to any Rust field (nullable/has_default)"
                    })

        # Check: Rust fields with no matching SQL column
        for db_name, (fname, ftype) in db_to_rust.items():
            if db_name not in sql_cols:
                is_option = "Option<" in ftype
                target = issues["critical"] if not is_option else issues["high"]
                target.append({
                    "type": "RUST_FIELD_WITHOUT_SQL_COLUMN",
                    "table": tname,
                    "struct": sname,
                    "field": fname,
                    "db_name": db_name,
                    "rust_type": ftype,
                    "detail": f"Rust field '{fname}: {ftype}' maps to SQL column '{db_name}' which does not exist in table '{tname}'"
                })

        # Check type mismatches for matched columns
        for db_name, (fname, ftype) in db_to_rust.items():
            if db_name not in sql_cols:
                continue
            cdef = sql_cols[db_name].upper()
            inferred_sql = rust_type_to_sql_type(ftype).upper()

            # Check nullable mismatch
            is_sql_nullable = ("NOT NULL" not in cdef) or "DEFAULT" in cdef
            is_rust_nullable = "Option<" in ftype

            if is_sql_nullable and not is_rust_nullable:
                issues["high"].append({
                    "type": "NULLABILITY_MISMATCH_SQL_NULLABLE_RUST_NOT",
                    "table": tname,
                    "struct": sname,
                    "field": fname,
                    "db_column": db_name,
                    "rust_type": ftype,
                    "sql_def": sql_cols[db_name].strip(),
                    "detail": f"SQL column '{db_name}' is nullable but Rust field '{fname}: {ftype}' is not Option"
                })
            elif not is_sql_nullable and is_rust_nullable:
                issues["medium"].append({
                    "type": "NULLABILITY_MISMATCH_SQL_NOT_NULL_RUST_OPTIONAL",
                    "table": tname,
                    "struct": sname,
                    "field": fname,
                    "db_column": db_name,
                    "rust_type": ftype,
                    "sql_def": sql_cols[db_name].strip(),
                    "detail": f"SQL column '{db_name}' is NOT NULL but Rust field '{fname}: {ftype}' is Option (overly permissive)"
                })

    # Report orphan tables (SQL only, no Rust struct)
    for tname in sorted(sql_orphan):
        cols = sql_tables[tname]["columns"]
        src = sql_tables[tname]["source"]
        issues["low"].append({
            "type": "TABLE_NO_RUST_STRUCT",
            "table": tname,
            "source": src,
            "columns": len(cols),
            "detail": f"Table '{tname}' ({len(cols)} columns) has no matching Rust FromRow struct"
        })

    return {
        "issues": issues,
        "matched_pairs": matched,
        "sql_orphans": sorted(sql_orphan),
        "rust_orphans": sorted(rust_orphan),
    }


def main():
    print("=" * 80)
    print("DATABASE MIGRATION FULL AUDIT REPORT")
    print("Schema vs Code Alignment Verification")
    print("=" * 80)

    project_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

    # 1. Extract SQL tables
    print("\n[Phase 1] Extracting SQL table definitions from migrations...")
    sql_tables = extract_tables_from_migrations()
    print(f"  Found {len(sql_tables)} tables across all active migrations")

    # 2. Extract Rust structs
    print("\n[Phase 2] Extracting Rust FromRow structs...")
    rust_structs = extract_rust_structs()
    print(f"  Found {len(rust_structs)} FromRow structs")

    # 3. Compare
    print("\n[Phase 3] Comparing Schema vs Code...")
    result = compare_schema_vs_code(sql_tables, rust_structs)

    issues = result["issues"]

    # 4. Generate report
    report_lines = []
    report_lines.append("# Database Migration Full Audit Report")
    report_lines.append("")
    report_lines.append(f"> **Date**: {__import__('datetime').date.today().isoformat()}")
    report_lines.append(f"> **Scope**: All active migration files vs all `FromRow` Rust structs")
    report_lines.append(f"> **SQL Tables**: {len(sql_tables)} | **Rust Structs**: {len(rust_structs)} | **Matched Pairs**: {len(result['matched_pairs'])}")
    report_lines.append("")

    # Summary
    total_issues = sum(len(v) for v in issues.values())
    report_lines.append("## Issue Summary")
    report_lines.append("")
    report_lines.append(f"| Severity | Count | Description |")
    report_lines.append(f"|----------|-------|-------------|")
    report_lines.append(f"| CRITICAL | {len(issues['critical'])} | Runtime crash (missing column / type panic) |")
    report_lines.append(f"| HIGH | {len(issues['high'])} | Conditional runtime error (nullable mismatch, missing field) |")
    report_lines.append(f"| MEDIUM | {len(issues['medium'])} | Naming inconsistency / overly permissive types |")
    report_lines.append(f"| LOW | {len(issues['low'])} | Unused SQL column / no Rust struct |")
    report_lines.append(f"| INFO | {len(issues['info'])} | Informational only |")
    report_lines.append(f"| **Total** | **{total_issues}** | |")
    report_lines.append("")

    # Matched pairs
    report_lines.append("## Matched Table-Struct Pairs")
    report_lines.append("")
    report_lines.append(f"| SQL Table | Rust Struct | SQL Cols | Rust Fields | Source |")
    report_lines.append(f"|----------|-------------|----------|--------------|--------|")
    for tname, sname in sorted(result["matched_pairs"], key=lambda x: x[0]):
        scols = len(sql_tables[tname]["columns"])
        rfields = len(rust_structs[sname]["fields"])
        src = sql_tables[tname]["source"]
        report_lines.append(f"| `{tname}` | `{sname}` | {scols} | {rfields} | {src} |")
    report_lines.append("")

    # Detail each severity level
    for sev in ["critical", "high", "medium", "low", "info"]:
        items = issues[sev]
        if not items:
            continue
        label = sev.upper()
        report_lines.append(f"## {label} Issues ({len(items)})")
        report_lines.append("")
        report_lines.append("| # | Type | Table / Struct | Field / Column | Detail |")
        report_lines.append("|---|------|----------------|-----------------|--------|")
        for i, item in enumerate(items, 1):
            itype = item["type"]
            tbl = item.get("table", item.get("struct", ""))
            fld = item.get("field", item.get("column", item.get("db_column", "")))
            detail = item.get("detail", "")
            report_lines.append(f"| {i} | `{itype}` | `{tbl}` | `{fld}` | {detail} |")
        report_lines.append("")

    # Orphan tables
    if result["sql_orphans"]:
        report_lines.append("## Orphan Tables (No Matching Rust Struct)")
        report_lines.append("")
        report_lines.append("| Table | Columns | Source |")
        report_lines.append("|-------|---------|--------|")
        for tname in result["sql_orphans"]:
            tdef = sql_tables[tname]
            report_lines.append(f"`{tname}` | {len(tdef['columns'])} | {tdef['source']} |")
        report_lines.append("")

    # Orphan structs
    if result["rust_orphans"]:
        report_lines.append("## Orphan Structs (No Matching SQL Table)")
        report_lines.append("")
        report_lines.append("| Struct | Fields | File |")
        report_lines.append("|--------|--------|------|")
        for sname in result["rust_orphans"]:
            sinfo = rust_structs[sname]
            report_lines.append(f"`{sname}` | {len(sinfo['fields'])} | {sinfo['file']} |")
        report_lines.append("")

    # Write report
    report_text = "\n".join(report_lines)
    out_path = os.path.join(project_root, "docs/db", "FULL_SCHEMA_AUDIT_REPORT.md")
    os.makedirs(os.path.dirname(out_path), exist_ok=True)
    with open(out_path, "w") as f:
        f.write(report_text)
    print(f"\n  Report saved to: {out_path}")

    # Also save raw JSON
    json_out = os.path.join(project_root, "docs/db", "_audit_extract.json")
    output = {
        "summary": {
            "total_sql_tables": len(sql_tables),
            "total_rust_structs": len(rust_structs),
            "matched_pairs": len(result["matched_pairs"]),
            "sql_orphans": len(result["sql_orphans"]),
            "rust_orphans": len(result["rust_orphans"]),
            "issues_by_severity": {k: len(v) for k, v in issues.items()},
            "total_issues": total_issues,
        },
        "issues": issues,
        "matched_pairs": result["matched_pairs"],
        "sql_orphans": result["sql_orphans"],
        "rust_orphans": result["rust_orphans"],
        "sql_tables": {k: {"columns": v["columns"], "source": v["source"]} for k, v in sql_tables.items()},
        "rust_structs": rust_structs,
    }
    with open(json_out, "w") as f:
        json.dump(output, f, indent=2, default=str)

    # Print summary to console
    print(f"\n{'='*80}")
    print("AUDIT SUMMARY")
    print(f"{'='*80}")
    print(f"  SQL Tables:       {len(sql_tables)}")
    print(f"  Rust Structs:     {len(rust_structs)}")
    print(f"  Matched Pairs:    {len(result['matched_pairs'])}")
    print(f"  Orphan Tables:    {len(result['sql_orphans'])}")
    print(f"  Orphan Structs:   {len(result['rust_orphans'])}")
    print(f"  ---")
    print(f"  CRITICAL issues:  {len(issues['critical'])}")
    print(f"  HIGH issues:      {len(issues['high'])}")
    print(f"  MEDIUM issues:    {len(issues['medium'])}")
    print(f"  LOW issues:       {len(issues['low'])}")
    print(f"  INFO items:       {len(issues['info'])}")
    print(f"  TOTAL issues:     {total_issues}")
    print(f"{'='*80}")


if __name__ == "__main__":
    main()
