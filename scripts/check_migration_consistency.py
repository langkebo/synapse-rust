#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import json
import re
from pathlib import Path


REQUIRED_V8_BATCHES: list[str] = []
TIMESTAMP_RE = re.compile(r"^\d{14}_.*\.sql$")


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def requires_undo(path: Path) -> bool:
    if path.name.startswith("00000000_unified_schema_v"):
        return False
    if path.name.startswith("00000001_extensions_v8"):
        return False
    return bool(TIMESTAMP_RE.match(path.name) or path.name.startswith("V"))


def collect_forward_sql(path: Path) -> list[Path]:
    return sorted(
        item for item in path.glob("*.sql") if not item.name.endswith(".undo.sql")
    )


def emit(report: dict, json_report: str | None) -> int:
    if json_report:
        json_path = Path(json_report)
        json_path.parent.mkdir(parents=True, exist_ok=True)
        json_path.write_text(
            json.dumps(report, ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0 if report["status"] == "ok" else 1


def check_single_source(primary_dir: Path, deploy_dir: Path, compose_file: Path) -> dict:
    """Single-source model: the deploy migrator mounts the canonical `migrations/`.

    History: `docker/deploy/migrations` used to be a hand-synced copy. It drifted
    silently — carrying 42 files from a dead v7 lineage while missing 13 recent
    migrations (schema_p1_federation_and_integrity, schema_p2_data_integrity,
    schema_p3_perf, schema_cleanup_dedup_and_dead_code, extend_room_version_check,
    event_relations_pagination_index, ...). A fresh deploy therefore built a schema
    without those fixes, and CI failed on exactly that (`missing_deploy_mirror` x8).

    The copy is gone; `docker-compose.yml` binds `../../migrations` straight into
    the migrator. A symlink was tried first and does NOT work here: BSD/macOS
    `find` does not follow a symlink given as the search root, so
    `find "$MIGRATIONS_DIR" -maxdepth 1 -type f -name '00000000_unified_schema_v*.sql'`
    matched nothing and the migrator aborted with "找不到统一基线脚本".

    What is verified now:
      1. no stale real `docker/deploy/migrations` directory survives (upgrade guard);
      2. the compose file actually mounts the canonical directory;
      3. the canonical directory's undo pairing (genuine gaps live here).
    """
    issues: list[dict[str, str]] = []
    warnings: list[dict[str, str]] = []

    if deploy_dir.is_symlink():
        # Tolerated for local experiments, but not the supported layout.
        warnings.append(
            {
                "type": "deploy_migrations_symlink",
                "file": "docker/deploy/migrations",
                "detail": (
                    "symlink present; the supported layout mounts the canonical "
                    "directory directly. Note BSD/macOS `find` does not follow a "
                    "symlink search root, which breaks baseline detection."
                ),
            }
        )
    elif deploy_dir.exists():
        issues.append(
            {
                "type": "stale_deploy_migrations",
                "file": "docker/deploy/migrations",
                "detail": (
                    "a real migrations copy exists again. It is not mounted (see "
                    "docker-compose.yml) and will silently drift from the canonical "
                    "directory, exactly as before. Delete it."
                ),
            }
        )

    if not compose_file.exists():
        issues.append({"type": "missing_compose_file", "file": str(compose_file)})
    else:
        compose = compose_file.read_text(encoding="utf-8")
        if "../../migrations:/migrations" not in compose:
            issues.append(
                {
                    "type": "compose_not_mounting_canonical_migrations",
                    "file": "docker/deploy/docker-compose.yml",
                    "detail": "expected a bind mount of `../../migrations:/migrations:ro`",
                }
            )

    primary_forward = collect_forward_sql(primary_dir)

    # Canonical undo pairing — this is where real, actionable gaps show up.
    for path in primary_forward:
        if requires_undo(path):
            undo_name = path.with_suffix(".undo.sql").name
            if not (primary_dir / undo_name).exists():
                issues.append({"type": "missing_primary_undo", "file": undo_name})

    return {
        "status": "ok" if not issues else "failed",
        "model": "single-source",
        "summary": {
            "issues": len(issues),
            "warnings": len(warnings),
            "primary_forward_files": len(primary_forward),
            "deploy_symlink": deploy_dir.is_symlink(),
        },
        "issues": issues,
        "warnings": warnings,
    }


def main() -> int:
    parser = argparse.ArgumentParser(
        description=(
            "Validate the migrations directory and its deploy integration. "
            "By default the deploy path mounts the canonical migrations directory "
            "directly (single source); a legacy hand-synced mirror is also checked "
            "if one is present."
        )
    )
    parser.add_argument(
        "--json-report",
        help="Optional path where the JSON report is written.",
    )
    args = parser.parse_args()

    project_root = Path(__file__).resolve().parent.parent
    primary_dir = project_root / "migrations"
    deploy_root = project_root / "docker" / "deploy"
    deploy_dir = deploy_root / "migrations"
    compose_file = deploy_root / "docker-compose.yml"

    # Single-source model: there is no separately-maintained mirror, so per-file
    # mirror comparison is meaningless (it would report every canonical file as
    # "missing" on the deploy side). This covers both the supported layout (no
    # deploy copy at all) and a symlinked copy.
    if not deploy_dir.exists() or deploy_dir.is_symlink():
        return emit(
            check_single_source(primary_dir, deploy_dir, compose_file),
            args.json_report,
        )

    # Legacy layout: a real, hand-synced mirror is present — check parity.
    issues: list[dict[str, str]] = []
    warnings: list[dict[str, str]] = []
    warnings.append(
        {
            "type": "legacy_deploy_mirror_present",
            "file": "docker/deploy/migrations",
            "detail": (
                "a hand-synced migrations mirror exists; the supported layout "
                "mounts the canonical directory instead. Prefer deleting it."
            ),
        }
    )

    primary_forward = collect_forward_sql(primary_dir)
    deploy_forward = collect_forward_sql(deploy_dir)

    primary_names = {path.name for path in primary_forward}
    deploy_names = {path.name for path in deploy_forward}

    for filename in REQUIRED_V8_BATCHES:
        if filename not in primary_names:
            issues.append({"type": "missing_primary_batch", "file": filename})
        if filename not in deploy_names:
            issues.append({"type": "missing_deploy_batch", "file": filename})

    for path in primary_forward:
        mirror = deploy_dir / path.name
        if not mirror.exists():
            issues.append({"type": "missing_deploy_mirror", "file": path.name})
            continue
        if sha256(path) != sha256(mirror):
            issues.append({"type": "content_mismatch", "file": path.name})

        if requires_undo(path):
            undo_name = path.with_suffix(".undo.sql").name
            if not (primary_dir / undo_name).exists():
                issues.append({"type": "missing_primary_undo", "file": undo_name})
            if not (deploy_dir / undo_name).exists():
                issues.append({"type": "missing_deploy_undo", "file": undo_name})

    for extra in sorted(deploy_names - primary_names):
        warnings.append({"type": "deploy_extra_file", "file": extra})

    latest_baselines = sorted(
        name for name in primary_names if name.startswith("00000000_unified_schema_v")
    )
    if latest_baselines:
        latest = latest_baselines[-1]
        # Warn only when the latest baseline has not been mirrored to deploy yet.
        # Once mirrored (via missing_deploy_mirror) this check passes automatically.
        if latest not in deploy_names:
            issues.append(
                {
                    "type": "unexpected_latest_baseline",
                    "file": latest,
                }
            )

    report = {
        "status": "ok" if not issues else "failed",
        "model": "mirror",
        "summary": {
            "issues": len(issues),
            "warnings": len(warnings),
            "primary_forward_files": len(primary_forward),
            "deploy_forward_files": len(deploy_forward),
        },
        "issues": issues,
        "warnings": warnings,
    }

    return emit(report, args.json_report)


if __name__ == "__main__":
    raise SystemExit(main())
