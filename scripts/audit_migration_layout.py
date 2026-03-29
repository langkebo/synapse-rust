#!/usr/bin/env python3
import os
import re
import sys
from dataclasses import dataclass
from pathlib import Path


MIGRATION_DIR = Path(__file__).resolve().parents[1] / "migrations"
REQUIRED_DIRS = [
    MIGRATION_DIR / "archive",
    MIGRATION_DIR / "rollback",
    MIGRATION_DIR / "incremental",
    MIGRATION_DIR / "hotfix",
]

ROLLBACK_REQUIRED_FROM = 20260330000001
TIMESTAMPED_MIGRATION = re.compile(r"^(?P<ts>\d{14})_(?P<name>.+)\.sql$")


@dataclass(frozen=True)
class AuditResult:
    missing_dirs: list[str]
    missing_rollbacks: list[str]


def migration_timestamp(file_name: str) -> int | None:
    m = TIMESTAMPED_MIGRATION.match(file_name)
    if not m:
        return None
    return int(m.group("ts"))


def audit_dirs() -> list[str]:
    missing = []
    for d in REQUIRED_DIRS:
        if not d.exists() or not d.is_dir():
            missing.append(str(d))
    return missing


def audit_rollbacks() -> list[str]:
    missing = []
    for entry in MIGRATION_DIR.iterdir():
        if not entry.is_file():
            continue
        if entry.suffix != ".sql":
            continue
        ts = migration_timestamp(entry.name)
        if ts is None or ts < ROLLBACK_REQUIRED_FROM:
            continue
        rollback = MIGRATION_DIR / "rollback" / (entry.stem + ".rollback.sql")
        if not rollback.exists():
            missing.append(entry.name)
    return sorted(missing)


def main() -> int:
    if not MIGRATION_DIR.exists():
        print(f"migrations dir not found: {MIGRATION_DIR}", file=sys.stderr)
        return 2

    result = AuditResult(
        missing_dirs=audit_dirs(),
        missing_rollbacks=audit_rollbacks(),
    )

    if result.missing_dirs:
        print("Missing required migration directories:", file=sys.stderr)
        for d in result.missing_dirs:
            print(f"- {d}", file=sys.stderr)

    if result.missing_rollbacks:
        print(
            f"Missing rollback scripts for migrations >= {ROLLBACK_REQUIRED_FROM}:",
            file=sys.stderr,
        )
        for name in result.missing_rollbacks:
            print(f"- {name}", file=sys.stderr)

    if result.missing_dirs or result.missing_rollbacks:
        return 1

    print("Migration layout audit passed")
    return 0


if __name__ == "__main__":
    os.chdir(str(Path(__file__).resolve().parents[1]))
    sys.exit(main())
