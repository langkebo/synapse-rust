#!/usr/bin/env python3
"""
Schema Contract Coverage Checker

Validates that migration files define expected tables, columns, indexes, and constraints.
Enhanced version with coverage threshold support and detailed reporting.

Usage:
    python3 check_schema_contract_coverage.py [--threshold PERCENT] [--report OUTPUT]
"""

import argparse
import pathlib
import re
import sys
import json
from typing import Dict, List, Set, Tuple

ROOT = pathlib.Path(__file__).resolve().parents[1]
MIGRATIONS_DIR = ROOT / "migrations"

TABLE_PATTERN = re.compile(
    r"CREATE\s+TABLE(?:\s+IF\s+NOT\s+EXISTS)?\s+([a-z_][a-z0-9_]*)\s*\((.*?)\);",
    re.IGNORECASE | re.DOTALL,
)
INDEX_PATTERN = re.compile(
    r"CREATE\s+(?:UNIQUE\s+)?INDEX(?:\s+CONCURRENTLY)?(?:\s+IF\s+NOT\s+EXISTS)?\s+([a-z_][a-z0-9_]*)\s+ON\s+([a-z_][a-z0-9_]*)",
    re.IGNORECASE,
)
CONSTRAINT_PATTERN = re.compile(
    r"\bCONSTRAINT\s+([a-z_][a-z0-9_]*)\b",
    re.IGNORECASE,
)
COLUMN_PATTERN = re.compile(r"^([a-z_][a-z0-9_]*)\s+", re.IGNORECASE)

TABLE_CONTRACTS: Dict[str, Dict[str, List[str]]] = {
    "account_data": {
        "columns": ["user_id", "data_type", "content", "created_ts", "updated_ts"],
        "indexes": ["idx_account_data_user"],
        "constraints": ["pk_account_data", "uq_account_data_user_type"],
    },
    "events": {
        "columns": [
            "event_id",
            "room_id",
            "sender",
            "event_type",
            "content",
            "origin_server_ts",
            "state_key",
            "is_redacted",
            "unsigned",
        ],
        "indexes": [
            "idx_events_sender",
            "idx_events_type",
            "idx_events_origin_server_ts",
            "idx_events_not_redacted",
        ],
        "constraints": ["pk_events", "fk_events_room"],
    },
    "push_rules": {
        "columns": [
            "user_id",
            "scope",
            "rule_id",
            "kind",
            "conditions",
            "actions",
            "pattern",
            "is_default",
            "is_enabled",
            "created_ts",
            "updated_ts",
        ],
        "indexes": ["idx_push_rules_user"],
        "constraints": ["pk_push_rules", "uq_push_rules_user_scope_kind_rule"],
    },
    "room_account_data": {
        "columns": [
            "user_id",
            "room_id",
            "data_type",
            "data",
            "created_ts",
            "updated_ts",
        ],
        "constraints": ["pk_room_account_data", "uq_room_account_data_user_room_type"],
    },
    "room_memberships": {
        "columns": [
            "room_id",
            "user_id",
            "membership",
            "joined_ts",
            "invited_ts",
            "left_ts",
            "banned_ts",
            "sender",
            "event_id",
            "is_banned",
            "updated_ts",
        ],
        "indexes": [
            "idx_room_memberships_room",
            "idx_room_memberships_user",
            "idx_room_memberships_membership",
            "idx_room_memberships_user_membership",
            "idx_room_memberships_joined",
        ],
        "constraints": [
            "pk_room_memberships",
            "uq_room_memberships_room_user",
            "fk_room_memberships_room",
            "fk_room_memberships_user",
        ],
    },
    "room_retention_policies": {
        "columns": [
            "room_id",
            "max_lifetime",
            "min_lifetime",
            "is_expire_on_clients",
            "is_server_default",
            "created_ts",
            "updated_ts",
        ],
        "indexes": ["idx_room_retention_policies_server_default"],
        "constraints": [
            "uq_room_retention_policies_room",
            "fk_room_retention_policies_room",
        ],
    },
    "room_summary_state": {
        "columns": [
            "room_id",
            "event_type",
            "state_key",
            "event_id",
            "content",
            "updated_ts",
        ],
        "indexes": ["idx_room_summary_state_room"],
        "constraints": [
            "uq_room_summary_state_room_type_state",
            "fk_room_summary_state_room",
        ],
    },
    "room_summary_stats": {
        "columns": [
            "room_id",
            "total_events",
            "total_state_events",
            "total_messages",
            "total_media",
            "storage_size",
            "last_updated_ts",
        ],
        "constraints": ["fk_room_summary_stats_room"],
    },
    "room_summary_update_queue": {
        "columns": [
            "room_id",
            "event_id",
            "event_type",
            "state_key",
            "priority",
            "status",
            "created_ts",
            "processed_ts",
            "error_message",
            "retry_count",
        ],
        "indexes": ["idx_room_summary_update_queue_status_priority_created"],
        "constraints": ["fk_room_summary_update_queue_room"],
    },
    "search_index": {
        "columns": [
            "event_id",
            "room_id",
            "user_id",
            "event_type",
            "type",
            "content",
            "created_ts",
            "updated_ts",
        ],
        "indexes": [
            "idx_search_index_room",
            "idx_search_index_user",
            "idx_search_index_type",
        ],
        "constraints": ["uq_search_index_event"],
    },
    "device_trust_status": {
        "columns": [
            "user_id",
            "device_id",
            "trust_level",
            "verified_by_device_id",
            "verified_at",
            "created_ts",
            "updated_ts",
        ],
        "indexes": ["idx_device_trust_status_user_level"],
        "constraints": ["uq_device_trust_status_user_device"],
    },
    "cross_signing_trust": {
        "columns": [
            "user_id",
            "target_user_id",
            "master_key_id",
            "is_trusted",
            "trusted_at",
            "created_ts",
            "updated_ts",
        ],
        "indexes": ["idx_cross_signing_trust_user_trusted"],
        "constraints": ["uq_cross_signing_trust_user_target"],
    },
    "device_verification_request": {
        "columns": [
            "user_id",
            "new_device_id",
            "requesting_device_id",
            "verification_method",
            "status",
            "request_token",
            "commitment",
            "pubkey",
            "created_ts",
            "expires_at",
            "completed_at",
        ],
        "indexes": [
            "idx_device_verification_request_user_device_pending",
            "idx_device_verification_request_expires_pending",
        ],
    },
    "verification_requests": {
        "columns": [
            "transaction_id",
            "from_user",
            "from_device",
            "to_user",
            "to_device",
            "method",
            "state",
            "created_ts",
            "updated_ts",
        ],
        "indexes": ["idx_verification_requests_to_user_state"],
    },
    "verification_sas": {
        "columns": [
            "tx_id",
            "from_device",
            "to_device",
            "method",
            "state",
            "exchange_hashes",
            "commitment",
            "pubkey",
            "sas_bytes",
            "mac",
        ],
    },
    "verification_qr": {
        "columns": [
            "tx_id",
            "from_device",
            "to_device",
            "state",
            "qr_code_data",
            "scanned_data",
        ],
    },
    "replication_positions": {
        "columns": ["worker_id", "stream_name", "stream_position", "updated_ts"],
        "constraints": [
            "uq_replication_positions_worker_stream",
            "fk_replication_positions_worker",
        ],
    },
    "worker_task_assignments": {
        "columns": [
            "task_id",
            "task_type",
            "task_data",
            "priority",
            "status",
            "assigned_worker_id",
            "assigned_ts",
            "created_ts",
            "completed_ts",
            "result",
            "error_message",
        ],
        "indexes": [
            "idx_worker_task_assignments_status_priority_created",
            "idx_worker_task_assignments_worker_status",
        ],
        "constraints": ["fk_worker_task_assignments_worker"],
    },
}


def iter_sql_files() -> List[pathlib.Path]:
    return sorted(MIGRATIONS_DIR.rglob("*.sql"))


def collect_schema_metadata() -> Tuple[
    Dict[str, Set[str]], Dict[str, Set[str]], Dict[str, Set[str]]
]:
    table_columns: Dict[str, Set[str]] = {}
    table_constraints: Dict[str, Set[str]] = {}
    table_indexes: Dict[str, Set[str]] = {}

    for path in iter_sql_files():
        text = path.read_text()

        for match in TABLE_PATTERN.finditer(text):
            table_name = match.group(1).lower()
            body = match.group(2)
            columns = table_columns.setdefault(table_name, set())
            constraints = table_constraints.setdefault(table_name, set())

            for raw_line in body.splitlines():
                line = raw_line.strip().rstrip(",")
                if not line:
                    continue
                constraint_match = CONSTRAINT_PATTERN.search(line)
                if constraint_match:
                    constraints.add(constraint_match.group(1).lower())
                    continue
                if line.upper().startswith(
                    ("PRIMARY KEY", "UNIQUE", "FOREIGN KEY", "CHECK", "EXCLUDE")
                ):
                    continue
                column_match = COLUMN_PATTERN.match(line)
                if column_match:
                    columns.add(column_match.group(1).lower())

        for match in INDEX_PATTERN.finditer(text):
            index_name = match.group(1).lower()
            table_name = match.group(2).lower()
            table_indexes.setdefault(table_name, set()).add(index_name)

    return table_columns, table_indexes, table_constraints


def print_contract_scope_note() -> None:
    print(
        "Schema contract coverage note: checks migration source coverage for expected "
        "tables, columns, indexes, and constraints; it is not a full ALTER TABLE interpreter."
    )


def calculate_coverage(
    table_columns: Dict[str, Set[str]],
    table_indexes: Dict[str, Set[str]],
    table_constraints: Dict[str, Set[str]],
) -> Tuple[int, int, List[str]]:
    """Calculate coverage percentage and return (passed, total, failures)."""
    total_checks = 0
    passed_checks = 0
    failures: List[str] = []

    for table_name, contract in sorted(TABLE_CONTRACTS.items()):
        # Check table existence
        total_checks += 1
        if table_name not in table_columns:
            failures.append(f"- {table_name}: missing table definition")
            continue
        passed_checks += 1

        # Check columns
        for column in contract.get("columns", []):
            total_checks += 1
            if column.lower() in table_columns[table_name]:
                passed_checks += 1
            else:
                failures.append(f"- {table_name}: missing column '{column}'")

        # Check indexes
        for index in contract.get("indexes", []):
            total_checks += 1
            if index.lower() in table_indexes.get(table_name, set()):
                passed_checks += 1
            else:
                failures.append(f"- {table_name}: missing index '{index}'")

        # Check constraints
        for constraint in contract.get("constraints", []):
            total_checks += 1
            if constraint.lower() in table_constraints.get(table_name, set()):
                passed_checks += 1
            else:
                failures.append(f"- {table_name}: missing constraint '{constraint}'")

    return passed_checks, total_checks, failures


def generate_coverage_report(
    passed: int, total: int, failures: List[str], output_path: str
) -> None:
    """Generate detailed coverage report in Markdown format."""
    coverage_pct = (passed / total * 100) if total > 0 else 0

    with open(output_path, "w") as f:
        f.write("# Schema Contract Coverage Report\n\n")
        f.write(f"> Generated: {pathlib.Path.cwd()}\n\n")
        f.write("## Summary\n\n")
        f.write(
            f"- **Coverage**: {coverage_pct:.1f}% ({passed}/{total} checks passed)\n"
        )
        f.write(f"- **Tables Checked**: {len(TABLE_CONTRACTS)}\n")
        f.write(f"- **Status**: {'✅ PASS' if not failures else '❌ FAIL'}\n\n")

        if failures:
            f.write("## Failures\n\n")
            for failure in failures:
                f.write(f"{failure}\n")
            f.write("\n")

        f.write("## Contract Details\n\n")
        for table_name, contract in sorted(TABLE_CONTRACTS.items()):
            f.write(f"### {table_name}\n\n")
            if "columns" in contract:
                f.write(f"- Columns: {len(contract['columns'])}\n")
            if "indexes" in contract:
                f.write(f"- Indexes: {len(contract['indexes'])}\n")
            if "constraints" in contract:
                f.write(f"- Constraints: {len(contract['constraints'])}\n")
            f.write("\n")


def write_json_report(output_path: str | None, payload: dict) -> None:
    if not output_path or output_path == "-":
        print(json.dumps(payload, ensure_ascii=False, indent=2))
        return
    path = pathlib.Path(output_path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )


def structured_failures(failures: List[str], threshold: float) -> List[dict]:
    output = []
    reproduce = (
        f"python3 scripts/check_schema_contract_coverage.py --threshold {threshold:g}"
    )
    for failure in failures:
        text = failure.lstrip("- ").strip()
        if ": missing table definition" in text:
            table = text.split(":", 1)[0].strip()
            output.append(
                {
                    "migration_id": None,
                    "domain": "schema_contract_coverage",
                    "table": table,
                    "column_or_index": None,
                    "query_or_test_name": None,
                    "failure_class": "schema_missing",
                    "reproduce_command": reproduce,
                }
            )
            continue
        if ": missing column '" in text:
            table, rest = text.split(":", 1)
            column = rest.split("missing column '", 1)[1].rstrip("'").strip()
            output.append(
                {
                    "migration_id": None,
                    "domain": "schema_contract_coverage",
                    "table": table.strip(),
                    "column_or_index": f"column:{column}",
                    "query_or_test_name": None,
                    "failure_class": "schema_shape_mismatch",
                    "reproduce_command": reproduce,
                }
            )
            continue
        if ": missing index '" in text:
            table, rest = text.split(":", 1)
            index_name = rest.split("missing index '", 1)[1].rstrip("'").strip()
            output.append(
                {
                    "migration_id": None,
                    "domain": "schema_contract_coverage",
                    "table": table.strip(),
                    "column_or_index": f"index:{index_name}",
                    "query_or_test_name": None,
                    "failure_class": "schema_shape_mismatch",
                    "reproduce_command": reproduce,
                }
            )
            continue
        if ": missing constraint '" in text:
            table, rest = text.split(":", 1)
            constraint_name = (
                rest.split("missing constraint '", 1)[1].rstrip("'").strip()
            )
            output.append(
                {
                    "migration_id": None,
                    "domain": "schema_contract_coverage",
                    "table": table.strip(),
                    "column_or_index": f"constraint:{constraint_name}",
                    "query_or_test_name": None,
                    "failure_class": "schema_shape_mismatch",
                    "reproduce_command": reproduce,
                }
            )
            continue
        output.append(
            {
                "migration_id": None,
                "domain": "schema_contract_coverage",
                "table": None,
                "column_or_index": None,
                "query_or_test_name": None,
                "failure_class": "unknown",
                "reproduce_command": reproduce,
                "message": text,
            }
        )
    return output


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Check schema contract coverage with threshold support"
    )
    parser.add_argument(
        "--threshold",
        type=float,
        default=90.0,
        help="Minimum coverage percentage required (default: 90.0)",
    )
    parser.add_argument(
        "--report", type=str, help="Generate detailed coverage report to file"
    )
    parser.add_argument(
        "--json-report",
        nargs="?",
        const="-",
        help="Write a structured JSON report to a file, or '-' to print to stdout.",
    )
    args = parser.parse_args()

    table_columns, table_indexes, table_constraints = collect_schema_metadata()
    passed, total, failures = calculate_coverage(
        table_columns, table_indexes, table_constraints
    )

    coverage_pct = (passed / total * 100) if total > 0 else 0

    if args.json_report is not None:
        report = {
            "tool": "check_schema_contract_coverage",
            "status": "fail" if failures or coverage_pct < args.threshold else "pass",
            "coverage_pct": coverage_pct,
            "passed": passed,
            "total": total,
            "threshold": args.threshold,
            "failures": structured_failures(failures, args.threshold),
        }
        write_json_report(args.json_report, report)

    print_contract_scope_note()
    print(f"\nCoverage: {coverage_pct:.1f}% ({passed}/{total} checks passed)")
    print(f"Threshold: {args.threshold}%")

    if args.report:
        generate_coverage_report(passed, total, failures, args.report)
        print(f"Report generated: {args.report}")

    if failures:
        print("\nSchema contract coverage failures:")
        for failure in failures:
            print(failure)

    if coverage_pct < args.threshold:
        print(f"\n❌ Coverage {coverage_pct:.1f}% is below threshold {args.threshold}%")
        return 1

    print(f"\n✅ Coverage {coverage_pct:.1f}% meets threshold {args.threshold}%")
    print(
        f"Schema contract coverage passed: "
        f"{len(TABLE_CONTRACTS)} tables checked for columns, indexes, and constraints."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
