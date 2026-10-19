#!/usr/bin/env python3
"""Generate `docs/synapse-rust/ROUTE_HANDLERS.json` from the route extractor.

The contract surface had no machine-readable answer to "which Rust function
serves this `(method, path)`, and what kind of JSON body does it take?". The
extractor already resolves that internally (`Resolver.resolve_handler_def`,
`Resolver.handler_body_kind`), but never exported it, so every downstream
consumer re-implemented its own heuristics. This sidecar publishes the
resolved answer once, as a deterministic artifact.

Row model (per route, sorted by `(path, method)`):

* `status`
  * `exact`      — exactly one handler definition resolved; `file`/`handler`
                   name it and `body` classifies its JSON body.
  * `unresolved` — the route is registered but its handler definition is not
                   visible to the static parser (dynamic fn value, macro,
                   definition outside the scanned tree). `candidates` lists the
                   raw `(file, handler)` registration sites, if any.
  * `ambiguous`  — more than one distinct definition matches; `candidates`
                   lists all of them. Never guessed down to one.
* `body`
  * `typed`    — `Json<T>` / `MatrixJson<T>` with a resolvable `T` (`body_type`).
  * `opaque`   — bare `Json<Value>` / `MatrixJson<Value>`: protocol-arbitrary.
  * `foreign`  — `Json<T>` whose `T` is defined outside the scanned modules
                 (`body_type` still carries the name).
  * `none`     — no JSON body extractor.
  * `ambiguous`— several signatures / no single definition.
* `body_opaque` — the ledger declaration from `ledger_annotations.txt`. This is
  author intent, not inference; `body == "opaque"` without the flag is the
  counted debt pinned by `EXPECTED_OPAQUE_UNANNOTATED`.

Determinism: routes are sorted, keys have a fixed order, there is no timestamp
and no locale-dependent formatting. The explicit `schema_version` lets a
consumer pin the shape instead of sniffing fields.

Usage:
    python3 scripts/contract/gen_route_handlers.py
    python3 scripts/contract/gen_route_handlers.py --check   # drift gate (CI)

`--check` renders into memory and compares; it never writes, so it cannot
silently repair a drifted tree.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
import sys
from collections import Counter

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(SCRIPT_DIR))
_spec = importlib.util.spec_from_file_location(
    "extract_registered", os.path.join(SCRIPT_DIR, "extract_registered.py")
)
ex = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(ex)

SCHEMA_VERSION = 1
OUT = os.path.join(ROOT, "docs", "synapse-rust", "ROUTE_HANDLERS.json")


def _defs_named(res, name: str) -> list:
    """Every `(file, fn)` definition of `name`, sorted for determinism."""
    return sorted({(f, n) for (f, n, _body) in res.fn_all if n == name})


def resolve_route(res, method: str, path: str) -> dict:
    """Classify one route's handler. See the module docstring for the model."""
    entries = sorted(res.handlers.get((method, path), set()))
    resolved: set = set()
    raw_candidates: set = set()
    for owner, handler in entries:
        idx = res.resolve_handler_def(handler, owner)
        if idx is None:
            raw_candidates.add((owner, handler))
            raw_candidates.update(_defs_named(res, handler))
        else:
            file, fname, _body = res.fn_all[idx]
            resolved.add((file, fname))

    if len(resolved) == 1:
        status = "exact"
        file, handler = next(iter(resolved))
        candidates = []
    elif len(resolved) > 1:
        status = "ambiguous"
        file = handler = None
        candidates = sorted(resolved)
    else:
        file = handler = None
        candidates = sorted(raw_candidates)
        # A name with several surviving definitions is genuinely ambiguous even
        # when none of them resolves through the import graph.
        names = {fn for (_f, fn) in entries}
        ambiguous = any(len(_defs_named(res, n)) > 1 for n in names)
        status = "ambiguous" if ambiguous else "unresolved"

    body, body_type = (
        res.handler_body_kind(method, path)
        if status == "exact"
        else ("ambiguous", None)
    )
    return {
        "status": status,
        "file": file,
        "handler": handler,
        "candidates": [{"file": f, "handler": h} for f, h in candidates],
        "body": body,
        "body_type": body_type,
    }


def build() -> dict:
    res = ex.Resolver(ex.load_sources())
    routes = res.all_routes()  # populates `handlers`
    annotations = ex.load_ledger_annotations()

    rows = []
    for method, path in sorted(routes, key=lambda k: (k[1], k[0])):
        row = {"method": method, "path": path}
        row.update(resolve_route(res, method, path))
        row["body_opaque"] = bool(
            annotations.get((method, path), {}).get("body_opaque")
        )
        rows.append(row)

    status_counts = Counter(r["status"] for r in rows)
    body_counts = Counter(r["body"] for r in rows)
    return {
        "schema_version": SCHEMA_VERSION,
        "generator": "scripts/contract/gen_route_handlers.py",
        "description": (
            "Resolved handler definition and JSON-body kind per registered route. "
            "Generated from the real `.route(...)` surface; do not edit by hand."
        ),
        "counts": {
            "routes": len(rows),
            "status": dict(sorted(status_counts.items())),
            "body": dict(sorted(body_counts.items())),
            "body_opaque_declared": sum(1 for r in rows if r["body_opaque"]),
        },
        "routes": rows,
    }


def render(doc: dict) -> str:
    """One route per line: readable diffs without a 1 MB indent-2 blowup."""
    lines = [
        "{",
        f'  "schema_version": {doc["schema_version"]},',
        f'  "generator": {json.dumps(doc["generator"], ensure_ascii=False)},',
        f'  "description": {json.dumps(doc["description"], ensure_ascii=False)},',
        '  "counts": '
        + json.dumps(doc["counts"], ensure_ascii=False, sort_keys=True)
        + ",",
        '  "routes": [',
    ]
    body = [json.dumps(r, ensure_ascii=False) for r in doc["routes"]]
    lines.append(",\n".join("    " + b for b in body))
    lines.append("  ]")
    lines.append("}")
    return "\n".join(lines) + "\n"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument(
        "--check",
        action="store_true",
        help="fail (exit 1) if the committed file differs",
    )
    args = ap.parse_args()

    want = render(build())
    if args.check:
        try:
            with open(OUT, encoding="utf-8") as fh:
                have = fh.read()
        except OSError:
            have = None
        if have != want:
            print("gen_route_handlers: DRIFT — regenerate and commit:")
            print(f"  - {os.path.relpath(OUT, ROOT)}")
            print("  Run: python3 scripts/contract/gen_route_handlers.py")
            return 1
        print("gen_route_handlers: --check OK, sidecar matches the extractor.")
        return 0

    with open(OUT, "w", encoding="utf-8") as fh:
        fh.write(want)
    print(f"gen_route_handlers: wrote {os.path.relpath(OUT, ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
