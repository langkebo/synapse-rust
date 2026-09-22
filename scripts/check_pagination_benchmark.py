#!/usr/bin/env python3
"""Compute-path smoke check for the *simulated* pagination functions.

⚠️  This is **not** the pagination performance gate. It compares two
in-memory functions in `benches/performance_api_benchmarks.rs`
(`pagination_offset_deep_page` walks a synthetic `Vec` of 250k rows,
`pagination_keyset_deep_page` binary-searches the same `Vec`). Neither touches a
database, and the simulated margin is ~1500x, so **no real `synapse-storage`
SQL regression can move the ratio below the 30% threshold** (E4,
`docs/archive/GATE_INTEGRITY_FOLLOWUP_2026-09-19_LOG.md` §9).

What this script still buys: a fail-closed smoke check that the two compute
benchmarks keep existing, keep being emitted by the harness, and that the
compute path itself has not regressed. Its parser guards (duplicate row /
missing file / empty output → exit 2) stay in place.

The real pagination protection lives in the DB-backed gate:
  * bench: `benches/performance_pagination_benchmarks.rs`
  * gate:  `scripts/ci/pagination_perf_gate.sh`
    (real keyset SQL vs real `LIMIT/OFFSET`, threshold `PAGINATION_MIN_GAIN`,
    plus a same-page correctness check; wired into
    `.github/workflows/benchmark.yml::pagination-perf-gate`)
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


BENCH_RE = re.compile(
    r"^(?:test\s+)?(?P<name>pagination_(?:offset|keyset)_deep_page)\s+.*bench:\s+"
    r"(?P<value>[0-9,]+(?:\.[0-9]+)?)\s+(?P<unit>ns|us|ms)/iter"
)
UNIT_SCALE = {"ns": 1.0, "us": 1_000.0, "ms": 1_000_000.0}
REQUIRED_ROWS = ("pagination_offset_deep_page", "pagination_keyset_deep_page")


def fail(message: str) -> None:
    """Fail closed (exit 2): a gate that cannot prove its premise must not print OK."""
    print(f"ERROR: check_pagination_benchmark: {message}", file=sys.stderr)
    raise SystemExit(2)


def parse_benchmarks(text: str) -> dict[str, float]:
    results: dict[str, float] = {}
    for line in text.splitlines():
        match = BENCH_RE.match(line.strip())
        if not match:
            continue
        name = match.group("name")
        if name in results:
            # `benchmark.txt` is appended to by several `cargo bench` steps, so a
            # repeated row means two samples exist under one name. The old
            # implementation kept the last one silently, which let a slower
            # sample be discarded and turned the comparison into a comparison
            # against a value that was never the measurement.
            fail(
                f"benchmark row `{name}` appears more than once; refusing to guess "
                "which sample to compare (regenerate benchmark.txt from a single run)"
            )
        value = float(match.group("value").replace(",", ""))
        results[name] = value * UNIT_SCALE[match.group("unit")]
    return results


def main() -> int:
    parser = argparse.ArgumentParser(
        description=(
            "Smoke check the in-memory pagination compute benchmarks. "
            "This is NOT the DB-backed pagination gate; see "
            "scripts/ci/pagination_perf_gate.sh."
        )
    )
    parser.add_argument("benchmark_file")
    parser.add_argument("--minimum-improvement", type=float, default=0.30)
    args = parser.parse_args()

    bench_file = Path(args.benchmark_file)
    if not bench_file.is_file():
        fail(f"benchmark output not found: {bench_file} — nothing was measured")
    text = bench_file.read_text(encoding="utf-8")
    if not text.strip():
        fail(
            f"benchmark output is empty: {bench_file} — pagination benchmark rows "
            "were not found (nothing was measured)"
        )

    results = parse_benchmarks(text)

    missing = [name for name in REQUIRED_ROWS if name not in results]
    if missing:
        fail(
            "pagination benchmark rows were not found in benchmark output: "
            + ", ".join(missing)
            + " (the harness may have silently skipped them; check BENCH_REQUIRE)"
        )

    offset = results["pagination_offset_deep_page"]
    keyset = results["pagination_keyset_deep_page"]
    if offset <= 0.0:
        fail(f"offset measurement is not positive ({offset}); the ratio is undefined")

    improvement = (offset - keyset) / offset
    print(
        "in-memory compute smoke check (NOT the DB-backed pagination gate; "
        "the real guard is scripts/ci/pagination_perf_gate.sh)"
    )
    print(
        f"offset={offset:.2f}ns keyset={keyset:.2f}ns improvement={improvement * 100:.2f}%"
    )

    return 0 if improvement >= args.minimum_improvement else 1


if __name__ == "__main__":
    raise SystemExit(main())
