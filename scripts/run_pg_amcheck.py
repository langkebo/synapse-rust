#!/usr/bin/env python3
import argparse
import os
import shutil
import subprocess
import sys
from urllib.parse import urlparse


def parse_database_url(database_url: str) -> dict[str, str]:
    parsed = urlparse(database_url)
    if parsed.scheme not in {"postgres", "postgresql"}:
        raise ValueError(f"Unsupported database URL scheme: {parsed.scheme}")
    return {
        "host": parsed.hostname or "localhost",
        "port": str(parsed.port or 5432),
        "user": parsed.username or os.getenv("PGUSER", "postgres"),
        "password": parsed.password or os.getenv("PGPASSWORD", ""),
        "database": parsed.path.lstrip("/") or os.getenv("PGDATABASE", "postgres"),
    }


def run_command(command: list[str], env: dict[str, str]) -> int:
    print("Running:", " ".join(command))
    completed = subprocess.run(command, env=env)
    return completed.returncode


def run_command_captured(command: list[str], env: dict[str, str]) -> subprocess.CompletedProcess:
    print("Running:", " ".join(command))
    return subprocess.run(command, env=env, capture_output=True, text=True)


def write_report(report_path: str, command: list[str], completed: subprocess.CompletedProcess) -> None:
    report = [
        "command: " + " ".join(command),
        "exit_code: " + str(completed.returncode),
        "",
        "stdout:",
        completed.stdout or "",
        "",
        "stderr:",
        completed.stderr or "",
        "",
    ]
    path = os.path.abspath(report_path)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        f.write("\n".join(report))
    print(f"Wrote pg_amcheck report to {path}")


def run_psql(connection: dict[str, str], container: str | None, env: dict[str, str], sql: str) -> int:
    if container:
        command = [
            "docker",
            "exec",
            "-i",
            container,
            "psql",
            "-U",
            connection["user"],
            "-d",
            connection["database"],
            "-v",
            "ON_ERROR_STOP=1",
            "-c",
            sql,
        ]
    else:
        command = [
            "psql",
            "-h",
            connection["host"],
            "-p",
            connection["port"],
            "-U",
            connection["user"],
            "-d",
            connection["database"],
            "-v",
            "ON_ERROR_STOP=1",
            "-c",
            sql,
        ]
    return run_command(command, env)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--database-url",
        default=os.getenv("DATABASE_URL"),
    )
    parser.add_argument(
        "--schema",
        default=os.getenv("PG_AMCHECK_SCHEMA", "public"),
    )
    parser.add_argument(
        "--container",
        default=os.getenv("PG_AMCHECK_CONTAINER"),
    )
    parser.add_argument(
        "--output",
        default=os.getenv("PG_AMCHECK_REPORT"),
    )
    args = parser.parse_args()

    if not args.database_url:
        print("DATABASE_URL is required", file=sys.stderr)
        return 2

    connection = parse_database_url(args.database_url)
    env = os.environ.copy()
    if connection["password"]:
        env["PGPASSWORD"] = connection["password"]

    if args.container and not shutil.which("docker"):
        print("docker is required when PG_AMCHECK_CONTAINER is set", file=sys.stderr)
        return 2

    if args.container or shutil.which("psql"):
        setup_code = run_psql(
            connection,
            args.container,
            env,
            "CREATE EXTENSION IF NOT EXISTS amcheck;",
        )
        if setup_code != 0:
            return setup_code

    if shutil.which("pg_amcheck"):
        command = [
            "pg_amcheck",
            "-h",
            connection["host"],
            "-p",
            connection["port"],
            "-U",
            connection["user"],
            "-d",
            connection["database"],
            "--schema",
            args.schema,
            "--no-dependent-indexes",
        ]
        if args.output:
            completed = run_command_captured(command, env)
            write_report(args.output, command, completed)
            return completed.returncode
        return run_command(command, env)

    if args.container:
        command = [
            "docker",
            "exec",
            args.container,
            "pg_amcheck",
            "-U",
            connection["user"],
            "-d",
            connection["database"],
            "--schema",
            args.schema,
            "--no-dependent-indexes",
        ]
        if args.output:
            completed = run_command_captured(command, env)
            write_report(args.output, command, completed)
            return completed.returncode
        return run_command(command, env)

    print("pg_amcheck is not available and PG_AMCHECK_CONTAINER is not set", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
