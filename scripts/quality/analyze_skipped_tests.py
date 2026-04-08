#!/usr/bin/env python3
"""Analyze api integration skipped cases and classify likely root causes.

Usage:
  python3 scripts/quality/analyze_skipped_tests.py \
      --input test-results/api-integration.skipped.txt
"""

from __future__ import annotations

import argparse
import sys
from collections import Counter
from dataclasses import dataclass
from pathlib import Path
from typing import TextIO


@dataclass
class SkipCase:
    name: str
    reason: str
    category: str


def classify_reason(reason: str, name: str = "") -> str:
    r = reason.strip().lower()
    n = name.strip().lower()
    if not r and "destructive test" in n:
        return "safety_guard"
    if "requires federation signed request" in r:
        return "federation_prerequisite"
    if "federation signing key not configured" in r:
        return "federation_prerequisite"
    if "federation signer binary unavailable" in r:
        return "federation_prerequisite"
    if "destructive test" in r:
        return "safety_guard"
    if "http 500" in r:
        return "backend_error"
    if "not found" in r:
        return "endpoint_or_feature_gap"
    if not r:
        return "unknown"
    return "other"


def parse_cases(path: Path) -> list[SkipCase]:
    cases: list[SkipCase] = []
    if not path.exists():
        return cases
    for line in path.read_text(encoding="utf-8").splitlines():
        raw = line.strip()
        if not raw:
            continue
        parts = raw.split("\t", 1)
        name = parts[0].strip()
        reason = parts[1].strip() if len(parts) > 1 else ""
        cases.append(SkipCase(name=name, reason=reason, category=classify_reason(reason, name)))
    return cases


def emit_report(cases: list[SkipCase], stream: TextIO) -> bool:
    print(f"TOTAL_SKIPPED\t{len(cases)}", file=stream)
    if not cases:
        return False

    by_category = Counter(c.category for c in cases)
    print("CATEGORY_COUNTS", file=stream)
    for category, count in sorted(by_category.items(), key=lambda kv: (-kv[1], kv[0])):
        print(f"{count}\t{category}", file=stream)

    print("BACKEND_GAP_CANDIDATES", file=stream)
    has_candidate = False
    for case in cases:
        if case.category in {"backend_error", "endpoint_or_feature_gap", "unknown", "other"}:
            has_candidate = True
            print(f"- {case.name}\t{case.reason}", file=stream)
    if not has_candidate:
        print("- none", file=stream)
    return has_candidate


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--input",
        default="test-results/api-integration.skipped.txt",
        help="Path to skipped results file.",
    )
    parser.add_argument(
        "--output",
        default="",
        help="Optional output report path.",
    )
    parser.add_argument(
        "--fail-on-backend-gap",
        action="store_true",
        help="Exit non-zero when backend gap candidates are found.",
    )
    args = parser.parse_args()

    cases = parse_cases(Path(args.input))
    has_candidate = emit_report(cases, stream=sys.stdout)

    if args.output:
        out_path = Path(args.output)
        out_path.parent.mkdir(parents=True, exist_ok=True)
        with out_path.open("w", encoding="utf-8") as f:
            emit_report(cases, stream=f)

    if args.fail_on_backend_gap and has_candidate:
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
