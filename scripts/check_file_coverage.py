#!/usr/bin/env python3
"""Per-file coverage threshold enforcement for tarpaulin JSON reports.

Reads a tarpaulin JSON report (--out Json), compares per-file line coverage
against configurable thresholds, and exits non-zero if any file falls below
its floor.  Designed to be called from CI after `cargo tarpaulin --out Json`.

Policy (from .tarpaulin.toml and TDD落地执行清单 P4-1):
  - TDD-mandated files (Phase 3 trait seams):                    ≥ 80%
  - New files (not in the baseline):                              ≥ 60%  (ramp-up grace)
  - Existing touched files:                                       must not regress below prior baseline
  - All other src/**/*.rs files:                                   ≥ 70%  (global floor, warn-only)

Usage:
  python3 scripts/check_file_coverage.py \\
      --report tarpaulin-report.json \\
      --baseline artifacts/coverage_baseline.json \\
      --threshold 80 \\
      --tdd-files artifacts/tdd_file_list.txt
"""

import argparse
import json
import pathlib
import sys
from typing import Dict, List, Optional

ROOT = pathlib.Path(__file__).resolve().parents[1]


def load_baseline(path: pathlib.Path) -> Dict[str, float]:
    """Load prior coverage baseline (file_path -> line_pct)."""
    if not path.exists():
        return {}
    with open(path) as f:
        data = json.load(f)
    if isinstance(data, dict) and "files" in data:
        return {item["path"]: item["line_pct"] for item in data["files"]}
    if isinstance(data, dict):
        return data
    return {}


def save_baseline(path: pathlib.Path, files: Dict[str, float]) -> None:
    """Save coverage snapshot for future regression checks."""
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = {
        "files": [
            {"path": p, "line_pct": round(v, 2)} for p, v in sorted(files.items())
        ]
    }
    with open(path, "w") as f:
        json.dump(payload, f, indent=2)
        f.write("\n")


def load_tdd_files(path: Optional[pathlib.Path]) -> set:
    """Load list of TDD-mandated files (one relative path per line)."""
    if path is None or not path.exists():
        return set()
    with open(path) as f:
        return {line.strip() for line in f if line.strip() and not line.startswith("#")}


def load_core_prefixes(path: Optional[pathlib.Path]) -> List[str]:
    """Load list of core path prefixes (one per line, supports dir/ prefix matching)."""
    if path is None or not path.exists():
        return []
    with open(path) as f:
        return [line.strip() for line in f if line.strip() and not line.startswith("#")]


def _matches_core_prefix(path: str, core_prefixes: List[str]) -> bool:
    """Check if a path matches any core prefix (directory or file prefix)."""
    for prefix in core_prefixes:
        if path.startswith(prefix):
            return True
    return False


def parse_tarpaulin_json(report_path: pathlib.Path) -> Dict[str, float]:
    """Parse a tarpaulin JSON report and return {rel_path: line_pct}.

    Handles both the 'files' array format and the 'coverage' map format.
    """
    with open(report_path) as f:
        data = json.load(f)

    result: Dict[str, float] = {}

    # Format: { "files": [ { "path": "...", "coverage": [...] }, ... ] }
    if "files" in data:
        for entry in data["files"]:
            rel = _normalize_path(entry.get("path", ""))
            if not rel or not _is_src_rs(rel):
                continue
            cov = entry.get("coverage", [])
            result[rel] = _compute_line_pct(cov)
        return result

    # Format: { "path": coverage_array, ... } (flat map)
    for path, cov in data.items():
        rel = _normalize_path(path)
        if not rel or not _is_src_rs(rel):
            continue
        if isinstance(cov, list):
            result[rel] = _compute_line_pct(cov)
        elif isinstance(cov, (int, float)):
            result[rel] = float(cov)

    return result


def _normalize_path(p: str) -> str:
    """Strip absolute prefix, './', and normalize separators."""
    p = p.replace("\\", "/")
    # Strip up to and including 'src/' or project root
    for marker in ["/src/", "src/"]:
        idx = p.find(marker)
        if idx != -1:
            return p[idx + len(marker) :]
    try:
        rel = str(pathlib.Path(p).relative_to(ROOT))
        if rel.startswith("src/"):
            return rel[len("src/") :]
        return rel
    except ValueError:
        return p


def _is_src_rs(rel: str) -> bool:
    """Only enforce thresholds on Rust source files under src/."""
    return rel.endswith(".rs") and not rel.startswith("tests/")


def _compute_line_pct(coverage: list) -> float:
    """Given tarpaulin's per-line counts, return line coverage percentage."""
    if not coverage:
        return 0.0
    covered = sum(1 for entry in coverage if _count(entry) > 0)
    total = len(coverage)
    return (covered / total) * 100.0 if total > 0 else 0.0


def _count(entry) -> int:
    """Extract the hit count from a tarpaulin coverage entry."""
    if isinstance(entry, dict):
        return entry.get("count", 0)
    if isinstance(entry, (int, float)):
        return int(entry)
    return 0


def check_file_coverage(
    current: Dict[str, float],
    baseline: Dict[str, float],
    tdd_files: set,
    global_threshold: float,
    tdd_threshold: float,
    new_file_threshold: float,
    core_prefixes: List[str],
    core_threshold: float,
    report_path: pathlib.Path,
) -> int:
    """Enforce per-file coverage thresholds.  Returns exit code.

    Priority (highest wins): TDD > core > new/touched baseline.
    """
    failures: List[str] = []
    warnings: List[str] = []
    core_failures: List[str] = []
    all_paths = sorted(set(current.keys()) | set(baseline.keys()))

    for path in all_paths:
        cur = current.get(path)
        prev = baseline.get(path)

        if cur is None:
            continue

        is_tdd = path in tdd_files
        is_core = not is_tdd and _matches_core_prefix(path, core_prefixes)
        is_new = prev is None

        if is_tdd:
            floor = tdd_threshold
            tag = "TDD"
        elif is_core:
            floor = core_threshold
            tag = "CORE"
        elif is_new:
            floor = new_file_threshold
            tag = "NEW"
        else:
            floor = max(prev, global_threshold)
            tag = "TOUCHED"

        if cur < floor:
            delta = cur - (prev or 0.0)
            msg = (
                f"[{tag}] {path}: {cur:.1f}% < {floor:.0f}% "
                f"(was {prev:.1f}%, delta={delta:+.1f}%)"
            )
            if is_core:
                core_failures.append(msg)
            else:
                failures.append(msg)
        elif is_new and cur < global_threshold:
            warnings.append(
                f"[{tag}] {path}: {cur:.1f}% (below global {global_threshold:.0f}% "
                f"but above new-file ramp-up {new_file_threshold:.0f}%)"
            )

    if warnings:
        print("=== Coverage warnings (ramp-up grace) ===")
        for w in warnings:
            print(f"  {w}")
        print()

    if core_failures:
        print(f"=== Core-path coverage failures ({len(core_failures)} files) ===")
        for f in core_failures:
            print(f"  {f}")
        print()

    if failures:
        print(f"=== Coverage failures ({len(failures)} files) ===")
        for f in failures:
            print(f"  {f}")
        print()

    if failures or core_failures:
        print(
            f"Thresholds: TDD ≥{tdd_threshold:.0f}%, "
            f"core ≥{core_threshold:.0f}%, "
            f"new files ≥{new_file_threshold:.0f}%, "
            f"touched must not regress below baseline, "
            f"global floor ≥{global_threshold:.0f}%"
        )
        return 1

    print(
        f"All {len(current)} source files meet coverage thresholds "
        f"(TDD≥{tdd_threshold:.0f}%, core≥{core_threshold:.0f}%, "
        f"new≥{new_file_threshold:.0f}%, global≥{global_threshold:.0f}%)."
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Enforce per-file coverage thresholds from a tarpaulin JSON report."
    )
    parser.add_argument(
        "--report",
        required=True,
        type=pathlib.Path,
        help="Path to tarpaulin JSON report.",
    )
    parser.add_argument(
        "--baseline",
        required=True,
        type=pathlib.Path,
        help="Path to prior coverage baseline JSON (created if missing).",
    )
    parser.add_argument(
        "--threshold",
        type=float,
        default=80.0,
        help="TDD-mandated file line-coverage floor (default: 80).",
    )
    parser.add_argument(
        "--tdd-files",
        type=pathlib.Path,
        default=None,
        help="File listing TDD-mandated paths (one per line, relative to src/).",
    )
    parser.add_argument(
        "--global-floor",
        type=float,
        default=70.0,
        help="Global coverage floor for all src files (default: 70).",
    )
    parser.add_argument(
        "--new-file-floor",
        type=float,
        default=60.0,
        help="Ramp-up coverage floor for files without a baseline (default: 60).",
    )
    parser.add_argument(
        "--core-files",
        type=pathlib.Path,
        default=None,
        help="File listing core security paths (one prefix per line, matched by prefix).",
    )
    parser.add_argument(
        "--core-threshold",
        type=float,
        default=70.0,
        help="Core-path coverage floor (default: 70).",
    )
    parser.add_argument(
        "--save-baseline",
        type=pathlib.Path,
        default=None,
        help="Path to write the updated baseline snapshot (default: overwrite --baseline).",
    )
    args = parser.parse_args()

    if not args.report.exists():
        print(f"Coverage report not found: {args.report}", file=sys.stderr)
        return 1

    current = parse_tarpaulin_json(args.report)
    baseline = load_baseline(args.baseline)
    tdd_files = load_tdd_files(args.tdd_files)
    core_prefixes = load_core_prefixes(args.core_files)

    if not current:
        print("No source-file coverage data found in report.", file=sys.stderr)
        return 1

    exit_code = check_file_coverage(
        current=current,
        baseline=baseline,
        tdd_files=tdd_files,
        global_threshold=args.global_floor,
        tdd_threshold=args.threshold,
        new_file_threshold=args.new_file_floor,
        core_prefixes=core_prefixes,
        core_threshold=args.core_threshold,
        report_path=args.report,
    )

    save_path = args.save_baseline or args.baseline
    save_baseline(save_path, current)

    return exit_code


if __name__ == "__main__":
    sys.exit(main())
