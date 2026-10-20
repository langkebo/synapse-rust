#!/usr/bin/env python3
"""Generate `docs/synapse-rust/ROUTE_HANDLERS.json` from the route extractor.

The contract surface had no machine-readable answer to "which Rust function
serves this `(method, path)`, what kind of JSON body does it take, and which
query parameters does it deserialize?". The extractor already resolves that
internally (`Resolver.resolve_handler_def`, `Resolver.handler_body_kind`,
`Resolver.handler_query_info`), but never exported it, so every downstream
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
* `registered_by` — the ledger label from `ledger_origins.txt` (the same value
  the SDK contract-sync uses to pick a directory). `null` when undecidable.
* `confidence` / `hint`
  * `high` — `status == "exact"`.
  * `low`  — `ambiguous`, or `unresolved` with a `hint`: a candidate registered
             under a fully-qualified name (`assembly.rs` registers
             `handlers::foo`) whose *tail* resolves to exactly one definition.
             The hint is a lead for a consumer, never a replacement for
             `status == "exact"`.
  * `none` — `unresolved` with no lead.
* `body`
  * `typed`    — `Json<T>` / `MatrixJson<T>` with a resolvable `T` (`body_type`).
  * `opaque`   — bare `Json<Value>` / `MatrixJson<Value>`, or a raw `Bytes`
                 extractor the handler parses itself
                 (`serde_json::from_slice`/`from_str`): protocol-arbitrary.
  * `foreign`  — `Json<T>` whose `T` is defined outside the scanned modules
                 (`body_type` still carries the name).
  * `none`     — no JSON body extractor (a `Bytes` handler that never parses
                 JSON is a binary body, not a JSON one).
  * `ambiguous`— several signatures / no single definition.
* `body_opaque` — the ledger declaration from `ledger_annotations.txt`. This is
  author intent, not inference; `body == "opaque"` without the flag is the
  counted debt pinned by `EXPECTED_OPAQUE_UNANNOTATED`.
* `query` / `query_params` / `query_type` — the same classification for the
  `Query<..>` *extractor* instead of the body:
  * `typed`    — one struct `Query<T>`; `query_params` is `T`'s sorted
                 accepted-wire-name list (`[]` for a fieldless struct).
  * `literal`  — a dynamic extractor (`Query<Vec<(String, String)>>` etc.)
                 whose key set is nonetheless provable: the handler, or a
                 same-file helper it passes the query to, compares string
                 literals against the query's key. `query_params` is that
                 literal set. It is evidence, not a struct — an unannotated
                 literal or an annotation with no literal behind it is RED.
  * `opaque`   — `Query<Value>` / `Query<HashMap<..>>`: the key set is decided
                 at runtime and no key literal is provable in the handler, so
                 there is nothing to enumerate. Counted debt: it must be
                 declared in `ledger_annotations.txt` rather than guessed.
  * `foreign`  — a named `Query<T>` defined outside the scanned route tree
                 (usually another member crate); `query_type` names it.
  * `none`     — no `Query<..>` extractor: the route takes no query params.
  * `ambiguous`— unresolvable handler, several disagreeing handlers, several
                 `Query<..>` extractors, or competing struct definitions.
                 Rows whose handler is not `exact` are also `ambiguous` here,
                 mirroring `body`. `query_params` is `null` unless `typed` or
                 `literal`.
* `unsupported` — the ledger declaration from `ledger_annotations.txt`: the
  route is reachable but its documented response is not implemented (today the
  vendor keys endpoint answers 501 `M_UNKNOWN` instead of a placeholder
  `keys: []`).
* `dynamic` — the ledger declaration from `ledger_annotations.txt`: the route
  exists but no single handler definition can be named statically *by
  construction* (fallback chain). The declared set and the `confidence: none`
  set are required to be identical in both directions, so a new blind spot and
  a stale exemption are both RED — the "static blind spot" count can no longer
  drift unnoticed.

`file`/`handler`/`candidates[].file`/`hint.file` are relative to `file_root`
(`synapse-web/src/routes`) so the sidecar stays readable without repeating a
long prefix on every row.

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
import re
import sys
from collections import Counter

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(SCRIPT_DIR))
_spec = importlib.util.spec_from_file_location(
    "extract_registered", os.path.join(SCRIPT_DIR, "extract_registered.py")
)
ex = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(ex)

SCHEMA_VERSION = 3
OUT = os.path.join(ROOT, "docs", "synapse-rust", "ROUTE_HANDLERS.json")
# Every `file` in this sidecar is relative to this root.
FILE_ROOT = "synapse-web/src/routes"
ROUTES_DIR = os.path.join(ROOT, FILE_ROOT)
DERIVED_TABLES = {
    "always": "derived_route_table_always.inc.rs",
    "worker": "derived_route_table_worker.inc.rs",
    "oidc": "derived_route_table_oidc.inc.rs",
}
_DERIVED_ROW_RE = re.compile(r"\bRouteEntry::new\s*\(")


def _defs_named(res, name: str) -> list:
    """Every `(file, fn)` definition of `name`, sorted for determinism."""
    return sorted({(f, n) for (f, n, _body) in res.fn_all if n == name})


def _hint_for(res, candidates) -> dict | None:
    """Best-effort lead for an `unresolved` row, or `None`.

    `assembly.rs` registers handlers under fully-qualified paths
    (`handlers::get_well_known_client`), which the import graph cannot resolve.
    The tail after the last `::` often names exactly one definition; when every
    candidate agrees on that single definition it is safe to publish as a hint.
    """
    hits = set()
    for _file, qualified in candidates:
        tail = qualified.rsplit("::", 1)[-1]
        matches = _defs_named(res, tail)
        if len(matches) == 1:
            hits.add(matches[0])
    if len(hits) != 1:
        return None
    file, handler = next(iter(hits))
    return {"file": file, "handler": handler}


_QUERY_KIND = {
    "ok": "typed",
    "none": "none",
    "manual": "opaque",
    "literal": "literal",
    "foreign": "foreign",
    "ambiguous": "ambiguous",
}


def _query_contract(res, method: str, path: str, exact: bool):
    """`(query, query_params, query_type)`, mirroring the `body` model.

    `exact=False` keeps the whole row `ambiguous`: when the handler itself
    cannot be named, publishing a query kind (even "none") would turn a
    resolver blind spot into a false contract.
    """
    if not exact:
        return ("ambiguous", None, None)
    status, params, type_name = res.handler_query_info(method, path)
    kind = _QUERY_KIND.get(status, "ambiguous")
    if kind == "ambiguous":
        return (kind, None, None)
    return (kind, params, type_name)


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

    hint = None
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
        if status == "unresolved":
            hint = _hint_for(res, candidates)

    if status == "exact":
        confidence = "high"
    elif status == "ambiguous" or hint is not None:
        confidence = "low"
    else:
        confidence = "none"

    body, body_type = (
        res.handler_body_kind(method, path)
        if status == "exact"
        else ("ambiguous", None)
    )
    query, query_params, query_type = _query_contract(
        res, method, path, status == "exact"
    )
    return {
        "status": status,
        "file": file,
        "handler": handler,
        "registered_by": ex.resolve_label(
            path, res.registrars.get((method, path), set()), res.origin_table
        ),
        "confidence": confidence,
        "hint": hint,
        "candidates": [{"file": f, "handler": h} for f, h in candidates],
        "body": body,
        "body_type": body_type,
        "query": query,
        "query_params": query_params,
        "query_type": query_type,
    }


def derived_table_counts() -> dict:
    """Row counts of the three profile tables, plus their distinct routes.

    These are a *different* count from `counts.routes`: the tables are per
    profile, so a route served at two ranks is present twice. Publishing both
    next to each other is what makes the 1048-vs-1046 discrepancy explainable
    instead of looking like an off-by-two bug.
    """
    counts = {}
    distinct: set = set()
    row_re = re.compile(
        r"RouteEntry::new\(\s*axum::http::Method::(\w+),\s*\"([^\"]+)\""
    )
    for profile, name in DERIVED_TABLES.items():
        path = os.path.join(ROUTES_DIR, name)
        try:
            text = open(path, encoding="utf-8").read()
        except OSError as exc:
            raise SystemExit(
                f"gen_route_handlers: cannot read {name}: {exc}. "
                "Run scripts/contract/gen_derived_routes.py first."
            ) from exc
        n = len(_DERIVED_ROW_RE.findall(text))
        if n == 0:
            raise SystemExit(
                f"gen_route_handlers: {name} has no RouteEntry::new rows — "
                "the table layout changed; refusing to publish a bogus count."
            )
        counts[profile] = n
        distinct |= set(row_re.findall(text))
    counts["total"] = sum(counts[p] for p in DERIVED_TABLES)
    counts["distinct"] = len(distinct)
    return counts


def check_dynamic_allowlist(rows, annotations) -> None:
    """`confidence: none` must equal the `dynamic=true` declarations, both ways.

    The 15 rows where the static parser cannot name a handler are a *known,
    finite* list, not a free-running counter: every new blind spot has to be
    declared (and justified) in `ledger_annotations.txt`, and every declaration
    that the extractor can now resolve has to be removed. A one-directional
    allowlist could only grow, silently absorbing the next regression.
    """
    none_rows = {(r["method"], r["path"]) for r in rows if r["confidence"] == "none"}
    declared = {
        (method, path)
        for (method, path), row in annotations.items()
        if row.get("dynamic")
    }
    undeclared = sorted(none_rows - declared)
    stale = sorted(declared - none_rows)
    if not undeclared and not stale:
        return
    lines = [
        "gen_route_handlers: dynamic allowlist drift — `confidence: none` and "
        "`dynamic=true` must coincide in both directions."
    ]
    if undeclared:
        lines.append(f"  blind spots without `dynamic=true` ({len(undeclared)}):")
        lines += [f"    - {m} {p}" for m, p in undeclared]
        lines.append(
            "  Fix: add `dynamic=true` to scripts/contract/ledger_annotations.txt "
            "(and to EXPECTED_ANNOTATIONS in extract_registered.py)."
        )
    if stale:
        lines.append(f"  stale `dynamic=true` declarations ({len(stale)}):")
        lines += [f"    - {m} {p}" for m, p in stale]
        lines.append(
            "  Fix: the extractor resolves these now (or the path is gone) — "
            "remove the line; a stale exemption only hides the next regression."
        )
    raise SystemExit("\n".join(lines))


def build() -> dict:
    res = ex.Resolver(ex.load_sources())
    routes = res.all_routes()  # populates `handlers` and `registrars`
    annotations = ex.load_ledger_annotations()

    rows = []
    for method, path in sorted(routes, key=lambda k: (k[1], k[0])):
        row = {"method": method, "path": path}
        row.update(resolve_route(res, method, path))
        ann = annotations.get((method, path), {})
        row["body_opaque"] = bool(ann.get("body_opaque"))
        row["unsupported"] = bool(ann.get("unsupported"))
        row["dynamic"] = bool(ann.get("dynamic"))
        rows.append(row)

    check_dynamic_allowlist(rows, annotations)

    status_counts = Counter(r["status"] for r in rows)
    body_counts = Counter(r["body"] for r in rows)
    query_counts = Counter(r["query"] for r in rows)
    confidence_counts = Counter(r["confidence"] for r in rows)
    derived = derived_table_counts()
    return {
        "schema_version": SCHEMA_VERSION,
        "generator": "scripts/contract/gen_route_handlers.py",
        "description": (
            "Resolved handler definition, JSON-body kind and query-extractor kind "
            "per registered route. Generated from the real `.route(...)` surface; "
            "do not edit by hand."
        ),
        "file_root": FILE_ROOT,
        "counts": {
            "routes": len(rows),
            "status": dict(sorted(status_counts.items())),
            "confidence": dict(sorted(confidence_counts.items())),
            "body": dict(sorted(body_counts.items())),
            "query": dict(sorted(query_counts.items())),
            "body_opaque_declared": sum(1 for r in rows if r["body_opaque"]),
            "unsupported": sum(1 for r in rows if r["unsupported"]),
            "dynamic_declared": sum(1 for r in rows if r["dynamic"]),
            "derived_tables": derived,
        },
        "counts_note": {
            "routes": (
                "distinct (method, path) pairs this sidecar resolves; compare "
                "with status/confidence/body, which are per-route buckets."
            ),
            "query": (
                "per-route `Query<..>` extractor kind. `query_params` is populated "
                "for `typed` and `literal`; `literal` is a dynamic extractor whose "
                "key set is proven by string literals compared against the query "
                "key in the handler (or a same-file helper), while `opaque` "
                "(Value/HashMap with no such literal) and `foreign` (struct outside "
                "the scanned tree) have no addressable field list, and `ambiguous` "
                "includes every row whose handler cannot be named statically (the "
                "declared dynamic set)."
            ),
            "derived_tables": (
                "row counts of the three per-profile tables. `total` is their "
                "sum; `distinct` de-duplicates by (method, path). The gap is "
                "the two `/.well-known/{openid-configuration,jwks.json}` OIDC "
                "twins, which ship as an Always row plus an Oidc row."
            ),
            "sdk_ledger": (
                "the SDK-side ledger is a cross-repo view and is intentionally "
                "not duplicated here — copying it would drift on every SDK "
                "change and turn this backend artifact into a mirror of another "
                "repository."
            ),
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
        f'  "file_root": {json.dumps(doc["file_root"], ensure_ascii=False)},',
        '  "counts": '
        + json.dumps(doc["counts"], ensure_ascii=False, sort_keys=True)
        + ",",
        '  "counts_note": '
        + json.dumps(doc["counts_note"], ensure_ascii=False, sort_keys=True)
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
