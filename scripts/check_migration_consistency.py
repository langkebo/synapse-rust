#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path


TIMESTAMP_RE = re.compile(r"^\d{14}_.*\.sql$")
BASELINE_PREFIX = "00000000_unified_schema_v"
EXTENSION_PREFIX = "00000001_extensions"

# 扫描面自检（2026-09-19，GATE_INTEGRITY_SWEEP §6 C10）：consolidated baseline 是
# 唯一正向文件时，undo 配对子检查迭代 0 个增量迁移，脚本只剩 compose 挂载串这一条
# 近乎恒真的断言（旧的 `REQUIRED_V8_BATCHES: list[str] = []` 循环同样在空表上空转，
# 已按"同一职责一份实现 + 禁止冗余残留"删除）。这里显式承认
# consolidated-baseline-only 形态并打印 marker，同时保证命名约定变化/文件被删
# 都会让扫描面自检报错退出，而不是静默通过。
CONSOLIDATED_BASELINE_ONLY_MARKER = "consolidated-baseline-only"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def is_baseline(path: Path) -> bool:
    return path.name.startswith(BASELINE_PREFIX) and path.name.endswith(".sql")


def is_extension(path: Path) -> bool:
    return path.name.startswith(EXTENSION_PREFIX) and path.name.endswith(".sql")


def requires_undo(path: Path) -> bool:
    if is_baseline(path) or is_extension(path):
        return False
    return bool(TIMESTAMP_RE.match(path.name) or path.name.startswith("V"))


def collect_forward_sql(path: Path) -> list[Path]:
    return sorted(
        item for item in path.glob("*.sql") if not item.name.endswith(".undo.sql")
    )


def scan_surface(
    forward: list[Path], label: str
) -> tuple[list[dict[str, str]], list[Path], list[Path]]:
    """Non-vacuity guard for the forward-migration scan surface.

    Returns `(issues, baseline_files, incremental_files)`. The undo-pairing
    sub-check only iterates `incremental_files`, so if the naming convention
    drifts — or a file is deleted — that set can go empty and the sub-check
    passes on nothing. Two measured facts are asserted instead:

      * exactly one consolidated baseline exists, and every other forward
        `.sql` is a timestamp/`V*` incremental or a known `00000001_extensions*`
        chain member; anything else is reported as unaccounted;
      * when no incremental migration matched at all, the forward chain must
        *be* that single consolidated baseline — the empty case is the
        documented `consolidated-baseline-only` exemption, not an accident.
    """
    baselines = [p for p in forward if is_baseline(p)]
    incrementals = [p for p in forward if requires_undo(p)]
    issues: list[dict[str, str]] = []

    if not baselines:
        issues.append(
            {
                "type": "missing_unified_baseline",
                "file": f"{label}/{BASELINE_PREFIX}*.sql",
                "detail": (
                    "no consolidated baseline found: every forward .sql is an "
                    "incremental migration, so the undo-pairing sub-check no "
                    "longer evaluates the documented model"
                ),
            }
        )
    elif len(baselines) > 1:
        issues.append(
            {
                "type": "multiple_unified_baselines",
                "file": ", ".join(p.name for p in baselines),
                "detail": (
                    "more than one `00000000_unified_schema_v*.sql` baseline: the "
                    "migrator treats every non-latest baseline as an incremental "
                    "migration, so a fresh database gets two baseline versions applied"
                ),
            }
        )

    for path in forward:
        if path in baselines or path in incrementals or is_extension(path):
            continue
        issues.append(
            {
                "type": "unaccounted_forward_migration",
                "file": path.name,
                "detail": (
                    f"{label}/{path.name} is neither the consolidated baseline, an "
                    f"`{EXTENSION_PREFIX}*` file, nor a timestamp/`V*` incremental "
                    "migration, so the undo-pairing sub-check silently skips it. A "
                    "changed naming convention must not empty the scan surface."
                ),
            }
        )

    if not incrementals and (len(baselines) != 1 or forward != baselines):
        issues.append(
            {
                "type": "empty_incremental_scan_surface",
                "file": label,
                "detail": (
                    "no incremental migration was iterated, yet the forward chain is "
                    "not the single consolidated baseline "
                    f"(found: {[p.name for p in forward]}): the undo-pairing "
                    "sub-check is vacuous, not clean."
                ),
            }
        )

    return issues, baselines, incrementals


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


def check_single_source(
    primary_dir: Path, deploy_dir: Path, compose_file: Path
) -> dict:
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

    # Non-vacuity first: prove the surface is either genuine incrementals or the
    # measured consolidated-baseline-only case before reporting "ok".
    surface_issues, baseline_files, incrementals = scan_surface(
        primary_forward, "migrations"
    )
    issues.extend(surface_issues)

    # Canonical undo pairing — this is where real, actionable gaps show up.
    for path in incrementals:
        undo_name = path.with_suffix(".undo.sql").name
        if not (primary_dir / undo_name).exists():
            issues.append({"type": "missing_primary_undo", "file": undo_name})

    marker = (
        CONSOLIDATED_BASELINE_ONLY_MARKER
        if not incrementals and not surface_issues
        else None
    )
    if marker:
        print(
            f"check_migration_consistency: {marker} — migrations/ holds only the "
            f"consolidated baseline ({', '.join(p.name for p in baseline_files)}); the "
            "undo-pairing sub-check is intentionally vacuous. Adding any timestamped/V* "
            "migration makes it run again.",
            file=sys.stderr,
        )

    return {
        "status": "ok" if not issues else "failed",
        "model": "single-source",
        "marker": marker,
        "scan_surface": {
            "forward_files": [p.name for p in primary_forward],
            "baseline_files": [p.name for p in baseline_files],
            "incremental_files": [p.name for p in incrementals],
        },
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

    # Same non-vacuity guard as the single-source branch (one implementation).
    surface_issues, baseline_files, incrementals = scan_surface(
        primary_forward, "migrations"
    )
    issues.extend(surface_issues)

    for path in primary_forward:
        mirror = deploy_dir / path.name
        if not mirror.exists():
            issues.append({"type": "missing_deploy_mirror", "file": path.name})
            continue
        if sha256(path) != sha256(mirror):
            issues.append({"type": "content_mismatch", "file": path.name})

    for path in incrementals:
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

    marker = (
        CONSOLIDATED_BASELINE_ONLY_MARKER
        if not incrementals and not surface_issues
        else None
    )
    if marker:
        print(
            f"check_migration_consistency: {marker} — migrations/ holds only the "
            f"consolidated baseline ({', '.join(p.name for p in baseline_files)}); the "
            "undo-pairing sub-check is intentionally vacuous.",
            file=sys.stderr,
        )

    report = {
        "status": "ok" if not issues else "failed",
        "model": "mirror",
        "marker": marker,
        "scan_surface": {
            "forward_files": [p.name for p in primary_forward],
            "baseline_files": [p.name for p in baseline_files],
            "incremental_files": [p.name for p in incrementals],
        },
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
