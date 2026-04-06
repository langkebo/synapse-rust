#!/usr/bin/env python3
"""
Build a sqlx-compatible migration source directory.

sqlx scans every `.sql` file in the source directory and does not understand this
repository's `.undo.sql` rollback companions. This helper copies only forward
migrations from the root `migrations/` directory into a clean destination.
"""

from __future__ import annotations

import argparse
import shutil
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MIGRATIONS_DIR = ROOT / "migrations"


def forward_root_migrations() -> list[Path]:
    return sorted(
        path
        for path in MIGRATIONS_DIR.glob("*.sql")
        if path.is_file()
        and not path.name.endswith(".undo.sql")
        and not path.name.endswith(".down.sql")
        and not path.name.endswith(".rollback.sql")
    )


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Build a forward-only migration source directory for sqlx"
    )
    parser.add_argument("destination", help="Directory to write sqlx-compatible migrations into")
    args = parser.parse_args()

    destination = Path(args.destination)
    if not destination.is_absolute():
        destination = ROOT / destination

    if destination.exists():
        shutil.rmtree(destination)
    destination.mkdir(parents=True, exist_ok=True)

    copied = 0
    for migration in forward_root_migrations():
        shutil.copy2(migration, destination / migration.name)
        copied += 1

    print(f"Prepared sqlx migration source: {destination} ({copied} files)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
