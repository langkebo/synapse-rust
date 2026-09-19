#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import shutil
import sys
from pathlib import Path


BASELINE_PREFIX = "00000000_unified_schema_v"
EXTENSION_PREFIX = "00000001_extensions"


def fail(message: str) -> None:
    """Fail closed (exit 2): the output feeds `sqlx migrate run`, so an
    empty/incomplete source must never be produced silently."""
    print(f"ERROR: build_sqlx_migration_source: {message}", file=sys.stderr)
    raise SystemExit(2)


def is_extension(path: Path) -> bool:
    return path.name.startswith(EXTENSION_PREFIX) and not path.name.endswith(
        ".undo.sql"
    )


def is_baseline(path: Path) -> bool:
    return path.name.startswith(BASELINE_PREFIX) and path.suffix == ".sql"


def forward_migrations(migrations_dir: Path) -> list[Path]:
    return sorted(
        path
        for path in migrations_dir.glob("*.sql")
        if not path.name.endswith(".undo.sql")
    )


def active_forward_migrations(migrations_dir: Path) -> list[Path]:
    forward = forward_migrations(migrations_dir)
    if not forward:
        fail(f"no forward .sql migrations under {migrations_dir}")

    baselines = [path for path in forward if is_baseline(path)]
    if not baselines:
        fail(f"no unified schema baseline ({BASELINE_PREFIX}*.sql) under {migrations_dir}")

    latest_baseline = sorted(baselines)[-1]
    extensions = [path for path in forward if is_extension(path)]
    latest_extension = sorted(extensions)[-1] if extensions else None

    selected = [latest_baseline]
    if latest_extension:
        selected.append(latest_extension)

    # All timestamp-based migrations are superseded by the consolidated baseline
    # (their objects were folded into it — see scripts/check_baseline_consolidation.py).
    for path in forward:
        if path in selected:
            continue
        if path.name.startswith("V") and path.suffix == ".sql":
            selected.append(path)

    # The sqlx source is what `sqlx migrate run` applies, so silently dropping a
    # forward migration produces an incomplete schema. Assert the selection is
    # total instead of trusting the "superseded" assumption: a new timestamp
    # migration must either be folded into the baseline or explicitly selected.
    dropped = [path.name for path in forward if path not in selected]
    if dropped:
        fail(
            f"forward-only source would silently drop {len(dropped)} migration(s): "
            + ", ".join(dropped)
            + " — fold them into the consolidated baseline"
            " (migrations/README.md) or teach active_forward_migrations() to include them."
        )

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
    # `SYNAPSE_MIGRATIONS_DIR` overrides the scan surface (mirrors
    # `SYNAPSE_WEB_CRATE_DIR` / `SYNAPSE_WEB_ROUTES_DIR` in the other gates) so
    # the fail-closed guards can be self-tested against a temp tree.
    migrations_dir = Path(
        os.environ.get("SYNAPSE_MIGRATIONS_DIR") or project_root / "migrations"
    ).resolve()
    output_dir = Path(args.output_dir).resolve()

    if not migrations_dir.is_dir():
        fail(f"migrations directory not found: {migrations_dir}")

    selected = active_forward_migrations(migrations_dir)

    if output_dir.exists():
        shutil.rmtree(output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    for path in selected:
        shutil.copy2(path, output_dir / path.name)

    # Verify the copy actually produced a complete, byte-identical source before
    # writing the manifest, so a partial copy cannot pass as a good source.
    missing = [path.name for path in selected if not (output_dir / path.name).is_file()]
    if missing:
        fail(f"copy did not produce {len(missing)} file(s) in {output_dir}: {', '.join(missing)}")
    copied = sorted(path.name for path in output_dir.glob("*.sql"))
    expected = sorted(path.name for path in selected)
    if copied != expected:
        fail(f"output {output_dir} contains {copied!r}, expected {expected!r}")
    for path in selected:
        if (output_dir / path.name).read_bytes() != path.read_bytes():
            fail(f"copied migration {path.name} differs from {path}")

    baseline = next((path for path in selected if is_baseline(path)), None)
    if baseline is None:
        fail("selected migration set does not contain a unified schema baseline")

    manifest = {
        "baseline": baseline.name,
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
