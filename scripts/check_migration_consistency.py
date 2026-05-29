#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import json
import re
from pathlib import Path


REQUIRED_V7_BATCHES = [
    "20260515000001_consolidated_schema_contract_and_features_v7.sql",
    "20260515000002_consolidated_stream_ordering_online_fix_v7.sql",
    "20260515000003_consolidated_drop_redundant_tables_v7.sql",
    "20260515000004_consolidated_schema_fixes_v7.sql",
    "20260515000005_consolidated_table_indexes_v7.sql",
    "20260515000006_consolidated_constraint_governance_v7.sql",
    "20260515000007_rooms_summaries_materialized_view_v7.sql",
    "20260515000008_consolidated_field_rename_expires_at_v7.sql",
]
TIMESTAMP_RE = re.compile(r"^\d{14}_.*\.sql$")


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def requires_undo(path: Path) -> bool:
    if path.name.startswith("00000000_unified_schema_v"):
        return False
    if path.name.startswith("00000001_extensions_"):
        return False
    return bool(TIMESTAMP_RE.match(path.name) or path.name.startswith("V"))


def collect_forward_sql(path: Path) -> list[Path]:
    return sorted(
        item for item in path.glob("*.sql") if not item.name.endswith(".undo.sql")
    )


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Validate migration/deploy mirror consistency."
    )
    parser.add_argument(
        "--json-report",
        help="Optional path where the JSON report is written.",
    )
    args = parser.parse_args()

    project_root = Path(__file__).resolve().parent.parent
    primary_dir = project_root / "migrations"
    deploy_dir = project_root / "docker" / "deploy" / "migrations"

    issues: list[dict[str, str]] = []
    warnings: list[dict[str, str]] = []

    primary_forward = collect_forward_sql(primary_dir)
    deploy_forward = collect_forward_sql(deploy_dir)

    primary_names = {path.name for path in primary_forward}
    deploy_names = {path.name for path in deploy_forward}

    for filename in REQUIRED_V7_BATCHES:
        if filename not in primary_names:
            issues.append({"type": "missing_primary_batch", "file": filename})
        if filename not in deploy_names:
            issues.append({"type": "missing_deploy_batch", "file": filename})

    for path in primary_forward:
        mirror = deploy_dir / path.name
        if not mirror.exists():
            issues.append({"type": "missing_deploy_mirror", "file": path.name})
            continue
        if sha256(path) != sha256(mirror):
            issues.append({"type": "content_mismatch", "file": path.name})

        if requires_undo(path):
            undo_name = path.with_suffix(".undo.sql").name
            if not (primary_dir / undo_name).exists():
                issues.append({"type": "missing_primary_undo", "file": undo_name})
            if not (deploy_dir / undo_name).exists():
                issues.append({"type": "missing_deploy_undo", "file": undo_name})

    for extra in sorted(deploy_names - primary_names):
        warnings.append({"type": "deploy_extra_file", "file": extra})

    latest_baselines = sorted(
        name for name in primary_names if name.startswith("00000000_unified_schema_v")
    )
    if latest_baselines and latest_baselines[-1] != "00000000_unified_schema_v7.sql":
        issues.append(
            {
                "type": "unexpected_latest_baseline",
                "file": latest_baselines[-1],
            }
        )

    report = {
        "status": "ok" if not issues else "failed",
        "summary": {
            "issues": len(issues),
            "warnings": len(warnings),
            "primary_forward_files": len(primary_forward),
            "deploy_forward_files": len(deploy_forward),
        },
        "issues": issues,
        "warnings": warnings,
    }

    if args.json_report:
        json_path = Path(args.json_report)
        json_path.parent.mkdir(parents=True, exist_ok=True)
        json_path.write_text(
            json.dumps(report, ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )

    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0 if not issues else 1


if __name__ == "__main__":
    raise SystemExit(main())
