#!/usr/bin/env python3
"""File-size ratchet for CQ-04 ("超大文件")。

`.clippy.toml` declares `too-many-lines-threshold = 600`, but that threshold is
consumed *only* by the `clippy::too_many_lines` pedantic lint, and this
workspace never enables `clippy::pedantic` — so it currently enforces nothing
(see the NOTE at the top of `.clippy.toml`). This gate makes the same 600-line
intent real, without a wholesale split of the ~250 files already over the line.

Semantics (an "ice ratchet", same one-way spirit as `check_trait_ratchet.py`):

  * threshold = 600 physical lines per `.rs` file (matches `.clippy.toml`);
  * the baseline (`scripts/ci/file_size_baseline`) grandfathers every file
    currently over the threshold at its current size — that is its ceiling;
  * a grandfathered file may NOT grow past its recorded ceiling;
  * a file NOT in the baseline may NOT exceed the threshold (new offender);
  * the ratchet is **one-way**: when a grandfathered file shrinks — or drops to
    the threshold or below, or is deleted — the gate *lowers* (or removes) that
    entry itself instead of only printing "run --update to tighten". A ceiling
    that can only be raised by a separate command is not a ratchet; it is a
    plaque recording the worst day the file ever had.
  * `--update` therefore **refuses to add a new offender** by default: a file
    that is over the threshold and not yet in the baseline must be split, not
    grandfathered. `--allow-new` is the explicit, reviewed escape hatch (and
    prints a warning) for the rare case where splitting is impossible.

`threshold` is deliberately a single constant here (not a tuned number per
file): the goal is one visible decision point — "this file is too big, shrink
it or say why" — not a knob to turn green.

Baseline: scripts/ci/file_size_baseline   (<lines> <path>, one per line)

Usage:
    python3 scripts/ci/check_file_size_ratchet.py           # gate; tightens lowered ceilings in place
    python3 scripts/ci/check_file_size_ratchet.py --update  # rewrite ceilings; refuses new offenders
    python3 scripts/ci/check_file_size_ratchet.py --update --allow-new
                                                            # reviewed escape hatch: grandfather new files too
"""

from __future__ import annotations

import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
BASELINE = ROOT / "scripts" / "ci" / "file_size_baseline"

# Workspace source roots — identical set to check_trait_ratchet.py so the two
# ratchets share one notion of "the code we own".
ROOTS = [
    "synapse-common/src",
    "synapse-cache/src",
    "synapse-storage/src",
    "synapse-e2ee/src",
    "synapse-federation/src",
    "synapse-services/src",
    "synapse-web/src",
    "synapse-test-utils/src",
    "src",
]

# Never walked: build output, stale worktree copies (double every count), the
# vendored `pastey` macro crate, and CI artifact scratch dirs.
EXCLUDED = (
    "/target/",
    "/.claude/",
    "/.worktrees/",
    "/vendor/",
    "/artifacts/",
)

# A physical line over this many is "too big". Mirrors `.clippy.toml`
# `too-many-lines-threshold = 600`.
THRESHOLD = 600


def _is_generated(name: str) -> bool:
    # `.inc.rs` files are machine-emitted ledger tables (e.g.
    # `derived_route_table_always.inc.rs`), regenerated wholesale — they are not
    # hand-maintained, so a hand-split ratchet on them would be meaningless.
    return name.endswith(".inc.rs")


def scan() -> dict[str, int]:
    """Return {relative-path: physical line count} for every scanned .rs file."""
    counts: dict[str, int] = {}
    for root in ROOTS:
        base = ROOT / root
        if not base.is_dir():
            continue
        for dirpath, _dirnames, filenames in os.walk(base):
            # Every `EXCLUDED` marker is **repo-relative** ("/target/",
            # "/.worktrees/", ...), so it must be matched against a repo-relative
            # path. Matching the absolute `dirpath` instead silently breaks the
            # gate in any checkout whose own location happens to contain one of
            # those segments — most notably a git worktree at
            # `<repo>/.worktrees/<name>/`, where **every** file is excluded, the
            # scan comes back empty and the fail-closed branch at the bottom of
            # `main()` exits 2. The practical effect was that `git commit` was
            # impossible inside any isolated worktree, i.e. the only safe way to
            # avoid touching files another session is editing (measured
            # 2026-10-23).
            rel_dir = "/" + (Path(dirpath).relative_to(ROOT).as_posix() + "/")
            if any(marker in rel_dir for marker in EXCLUDED):
                continue
            for name in filenames:
                if not name.endswith(".rs") or _is_generated(name):
                    continue
                path = Path(dirpath) / name
                text = path.read_text(encoding="utf-8", errors="replace")
                counts[path.relative_to(ROOT).as_posix()] = text.count("\n") + (
                    0 if text.endswith("\n") or text == "" else 1
                )
    return counts


def over_threshold(counts: dict[str, int]) -> dict[str, int]:
    return {p: n for p, n in counts.items() if n > THRESHOLD}


def read_baseline() -> dict[str, int]:
    baseline: dict[str, int] = {}
    if not BASELINE.exists():
        return baseline
    for line in BASELINE.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        parts = line.split(None, 1)
        if len(parts) != 2:
            continue
        baseline[parts[1]] = int(parts[0])
    return baseline


def write_baseline(over: dict[str, int]) -> None:
    lines = [
        "# File-size ratchet baseline (CQ-04). See scripts/ci/check_file_size_ratchet.py.",
        "# Format: <max_lines> <path>. A grandfathered file may only shrink;",
        "# the entry must be dropped once its file is gone or <= the threshold.",
        "# Regenerate: python3 scripts/ci/check_file_size_ratchet.py --update",
        f"# Threshold: {THRESHOLD} lines.",
        "",
    ]
    for path in sorted(over, key=lambda p: (-over[p], p)):
        lines.append(f"{over[path]} {path}")
    BASELINE.write_text("\n".join(lines) + "\n", encoding="utf-8")


def new_offenders_of(
    baseline: dict[str, int], over: dict[str, int]
) -> list[tuple[str, int]]:
    """Files over the threshold that the baseline does not know about.

    These are *never* added by `--update` unless `--allow-new` is passed: a file
    that has never been grandfathered has no excuse to start life at 700 lines.
    """
    return sorted((p, over[p]) for p in over if p not in baseline)


def plan_tightening(
    baseline: dict[str, int], over: dict[str, int]
) -> tuple[dict[str, int], list[tuple[str, int, int]], list[str]]:
    """Return `(tightened_baseline, lowered, dropped)` — the one-way half.

    * `lowered`: still over the threshold, but smaller than its ceiling.
    * `dropped`: gone, or now at/under the threshold.
    * New offenders are deliberately absent: they must be split, not blessed.
    """
    lowered = sorted(
        (p, baseline[p], over[p])
        for p in baseline
        if p in over and over[p] < baseline[p]
    )
    dropped = sorted(p for p in baseline if p not in over)
    tightened = {p: min(baseline[p], over[p]) for p in baseline if p in over}
    return tightened, lowered, dropped


def main() -> int:
    counts = scan()
    if not counts:
        print(
            "FAIL: scanned zero .rs files — the scanner is broken (fail-closed)",
            file=sys.stderr,
        )
        return 2

    over = over_threshold(counts)

    if "--update" in sys.argv:
        baseline = read_baseline()
        new = new_offenders_of(baseline, over)
        if new and "--allow-new" not in sys.argv:
            print(
                f"FAIL: refusing to grandfather {len(new)} new file(s) over "
                f"{THRESHOLD} lines. Split them instead; pass --allow-new only for "
                "a reviewed exception:",
                file=sys.stderr,
            )
            for path, n in new:
                print(f"  {n:5d}  {path}", file=sys.stderr)
            return 2
        if new:
            print(
                f"WARNING: --allow-new grandfathers {len(new)} new file(s) that were "
                "never reviewed as acceptable:",
                file=sys.stderr,
            )
            for path, n in new:
                print(f"  {n:5d}  {path}", file=sys.stderr)
        write_baseline(over)
        print(
            f"file_size: baseline updated — {len(over)} file(s) over {THRESHOLD} lines"
        )
        return 0

    baseline = read_baseline()
    if not baseline:
        print(
            f"FAIL: baseline {BASELINE.relative_to(ROOT)} missing/empty — "
            "regenerate with --update (fail-closed)",
            file=sys.stderr,
        )
        return 2

    new_offenders = new_offenders_of(baseline, over)
    grown = sorted(
        (p, baseline[p], over[p])
        for p in over
        if p in baseline and over[p] > baseline[p]
    )
    tightened, lowered, dropped = plan_tightening(baseline, over)

    print(
        f"file_size: {len(counts)} files scanned, {len(over)} over {THRESHOLD} "
        f"(baseline {len(baseline)})"
    )

    failed = False
    if new_offenders:
        failed = True
        print(
            f"FAIL: {len(new_offenders)} file(s) exceed {THRESHOLD} lines and are not "
            "grandfathered:",
            file=sys.stderr,
        )
        for path, n in new_offenders:
            print(f"  {n:5d}  {path}", file=sys.stderr)

    if grown:
        failed = True
        print(
            f"FAIL: {len(grown)} grandfathered file(s) grew past their ceiling:",
            file=sys.stderr,
        )
        for path, was, now in grown:
            print(f"  {path}: {was} -> {now}", file=sys.stderr)

    if failed:
        print(
            "\n  Fix: split the file. `--update` will not grandfather a file that "
            "was never in the baseline; for a genuinely deliberate case pass "
            "`--update --allow-new`.",
            file=sys.stderr,
        )
        return 1

    # No hard violation. The ratchet still has to move: lower every ceiling the
    # file no longer needs, and drop entries whose file is gone. Doing this here
    # (rather than asking a human to run --update) is what makes it one-way.
    if tightened != baseline:
        write_baseline(tightened)
        for path, was, now in lowered:
            print(f"  tightened {path}: {was} -> {now}")
        for path in dropped:
            print(f"  dropped stale entry {path}")
        print(
            f"OK: baseline tightened in place "
            f"({len(lowered)} lowered, {len(dropped)} dropped; ratchet is one-way)"
        )
    else:
        print("OK: file sizes at baseline")
    return 0


if __name__ == "__main__":
    sys.exit(main())
