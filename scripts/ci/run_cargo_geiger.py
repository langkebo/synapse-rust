#!/usr/bin/env python3
"""cargo-geiger unsafe-usage gate (two-scan difference).

Policy
------
  * **Production** (what we ship) unsafe usage must be **zero**.
  * **Test-only** unsafe is tracked against a ratchet baseline
    (`scripts/ci/geiger_baseline.json`).

How "production" and "test-only" are separated (option C)
---------------------------------------------------------
`cargo-geiger --output-format Json` emits a `SafetyReport` per package and contains
**no file paths at all**, so production/test split can only be done by scanning
twice and subtracting. Two schema generations exist and the gate now supports only
the one it can observe (failing loudly otherwise, so a silent zero is impossible):

  * **0.13** (current):
    `{"packages": [ {"package": {"id": {"name": …, "version": …, "source":
    {"Path": "file://…"} | {"Registry": …}}}, "unsafety": {"used": {"functions":
    {"safe": n, "unsafe_": m}, "exprs": …, "item_impls": …, "methods": …}, …}}, … ],
    "packages_without_metrics": [...], "used_but_not_scanned_files": [...]}`
    — a **list**, counters **nested**, workspace members marked by
    `source = {"Path": "file://…"}`.
  * **0.12 and earlier**: `{"packages": {<package id>: {…}}}` — a map keyed by the
    old-style package id (`"path+file://…"`). The previous design ("split the JSON
    entries by `/tests/` in the file path") could never work on either: it iterated
    the top-level object as if it were a list of file entries and read counter names
    (`extern_blocks`/…) that are not in the schema. On top of that `--output-format
    json` was the wrong case (the enum is `Json`, case-sensitive), so the subprocess
    exited non-zero before scanning. Result: the gate crashed on 0.13 (measured
    2026-09-21, CI run 35553786373: `expected a SafetyReport with a packages object,
    got ['packages', 'packages_without_metrics', 'used_but_not_scanned_files']`) and
    counted 0 before that.

This version runs cargo-geiger **twice** and subtracts per package:

    prod = cargo geiger                      (test targets excluded)
    all  = cargo geiger --include-tests      (test targets included)
    test = all - prod                        (unsafe that exists only in tests)

Counter names are **not** hardcoded: per package we sum every `unsafe_` integer
found anywhere under `unsafety.used`, so an upstream counter rename keeps working,
while any structural surprise (wrong `packages` type, non-dict entry, missing
`unsafety`/`used`, no `unsafe_` counter at all, a path package listed in
`packages_without_metrics`, differing package sets, negative difference) is a **loud
failure** instead of a silent zero.

Scope: only packages whose `id.source` is a `Path` (i.e. this workspace's members)
are counted. Third-party registry crates are ignored — their unsafe is not ours to
fix, and including it would drown the counters we enforce.

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
from datetime import date
from pathlib import Path

ROOT_DIR = Path(__file__).resolve().parent.parent.parent
DEFAULT_BASELINE = ROOT_DIR / "scripts" / "ci" / "geiger_baseline.json"
DEFAULT_REPORT = ROOT_DIR / "artifacts" / "cargo-geiger.json"

# Keys the gate actually consumes. Anything else in the baseline file is inert by
# construction, which is how `"prod_unsafe_total": 4` once sat here for months while
# the gate hard-failed at `prod_total > 0` — a field contradicting the enforced
# policy that nothing read. Everything below is read *and* enforced: the itemised
# site lists must add up to their totals, every site needs a justification and a
# `review_by` date that has not passed.
KNOWN_BASELINE_KEYS = frozenset(
    {
        "test_unsafe_total",
        "prod_unsafe_total",
        "prod_unsafe_sites",
        "test_unsafe_sites",
        "note",
    }
)

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


def is_path_package(source) -> bool:
    """True when a package `id.source` marks a workspace-member (path) package."""
    return isinstance(source, dict) and "Path" in source


def unsafe_used_total(used: dict, label: str, who: str) -> int:
    """Sum every `unsafe_` counter anywhere under `unsafety.used`.

    Counter names may be renamed upstream (`exprs`/`functions`/`item_impls`/…),
    so nothing is hardcoded — but finding **no** `unsafe_` counter at all is a
    loud failure: that is exactly the shape change that used to make this gate
    report zero.
    """
    total = 0
    seen = 0
    stack: list[object] = [used]
    while stack:
        node = stack.pop()
        if not isinstance(node, dict):
            print(
                f"FAIL: {label}: {who}: expected an object in `unsafety.used`, got "
                f"{type(node).__name__}",
                file=sys.stderr,
            )
            sys.exit(2)
        for key, value in node.items():
            if key == "unsafe_":
                if isinstance(value, bool) or not isinstance(value, int):
                    print(
                        f"FAIL: {label}: {who}: counter 'unsafe_' is not an integer "
                        f"({value!r}); refusing to guess.",
                        file=sys.stderr,
                    )
                    sys.exit(2)
                total += value
                seen += 1
            elif isinstance(value, dict):
                stack.append(value)
            elif isinstance(value, int) and not isinstance(value, bool):
                continue
            else:
                print(
                    f"FAIL: {label}: {who}: unexpected {key!r} value {value!r} under "
                    "`unsafety.used`; the schema changed — fix the parser instead of "
                    "letting the gate guess.",
                    file=sys.stderr,
                )
                sys.exit(2)
    if seen == 0:
        print(
            f"FAIL: {label}: {who}: no `unsafe_` counter found under `unsafety.used` "
            f"(keys: {sorted(used.keys())}). cargo-geiger's schema changed — fix the "
            "parser instead of letting the gate count zero.",
            file=sys.stderr,
        )
        sys.exit(2)
    return total


def shipped_unsafe_totals(report: dict, label: str) -> dict[str, int]:
    """Sum unsafe usages per **workspace (path) package** in a SafetyReport.

    Fails loudly on any structural surprise: a silently-zero gate is precisely the
    defect this function exists to prevent.
    """
    if not isinstance(report, dict) or not isinstance(report.get("packages"), list):
        keys = (
            sorted(report.keys()) if isinstance(report, dict) else type(report).__name__
        )
        shape = (
            type(report.get("packages")).__name__ if isinstance(report, dict) else "n/a"
        )
        print(
            f"FAIL: {label}: expected a SafetyReport whose `packages` is a LIST "
            f"(cargo-geiger >= 0.13), got report keys {keys} with `packages` = {shape}. "
            "cargo-geiger's schema may have changed — fix the parser instead of letting "
            "the gate count zero.",
            file=sys.stderr,
        )
        sys.exit(2)

    totals: dict[str, int] = {}
    for entry in report["packages"]:
        if not isinstance(entry, dict):
            print(
                f"FAIL: {label}: package entry is not an object ({type(entry).__name__})",
                file=sys.stderr,
            )
            sys.exit(2)
        package = entry.get("package")
        pkg_id = package.get("id") if isinstance(package, dict) else None
        if not isinstance(pkg_id, dict):
            print(
                f"FAIL: {label}: package entry has no `package.id` object "
                f"(keys: {sorted(entry.keys())})",
                file=sys.stderr,
            )
            sys.exit(2)
        # Only our own crates: workspace members carry a `Path` source.
        if not is_path_package(pkg_id.get("source")):
            continue
        name = str(pkg_id.get("name", "<unnamed>"))
        version = pkg_id.get("version")
        who = f"{name} {version}" if version else name
        unsafety = entry.get("unsafety")
        if not isinstance(unsafety, dict):
            print(
                f"FAIL: {label}: package {who} has no `unsafety` object "
                f"(keys: {sorted(entry.keys())})",
                file=sys.stderr,
            )
            sys.exit(2)
        used = unsafety.get("used")
        if not isinstance(used, dict):
            print(
                f"FAIL: {label}: package {who} has no `unsafety.used` object "
                f"(keys: {sorted(unsafety.keys())})",
                file=sys.stderr,
            )
            sys.exit(2)
        totals[who] = unsafe_used_total(used, label, who)

    without_metrics = report.get("packages_without_metrics")
    if isinstance(without_metrics, list):
        blind = [
            str(item.get("id", {}).get("name", item))
            for item in without_metrics
            if isinstance(item, dict)
            and is_path_package(item.get("id", {}).get("source"))
        ]
        if blind:
            print(
                f"FAIL: {label}: workspace package(s) {blind} appear in "
                "`packages_without_metrics` — the scan did not measure them, so the "
                "unsafe total below would be an undercount. Refusing to report a green gate.",
                file=sys.stderr,
            )
            sys.exit(2)

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
    return {
        "prod_unsafe_total": 0,
        "test_unsafe_total": 0,
        "note": "baseline not found; using zero-defaults",
    }


def validate_baseline_keys(baseline: dict, path: Path) -> str | None:
    """Reject inert fields; validate every field the gate reads.

    Since 2026-09-21 the production counter is a one-way ratchet, so its units must
    be **itemised, justified and dated** right here: the site lists must add up to
    their totals (otherwise the baseline documents one thing and enforces another)
    and no `review_by` date may be in the past (a stale justification is how an
    accepted exception silently becomes permanent).
    """
    unknown = sorted(set(baseline) - KNOWN_BASELINE_KEYS)
    if unknown:
        return (
            f"{path} contains keys the gate does not read: {unknown}. Every field here must be "
            f"enforced (known: {sorted(KNOWN_BASELINE_KEYS)}) — an inert field is how a "
            f"`prod_unsafe_total` ceiling once contradicted the enforced policy without "
            f"changing it."
        )
    for key in ("prod_unsafe_total", "test_unsafe_total"):
        value = baseline.get(key, 0)
        if isinstance(value, bool) or not isinstance(value, int) or value < 0:
            return f"{path}: {key} must be a non-negative integer, got {value!r}"

    today = date.today().isoformat()
    for total_key, sites_key in (
        ("prod_unsafe_total", "prod_unsafe_sites"),
        ("test_unsafe_total", "test_unsafe_sites"),
    ):
        sites = baseline.get(sites_key)
        if not isinstance(sites, list):
            return (
                f"{path}: {sites_key} must be a list itemising every unit counted by "
                f"{total_key} (got {type(sites).__name__})"
            )
        summed = 0
        for index, site in enumerate(sites):
            if not isinstance(site, dict):
                return f"{path}: {sites_key}[{index}] is not an object"
            for field in ("package", "count", "why", "review_by"):
                if field not in site:
                    return f"{path}: {sites_key}[{index}] is missing `{field}`"
            count = site["count"]
            if isinstance(count, bool) or not isinstance(count, int) or count < 1:
                return f"{path}: {sites_key}[{index}].count must be a positive integer, got {count!r}"
            if not isinstance(site["why"], str) or not site["why"].strip():
                return f"{path}: {sites_key}[{index}].why must be a non-empty justification"
            review_by = site["review_by"]
            if (
                not isinstance(review_by, str)
                or len(review_by) != 10
                or review_by[4] != "-"
            ):
                return f"{path}: {sites_key}[{index}].review_by must be an ISO date (YYYY-MM-DD)"
            if review_by < today:
                return (
                    f"{path}: {sites_key}[{index}] (package {site['package']!r}) is overdue: "
                    f"review_by {review_by} < today {today}. Re-review the site with fresh "
                    f"evidence and either renew the date or remove the unsafe usage."
                )
            summed += count
        if summed != baseline.get(total_key, 0):
            return (
                f"{path}: {sites_key} sums to {summed} but {total_key} is "
                f"{baseline.get(total_key, 0)} — the itemised list must account for every unit "
                "the gate enforces."
            )
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
    baseline_prod = baseline.get("prod_unsafe_total", 0)

    offline = args.prod_report is not None or args.all_report is not None
    if offline:
        if args.prod_report is None or args.all_report is None:
            print(
                "FAIL: offline mode needs BOTH --prod-report and --all-report",
                file=sys.stderr,
            )
            return 2
        prod_report = json.loads(args.prod_report.read_text())
        all_report = json.loads(args.all_report.read_text())
    else:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        prod_report = run_geiger([])
        all_report = run_geiger(["--include-tests"])
        args.report.write_text(
            json.dumps({"prod": prod_report, "all": all_report}, indent=2)
        )
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

    # ── Gate 1: Production unsafe is a **one-way ratchet** (2026-09-21 ruling) ──
    #
    # Policy history: this used to be a hard zero with no allowlist. Fixing the
    # 0.13 parser (see the module docstring) made the gate report its first *true*
    # verdict ever — 2 production unsafe usages — and a hard zero would have forced
    # either a redesign of the test-schema janitor's `atexit` backstop or an
    # unexplained red. The ruling was to keep the number **visible and itemised**
    # instead: production unsafe may only go DOWN, every unit must be justified in
    # the baseline, and an unexplained decrease fails so the baseline cannot rot
    # into a loose ceiling.
    if prod_total > baseline_prod:
        print(f"\nFAIL: production unsafe increased ({baseline_prod} -> {prod_total}).")
        print(
            "      Production unsafe is a one-way ratchet: it may only go down. New unsafe in"
        )
        print(
            "      shipped code must be removed, not baselined — rewrite it without `unsafe`,"
        )
        print(
            "      or move it under `tests/` (the prod scan excludes test *targets*, not test"
        )
        print("      *modules* in src/).")
        return 1
    if prod_total < baseline_prod:
        print(
            f"\nFAIL: production unsafe decreased ({baseline_prod} -> {prod_total}) — good news,"
        )
        print(
            f"      but the ratchet must be tightened: set `prod_unsafe_total` to {prod_total} in"
        )
        print(
            f"      {args.baseline}, delete the now-obsolete justification(s), and say why."
        )
        return 1

    # ── Gate 2: Test-only unsafe must not exceed baseline (ratchet) ──
    if test_total > baseline_test:
        print(
            f"\nFAIL: test-only unsafe ({test_total}) exceeds baseline ({baseline_test})."
        )
        print(f"      Raise `test_unsafe_total` in {args.baseline} with justification.")
        return 1

    print("\ncargo-geiger: PASS")
    print(
        f"  Production unsafe: {prod_total} (baseline: {baseline_prod}; ratchet, may only go down)"
    )
    print(f"  Test-only unsafe:  {test_total} (baseline: {baseline_test})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
