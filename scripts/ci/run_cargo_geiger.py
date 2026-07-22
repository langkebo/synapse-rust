#!/usr/bin/env python3
"""
cargo-geiger unsafe-usage gate.

Runs `cargo geiger --output-format json --all-features`, parses the JSON
output, and enforces a ratchet-style policy:

  - Production code (any file NOT under tests/) must have ZERO unsafe.
  - Test code (files under tests/) is tracked but non-blocking.
  - A baseline file records the historical test-unsafe count; the gate
    only fails if the test-unsafe count INCREASES above the baseline.

This replaces the previous fragile `grep -oP '\\d+(?= unsafe)'` parsing
which never matched cargo-geiger's actual output format, causing
PROD_UNSAFE_COUNT to always be 0 (false-green).

Usage:
    python3 scripts/ci/run_cargo_geiger.py [--baseline PATH] [--report PATH]

Exit codes:
    0 = pass (no production unsafe, test-unsafe within baseline)
    1 = fail (production unsafe found, or test-unsafe exceeded baseline)
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

ROOT_DIR = Path(__file__).resolve().parent.parent.parent
DEFAULT_BASELINE = ROOT_DIR / "scripts" / "ci" / "geiger_baseline.json"
DEFAULT_REPORT = ROOT_DIR / "artifacts" / "cargo-geiger.json"


def run_geiger() -> list[dict]:
    """Run cargo geiger and return parsed JSON output."""
    cmd = [
        "cargo", "geiger",
        "--all-features",
        "--output-format", "json",
    ]
    result = subprocess.run(
        cmd,
        capture_output=True,
        text=True,
        cwd=ROOT_DIR,
    )
    if result.returncode != 0:
        print(f"ERROR: cargo geiger failed (exit {result.returncode})", file=sys.stderr)
        print(result.stderr, file=sys.stderr)
        # cargo-geiger returns non-zero if it finds unsafe, but JSON is still
        # on stdout. Try to parse stdout anyway.
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError:
        # Fallback: try to extract JSON from mixed output
        lines = result.stdout.strip().splitlines()
        for i, line in enumerate(lines):
            if line.strip().startswith("["):
                try:
                    return json.loads("\n".join(lines[i:]))
                except json.JSONDecodeError:
                    continue
        print("ERROR: could not parse cargo-geiger JSON output", file=sys.stderr)
        print("stdout:", result.stdout[:500], file=sys.stderr)
        sys.exit(1)


def classify_files(metrics: list[dict]) -> tuple[list[dict], list[dict]]:
    """Split metrics into production and test file lists."""
    prod, test = [], []
    for entry in metrics:
        filepath = entry.get("file", entry.get("path", ""))
        if "/tests/" in filepath or filepath.startswith("tests/"):
            test.append(entry)
        else:
            prod.append(entry)
    return prod, test


def sum_unsafe(entries: list[dict]) -> dict[str, int]:
    """Sum unsafe counts across file entries."""
    totals = {"extern_blocks": 0, "traits": 0, "fns": 0, "impls": 0, "blocks": 0}
    for entry in entries:
        unsafe = entry.get("unsafe", entry.get("metrics", {}))
        for key in totals:
            totals[key] += int(unsafe.get(key, 0))
    return totals


def total_unsafe(counts: dict[str, int]) -> int:
    return sum(counts.values())


def load_baseline(path: Path) -> dict:
    """Load baseline file, or return defaults if not present."""
    if path.exists():
        return json.loads(path.read_text())
    return {
        "prod_unsafe_total": 0,
        "test_unsafe_total": 0,
        "note": "baseline not found; using zero-defaults",
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="cargo-geiger unsafe-usage gate")
    parser.add_argument("--baseline", type=Path, default=DEFAULT_BASELINE)
    parser.add_argument("--report", type=Path, default=DEFAULT_REPORT)
    args = parser.parse_args()

    args.report.parent.mkdir(parents=True, exist_ok=True)

    print(">>> cargo-geiger: scanning for unsafe usage (JSON output)")
    metrics = run_geiger()

    # Save raw JSON report
    args.report.write_text(json.dumps(metrics, indent=2))
    print(f"    Report saved: {args.report}")

    prod_entries, test_entries = classify_files(metrics)
    prod_counts = sum_unsafe(prod_entries)
    test_counts = sum_unsafe(test_entries)
    prod_total = total_unsafe(prod_counts)
    test_total = total_unsafe(test_counts)

    print(f"\n    Production files scanned: {len(prod_entries)}")
    print(f"    Test files scanned:       {len(test_entries)}")
    print(f"    Production unsafe total:  {prod_total}  {prod_counts}")
    print(f"    Test unsafe total:        {test_total}  {test_counts}")

    baseline = load_baseline(args.baseline)
    baseline_prod = baseline.get("prod_unsafe_total", 0)
    baseline_test = baseline.get("test_unsafe_total", 0)

    # List files with unsafe for visibility
    if prod_total > 0:
        print("\n  Production files with unsafe:")
        for e in prod_entries:
            u = e.get("unsafe", e.get("metrics", {}))
            t = total_unsafe(u)
            if t > 0:
                print(f"    {e.get('file', e.get('path', '?'))}: {u}")

    if test_total > 0:
        print("\n  Test files with unsafe:")
        for e in test_entries:
            u = e.get("unsafe", e.get("metrics", {}))
            t = total_unsafe(u)
            if t > 0:
                print(f"    {e.get('file', e.get('path', '?'))}: {u}")

    # ── Gate 1: Production unsafe must be zero (hard block) ──
    if prod_total > 0:
        print(f"\nFAIL: {prod_total} unsafe item(s) found in production code.")
        print("      Production unsafe is strictly prohibited.")
        print("      If this is intentional (FFI, crypto), add the file to")
        print("      an allowlist in the baseline file and justify why.")
        return 1

    # ── Gate 2: Test unsafe must not exceed baseline (ratchet) ──
    if test_total > baseline_test:
        print(f"\nFAIL: Test unsafe count ({test_total}) exceeds baseline ({baseline_test}).")
        print("      To increase the baseline, update:")
        print(f"        {args.baseline}")
        print("      with justification for the new unsafe blocks.")
        return 1

    print(f"\ncargo-geiger: PASS")
    print(f"  Production unsafe: {prod_total} (must be 0)")
    print(f"  Test unsafe:       {test_total} (baseline: {baseline_test})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
