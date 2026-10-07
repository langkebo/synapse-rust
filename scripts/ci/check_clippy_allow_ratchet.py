#!/usr/bin/env python3
"""Clippy `allow` hygiene ratchet for CQ-01 ("allow 逸散 + 无理由 allow")。

The audit found ~250 `#[allow(…)]` / `#![allow(…)]` sites across the workspace
with no notion of *why* any of them exists.  Suppression is a debt that
compounds: a lint silenced today to get a build green stays silenced after the
reason has rotted away.

This gate makes the suppression surface **explicit and one-way**, without a
wholesale rewrite of the sites that predate it.  Two mechanisms:

  1. **Lint allowlist** (`scripts/ci/clippy_allow_allowlist`)
     A lint kind may only be suppressed at all if it is listed there with a
     written justification.  A `clippy::*` allow whose lint is not listed fails
     the gate, so a brand-new class of suppression is a deliberate, reviewed
     decision rather than an incidental one.  The list is the "allow 白名单".

  2. **Reason-required ratchet** (`scripts/ci/clippy_allow_baseline`)
     An allow site is *justified* when it carries a local reason — either an
     inline `reason = "…"` (Rust lint-reason) or a `//` comment on the same line
     or the line directly above.  The baseline grandfathers, per file, the
     number of clippy allows that are NOT justified at their current site; that
     count may only go DOWN.  New reasonless allows fail.

Blanket group allows (`clippy::all` / `pedantic` / `nursery` / `restriction` /
`cargo` / `correctness` / `suspicious` / `style` / `complexity` / `perf`) can
never be justified and always fail — the audit confirmed none exist today, and
this keeps it that way.

Scope is the same nine production source roots as `check_trait_ratchet.py` /
`check_file_size_ratchet.py`: test-only fixtures under `tests/` are not
production abstraction and are deliberately not scanned.

Baselines:
    scripts/ci/clippy_allow_allowlist   <clippy::lint>  # reason
    scripts/ci/clippy_allow_baseline    <count> <path>   (unjustified clippy allows)

Usage:
    python3 scripts/ci/check_clippy_allow_ratchet.py           # gate
    python3 scripts/ci/check_clippy_allow_ratchet.py --update  # rewrite the count baseline
"""

from __future__ import annotations

import os
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
ALLOWLIST = ROOT / "scripts" / "ci" / "clippy_allow_allowlist"
BASELINE = ROOT / "scripts" / "ci" / "clippy_allow_baseline"

# Workspace source roots — identical set to check_trait_ratchet.py so every
# ratchet shares one notion of "the code we own".  `tests/` and `benches/` are
# deliberately outside: their `unwrap_used` / `panic` blanket allows are
# fixtures, not production suppression.
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

# Never walked: build output, stale worktree copies, the vendored `pastey`
# macro crate, and CI artifact scratch dirs.
EXCLUDED = (
    "/target/",
    "/.claude/",
    "/.worktrees/",
    "/vendor/",
    "/artifacts/",
)

# `#[allow(` / `#![allow(`.  The paren balance is resolved by hand below so a
# multi-line attribute or a `reason = "…(…)"` string does not truncate it.
ALLOW_OPEN = re.compile(r"#!?\[\s*allow\s*\(")

# A group lint silences every lint in the group; it can never carry a real
# reason and is banned outright.
BLANKET = frozenset(
    {
        "clippy::all",
        "clippy::pedantic",
        "clippy::nursery",
        "clippy::restriction",
        "clippy::cargo",
        "clippy::correctness",
        "clippy::suspicious",
        "clippy::style",
        "clippy::complexity",
        "clippy::perf",
    }
)


def _split_top_level(text: str) -> list[str]:
    """Split `inner` on commas that are not nested in `()` or a string."""
    parts: list[str] = []
    current = ""
    depth = 0
    in_str = False
    escaped = False
    for ch in text:
        if in_str:
            current += ch
            if escaped:
                escaped = False
            elif ch == "\\":
                escaped = True
            elif ch == '"':
                in_str = False
            continue
        if ch == '"':
            in_str = True
            current += ch
        elif ch == "(":
            depth += 1
            current += ch
        elif ch == ")":
            depth -= 1
            current += ch
        elif ch == "," and depth == 0:
            parts.append(current)
            current = ""
        else:
            current += ch
    if current.strip():
        parts.append(current)
    return parts


def _parse_attribute(inner: str) -> tuple[list[str], bool]:
    """Return (clippy lint kinds, has_inline_reason) for one attribute body."""
    lints: list[str] = []
    has_reason = False
    for part in _split_top_level(inner):
        token = part.strip()
        if not token:
            continue
        if token.startswith("reason") and "=" in token:
            has_reason = True
            continue
        if token.startswith("clippy::"):
            lints.append(token)
    return lints, has_reason


class Site:
    __slots__ = ("path", "line", "lints", "justified")

    def __init__(self, path: str, line: int, lints: list[str], justified: bool) -> None:
        self.path = path
        self.line = line
        self.lints = lints
        self.justified = justified


def _scan_file(path: Path, relpath: str) -> list[Site]:
    text = path.read_text(encoding="utf-8", errors="replace")
    lines = text.splitlines()
    sites: list[Site] = []
    for match in ALLOW_OPEN.finditer(text):
        line_start = text.rfind("\n", 0, match.start()) + 1
        # Skip doc-comment / line-comment mentions such as `/// `#[allow(x)]`` —
        # an example in prose is not a suppression.
        if "//" in text[line_start : match.start()]:
            continue

        # Resolve the matching close paren by hand (nesting + string aware).
        i = match.end()
        depth = 1
        in_str = False
        escaped = False
        while i < len(text) and depth > 0:
            ch = text[i]
            if in_str:
                if escaped:
                    escaped = False
                elif ch == "\\":
                    escaped = True
                elif ch == '"':
                    in_str = False
            elif ch == '"':
                in_str = True
            elif ch == "(":
                depth += 1
            elif ch == ")":
                depth -= 1
            i += 1
        inner = text[match.end() : i - 1]

        lints, inline_reason = _parse_attribute(inner)
        if not lints:
            continue  # pure rustc-lint allow (dead_code, missing_docs, …): out of scope

        line_no = text.count("\n", 0, match.start()) + 1
        close_line_no = text.count("\n", 0, i) + 1

        justified = inline_reason
        if not justified:
            # Trailing `// …` after the attribute on its closing line.
            close_line_end = text.find("\n", i)
            if close_line_end == -1:
                close_line_end = len(text)
            tail = text[i:close_line_end]
            if "//" in tail:
                justified = True
        if not justified and line_no >= 2:
            # A `//` comment on the line directly above (but not a `///`/`//!`
            # doc comment, which documents the item, not the suppression).
            prev = lines[line_no - 2].strip()
            if (
                prev.startswith("//")
                and not prev.startswith("///")
                and not prev.startswith("//!")
            ):
                justified = True

        sites.append(Site(relpath, line_no, lints, justified))
    return sites


def scan() -> tuple[list[Site], int]:
    sites: list[Site] = []
    files = 0
    for root in ROOTS:
        base = ROOT / root
        if not base.is_dir():
            continue
        for dirpath, _dirnames, filenames in os.walk(base):
            if any(marker in dirpath + "/" for marker in EXCLUDED):
                continue
            for name in filenames:
                if not name.endswith(".rs"):
                    continue
                files += 1
                sites.extend(
                    _scan_file(
                        Path(dirpath) / name,
                        Path(dirpath, name).relative_to(ROOT).as_posix(),
                    )
                )
    return sites, files


def unjustified_counts(sites: list[Site]) -> dict[str, int]:
    counts: dict[str, int] = {}
    for site in sites:
        if site.justified:
            continue
        counts[site.path] = counts.get(site.path, 0) + len(site.lints)
    return counts


def read_allowlist() -> tuple[set[str], list[str]]:
    """Return (allowed lint kinds, entries missing a written reason)."""
    allowed: set[str] = set()
    missing: list[str] = []
    if not ALLOWLIST.exists():
        return allowed, missing
    for raw in ALLOWLIST.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        lint, sep, reason = line.partition("#")
        lint = lint.strip()
        if not lint:
            continue
        allowed.add(lint)
        if not sep or not reason.strip():
            missing.append(lint)
    return allowed, missing


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


def write_baseline(counts: dict[str, int]) -> None:
    lines = [
        "# Clippy-allow ratchet baseline (CQ-01). See scripts/ci/check_clippy_allow_ratchet.py.",
        "# Format: <count> <path>, where <count> is the number of clippy allow",
        "# occurrences WITHOUT a local reason. The count may only go DOWN;",
        "# the entry must be dropped once the file reaches zero or is gone.",
        "# Regenerate: python3 scripts/ci/check_clippy_allow_ratchet.py --update",
        "",
    ]
    for path in sorted(counts, key=lambda p: (-counts[p], p)):
        lines.append(f"{counts[path]} {path}")
    BASELINE.write_text("\n".join(lines) + "\n", encoding="utf-8")


def main() -> int:
    sites, files = scan()
    if files == 0 or not sites:
        print(
            "FAIL: scanned zero clippy allows — the scanner is broken (fail-closed)",
            file=sys.stderr,
        )
        return 2

    counts = unjustified_counts(sites)
    all_lints = sorted({lint for site in sites for lint in site.lints})

    if "--update" in sys.argv:
        write_baseline(counts)
        print(
            f"clippy_allow: baseline updated — {sum(counts.values())} unjustified "
            f"allow(s) across {len(counts)} file(s)"
        )
        print(
            "clippy_allow: lint kinds seen (keep scripts/ci/clippy_allow_allowlist in sync):"
        )
        for lint in all_lints:
            print(f"  {lint}")
        return 0

    allowlist, allowlist_missing_reason = read_allowlist()
    if not allowlist:
        print(
            f"FAIL: allowlist {ALLOWLIST.relative_to(ROOT)} missing/empty (fail-closed)",
            file=sys.stderr,
        )
        return 2

    baseline = read_baseline()
    if not baseline:
        print(
            f"FAIL: baseline {BASELINE.relative_to(ROOT)} missing/empty — "
            "regenerate with --update (fail-closed)",
            file=sys.stderr,
        )
        return 2

    blanket = sorted({lint for site in sites for lint in site.lints if lint in BLANKET})
    unlisted = sorted({lint for lint in all_lints if lint not in allowlist})

    new_offenders = sorted((p, counts[p]) for p in counts if p not in baseline)
    grown = sorted(
        (p, baseline[p], counts[p])
        for p in counts
        if p in baseline and counts[p] > baseline[p]
    )
    stale = sorted(p for p in baseline if counts.get(p, 0) < baseline[p])

    print(
        f"clippy_allow: {len(sites)} site(s) in {files} file(s); "
        f"{sum(counts.values())} unjustified in {len(counts)} file(s) "
        f"(baseline {sum(baseline.values())} in {len(baseline)} file(s))"
    )

    failed = False

    if blanket:
        failed = True
        print(
            "FAIL: blanket clippy group allow(s) — they silence every lint in the "
            "group and can never be justified:",
            file=sys.stderr,
        )
        for lint in blanket:
            print(f"  {lint}", file=sys.stderr)

    if allowlist_missing_reason:
        failed = True
        print(
            f"FAIL: {len(allowlist_missing_reason)} allowlist entr(y|ies) lack a written "
            "reason — every entry is `<lint>  # why it may be suppressed`:",
            file=sys.stderr,
        )
        for lint in allowlist_missing_reason:
            print(f"  {lint}", file=sys.stderr)

    if unlisted:
        failed = True
        print(
            f"FAIL: {len(unlisted)} clippy lint kind(s) suppressed but not on the "
            f"allowlist {ALLOWLIST.relative_to(ROOT)} — add each with a reason, or "
            "remove the suppression:",
            file=sys.stderr,
        )
        for lint in unlisted:
            print(f"  {lint}", file=sys.stderr)

    if new_offenders:
        failed = True
        print(
            f"FAIL: {len(new_offenders)} file(s) have unjustified clippy allow(s) not in "
            'the baseline — add `reason = "…"` (or an adjacent `// why` comment):',
            file=sys.stderr,
        )
        for path, n in new_offenders:
            print(f"  {n:5d}  {path}", file=sys.stderr)

    if grown:
        failed = True
        print(
            f"FAIL: {len(grown)} file(s) added unjustified clippy allow(s) past their "
            "ceiling:",
            file=sys.stderr,
        )
        for path, was, now in grown:
            print(f"  {path}: {was} -> {now}", file=sys.stderr)

    if stale:
        failed = True
        print(
            f"FAIL: {len(stale)} stale baseline entr(y|ies) — the file is gone or its "
            "unjustified count dropped. Tighten the ratchet:",
            file=sys.stderr,
        )
        for path in stale:
            print(f"  {path}", file=sys.stderr)

    if failed:
        print(
            "\n  Fix: justify (or delete) the suppression, then "
            "`python3 scripts/ci/check_clippy_allow_ratchet.py --update`.",
            file=sys.stderr,
        )
        return 1

    if sum(counts.values()) < sum(baseline.values()):
        print(
            "OK: below baseline — run --update to tighten: "
            "python3 scripts/ci/check_clippy_allow_ratchet.py --update"
        )
    else:
        print("OK: clippy allow suppression at baseline")
    return 0


if __name__ == "__main__":
    sys.exit(main())
