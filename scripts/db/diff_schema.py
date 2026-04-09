#!/usr/bin/env python3
"""
Schema Drift Detection Tool
Compares two schema snapshots (database vs expected from migrations)
and generates a detailed drift report.
"""

import os
import sys
import json
import argparse
from typing import Dict, List, Any, Optional, Set
from dataclasses import dataclass
from enum import Enum


class Severity(Enum):
    BLOCKER = "🔴 Blocker"
    WARNING = "🟡 Warning"
    INFO = "🟢 Info"


@dataclass
class DriftItem:
    severity: Severity
    drift_type: str
    table: str
    item: Optional[str]
    expected: Optional[str]
    actual: Optional[str]
    description: str


@dataclass
class DriftReport:
    expected_version: Optional[str]
    actual_version: Optional[str]
    detected_at: str
    drift_items: List[DriftItem]
    summary: Dict[str, int]
    blocked: bool


def load_schema_from_json(json_file: str) -> Dict[str, Any]:
    """Load schema from JSON file."""
    with open(json_file, 'r') as f:
        return json.load(f)


def compare_columns(
    table_name: str,
    expected_cols: List[Dict],
    actual_cols: List[Dict]
) -> List[DriftItem]:
    """Compare columns between expected and actual schema."""
    items = []

    expected_map = {c['name']: c for c in expected_cols}
    actual_map = {c['name']: c for c in actual_cols}

    expected_names = set(expected_map.keys())
    actual_names = set(actual_map.keys())

    for col_name in expected_names - actual_names:
        items.append(DriftItem(
            severity=Severity.BLOCKER,
            drift_type="MISSING_COLUMN",
            table=table_name,
            item=col_name,
            expected=expected_map[col_name].get('data_type', 'unknown'),
            actual=None,
            description=f"Column '{col_name}' exists in expected schema but not in actual database"
        ))

    for col_name in actual_names - expected_names:
        items.append(DriftItem(
            severity=Severity.WARNING,
            drift_type="EXTRA_COLUMN",
            table=table_name,
            item=col_name,
            expected=None,
            actual=actual_map[col_name].get('data_type', 'unknown'),
            description=f"Column '{col_name}' exists in database but not in migration scripts"
        ))

    for col_name in expected_names & actual_names:
        exp = expected_map[col_name]
        act = actual_map[col_name]

        if exp.get('data_type') != act.get('data_type'):
            items.append(DriftItem(
                severity=Severity.BLOCKER,
                drift_type="TYPE_MISMATCH",
                table=table_name,
                item=col_name,
                expected=exp.get('data_type'),
                actual=act.get('data_type'),
                description=f"Column '{col_name}' type mismatch: expected {exp.get('data_type')}, got {act.get('data_type')}"
            ))

        if exp.get('is_nullable') != act.get('is_nullable'):
            items.append(DriftItem(
                severity=Severity.WARNING,
                drift_type="NULLABLE_MISMATCH",
                table=table_name,
                item=col_name,
                expected=str(exp.get('is_nullable')),
                actual=str(act.get('is_nullable')),
                description=f"Column '{col_name}' nullable mismatch"
            ))

        if exp.get('is_primary_key') != act.get('is_primary_key'):
            items.append(DriftItem(
                severity=Severity.BLOCKER,
                drift_type="PRIMARY_KEY_MISMATCH",
                table=table_name,
                item=col_name,
                expected=str(exp.get('is_primary_key')),
                actual=str(act.get('is_primary_key')),
                description=f"Column '{col_name}' primary key mismatch"
            ))

    return items


def compare_indexes(
    table_name: str,
    expected_idxs: List[Dict],
    actual_idxs: List[Dict]
) -> List[DriftItem]:
    """Compare indexes between expected and actual schema."""
    items = []

    expected_map = {idx['name']: idx for idx in expected_idxs}
    actual_map = {idx['name']: idx for idx in actual_idxs}

    expected_names = set(expected_map.keys())
    actual_names = set(actual_map.keys())

    for idx_name in expected_names - actual_names:
        items.append(DriftItem(
            severity=Severity.WARNING,
            drift_type="MISSING_INDEX",
            table=table_name,
            item=idx_name,
            expected=", ".join(expected_map[idx_name].get('columns', [])),
            actual=None,
            description=f"Index '{idx_name}' exists in migration scripts but not in database"
        ))

    for idx_name in actual_names - expected_names:
        items.append(DriftItem(
            severity=Severity.INFO,
            drift_type="EXTRA_INDEX",
            table=table_name,
            item=idx_name,
            expected=None,
            actual=", ".join(actual_map[idx_name].get('columns', [])),
            description=f"Index '{idx_name}' exists in database but not in migration scripts"
        ))

    return items


def compare_schemas(
    expected_schema: Dict[str, Any],
    actual_schema: Dict[str, Any]
) -> DriftReport:
    """Compare two schemas and generate drift report."""
    from datetime import datetime

    drift_items = []

    expected_tables = expected_schema.get('tables', {})
    actual_tables = actual_schema.get('tables', {})

    expected_table_names = set(expected_tables.keys())
    actual_table_names = set(actual_tables.keys())

    for table_name in expected_table_names - actual_table_names:
        drift_items.append(DriftItem(
            severity=Severity.BLOCKER,
            drift_type="MISSING_TABLE",
            table=table_name,
            item=None,
            expected=f"Table '{table_name}' exists in migrations",
            actual="Table not found in database",
            description=f"Table '{table_name}' exists in migration scripts but not in database"
        ))

    for table_name in actual_table_names - expected_table_names:
        drift_items.append(DriftItem(
            severity=Severity.WARNING,
            drift_type="EXTRA_TABLE",
            table=table_name,
            item=None,
            expected="Table not in migrations",
            actual=f"Table '{table_name}' exists in database",
            description=f"Table '{table_name}' exists in database but not in migration scripts"
        ))

    for table_name in expected_table_names & actual_table_names:
        expected_table = expected_tables[table_name]
        actual_table = actual_tables[table_name]

        drift_items.extend(compare_columns(
            table_name,
            expected_table.get('columns', []),
            actual_table.get('columns', [])
        ))

        drift_items.extend(compare_indexes(
            table_name,
            expected_table.get('indexes', []),
            actual_table.get('indexes', [])
        ))

    summary = {
        'total': len(drift_items),
        'blockers': sum(1 for i in drift_items if i.severity == Severity.BLOCKER),
        'warnings': sum(1 for i in drift_items if i.severity == Severity.WARNING),
        'info': sum(1 for i in drift_items if i.severity == Severity.INFO),
    }

    blocked = summary['blockers'] > 0

    return DriftReport(
        expected_version=expected_schema.get('version'),
        actual_version=actual_schema.get('version'),
        detected_at=datetime.now().isoformat(),
        drift_items=drift_items,
        summary=summary,
        blocked=blocked
    )


def format_report(report: DriftReport, format: str = 'text') -> str:
    """Format drift report in specified format."""
    if format == 'json':
        payload = {
            "expected_version": report.expected_version,
            "actual_version": report.actual_version,
            "detected_at": report.detected_at,
            "summary": report.summary,
            "blocked": report.blocked,
            "drift_items": [
                {
                    "severity": item.severity.name,
                    "severity_label": item.severity.value,
                    "drift_type": item.drift_type,
                    "table": item.table,
                    "item": item.item,
                    "expected": item.expected,
                    "actual": item.actual,
                    "description": item.description,
                }
                for item in report.drift_items
            ],
        }
        return json.dumps(payload, indent=2, ensure_ascii=False)

    if format == 'markdown':
        lines = []
        lines.append("# Schema Drift Detection Report")
        lines.append("")
        lines.append(f"- Detected at: `{report.detected_at}`")
        lines.append(f"- Expected version: `{report.expected_version or 'N/A'}`")
        lines.append(f"- Actual version: `{report.actual_version or 'N/A'}`")
        lines.append("")
        lines.append("## Summary")
        lines.append("")
        lines.append("| Total | Blockers | Warnings | Info | Blocked |")
        lines.append("|---:|---:|---:|---:|:---:|")
        lines.append(
            f"| {report.summary['total']} | {report.summary['blockers']} | {report.summary['warnings']} | {report.summary['info']} | {'YES' if report.blocked else 'NO'} |"
        )
        lines.append("")
        lines.append("## Drift Items")
        lines.append("")
        if not report.drift_items:
            lines.append("No drift items detected.")
            return "\n".join(lines)

        def severity_rank(s: Severity) -> int:
            return {Severity.BLOCKER: 0, Severity.WARNING: 1, Severity.INFO: 2}.get(s, 99)

        items = sorted(report.drift_items, key=lambda i: (severity_rank(i.severity), i.drift_type, i.table, i.item or ""))
        current_severity = None
        for item in items:
            if item.severity != current_severity:
                lines.append(f"### {item.severity.value}")
                lines.append("")
                current_severity = item.severity

            parts = [f"- **{item.drift_type}**: `{item.table}`"]
            if item.item:
                parts[0] += f" / `{item.item}`"
            lines.append(parts[0])
            if item.expected is not None or item.actual is not None:
                lines.append("")
                lines.append("| Expected | Actual |")
                lines.append("|---|---|")
                lines.append(f"| {item.expected or ''} | {item.actual or ''} |")
            lines.append("")
            lines.append(f"{item.description}")
            lines.append("")

        return "\n".join(lines)

    lines = []
    lines.append("=" * 80)
    lines.append("SCHEMA DRIFT DETECTION REPORT")
    lines.append("=" * 80)
    lines.append(f"Detected at: {report.detected_at}")
    lines.append(f"Expected version: {report.expected_version or 'N/A'}")
    lines.append(f"Actual version: {report.actual_version or 'N/A'}")
    lines.append("")
    lines.append("SUMMARY")
    lines.append("-" * 40)
    lines.append(f"Total drift items: {report.summary['total']}")
    lines.append(f"🔴 Blockers: {report.summary['blockers']}")
    lines.append(f"🟡 Warnings: {report.summary['warnings']}")
    lines.append(f"🟢 Info: {report.summary['info']}")
    lines.append("")

    if report.blocked:
        lines.append("⚠️  MERGE BLOCKED - Schema drift detected with blockers")
    else:
        lines.append("✅ No blockers detected")

    lines.append("")
    lines.append("DRIFT ITEMS")
    lines.append("-" * 40)

    if not report.drift_items:
        lines.append("No drift items detected.")
    else:
        current_severity = None
        for item in report.drift_items:
            if item.severity != current_severity:
                lines.append("")
                lines.append(f"{item.severity.value} {item.drift_type}")
                lines.append("-" * 40)
                current_severity = item.severity

            lines.append(f"  Table: {item.table}")
            if item.item:
                lines.append(f"  Item: {item.item}")
            if item.expected:
                lines.append(f"  Expected: {item.expected}")
            if item.actual:
                lines.append(f"  Actual: {item.actual}")
            lines.append(f"  Description: {item.description}")
            lines.append("")

    lines.append("=" * 80)

    return "\n".join(lines)


def main():
    """Main entry point."""
    parser = argparse.ArgumentParser(description='Detect schema drift')
    parser.add_argument('expected', help='Expected schema JSON file (from migrations)')
    parser.add_argument('actual', help='Actual schema JSON file (from database)')
    parser.add_argument('--format', '-f', choices=['text', 'json', 'markdown'], default='text')
    parser.add_argument('--output', '-o', help='Output file (default: stdout)')
    parser.add_argument('--exit-code', action='store_true', help='Exit with code 1 if blocked')

    args = parser.parse_args()

    try:
        expected_schema = load_schema_from_json(args.expected)
        actual_schema = load_schema_from_json(args.actual)
    except FileNotFoundError as e:
        print(f"Error: File not found: {e}", file=sys.stderr)
        sys.exit(1)
    except json.JSONDecodeError as e:
        print(f"Error: Invalid JSON: {e}", file=sys.stderr)
        sys.exit(1)

    report = compare_schemas(expected_schema, actual_schema)
    output = format_report(report, args.format)

    if args.output:
        with open(args.output, 'w') as f:
            f.write(output)
    else:
        print(output)

    if args.exit_code and report.blocked:
        sys.exit(1)


if __name__ == '__main__':
    main()
