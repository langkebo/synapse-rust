#!/usr/bin/env python3
import argparse
import hashlib
from datetime import datetime, UTC
from pathlib import Path
import subprocess
import sys


REPO_ROOT = Path(__file__).resolve().parents[1]
MIGRATIONS_DIR = REPO_ROOT / "migrations"
GOVERNED_DIRS = [
    MIGRATIONS_DIR,
    MIGRATIONS_DIR / "incremental",
    MIGRATIONS_DIR / "hotfix",
]
SPECIAL_FILES = {
    "00000000_unified_schema_v6.sql": "baseline",
    "99999999_unified_incremental_migration.sql": "incremental",
}


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def git_commit() -> str:
    try:
        return (
            subprocess.check_output(
                ["git", "rev-parse", "HEAD"],
                cwd=REPO_ROOT,
                text=True,
            )
            .strip()
        )
    except Exception:
        return "unknown"


def migration_type(path: Path) -> str:
    if path.name in SPECIAL_FILES:
        return SPECIAL_FILES[path.name]
    if path.name.endswith(".undo.sql") or path.name.endswith(".down.sql"):
        return "undo"
    if path.parent == MIGRATIONS_DIR / "hotfix":
        return "hotfix"
    return "incremental"


def render_table(headers: list[str], rows: list[list[str]]) -> list[str]:
    separator = ["-" * max(len(header), 3) for header in headers]
    output = [
        "| " + " | ".join(headers) + " |",
        "| " + " | ".join(separator) + " |",
    ]
    for row in rows:
        output.append("| " + " | ".join(row) + " |")
    return output


def manifest_name(path: Path) -> str:
    return path.relative_to(MIGRATIONS_DIR).as_posix()


def collect_migrations() -> list[Path]:
    files = []
    for directory in GOVERNED_DIRS:
        for path in sorted(directory.glob("*.sql")):
            if path.name.endswith(".undo.sql") or path.name.endswith(".down.sql"):
                continue
            if path.name.endswith(".rollback.sql"):
                continue
            files.append(path)
    return files


def collect_rollbacks() -> list[Path]:
    files = list(sorted((MIGRATIONS_DIR / "rollback").glob("*.rollback.sql")))
    files.extend(sorted(MIGRATIONS_DIR.glob("*.undo.sql")))
    files.extend(sorted(MIGRATIONS_DIR.glob("*.down.sql")))
    files.extend(sorted((MIGRATIONS_DIR / "incremental").glob("*.undo.sql")))
    files.extend(sorted((MIGRATIONS_DIR / "incremental").glob("*.down.sql")))
    files.extend(sorted((MIGRATIONS_DIR / "hotfix").glob("*.undo.sql")))
    files.extend(sorted((MIGRATIONS_DIR / "hotfix").glob("*.down.sql")))
    return files


def collect_archive_files() -> list[Path]:
    archive_dir = MIGRATIONS_DIR / "archive"
    return sorted(path for path in archive_dir.rglob("*.sql") if path.is_file())


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--release", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--jira", default="N/A")
    parser.add_argument("--owner", default="synapse-rust")
    args = parser.parse_args()

    output_path = Path(args.output)
    if not output_path.is_absolute():
        output_path = REPO_ROOT / output_path
    output_path.parent.mkdir(parents=True, exist_ok=True)

    now = datetime.now(UTC).strftime("%Y-%m-%d %H:%M:%S UTC")
    commit_sha = git_commit()

    migration_rows = [
        [
            manifest_name(path),
            str(path.stat().st_size),
            sha256_file(path),
            migration_type(path),
            "YES" if path.name in SPECIAL_FILES else "NO",
        ]
        for path in collect_migrations()
    ]
    rollback_rows = [
        [
            manifest_name(path),
            str(path.stat().st_size),
            sha256_file(path),
            path.name.replace(".rollback.sql", ".sql")
            .replace(".undo.sql", ".sql")
            .replace(".down.sql", ".sql"),
        ]
        for path in collect_rollbacks()
    ]
    archive_rows = [
        [
            path.relative_to(MIGRATIONS_DIR).as_posix(),
            str(path.stat().st_size),
            sha256_file(path),
            now.split(" ")[0],
            "00000000_unified_schema_v6.sql",
        ]
        for path in collect_archive_files()
    ]

    lines = [
        "# Migration Manifest",
        "",
        f"Manifest Version: v1.0",
        f"Release Version: {args.release}",
        f"Generated Date: {now}",
        f"Git Commit: {commit_sha}",
        f"Jira Ticket: {args.jira}",
        f"Owner: {args.owner}",
        "",
        "## Migration Scripts",
        "",
        *render_table(
            ["Filename", "Size (bytes)", "SHA-256", "Type", "Applied"],
            migration_rows or [["(none)", "0", "", "", ""]],
        ),
        "",
        "## Rollback Scripts",
        "",
        *render_table(
            ["Filename", "Size (bytes)", "SHA-256", "Associated Migration"],
            rollback_rows or [["(none)", "0", "", ""]],
        ),
        "",
        "## Archive Scripts",
        "",
        *render_table(
            ["Filename", "Size (bytes)", "SHA-256", "Archived Date", "Replacement"],
            archive_rows or [["(none)", "0", "", "", ""]],
        ),
        "",
        "## Sign-off",
        "",
        *render_table(
            ["Role", "Name", "Date", "Signature"],
            [
                ["Backend Owner", "", "", ""],
                ["DBA", "", "", ""],
                ["SRE", "", "", ""],
                ["QA", "", "", ""],
            ],
        ),
        "",
    ]

    output_path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"Wrote manifest to {output_path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
