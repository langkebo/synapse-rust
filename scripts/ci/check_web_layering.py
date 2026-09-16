#!/usr/bin/env python3
"""Layering ratchet: `src/web/` must not talk to `synapse_storage` (A2 / B4-4).

The HTTP layer is supposed to reach persistence only through `synapse-services`.
Every direct `synapse_storage` reference under `src/web/` is a place where a route
bypasses the service layer, so business rules drift into handlers and the storage
crate's surface becomes a de-facto public API.

The gate is an allowlist, not a bare count:

* a file **outside** the allowlist that references `synapse_storage`  -> FAIL
  (a new offender cannot hide behind a constant total)
* an allowlist entry that **no longer** references it                 -> FAIL
  (the list can only shrink, so nobody has to remember to prune it)

Comments are stripped before matching, so documentation that merely mentions the
crate does not count as a violation.

Usage:
    python3 scripts/ci/check_web_layering.py            # gate
    python3 scripts/ci/check_web_layering.py --update   # rewrite the allowlist
"""

from __future__ import annotations

import os
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
WEB = ROOT / "src" / "web"
ALLOWLIST = ROOT / "scripts" / "ci" / "web_layering_allowlist.txt"

COMMENT_BLOCK = re.compile(r"/\*.*?\*/", re.S)
COMMENT_LINE = re.compile(r"//[^\n]*")
REFERENCE = re.compile(r"\bsynapse_storage\b")


def is_excluded(dirpath: str) -> bool:
    return any(marker in dirpath for marker in ("/target/", "/.claude/"))


def offenders() -> set[str]:
    found: set[str] = set()
    for dirpath, _dirnames, filenames in os.walk(WEB):
        if is_excluded(dirpath):
            continue
        for name in filenames:
            if not name.endswith(".rs"):
                continue
            path = Path(dirpath) / name
            text = path.read_text(encoding="utf-8", errors="replace")
            text = COMMENT_LINE.sub("", COMMENT_BLOCK.sub("", text))
            if REFERENCE.search(text):
                found.add(str(path.relative_to(ROOT)))
    return found


def read_allowlist() -> set[str]:
    if not ALLOWLIST.exists():
        return set()
    entries = set()
    for line in ALLOWLIST.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if line and not line.startswith("#"):
            entries.add(line)
    return entries


def main() -> int:
    current = offenders()

    if "--update" in sys.argv:
        lines = [
            "# Files under src/web/ that still reference synapse_storage (A2 / B4-4).",
            "# This list may only SHRINK. The gate fails on a new offender AND on a stale entry,",
            "# so removing a line here is part of fixing the file, not optional bookkeeping.",
            "# Regenerate with: python3 scripts/ci/check_web_layering.py --update",
            "",
            *sorted(current),
            "",
        ]
        ALLOWLIST.write_text("\n".join(lines), encoding="utf-8")
        print(f"web_layering: allowlist rewritten with {len(current)} entries")
        return 0

    allowed = read_allowlist()
    new = sorted(current - allowed)
    stale = sorted(allowed - current)

    print(f"web_layering: {len(current)} offending file(s), allowlist {len(allowed)}")

    failed = False
    if new:
        print(f"\nFAIL: {len(new)} src/web/ file(s) reference synapse_storage but are not allowlisted:", file=sys.stderr)
        for path in new:
            print(f"  {path}", file=sys.stderr)
        print("  Route through synapse-services instead, or (if genuinely unavoidable) add the file", file=sys.stderr)
        print("  to the allowlist in the same commit and say why.", file=sys.stderr)
        failed = True
    if stale:
        print(f"\nFAIL: {len(stale)} allowlist entr(ies) no longer reference synapse_storage:", file=sys.stderr)
        for path in stale:
            print(f"  {path}", file=sys.stderr)
        print("  Remove them: python3 scripts/ci/check_web_layering.py --update", file=sys.stderr)
        failed = True
    if not failed:
        print("OK: no new offenders, no stale allowlist entries")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
