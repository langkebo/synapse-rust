import argparse
import csv
import pathlib
import re
from collections import Counter, defaultdict


def read_csv(path):
    with open(path, newline="", encoding="utf-8") as f:
        return list(csv.DictReader(f))


def ensure_dir(path):
    path.mkdir(parents=True, exist_ok=True)


def bool_standard_name(name):
    if name.startswith(("is_", "has_")):
        return name
    if name.startswith("can_"):
        return "is_" + name
    if name.startswith("allow_"):
        return "is_" + name[6:]
    if name.endswith("ed"):
        return "is_" + name
    return "is_" + name


def time_standard_name(name):
    if name.endswith("_at"):
        return name[:-3] + "_ts"
    return name


def build_reports(columns, constraints, foreign_keys):
    snake = re.compile(r"^[a-z][a-z0-9_]*$")
    all_tables = sorted({r["table_name"] for r in columns})
    bool_columns = [r for r in columns if r["data_type"] == "boolean"]
    snake_violations = [r for r in columns if not snake.match(r["column_name"])]
    bool_violations = [
        r for r in bool_columns if not r["column_name"].startswith(("is_", "has_"))
    ]
    pk_cols = {(r["table_name"], r["column_name"]) for r in constraints if r["constraint_type"] == "PRIMARY KEY"}
    fk_cols = {(r["source_table"], r["source_column"]) for r in foreign_keys}

    type_by_name = defaultdict(set)
    for r in columns:
        type_by_name[r["column_name"]].add(r["udt_name"])
    id_type_variants = {
        k: sorted(v)
        for k, v in type_by_name.items()
        if (k.endswith("_id") or k == "id") and len(v) > 1
    }

    table_columns = defaultdict(set)
    for r in columns:
        table_columns[r["table_name"]].add(r["column_name"])
    redundancy_pairs = []
    for table, names in table_columns.items():
        if "as_id" in names and "appservice_id" in names:
            redundancy_pairs.append((table, "as_id", "appservice_id"))
        if "created_at" in names and "created_ts" in names:
            redundancy_pairs.append((table, "created_at", "created_ts"))
        if "updated_at" in names and "updated_ts" in names:
            redundancy_pairs.append((table, "updated_at", "updated_ts"))
        if "enabled" in names and "is_enabled" in names:
            redundancy_pairs.append((table, "enabled", "is_enabled"))

    allow_non_fk = {
        "event_id", "transaction_id", "request_id", "backup_id", "media_id", "key_id",
        "rule_id", "session_id", "pgt_id", "ticket_id", "target_worker_id", "name_id"
    }
    missing_fk_candidates = []
    for r in columns:
        key = (r["table_name"], r["column_name"])
        if not r["column_name"].endswith("_id"):
            continue
        if r["column_name"] in allow_non_fk:
            continue
        if key in fk_cols or key in pk_cols:
            continue
        if r["column_name"] == "id":
            continue
        missing_fk_candidates.append(r)

    issues = []
    for r in snake_violations:
        issues.append({
            "severity": "high",
            "category": "naming",
            "table_name": r["table_name"],
            "column_name": r["column_name"],
            "current_type": r["udt_name"],
            "recommended": "使用 snake_case 字段名",
            "detail": "字段命名违反 snake_case 规则"
        })
    for r in bool_violations:
        issues.append({
            "severity": "medium",
            "category": "boolean_naming",
            "table_name": r["table_name"],
            "column_name": r["column_name"],
            "current_type": r["udt_name"],
            "recommended": bool_standard_name(r["column_name"]),
            "detail": "布尔字段未使用 is_/has_ 前缀"
        })
    for col, types in sorted(id_type_variants.items()):
        issues.append({
            "severity": "high",
            "category": "id_type_inconsistent",
            "table_name": "*",
            "column_name": col,
            "current_type": "|".join(types),
            "recommended": "统一为 varchar(255) 或 text 并在同语义域保持一致",
            "detail": "同名 ID 字段存在多种数据类型"
        })
    for table, left, right in redundancy_pairs:
        issues.append({
            "severity": "high",
            "category": "redundant_columns",
            "table_name": table,
            "column_name": f"{left}|{right}",
            "current_type": "-",
            "recommended": f"保留单一规范字段并建立兼容迁移",
            "detail": "存在语义重叠字段"
        })
    for r in missing_fk_candidates:
        issues.append({
            "severity": "medium",
            "category": "missing_fk",
            "table_name": r["table_name"],
            "column_name": r["column_name"],
            "current_type": r["udt_name"],
            "recommended": "评估并补充外键或显式记录无外键原因",
            "detail": "疑似关联字段缺少主外键约束"
        })

    mapping = []
    for r in columns:
        rec_name = r["column_name"]
        rec_type = r["udt_name"]
        note = ""
        if r["data_type"] == "boolean" and not rec_name.startswith(("is_", "has_")):
            rec_name = bool_standard_name(rec_name)
            note = "布尔命名标准化"
        if rec_name.endswith("_at"):
            rec_name = time_standard_name(rec_name)
            note = "时间字段统一为 _ts"
        if r["column_name"] in ("as_id", "appservice_id"):
            rec_name = "as_id"
            rec_type = "varchar(255)"
            note = "应用服务主关联键统一"
        if r["column_name"].endswith("_id") and r["udt_name"] in ("text", "varchar"):
            rec_type = "varchar(255)"
        if note:
            mapping.append({
                "table_name": r["table_name"],
                "column_name": r["column_name"],
                "current_type": r["udt_name"],
                "recommended_name": rec_name,
                "recommended_type": rec_type,
                "note": note
            })

    stats = {
        "table_count": len(all_tables),
        "column_count": len(columns),
        "constraint_count": len(constraints),
        "fk_count": len(foreign_keys),
        "snake_violations": len(snake_violations),
        "bool_violations": len(bool_violations),
        "id_type_variant_count": len(id_type_variants),
        "redundancy_count": len(redundancy_pairs),
        "missing_fk_candidates": len(missing_fk_candidates),
        "issue_count": len(issues),
        "issue_categories": Counter(i["category"] for i in issues),
    }
    return stats, issues, mapping


def write_csv(path, rows, fields):
    with open(path, "w", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(f, fieldnames=fields)
        writer.writeheader()
        for row in rows:
            writer.writerow(row)


def write_markdown(path, stats, issues):
    top = sorted(issues, key=lambda x: (x["severity"] != "high", x["category"], x["table_name"], x["column_name"]))[:80]
    lines = []
    lines.append("# 数据库字段审计报告")
    lines.append("")
    lines.append("## 概览")
    lines.append("")
    lines.append(f"- 表数量: {stats['table_count']}")
    lines.append(f"- 字段数量: {stats['column_count']}")
    lines.append(f"- 约束数量: {stats['constraint_count']}")
    lines.append(f"- 外键数量: {stats['fk_count']}")
    lines.append(f"- 审计问题总数: {stats['issue_count']}")
    lines.append("")
    lines.append("## 关键问题统计")
    lines.append("")
    lines.append(f"- snake_case 命名违规: {stats['snake_violations']}")
    lines.append(f"- 布尔前缀违规: {stats['bool_violations']}")
    lines.append(f"- 同名 ID 类型不一致: {stats['id_type_variant_count']}")
    lines.append(f"- 冗余字段对: {stats['redundancy_count']}")
    lines.append(f"- 疑似缺失外键: {stats['missing_fk_candidates']}")
    lines.append("")
    lines.append("## 问题分类")
    lines.append("")
    for k, v in stats["issue_categories"].most_common():
        lines.append(f"- {k}: {v}")
    lines.append("")
    lines.append("## 高优先级问题样例")
    lines.append("")
    lines.append("| 严重级别 | 类别 | 表 | 字段 | 当前类型 | 建议 | 说明 |")
    lines.append("|---|---|---|---|---|---|---|")
    for i in top:
        lines.append(
            f"| {i['severity']} | {i['category']} | {i['table_name']} | {i['column_name']} | {i['current_type']} | {i['recommended']} | {i['detail']} |"
        )
    with open(path, "w", encoding="utf-8") as f:
        f.write("\n".join(lines) + "\n")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--columns", default="/home/tzd/synapse-rust/reports/db_audit/columns.csv")
    parser.add_argument("--constraints", default="/home/tzd/synapse-rust/reports/db_audit/constraints.csv")
    parser.add_argument("--foreign-keys", default="/home/tzd/synapse-rust/reports/db_audit/foreign_keys.csv")
    parser.add_argument("--output-dir", default="/home/tzd/synapse-rust/docs/database-field-standardization")
    args = parser.parse_args()

    columns = read_csv(args.columns)
    constraints = read_csv(args.constraints)
    foreign_keys = read_csv(args.foreign_keys)
    stats, issues, mapping = build_reports(columns, constraints, foreign_keys)

    out = pathlib.Path(args.output_dir)
    ensure_dir(out)

    write_csv(
        out / "issues.csv",
        issues,
        ["severity", "category", "table_name", "column_name", "current_type", "recommended", "detail"],
    )
    write_csv(
        out / "field-mapping.csv",
        mapping,
        ["table_name", "column_name", "current_type", "recommended_name", "recommended_type", "note"],
    )
    write_markdown(out / "audit-current-state.md", stats, issues)


if __name__ == "__main__":
    main()
