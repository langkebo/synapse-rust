#!/usr/bin/env python3
from __future__ import annotations

import argparse
import re
from pathlib import Path


BENCH_RE = re.compile(
    r"^(?:test\s+)?(?P<name>pagination_(?:offset|keyset)_deep_page)\s+.*bench:\s+"
    r"(?P<value>[0-9,]+(?:\.[0-9]+)?)\s+(?P<unit>ns|us|ms)/iter"
)
UNIT_SCALE = {"ns": 1.0, "us": 1_000.0, "ms": 1_000_000.0}


def parse_benchmarks(text: str) -> dict[str, float]:
    results: dict[str, float] = {}
    for line in text.splitlines():
        match = BENCH_RE.match(line.strip())
        if not match:
            continue
        value = float(match.group("value").replace(",", ""))
        results[match.group("name")] = value * UNIT_SCALE[match.group("unit")]
    return results


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Assert keyset pagination benchmark gain."
    )
    parser.add_argument("benchmark_file")
    parser.add_argument("--minimum-improvement", type=float, default=0.30)
    args = parser.parse_args()

    bench_file = Path(args.benchmark_file)
    results = parse_benchmarks(bench_file.read_text(encoding="utf-8"))

    offset = results.get("pagination_offset_deep_page")
    keyset = results.get("pagination_keyset_deep_page")
    if offset is None or keyset is None:
        raise SystemExit("pagination benchmark rows were not found in benchmark output")

    improvement = (offset - keyset) / offset
    print(
        f"offset={offset:.2f}ns keyset={keyset:.2f}ns improvement={improvement * 100:.2f}%"
    )

    return 0 if improvement >= args.minimum_improvement else 1


if __name__ == "__main__":
    raise SystemExit(main())
