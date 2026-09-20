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
#: Drift is still caught by the wired `--check` steps (`gen_client_yaml.py
#: --check`, `gen_route_table.py --check`): change this value without
#: regenerating the committed artifact and CI goes red.
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
