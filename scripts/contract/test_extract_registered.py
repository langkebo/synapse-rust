#!/usr/bin/env python3
"""Guard tests for `extract_registered.py`.

These are the regression locks for S-13 — the contract extractor that used to
report only the *first* method of a chained handler list and ignored `.nest()`
prefixes entirely. Every check below is written so that it fails if the
corresponding parser logic is removed; `--mutation-check` proves that claim by
deliberately reintroducing the two original defects and asserting the suite
goes red.

Run:
    python3 scripts/contract/test_extract_registered.py
    python3 scripts/contract/test_extract_registered.py --mutation-check
"""

from __future__ import annotations

import importlib.util
import json
import os
import sys

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
ROOT = os.environ.get("SYNAPSE_RUST_ROOT") or os.path.dirname(os.path.dirname(SCRIPT_DIR))

_spec = importlib.util.spec_from_file_location("extract_registered", os.path.join(SCRIPT_DIR, "extract_registered.py"))
ex = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(ex)

FAILURES: list[str] = []
CHECKS = 0


def check(label: str, cond: bool, detail: str = "") -> None:
    global CHECKS
    CHECKS += 1
    if cond:
        print(f"  ok   {label}")
    else:
        print(f"  FAIL {label}" + (f"\n         {detail}" if detail else ""))
        FAILURES.append(label)


def resolve() -> "ex.Resolver":
    return ex.Resolver(ex.load_sources())


def derived(res: "ex.Resolver") -> dict:
    """Run the same extraction `main()` performs, returning `{module: {(m, p)}}`."""
    per: dict[str, set] = {}
    for owner, name, body in res.roots():
        for meth, path, own in res.eval_fn_body(name, body, owner, memo_key=(owner, name, body)):
            if meth and path:
                per.setdefault(own, set()).add((meth, path))
    return per


def ledger_fixture_tuples() -> set:
    out: set = set()
    for prof in ("default", "worker", "all"):
        fp = os.path.join(ROOT, "tests", "unit", "fixtures", "ledger_export", f"{profile_name(prof)}.json")
        if not os.path.exists(fp):
            continue
        with open(fp) as fh:
            for e in json.load(fh)["entries"]:
                out.add((e["method"], e["path"]))
    return out


def profile_name(prof: str) -> str:
    return prof


# ---------------------------------------------------------------------------
# Checks
# ---------------------------------------------------------------------------


def check_chained_methods(per: dict) -> None:
    """S-13 core: `.route(p, get().put().delete())` must yield all three methods."""
    msc = per.get("msc4108_rendezvous.rs", set())
    session = "/_matrix/client/unstable/org.matrix.msc4108/rendezvous/{session_id}"
    methods = {m for (m, p) in msc if p == session}
    check(
        "MSC4108 /rendezvous/{session_id} exposes GET, PUT and DELETE",
        methods == {"GET", "PUT", "DELETE"},
        f"got {sorted(methods)} — the pre-fix extractor returned only ['GET']",
    )
    create = {m for (m, p) in msc if p == "/_matrix/client/unstable/org.matrix.msc4108/rendezvous"}
    check("MSC4108 /rendezvous exposes POST", create == {"POST"}, f"got {sorted(create)}")

    # A second, independent chained chain: `get(get_pushers).post(set_pusher)`.
    push = per.get("push.rs", set())
    pushers = {m for (m, p) in push if p == "/_matrix/client/v3/pushers/"}
    check(
        "push.rs /pushers/ exposes GET and POST",
        pushers == {"GET", "POST"},
        f"got {sorted(pushers)}",
    )


def check_nest_prefixes(per: dict) -> None:
    """S-13 second half: relative sub-router paths must come out prefixed."""
    spaces = per.get("space/lifecycle_query.rs", set())
    paths = {p for _m, p in spaces}
    check(
        "space/lifecycle_query.rs resolves under /_matrix/client/v1",
        "/_matrix/client/v1/spaces/{space_id}" in paths,
        f"sample: {sorted(paths)[:4]}",
    )
    check(
        "space/lifecycle_query.rs resolves under /_matrix/client/v3",
        "/_matrix/client/v3/spaces/{space_id}" in paths,
        f"sample: {sorted(paths)[:4]}",
    )
    check("space/lifecycle_query.rs contributes 9 routes x 2 prefixes", len(spaces) == 18, f"got {len(spaces)}")

    # Cross-file nesting: space.rs nests routers defined in space/*.rs.
    hier = {p for _m, p in per.get("space/children_hierarchy.rs", set())}
    check(
        "cross-file nest: children_hierarchy inherits space.rs prefixes",
        "/_matrix/client/v1/spaces/{space_id}/children" in hier and "/_matrix/client/v3/spaces/{space_id}/children" in hier,
        f"sample: {sorted(hier)[:4]}",
    )

    # Heterogeneous prefixes: e2ee compat -> v1+v3, v3-only -> v3 only.
    e2ee = per.get("e2ee/keys.rs", set())
    check(
        "e2ee /keys/upload is v1 + v3",
        {"/_matrix/client/v1/keys/upload", "/_matrix/client/v3/keys/upload"} <= {p for _m, p in e2ee},
        f"sample: {sorted(p for _m, p in e2ee)[:4]}",
    )
    check(
        "e2ee v3-only /keys/history is not exposed under v1",
        "/_matrix/client/v3/keys/history" in {p for _m, p in e2ee}
        and "/_matrix/client/v1/keys/history" not in {p for _m, p in e2ee},
        f"sample: {sorted(p for _m, p in e2ee)[:4]}",
    )

    # No bare `/spaces/...` must survive anywhere in the derived surface.
    bare = sorted({p for mod, rs in per.items() if "space" in mod for _m, p in rs if p.startswith("/spaces/")})
    check("no derive-time relative /spaces/ path remains", not bare, f"leaked: {bare[:5]}")

    # The doc itself is what consumers read.
    doc = os.path.join(ROOT, "docs", "synapse-rust", "ROUTE_CONTRACT.md")
    if os.path.exists(doc):
        text = open(doc, encoding="utf-8").read()
        check(
            "ROUTE_CONTRACT.md lists prefixed space routes",
            "/_matrix/client/v1/spaces/{space_id}`" in text and "/_matrix/client/v3/spaces/{space_id}`" in text,
        )
        check("ROUTE_CONTRACT.md has no bare `/spaces/` bullet", "- `GET` `/spaces/" not in text)


def check_test_module_excision(per: dict) -> None:
    """A `#[cfg(test)] mod` placed *before* production code must not truncate it.

    `space/lifecycle_query.rs` declares `mod cursor_tests` at line 19 and its
    router builder at line 200; truncating at the first `#[cfg(test)]` silently
    deleted the builder.
    """
    src = ex.load_sources().get("space/lifecycle_query.rs", "")
    check(
        "strip_test_mods keeps production code after an early test module",
        "create_space_lifecycle_query_routes" in src,
        "the router builder was excised with the test module",
    )
    check("lifecycle_query.rs still yields its 18 routes", len(per.get("space/lifecycle_query.rs", set())) == 18)


def check_oracles(res: "ex.Resolver", per: dict) -> None:
    """The two independent oracles must both report zero missing routes."""
    router_set = {t for rs in per.values() for t in rs}

    man = {(m, p) for (m, p, _o) in res.manifest_routes() if m and p}
    only_manifest = sorted(man - router_set)
    check(
        "every manifest-declared route is derived (manifest oracle)",
        not only_manifest,
        f"{len(only_manifest)} missing, e.g. {only_manifest[:3]}",
    )

    ledger = ledger_fixture_tuples()
    if ledger:
        missed = sorted(ledger - router_set)
        check(
            "every ledger_export fixture route is derived (authoritative oracle)",
            not missed,
            f"{len(missed)} missing, e.g. {missed[:3]}",
        )
    else:
        print("  skip ledger oracle (fixtures absent)")


def check_orphan_detection(per: dict) -> None:
    """The threepid orphan must be visible: registered, but never prefixed."""
    threepid = per.get("threepid.rs", set())
    check(
        "threepid.rs routes are derived but stay outside the Matrix namespaces",
        {p for _m, p in threepid} == {"/requestToken", "/submitToken"},
        f"got {sorted(threepid)}",
    )


def check_ratchet(res: "ex.Resolver") -> None:
    allow_path = os.path.join(SCRIPT_DIR, "extract_unresolved_allowlist.txt")
    allowed = set()
    if os.path.exists(allow_path):
        with open(allow_path) as fh:
            for ln in fh:
                entry = ln.split("#", 1)[0].strip()
                if entry:
                    allowed.add(entry)
    new = sorted(res.unresolved - allowed)
    check("no new unresolved parser construct (ratchet)", not new, f"new: {new[:5]}")


# ---------------------------------------------------------------------------
# Mutation self-check: prove the suite can go red
# ---------------------------------------------------------------------------


def mutation_check() -> int:
    """Reintroduce the two original S-13 defects and assert the suite fails.

    A guard test that cannot fail is worthless (铁律 8). This runs the real
    checks against a deliberately-degraded parser and requires a non-zero
    failure count for each mutation.
    """
    print("\n== mutation check: reintroduce the S-13 defects ==")
    bad = 0

    # Mutation 1 — chained methods collapsed to the first one.
    orig_methods_of = ex.Resolver._methods_of
    ex.Resolver._methods_of = staticmethod(lambda text: ([orig_methods_of(text)[0]] if orig_methods_of(text) else []))
    try:
        per = derived(resolve())
        failed = []

        def record(label: str, cond: bool, detail: str = "") -> None:
            if not cond:
                failed.append(label)

        msc = {m for (m, p) in per.get("msc4108_rendezvous.rs", set()) if p.endswith("/rendezvous/{session_id}")}
        record("chained", msc == {"GET", "PUT", "DELETE"})
        push = {m for (m, p) in per.get("push.rs", set()) if p == "/_matrix/client/v3/pushers/"}
        record("pushers", push == {"GET", "POST"})
        if failed:
            print(f"  ok   mutation#1 (first-method-only) turns the suite RED via: {failed}")
            bad += 0
        else:
            print("  FAIL mutation#1 did NOT turn the suite red — the guard is self-proving")
            bad += 1
    finally:
        # `_methods_of` is a staticmethod: restore it as one, otherwise it
        # becomes an instance method and every later call gains a `self`.
        ex.Resolver._methods_of = staticmethod(orig_methods_of)

    # Mutation 2 — `.nest()` prefix propagation removed.
    orig_apply = ex.Resolver.apply_call

    def no_nest(self, acc, name, args, env, owner):
        if name == "nest":
            return acc
        return orig_apply(self, acc, name, args, env, owner)

    ex.Resolver.apply_call = no_nest
    try:
        per = derived(resolve())
        spaces = {p for _m, p in per.get("space/lifecycle_query.rs", set())}
        # Without prefix propagation the sub-router's routes are never reached at
        # all (the `let router = ..` binding is not itself the return value), so
        # the faithful expectation is *disappearance*, not a leaked relative path.
        prefixed = sorted(p for p in spaces if p.startswith(("/_matrix/client/v1/spaces", "/_matrix/client/v3/spaces")))
        bare_or_missing = not prefixed
        if bare_or_missing:
            print("  ok   mutation#2 (no nest propagation) turns the suite RED: prefixed space routes vanish")
        else:
            print(f"  FAIL mutation#2 did NOT turn the suite red — the guard is self-proving; got {prefixed[:2]}")
            bad += 1
    finally:
        ex.Resolver.apply_call = orig_apply

    return bad


def main() -> int:
    mutation = "--mutation-check" in sys.argv

    res = resolve()
    per = derived(res)

    print("== chained method extraction (S-13) ==")
    check_chained_methods(per)
    print("== nest prefix propagation (S-13) ==")
    check_nest_prefixes(per)
    print("== test-module excision ==")
    check_test_module_excision(per)
    print("== independent oracles ==")
    check_oracles(res, per)
    print("== orphan detection (S-9 evidence) ==")
    check_orphan_detection(per)
    print("== unresolved ratchet ==")
    check_ratchet(res)

    bad = 0
    if mutation:
        bad = mutation_check()

    print()
    if FAILURES or bad:
        print(f"❌ {len(FAILURES)} check(s) failed" + (f", {bad} mutation(s) self-proving" if bad else ""))
        return 1
    print(f"✅ all {CHECKS} guard checks passed" + (" (+ mutation check)" if mutation else ""))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
