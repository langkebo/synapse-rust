#!/usr/bin/env python3
import argparse
import os
import re
import sys
from pathlib import Path


DEFAULT_GLOB = "docs/db/DIAGNOSIS_EXTERNAL_EVIDENCE_*.md"
EXCLUDE_FILES = {"docs/db/DIAGNOSIS_EXTERNAL_EVIDENCE_TEMPLATE.md"}
FORBIDDEN_MARKERS = [
    "待补充",
    "待签字",
    "待审批",
    "待评审",
    "占位",
    "示例",
]


def load_text(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def has_http_link(text: str) -> bool:
    return bool(re.search(r"https?://\\S+", text))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--glob", default=os.getenv("EXTERNAL_EVIDENCE_GLOB", DEFAULT_GLOB))
    args = parser.parse_args()

    root = Path(__file__).resolve().parents[1]
    matches: list[Path] = sorted(root.glob(args.glob))
    matches = [p for p in matches if p.relative_to(root).as_posix() not in EXCLUDE_FILES]

    if not matches:
        print(
            f"No external evidence file found. Expected at least one file matching: {args.glob}",
            file=sys.stderr,
        )
        return 1

    failed = False
    for path in matches:
        rel = path.relative_to(root).as_posix()
        text = load_text(path)

        markers = [m for m in FORBIDDEN_MARKERS if m in text]
        if markers:
            failed = True
            print(f"{rel}: contains forbidden placeholders: {', '.join(markers)}", file=sys.stderr)

        if not has_http_link(text):
            failed = True
            print(f"{rel}: must contain at least one http(s) link to evidence", file=sys.stderr)

    if failed:
        return 1

    print("External evidence gate passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
