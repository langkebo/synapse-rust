#!/usr/bin/env python3
"""Merge multiple lcov.info files into a single coverage/lcov.info.

Usage:
    python3 scripts/merge_lcov.py /tmp/cov-storage/lcov.info /tmp/cov-rest/lcov.info \
        -o coverage/lcov.info

Semantics:
    - Each lcov record (SF + its lines) is kept verbatim when the source file
      appears in only one input. This preserves llvm-cov's original LF/LH
      (which may exceed the number of DA lines, e.g. function-definition lines
      that carry no execution counter).
    - When the same SF appears in multiple inputs, DA lines are merged by taking
      the max hit count per line, and LF/LH are recomputed from the merged DA.
    - Output order follows first-seen order across inputs.

Rationale (方案 B):
    `cargo tarpaulin --workspace` runs storage's db_tests (direct connection to
    the public schema) and the root crate's integration tests (which DROP the
    public schema in init_template_schema) with an unstable ordering, so one of
    them gets wiped. We run storage alone first (via cargo llvm-cov, single
    threaded), then the rest with `--exclude synapse-storage`, and merge here.
"""

import argparse
import sys
from collections import OrderedDict


def parse_lcov(path):
    """Parse an lcov file into an ordered dict: SF -> (block_lines, {line: hits})."""
    records = OrderedDict()
    current_sf = None
    current_block = []
    current_da = {}

    with open(path, "r", encoding="utf-8", errors="ignore") as f:
        for raw in f:
            line = raw.rstrip("\n")
            if line == "end_of_record":
                if current_sf is not None:
                    records[current_sf] = (current_block, current_da)
                current_sf = None
                current_block = []
                current_da = {}
                continue
            if line.startswith("SF:"):
                current_sf = line[3:]
                current_block = []
                current_da = {}
                continue
            if line.startswith("DA:"):
                parts = line[3:].split(",", 1)
                lineno = int(parts[0])
                hits = int(parts[1]) if len(parts) > 1 else 0
                current_da[lineno] = hits
                continue
            current_block.append(line)
    return records


def recompute_totals(da):
    lf = len(da)
    lh = sum(1 for h in da.values() if h > 0)
    return lf, lh


def main():
    ap = argparse.ArgumentParser(description="Merge lcov.info files")
    ap.add_argument("inputs", nargs="+", help="lcov.info files to merge")
    ap.add_argument("-o", "--output", default="coverage/lcov.info")
    args = ap.parse_args()

    merged = OrderedDict()  # sf -> {block, da, is_merged}
    for path in args.inputs:
        for sf, (block, da) in parse_lcov(path).items():
            if sf not in merged:
                merged[sf] = [list(block), dict(da), False]
            else:
                existing_block, existing_da, _ = merged[sf]
                for lineno, hits in da.items():
                    existing_da[lineno] = max(existing_da.get(lineno, 0), hits)
                merged[sf] = [existing_block, existing_da, True]

    total_lf = 0
    total_lh = 0
    with open(args.output, "w", encoding="utf-8") as f:
        for sf, (block, da, was_merged) in merged.items():
            f.write(f"SF:{sf}\n")
            for l in block:
                # Strip stale LF/LH/end markers from the block; we rewrite totals.
                if l.startswith(("LF:", "LH:")):
                    continue
                f.write(l + "\n")
            for lineno in sorted(da):
                f.write(f"DA:{lineno},{da[lineno]}\n")
            if was_merged:
                lf, lh = recompute_totals(da)
            else:
                # Preserve llvm-cov's original totals from the block.
                lf = lh = 0
                for l in block:
                    if l.startswith("LF:"):
                        lf = int(l[3:])
                    elif l.startswith("LH:"):
                        lh = int(l[3:])
                if lf == 0 and da:
                    lf, lh = recompute_totals(da)
            f.write(f"LF:{lf}\n")
            f.write(f"LH:{lh}\n")
            f.write("end_of_record\n")
            total_lf += lf
            total_lh += lh

    pct = total_lh / total_lf * 100 if total_lf else 0.0
    print(f"Merged {len(args.inputs)} lcov files -> {args.output}")
    print(f"  files={len(merged)}, LF={total_lf}, LH={total_lh}, coverage={pct:.2f}%")


if __name__ == "__main__":
    sys.exit(main())
