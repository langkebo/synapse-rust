#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import re
import shutil
from pathlib import Path


V8_CONSOLIDATED_START = 20260701000000
TIMESTAMP_RE = re.compile(r"^(?P<ts>\d{14})_.*\.sql$")


def is_extension(path: Path) -> bool:
    return path.name.startswith("00000001_extensions") and not path.name.endswith(
        ".undo.sql"
    )


def is_baseline(path: Path) -> bool:
    return path.name.startswith("00000000_unified_schema_v") and path.suffix == ".sql"


def active_forward_migrations(migrations_dir: Path) -> list[Path]:
    forward = sorted(
        path
        for path in migrations_dir.glob("*.sql")
        if not path.name.endswith(".undo.sql")
    )
    baselines = [path for path in forward if is_baseline(path)]
    if not baselines:
        raise SystemExit("no unified schema baseline was found")

    latest_baseline = sorted(baselines)[-1]
    extensions = [path for path in forward if is_extension(path)]
    latest_extension = sorted(extensions)[-1] if extensions else None

    selected = [latest_baseline]
    if latest_extension:
        selected.append(latest_extension)

    # All timestamp-based migrations are superseded by v8 baseline
    for path in forward:
        if path in selected:
            continue
        if path.name.startswith("V") and path.suffix == ".sql":
            selected.append(path)

    return sorted(selected)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Build the forward-only sqlx migration source for the consolidated v8 chain."
    )
    parser.add_argument(
        "output_dir", help="Directory where the sqlx migration source is written."
    )
    args = parser.parse_args()

    project_root = Path(__file__).resolve().parent.parent
    migrations_dir = project_root / "migrations"
    output_dir = Path(args.output_dir).resolve()

    if output_dir.exists():
        shutil.rmtree(output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    selected = active_forward_migrations(migrations_dir)
    for path in selected:
        shutil.copy2(path, output_dir / path.name)

    manifest = {
        "baseline": selected[0].name,
        "count": len(selected),
        "migrations": [path.name for path in selected],
    }
    (output_dir / "manifest.json").write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )

    print(json.dumps(manifest, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
