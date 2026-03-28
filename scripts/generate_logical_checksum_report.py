#!/usr/bin/env python3
import argparse
import json
import os
import subprocess
import sys
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from urllib.parse import urlparse


DEFAULT_TABLES = [
    "users",
    "rooms",
    "events",
    "room_memberships",
    "devices",
    "access_tokens",
    "refresh_tokens",
    "thread_roots",
    "thread_replies",
    "thread_read_receipts",
    "thread_relations",
    "room_invite_blocklist",
    "room_invite_allowlist",
    "device_verification_request",
]


@dataclass
class DatabaseConnection:
    database_url: str
    host: str
    port: str
    user: str
    password: str
    database: str


def parse_database_url(database_url: str) -> DatabaseConnection:
    parsed = urlparse(database_url)
    if parsed.scheme not in {"postgres", "postgresql"}:
        raise ValueError(f"Unsupported database URL scheme: {parsed.scheme}")
    return DatabaseConnection(
        database_url=database_url,
        host=parsed.hostname or "localhost",
        port=str(parsed.port or 5432),
        user=parsed.username or os.getenv("PGUSER", "postgres"),
        password=parsed.password or os.getenv("PGPASSWORD", ""),
        database=parsed.path.lstrip("/") or os.getenv("PGDATABASE", "postgres"),
    )


def sql_literal(value: str) -> str:
    return "'" + value.replace("'", "''") + "'"


def identifier(value: str) -> str:
    if not value.replace("_", "").isalnum():
        raise ValueError(f"Unsupported identifier: {value}")
    return '"' + value.replace('"', '""') + '"'


def run_psql(connection: DatabaseConnection, sql: str) -> str:
    env = os.environ.copy()
    if connection.password:
        env["PGPASSWORD"] = connection.password
    psql_container = os.getenv("PSQL_CONTAINER")
    if psql_container:
        command = [
            "docker",
            "exec",
            "-i",
            psql_container,
            "psql",
            "-U",
            connection.user,
            "-d",
            connection.database,
            "-At",
            "-F",
            "\t",
            "-v",
            "ON_ERROR_STOP=1",
            "-c",
            sql,
        ]
    else:
        command = [
            "psql",
            "-h",
            connection.host,
            "-p",
            connection.port,
            "-U",
            connection.user,
            "-d",
            connection.database,
            "-At",
            "-F",
            "\t",
            "-v",
            "ON_ERROR_STOP=1",
            "-c",
            sql,
        ]
    completed = subprocess.run(command, env=env, capture_output=True, text=True)
    if completed.returncode != 0:
        raise RuntimeError(completed.stderr.strip() or completed.stdout.strip())
    return completed.stdout.strip()


def load_tables(tables_file: str | None) -> list[str]:
    if not tables_file:
        return DEFAULT_TABLES
    content = Path(tables_file).read_text().splitlines()
    return [line.strip() for line in content if line.strip() and not line.strip().startswith("#")]


def table_exists(connection: DatabaseConnection, table_name: str) -> bool:
    sql = (
        "SELECT EXISTS ("
        "SELECT 1 FROM information_schema.tables "
        "WHERE table_schema = 'public' AND table_name = "
        f"{sql_literal(table_name)})"
    )
    return run_psql(connection, sql) == "t"


def primary_key_columns(connection: DatabaseConnection, table_name: str) -> list[str]:
    sql = f"""
    SELECT a.attname
    FROM pg_index i
    JOIN pg_class c ON c.oid = i.indrelid
    JOIN pg_namespace n ON n.oid = c.relnamespace
    JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum = ANY(i.indkey)
    WHERE i.indisprimary
      AND n.nspname = 'public'
      AND c.relname = {sql_literal(table_name)}
    ORDER BY array_position(i.indkey, a.attnum)
    """
    output = run_psql(connection, sql)
    return [line for line in output.splitlines() if line]


def table_row_count(connection: DatabaseConnection, table_name: str) -> int:
    sql = f"SELECT COUNT(*)::bigint FROM public.{identifier(table_name)}"
    output = run_psql(connection, sql)
    return int(output or "0")


def table_checksum(connection: DatabaseConnection, table_name: str, order_columns: list[str]) -> str | None:
    if not order_columns:
        return None
    order_expr = ", ".join(identifier(column) for column in order_columns)
    sql = f"""
    WITH ordered_rows AS (
        SELECT
            row_number() OVER (ORDER BY {order_expr}) AS row_no,
            md5(row_to_json(t)::text) AS row_hash
        FROM public.{identifier(table_name)} t
    )
    SELECT COALESCE(md5(string_agg(row_hash, '' ORDER BY row_no)), md5(''))
    FROM ordered_rows
    """
    output = run_psql(connection, sql)
    return output or None


def build_report(connection: DatabaseConnection, tables: list[str]) -> dict:
    report = {
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "database": {
            "host": connection.host,
            "port": connection.port,
            "database": connection.database,
        },
        "tables": [],
    }
    for table_name in tables:
        exists = table_exists(connection, table_name)
        if not exists:
            report["tables"].append(
                {
                    "table": table_name,
                    "exists": False,
                    "order_stable": False,
                    "row_count": None,
                    "checksum": None,
                    "order_columns": [],
                }
            )
            continue

        order_columns = primary_key_columns(connection, table_name)
        row_count = table_row_count(connection, table_name)
        checksum = table_checksum(connection, table_name, order_columns)
        report["tables"].append(
            {
                "table": table_name,
                "exists": True,
                "order_stable": bool(order_columns),
                "row_count": row_count,
                "checksum": checksum,
                "order_columns": order_columns,
            }
        )
    return report


def compare_reports(primary: dict, replica: dict) -> list[dict]:
    replica_tables = {entry["table"]: entry for entry in replica["tables"]}
    diffs = []
    for entry in primary["tables"]:
        other = replica_tables.get(entry["table"])
        if other is None:
            diffs.append({"table": entry["table"], "reason": "replica_missing"})
            continue
        if not entry["exists"] and not other["exists"]:
            continue
        if entry["exists"] != other["exists"]:
            diffs.append({"table": entry["table"], "reason": "existence_mismatch"})
            continue
        if entry["row_count"] != other["row_count"]:
            diffs.append(
                {
                    "table": entry["table"],
                    "reason": "row_count_mismatch",
                    "primary": entry["row_count"],
                    "replica": other["row_count"],
                }
            )
            continue
        if entry["order_stable"] and other["order_stable"] and entry["checksum"] != other["checksum"]:
            diffs.append(
                {
                    "table": entry["table"],
                    "reason": "checksum_mismatch",
                    "primary": entry["checksum"],
                    "replica": other["checksum"],
                }
            )
    return diffs


def write_json(output_path: str | None, payload: dict) -> None:
    if not output_path:
        print(json.dumps(payload, ensure_ascii=False, indent=2))
        return
    path = Path(output_path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n")
    print(f"Wrote report to {path}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--database-url", default=os.getenv("DATABASE_URL"))
    parser.add_argument("--replica-database-url", default=os.getenv("REPLICA_DATABASE_URL"))
    parser.add_argument("--tables-file", default=os.getenv("LOGICAL_CHECKSUM_TABLES_FILE"))
    parser.add_argument("--output", default=os.getenv("LOGICAL_CHECKSUM_REPORT"))
    parser.add_argument("--replica-output", default=os.getenv("LOGICAL_CHECKSUM_REPLICA_REPORT"))
    args = parser.parse_args()

    if not args.database_url:
        print("DATABASE_URL is required", file=sys.stderr)
        return 2

    tables = load_tables(args.tables_file)
    primary_connection = parse_database_url(args.database_url)
    primary_report = build_report(primary_connection, tables)
    result = {
        "mode": "single",
        "primary": primary_report,
        "comparison": {"checked": False, "differences": []},
    }

    if args.replica_database_url:
        replica_connection = parse_database_url(args.replica_database_url)
        replica_report = build_report(replica_connection, tables)
        diffs = compare_reports(primary_report, replica_report)
        result = {
            "mode": "compare",
            "primary": primary_report,
            "replica": replica_report,
            "comparison": {"checked": True, "differences": diffs},
        }
        if args.replica_output:
            write_json(args.replica_output, replica_report)
        write_json(args.output, result)
        if diffs:
            print(json.dumps(diffs, ensure_ascii=False, indent=2), file=sys.stderr)
            return 1
        return 0

    write_json(args.output, result)
    return 0


if __name__ == "__main__":
    sys.exit(main())
