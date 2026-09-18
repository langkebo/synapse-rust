#!/usr/bin/env python3
"""cargo-geiger unsafe-usage gate (two-scan difference).

Policy
------
  * **Production** (what we ship) unsafe usage must be **zero**.
  * **Test-only** unsafe is tracked against a ratchet baseline
    (`scripts/ci/geiger_baseline.json`).

How "production" and "test-only" are separated (option C)
---------------------------------------------------------
`cargo-geiger --output-format Json` emits a `SafetyReport` indexed **by package**
(`{"packages": {<package id>: {"package": …, "unsafety": …}}, …}`) and it contains
**no file paths at all**. The previous design ("split the JSON entries by `/tests/`
in the file path") therefore could never work: it iterated the top-level object as
if it were a list of file entries and read counter names (`extern_blocks`/… ) that
are not in the schema. On top of that `--output-format json` was the wrong case
(the enum is `Json`, case-sensitive), so the subprocess exited non-zero before
scanning. Result: the gate either crashed or counted 0 forever.

This version runs cargo-geiger **twice** and subtracts per package:

    prod = cargo geiger                      (test targets excluded)
    all  = cargo geiger --include-tests      (test targets included)
    test = all - prod                        (unsafe that exists only in tests)

Counter names are **not** hardcoded: per package we sum every integer under
`unsafety.used`, so an upstream counter rename keeps working, while any structural
surprise (missing `packages`, non-dict entry, non-integer counter, differing
package sets, negative difference) is a **loud failure** instead of a silent zero.

Scope: only packages that are path dependencies of this workspace
(`path+file://` in the package id) are counted. Third-party registry crates are
ignored — their unsafe is not ours to fix, and including it would make the
hard-zero Gate 1 unenforceable.

Usage
-----
    python3 scripts/ci/run_cargo_geiger.py [--baseline PATH] [--report PATH]
    # offline / test mode (no cargo-geiger needed):
    python3 scripts/ci/run_cargo_geiger.py --prod-report P.json --all-report A.json

Exit codes
----------
    0 = pass   1 = policy violation   2 = cannot evaluate (bad input/shape/tool)
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

# Keys the gate actually consumes. Anything else in the baseline file is inert by
# construction, which is how `"prod_unsafe_total": 4` sat here for months while
# Gate 1 hard-failed at `prod_total > 0` — a field contradicting the enforced
# policy that nothing read.
KNOWN_BASELINE_KEYS = frozenset({"test_unsafe_total", "note"})

# `--output-format Json` (capital J): OutputFormat is a case-sensitive strum enum,
# so the lowercase `json` made the subprocess exit before any scanning happened.
GEIGER_BASE_CMD = ["cargo", "geiger", "--all-features", "--output-format", "Json"]


def run_geiger(extra: list[str]) -> dict:
    """Run one cargo-geiger scan and return the parsed SafetyReport."""
    cmd = GEIGER_BASE_CMD + extra
    print(f">>> {' '.join(cmd)}")
    result = subprocess.run(cmd, capture_output=True, text=True, cwd=ROOT_DIR)
    # cargo-geiger exits non-zero when it finds unsafe, but still prints JSON.
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError:
        lines = result.stdout.strip().splitlines()
        for i, line in enumerate(lines):
            if line.strip().startswith("{"):
                try:
                    return json.loads("\n".join(lines[i:]))
                except json.JSONDecodeError:
                    continue
    print(
        f"FAIL: could not parse cargo-geiger JSON (exit {result.returncode}).\n"
        f"      stdout head: {result.stdout[:300]!r}\n"
        f"      stderr head: {result.stderr[:300]!r}",
        file=sys.stderr,
    )
    sys.exit(2)


def shipped_unsafe_totals(report: dict, label: str) -> dict[str, int]:
    """Sum unsafe usages per **workspace (path) package** in a SafetyReport.

    Fails loudly on any structural surprise: a silently-zero gate is precisely the
    defect this function exists to prevent.
    """
    if not isinstance(report, dict) or not isinstance(report.get("packages"), dict):
        keys = sorted(report.keys()) if isinstance(report, dict) else type(report).__name__
        print(
            f"FAIL: {label}: expected a SafetyReport with a `packages` object, got {keys}. "
            "cargo-geiger's schema may have changed — fix the parser instead of letting the "
            "gate count zero.",
            file=sys.stderr,
        )
        sys.exit(2)

    totals: dict[str, int] = {}
    for pkg_id, entry in report["packages"].items():
        if not isinstance(entry, dict):
            print(f"FAIL: {label}: package {pkg_id!r} is not an object", file=sys.stderr)
            sys.exit(2)
        # Only our own crates: workspace members are path dependencies.
        if "path+file://" not in str(pkg_id):
            continue
        unsafety = entry.get("unsafety")
        if not isinstance(unsafety, dict):
            print(
                f"FAIL: {label}: package {pkg_id!r} has no `unsafety` object "
                f"(keys: {sorted(entry.keys())})",
                file=sys.stderr,
            )
            sys.exit(2)
        used = unsafety.get("used")
        if not isinstance(used, dict):
            print(
                f"FAIL: {label}: package {pkg_id!r} has no `unsafety.used` object "
                f"(keys: {sorted(unsafety.keys())})",
                file=sys.stderr,
            )
            sys.exit(2)
        total = 0
        for counter, value in used.items():
            if isinstance(value, bool) or not isinstance(value, int):
                print(
                    f"FAIL: {label}: package {pkg_id!r} counter {counter!r} is not an integer "
                    f"({value!r}); refusing to guess.",
                    file=sys.stderr,
                )
                sys.exit(2)
            total += value
        totals[str(pkg_id)] = total

    if not totals:
        print(
            f"FAIL: {label}: no workspace (path) packages found in the report. Either the scan "
            "covered nothing or the package-id format changed; refusing to report a green gate.",
            file=sys.stderr,
        )
        sys.exit(2)
    return totals


def load_baseline(path: Path) -> dict:
    """Load baseline file, or return defaults if not present."""
    if path.exists():
        return json.loads(path.read_text())
    return {"test_unsafe_total": 0, "note": "baseline not found; using zero-defaults"}


def validate_baseline_keys(baseline: dict, path: Path) -> str | None:
    """Reject a baseline field that nothing enforces."""
    unknown = sorted(set(baseline) - KNOWN_BASELINE_KEYS)
    if unknown:
        return (
            f"{path} contains keys the gate does not read: {unknown}. Every field here must be "
            f"enforced (known: {sorted(KNOWN_BASELINE_KEYS)}) — an inert field is how a "
            f"`prod_unsafe_total` ceiling contradicted the hard-zero policy without changing it."
        )
    value = baseline.get("test_unsafe_total", 0)
    if isinstance(value, bool) or not isinstance(value, int) or value < 0:
        return f"{path}: test_unsafe_total must be a non-negative integer, got {value!r}"
    return None


def main() -> int:
    parser = argparse.ArgumentParser(description="cargo-geiger unsafe-usage gate")
    parser.add_argument("--baseline", type=Path, default=DEFAULT_BASELINE)
    parser.add_argument("--report", type=Path, default=DEFAULT_REPORT)
    parser.add_argument(
        "--prod-report",
        type=Path,
        default=None,
        help="parse this existing prod-scan JSON instead of running cargo-geiger",
    )
    parser.add_argument(
        "--all-report",
        type=Path,
        default=None,
        help="parse this existing with-tests-scan JSON instead of running cargo-geiger",
    )
    args = parser.parse_args()

    # Validate the baseline BEFORE spending minutes in cargo-geiger.
    baseline = load_baseline(args.baseline)
    baseline_problem = validate_baseline_keys(baseline, args.baseline)
    if baseline_problem:
        print(f"FAIL: {baseline_problem}", file=sys.stderr)
        return 2
    baseline_test = baseline.get("test_unsafe_total", 0)

    offline = args.prod_report is not None or args.all_report is not None
    if offline:
        if args.prod_report is None or args.all_report is None:
            print("FAIL: offline mode needs BOTH --prod-report and --all-report", file=sys.stderr)
            return 2
        prod_report = json.loads(args.prod_report.read_text())
        all_report = json.loads(args.all_report.read_text())
    else:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        prod_report = run_geiger([])
        all_report = run_geiger(["--include-tests"])
        args.report.write_text(json.dumps({"prod": prod_report, "all": all_report}, indent=2))
        print(f"    Report saved: {args.report}")

    prod_per_pkg = shipped_unsafe_totals(prod_report, "prod scan")
    all_per_pkg = shipped_unsafe_totals(all_report, "all scan")

    prod_total = sum(prod_per_pkg.values())

    # Packages present in only one scan would make the subtraction meaningless.
    only_prod = sorted(set(prod_per_pkg) - set(all_per_pkg))
    if only_prod:
        print(
            f"FAIL: these packages appear in the prod scan but not the with-tests scan: {only_prod}. "
            "The two scans must cover the same workspace; refusing to subtract.",
            file=sys.stderr,
        )
        return 2

    test_only = {pkg: all_per_pkg[pkg] - prod_per_pkg[pkg] for pkg in all_per_pkg}
    negative = {pkg: v for pkg, v in test_only.items() if v < 0}
    if negative:
        print(
            f"FAIL: subtracting the scans gave a negative test-only count for {negative}. "
            "The scans are not comparable (different features/targets?); refusing to guess.",
            file=sys.stderr,
        )
        return 2
    test_total = sum(test_only.values())

    print(f"\n    Workspace packages scanned: {len(all_per_pkg)}")
    print(f"    Production unsafe total:    {prod_total}")
    print(f"    Test-only unsafe total:     {test_total}")
    if prod_total > 0:
        print("\n  Packages with production unsafe:")
        for pkg, value in sorted(prod_per_pkg.items()):
            if value > 0:
                print(f"    {pkg}: {value}")
    if test_total > 0:
        print("\n  Packages with test-only unsafe:")
        for pkg, value in sorted(test_only.items()):
            if value > 0:
                print(f"    {pkg}: {value}")

    # ── Gate 1: Production unsafe must be zero (hard block) ──
    if prod_total > 0:
        print(f"\nFAIL: {prod_total} unsafe usage(s) in shipped code.")
        print("      Production unsafe is strictly prohibited — no allowlist, no baseline ceiling.")
        print("      If it is really only inside `#[cfg(test)]`, move that code under `tests/`,")
        print("      because the prod scan excludes test *targets*, not test *modules* in src/.")
        return 1

    # ── Gate 2: Test-only unsafe must not exceed baseline (ratchet) ──
    if test_total > baseline_test:
        print(f"\nFAIL: test-only unsafe ({test_total}) exceeds baseline ({baseline_test}).")
        print(f"      Raise `test_unsafe_total` in {args.baseline} with justification.")
        return 1

    print("\ncargo-geiger: PASS")
    print(f"  Production unsafe: {prod_total} (must be 0)")
    print(f"  Test-only unsafe:  {test_total} (baseline: {baseline_test})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
