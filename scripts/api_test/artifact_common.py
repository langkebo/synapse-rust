#!/usr/bin/env python3
"""Shared helpers for the OpenAPI artifact generators' `--check` modes.

`gen_client_yaml.py` and `gen_route_table.py` both regenerate into a temp path
and compare with the committed artifact. The comparison and its diagnostic live
here so the two gates cannot drift apart (AGENTS.md iron law 2: one
implementation per responsibility).
"""

from __future__ import annotations

import difflib

#: The fixed `generated_at` timestamp shared by every reproducer of the OpenAPI
#: artifacts. Three things must agree on this exact instant, and they used to
#: each carry their own literal (AGENTS.md iron law 2: one implementation per
#: responsibility):
#:
#:   * this constant (the only literal left),
#:   * `.github/workflows/ci.yml`'s `synapse_ledger_export --timestamp=…` export,
#:     which now reads it back with `python3 -c` instead of hard-coding a copy,
#:   * the committed artifacts it feeds — `docs/openapi/route-table.json`
#:     (`generated_at`) and the exported ledger.
#:
#: Drift is caught by the wired `--check` step for the **committed** artifact
#: (`gen_route_table.py --check`, run by CI against `docs/openapi/route-table.json`):
#: change this value without regenerating that file and CI goes red.
#: `gen_client_yaml.py --check` exists too, but it has **no default reference** —
#: its output is gitignored, so `--check` there requires an explicit `--expected`
#: snapshot and nothing in CI wires it. Do not describe it as a gate.
FIXED_TIMESTAMP = "2026-09-16T00:00:00Z"


def describe_drift(expected: str, actual: str, *, limit: int = 40) -> str:
    """A bounded unified diff of the committed vs regenerated artifact.

    Bounded on purpose: the OpenAPI spec is ~1.7 MB, and dumping both files to a
    CI log hides the one line that matters.
    """
    expected_lines = expected.splitlines()
    actual_lines = actual.splitlines()
    diff = list(
        difflib.unified_diff(
            expected_lines,
            actual_lines,
            fromfile="committed",
            tofile="regenerated",
            lineterm="",
            n=2,
        )
    )
    shown = "\n".join(diff[:limit])
    omitted = (
        "" if len(diff) <= limit else f"\n    ... {len(diff) - limit} more diff line(s)"
    )
    return (
        f"    committed: {len(expected_lines)} lines, regenerated: {len(actual_lines)} lines\n"
        f"{shown}{omitted}"
    )
