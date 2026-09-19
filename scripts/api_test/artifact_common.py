#!/usr/bin/env python3
"""Shared helpers for the OpenAPI artifact generators' `--check` modes.

`gen_client_yaml.py` and `gen_route_table.py` both regenerate into a temp path
and compare with the committed artifact. The comparison and its diagnostic live
here so the two gates cannot drift apart (AGENTS.md iron law 2: one
implementation per responsibility).
"""
from __future__ import annotations

import difflib


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
    omitted = "" if len(diff) <= limit else f"\n    ... {len(diff) - limit} more diff line(s)"
    return (
        f"    committed: {len(expected_lines)} lines, regenerated: {len(actual_lines)} lines\n"
        f"{shown}{omitted}"
    )
