#!/usr/bin/env python3
import pathlib
import re
import sys

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

TABLE_CONTRACTS: dict[str, dict[str, list[str]]] = {
    "room_summary_state": {
        "columns": ["room_id", "event_type", "state_key", "event_id", "content", "updated_ts"],
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
    "room_children": {
        "columns": [
            "parent_room_id",
            "child_room_id",
            "state_key",
            "content",
            "suggested",
            "created_ts",
            "updated_ts",
        ],
        "indexes": ["idx_room_children_parent_suggested", "idx_room_children_child"],
        "constraints": [
            "uq_room_children_parent_child",
            "fk_room_children_parent",
            "fk_room_children_child",
        ],
    },
    "retention_cleanup_queue": {
        "columns": [
            "room_id",
            "event_id",
            "event_type",
            "origin_server_ts",
            "scheduled_ts",
            "status",
            "created_ts",
            "processed_ts",
            "error_message",
            "retry_count",
        ],
        "indexes": ["idx_retention_cleanup_queue_status_origin"],
        "constraints": [
            "uq_retention_cleanup_queue_room_event",
            "fk_retention_cleanup_queue_room",
        ],
    },
    "retention_cleanup_logs": {
        "columns": [
            "room_id",
            "events_deleted",
            "state_events_deleted",
            "media_deleted",
            "bytes_freed",
            "started_ts",
            "completed_ts",
            "status",
            "error_message",
        ],
        "indexes": ["idx_retention_cleanup_logs_room_started"],
        "constraints": ["fk_retention_cleanup_logs_room"],
    },
    "retention_stats": {
        "columns": [
            "room_id",
            "total_events",
            "events_in_retention",
            "events_expired",
            "last_cleanup_ts",
            "next_cleanup_ts",
        ],
        "constraints": ["fk_retention_stats_room"],
    },
    "deleted_events_index": {
        "columns": ["room_id", "event_id", "deletion_ts", "reason"],
        "indexes": ["idx_deleted_events_index_room_ts"],
        "constraints": [
            "uq_deleted_events_index_room_event",
            "fk_deleted_events_index_room",
        ],
    },
    "device_trust_status": {
        "columns": [
            "user_id",
            "device_id",
            "trust_level",
            "verified_by_device_id",
            "verified_at",
            "created_at",
            "updated_at",
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
            "created_at",
            "updated_at",
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
            "created_at",
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
            "created_at",
            "updated_at",
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
    "moderation_rules": {
        "columns": [
            "rule_id",
            "rule_type",
            "pattern",
            "action",
            "reason",
            "created_by",
            "created_ts",
            "updated_ts",
            "is_active",
            "priority",
        ],
        "indexes": [
            "idx_moderation_rules_active_priority",
            "idx_moderation_rules_type_active",
        ],
    },
    "moderation_logs": {
        "columns": [
            "event_id",
            "room_id",
            "sender",
            "rule_id",
            "action_taken",
            "content_hash",
            "confidence",
            "created_ts",
        ],
        "indexes": [
            "idx_moderation_logs_event_created",
            "idx_moderation_logs_room_created",
            "idx_moderation_logs_sender_created",
        ],
    },
    "moderation_actions": {
        "columns": [
            "user_id",
            "action_type",
            "reason",
            "report_id",
            "created_ts",
            "expires_at",
        ],
        "indexes": ["idx_moderation_actions_user_created"],
    },
    "replication_positions": {
        "columns": ["worker_id", "stream_name", "stream_position", "updated_ts"],
        "constraints": [
            "uq_replication_positions_worker_stream",
            "fk_replication_positions_worker",
        ],
    },
    "worker_load_stats": {
        "columns": [
            "worker_id",
            "cpu_usage",
            "memory_usage",
            "active_connections",
            "requests_per_second",
            "average_latency_ms",
            "queue_depth",
            "recorded_ts",
        ],
        "indexes": ["idx_worker_load_stats_worker_recorded"],
        "constraints": ["fk_worker_load_stats_worker"],
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
    "worker_connections": {
        "columns": [
            "source_worker_id",
            "target_worker_id",
            "connection_type",
            "status",
            "established_ts",
            "last_activity_ts",
            "bytes_sent",
            "bytes_received",
            "messages_sent",
            "messages_received",
        ],
        "indexes": ["idx_worker_connections_source"],
        "constraints": [
            "uq_worker_connections_pair",
            "fk_worker_connections_source",
            "fk_worker_connections_target",
        ],
    },
}


def iter_sql_files() -> list[pathlib.Path]:
    return sorted(MIGRATIONS_DIR.rglob("*.sql"))


def collect_schema_metadata() -> tuple[dict[str, set[str]], dict[str, set[str]], dict[str, set[str]]]:
    table_columns: dict[str, set[str]] = {}
    table_constraints: dict[str, set[str]] = {}
    table_indexes: dict[str, set[str]] = {}

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


def main() -> int:
    table_columns, table_indexes, table_constraints = collect_schema_metadata()
    failures: list[str] = []

    for table_name, contract in sorted(TABLE_CONTRACTS.items()):
        if table_name not in table_columns:
            failures.append(f"- {table_name}: missing table definition")
            continue

        missing_columns = sorted(
            column
            for column in contract.get("columns", [])
            if column.lower() not in table_columns[table_name]
        )
        if missing_columns:
            failures.append(f"- {table_name}: missing columns {', '.join(missing_columns)}")

        missing_indexes = sorted(
            index
            for index in contract.get("indexes", [])
            if index.lower() not in table_indexes.get(table_name, set())
        )
        if missing_indexes:
            failures.append(f"- {table_name}: missing indexes {', '.join(missing_indexes)}")

        missing_constraints = sorted(
            constraint
            for constraint in contract.get("constraints", [])
            if constraint.lower() not in table_constraints.get(table_name, set())
        )
        if missing_constraints:
            failures.append(
                f"- {table_name}: missing constraints {', '.join(missing_constraints)}"
            )

    if failures:
        print("Schema contract coverage failed:")
        for failure in failures:
            print(failure)
        return 1

    print(
        "Schema contract coverage passed: "
        f"{len(TABLE_CONTRACTS)} tables checked for columns, indexes, and constraints."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
