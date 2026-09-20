#!/usr/bin/env python3
"""Layering ratchet: `synapse-web/src/` must not talk to `synapse_storage` (A2 / B4-4).

The HTTP layer is supposed to reach persistence only through `synapse-services`.
Every direct `synapse_storage` reference under `synapse-web/src/` is a place where a route
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
WEB = ROOT / "synapse-web" / "src"
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


# A working `synapse-web/src/` tree has hundreds of files; requiring a couple of
# dozen is a floor that a rename/move cannot accidentally satisfy.
MIN_SCANNED_FILES = 20


def scan_surface() -> int:
    """Number of `.rs` files the walk will inspect.

    Both branches of this gate are computed from that set, so an empty scan
    (the directory was renamed, deleted, or the walk excluded everything) made
    it print OK and exit 0 — the same "gate is dead but green" failure mode as
    `AGENTS.md` rule 8. Callers must treat a too-small surface as a failure, not
    as "no offenders".
    """
    if not WEB.is_dir():
        return 0
    count = 0
    for dirpath, _dirnames, filenames in os.walk(WEB):
        if is_excluded(dirpath):
            continue
        count += sum(1 for name in filenames if name.endswith(".rs"))
    return count


def main() -> int:
    # The scan-surface guard runs first, including for `--update`: rewriting the
    # allowlist from a broken scan would persist an empty (meaningless) list.
    scanned = scan_surface()
    if scanned < MIN_SCANNED_FILES:
        print(
            f"FAIL: web_layering scanned only {scanned} .rs file(s) under {WEB} "
            f"(expected at least {MIN_SCANNED_FILES}). The scan surface is missing or "
            "misconfigured, so 'no offenders' would be meaningless. Fix the path, or "
            "update MIN_SCANNED_FILES if the tree legitimately shrank.",
            file=sys.stderr,
        )
        return 1

    current = offenders()

    if "--update" in sys.argv:
        lines = [
            "# Files under synapse-web/src/ that still reference synapse_storage (A2 / B4-4).",
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
        print(
            f"\nFAIL: {len(new)} synapse-web/src/ file(s) reference synapse_storage but are not allowlisted:",
            file=sys.stderr,
        )
        for path in new:
            print(f"  {path}", file=sys.stderr)
        print(
            "  Route through synapse-services instead, or (if genuinely unavoidable) add the file",
            file=sys.stderr,
        )
        print("  to the allowlist in the same commit and say why.", file=sys.stderr)
        failed = True
    if stale:
        print(
            f"\nFAIL: {len(stale)} allowlist entr(ies) no longer reference synapse_storage:",
            file=sys.stderr,
        )
        for path in stale:
            print(f"  {path}", file=sys.stderr)
        print(
            "  Remove them: python3 scripts/ci/check_web_layering.py --update",
            file=sys.stderr,
        )
        failed = True
    if not failed:
        print("OK: no new offenders, no stale allowlist entries")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
