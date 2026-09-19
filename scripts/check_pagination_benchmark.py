#!/usr/bin/env python3
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
        description="Assert keyset pagination benchmark gain."
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
        f"offset={offset:.2f}ns keyset={keyset:.2f}ns improvement={improvement * 100:.2f}%"
    )

    return 0 if improvement >= args.minimum_improvement else 1


if __name__ == "__main__":
    raise SystemExit(main())
