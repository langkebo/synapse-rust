#!/usr/bin/env python3
import argparse
import json
import os
import re
import sys
from dataclasses import asdict, dataclass
from pathlib import Path


MIGRATION_DIR = Path(__file__).resolve().parents[1] / "migrations"
REQUIRED_DIRS = [
    MIGRATION_DIR / "archive",
    MIGRATION_DIR / "rollback",
    MIGRATION_DIR / "incremental",
    MIGRATION_DIR / "hotfix",
]
ROLLBACK_AUDIT_DIRS = [
    MIGRATION_DIR,
    MIGRATION_DIR / "incremental",
    MIGRATION_DIR / "hotfix",
]

ROLLBACK_REQUIRED_FROM = 20260330000001
TIMESTAMPED_MIGRATION = re.compile(r"^(?P<ts>\d{8}(?:\d{6})?)_(?P<name>.+)\.sql$")
SPECIAL_FILES = {
    "00000000_unified_schema_v6.sql",
    "99999999_unified_incremental_migration.sql",
    "MANIFEST-template.txt",
}


@dataclass(frozen=True)
class AuditResult:
    missing_dirs: list[str]
    missing_rollbacks: list[str]
    legacy_timestamped: list[str]
    versioned: list[str]
    unknown_layout: list[str]


def migration_timestamp(file_name: str) -> int | None:
    m = TIMESTAMPED_MIGRATION.match(file_name)
    if not m:
        return None
    return int(m.group("ts"))


def requires_rollback(file_name: str) -> bool:
    if file_name in SPECIAL_FILES:
        return False
    if file_name.endswith(".undo.sql") or file_name.endswith(".down.sql"):
        return False
    if file_name.endswith(".rollback.sql"):
        return False
    ts = migration_timestamp(file_name)
    if ts is not None:
        return ts >= ROLLBACK_REQUIRED_FROM
    return file_name.startswith("V") and file_name.endswith(".sql")


def audit_dirs() -> list[str]:
    missing = []
    for d in REQUIRED_DIRS:
        if not d.exists() or not d.is_dir():
            missing.append(str(d))
    return missing


def audit_rollbacks() -> list[str]:
    missing = []
    for directory in ROLLBACK_AUDIT_DIRS:
        for entry in directory.iterdir():
            if not entry.is_file():
                continue
            if entry.suffix != ".sql":
                continue
            if not requires_rollback(entry.name):
                continue
            rollback_candidates = [
                entry.with_name(entry.stem + ".undo.sql"),
                entry.with_name(entry.stem + ".down.sql"),
                MIGRATION_DIR / "rollback" / (entry.stem + ".rollback.sql"),
            ]
            if not any(candidate.exists() for candidate in rollback_candidates):
                missing.append(entry.relative_to(MIGRATION_DIR).as_posix())
    return sorted(missing)


def classify_root_migrations() -> tuple[list[str], list[str], list[str]]:
    legacy_timestamped: list[str] = []
    versioned: list[str] = []
    unknown_layout: list[str] = []

    for entry in MIGRATION_DIR.iterdir():
        if not entry.is_file():
            continue
        if entry.suffix != ".sql":
            continue
        if entry.name in SPECIAL_FILES:
            continue
        if entry.name.endswith(".undo.sql") or entry.name.endswith(".down.sql"):
            continue
        if TIMESTAMPED_MIGRATION.match(entry.name):
            legacy_timestamped.append(entry.name)
            continue
        if entry.name.startswith("V"):
            versioned.append(entry.name)
            continue
        unknown_layout.append(entry.name)

    return sorted(legacy_timestamped), sorted(versioned), sorted(unknown_layout)


def write_report(result: AuditResult) -> None:
    write_report_to(MIGRATION_DIR / "migration_layout_audit.json", result)


def structured_failures(result: AuditResult) -> list[dict]:
    reproduce = "python3 scripts/audit_migration_layout.py"
    failures: list[dict] = []
    for entry in result.missing_dirs:
        failures.append(
            {
                "migration_id": None,
                "domain": "migration_layout_audit",
                "table": None,
                "column_or_index": None,
                "query_or_test_name": None,
                "failure_class": "missing_required_dir",
                "reproduce_command": reproduce,
                "path": entry,
            }
        )
    for migration in result.missing_rollbacks:
        failures.append(
            {
                "migration_id": migration,
                "domain": "migration_layout_audit",
                "table": None,
                "column_or_index": None,
                "query_or_test_name": None,
                "failure_class": "missing_rollback",
                "reproduce_command": reproduce,
            }
        )
    for name in result.unknown_layout:
        failures.append(
            {
                "migration_id": name,
                "domain": "migration_layout_audit",
                "table": None,
                "column_or_index": None,
                "query_or_test_name": None,
                "failure_class": "unknown_layout",
                "reproduce_command": reproduce,
            }
        )
    return failures


def write_report_to(path: Path, result: AuditResult) -> None:
    payload = asdict(result)
    payload["failures"] = structured_failures(result)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser(description="Audit migration directory layout")
    parser.add_argument(
        "--report",
        type=str,
        default=str(MIGRATION_DIR / "migration_layout_audit.json"),
        help="Write JSON report to the given path",
    )
    args = parser.parse_args()

    if not MIGRATION_DIR.exists():
        print(f"migrations dir not found: {MIGRATION_DIR}", file=sys.stderr)
        return 2

    legacy_timestamped, versioned, unknown_layout = classify_root_migrations()
    result = AuditResult(
        missing_dirs=audit_dirs(),
        missing_rollbacks=audit_rollbacks(),
        legacy_timestamped=legacy_timestamped,
        versioned=versioned,
        unknown_layout=unknown_layout,
    )
    report_path = Path(args.report)
    if not report_path.is_absolute():
        report_path = (Path(__file__).resolve().parents[1] / report_path).resolve()
    write_report_to(report_path, result)

    if result.missing_dirs:
        print("Missing required migration directories:", file=sys.stderr)
        for d in result.missing_dirs:
            print(f"- {d}", file=sys.stderr)

    if result.missing_rollbacks:
        print("Missing rollback scripts for migrations requiring rollback support:", file=sys.stderr)
        for name in result.missing_rollbacks:
            print(f"- {name}", file=sys.stderr)

    if result.unknown_layout:
        print("Unknown migration layout detected:", file=sys.stderr)
        for name in result.unknown_layout:
            print(f"- {name}", file=sys.stderr)

    if result.missing_dirs or result.missing_rollbacks or result.unknown_layout:
        return 1

    print(
        "Migration layout audit passed "
        f"(legacy_timestamped={len(result.legacy_timestamped)}, versioned={len(result.versioned)})"
    )
    return 0


if __name__ == "__main__":
    os.chdir(str(Path(__file__).resolve().parents[1]))
    sys.exit(main())
