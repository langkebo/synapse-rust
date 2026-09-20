#!/usr/bin/env python3
"""Reject axum 0.7-style path literals (``:param`` / ``*wildcard``) in route files.

Why this gate exists
--------------------
The workspace uses **axum 0.8**, which requires ``{param}`` / ``{*wildcard}``.
``Router::route`` calls ``panic_on_err!(path_router.route(..))``, and
``PathRouter::route`` runs ``validate_v07_paths`` first, so a ``:param`` segment
does **not** degrade gracefully — it panics while the router is being built:

    Path segments must not start with ':'. For capture groups, use `{capture}`.

Because ``create_router`` is the single assembly point, that panic takes the
whole server down at startup. It is also invisible to ``cargo check`` (the
literal is just a ``&str``) and to unit tests that never build a router.

Real incident (HEAD ``60ea9fac``)
---------------------------------
``synapse-web/src/routes/assembly.rs`` registered
``/auth/:auth_type/fallback/web``. The panic went unnoticed because the
integration suite was simultaneously vacuous (the schema-clone helper swallowed
its error and every test early-returned), so nothing exercised ``create_router``.
The broken literal was even baked into the SDK contract: the ledger fixtures
recorded ``path_params: []`` for a path that has one.

What it checks
--------------
Every string literal passed as the *path* argument of ``.route(..)``,
``.route_service(..)`` and ``.nest(..)`` under ``synapse-web/src``. Only literal
arguments are inspected (a computed path cannot be checked statically, and
grepping for one is what ``extract_registered.py`` already ratchets).

Note on ``.without_v07_checks()``: it only silences the check for the router it
is called on, and it is NOT a licence for ``:param`` — matchit 0.8 treats such a
segment as a *literal*, so the route silently never matches a real request. The
top-level ``create_router`` calls it for other historical reasons; because a
nested router (like ``create_auth_compat_router``) does not inherit it, a
``:param`` there panics. Rejecting the syntax outright is both stricter and
simpler than modelling which router inherits the flag.

Exit codes: 0 = clean, 1 = violations found.
"""

from __future__ import annotations

import argparse
import glob
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
SCAN_ROOT = os.path.join(ROOT, "synapse-web", "src")

# `.route("...")`, `.route_service("...")`, `.nest("...")` — the literal may sit
# on the same line or on the next one, so whitespace (including newlines) is
# allowed between the opening paren and the literal.
RE_PATH_CALL = re.compile(r'\.(?:route|route_service|nest)\(\s*"([^"]*)"', re.S)


def scan(root: str) -> list[tuple[str, str]]:
    """Return ``(relative_path, path_literal)`` for every offending literal."""
    violations: list[tuple[str, str]] = []
    for file_path in sorted(
        glob.glob(os.path.join(root, "**", "*.rs"), recursive=True)
    ):
        with open(file_path, encoding="utf-8") as fh:
            source = fh.read()
        for literal in RE_PATH_CALL.findall(source):
            if not literal.startswith("/"):
                # Nested-service paths such as `Router::nest` on an already
                # absolute prefix are still absolute; anything else is not a
                # route path we can reason about here.
                continue
            if any(seg.startswith((":", "*")) for seg in literal.split("/")):
                violations.append((os.path.relpath(file_path, ROOT), literal))
    return violations


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument(
        "--root", default=SCAN_ROOT, help="directory to scan (default: synapse-web/src)"
    )
    args = ap.parse_args()

    if not os.path.isdir(args.root):
        print(f"::error::scan root does not exist: {args.root}", file=sys.stderr)
        return 2

    violations = scan(args.root)
    if not violations:
        print("axum path syntax: OK (no `:param` / `*wildcard` route literals)")
        return 0

    print(
        "::error::axum 0.7-style path syntax found — axum 0.8 panics on these at router build time",
        file=sys.stderr,
    )
    for rel, literal in violations:
        print(f"  {rel}: {literal}", file=sys.stderr)
    print("", file=sys.stderr)
    print(
        "  Use `{param}` / `{*wildcard}` instead. `:param` is a LITERAL segment in matchit 0.8,",
        file=sys.stderr,
    )
    print(
        "  so even under `.without_v07_checks()` the route never matches a real request.",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    sys.exit(main())
