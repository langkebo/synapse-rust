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


def ledger_fixture_tuples(lane: str = "ledger_export") -> set:
    out: set = set()
    for prof in ("default", "worker", "all"):
        fp = os.path.join(ROOT, "tests", "unit", "fixtures", lane, f"{profile_name(prof)}.json")
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


def check_positive_contract(per: dict) -> None:
    """S-14: no served route may be absent from every ledger lane.

    This is the direction that had no guard at all. The existing oracle asserts
    `declared ⊆ derived` — the ledger never lies about a route that does not
    exist — which a ledger that simply *omits* things satisfies perfectly. The
    omission direction is the dangerous one: the route works, so nothing fails
    at runtime; the SDK never generates a client for it, ROUTE_CONTRACT.md never
    lists it, and the gap is only discovered when a client cannot call an
    endpoint the server is serving.

    Both lanes are needed. `ledger_export/` is the default-feature compile and
    `ledger_export_sdk/` is the `all-extensions` compile the SDK ingests;
    judging against the default lane alone reports every feature-gated router
    (voice, cas, saml, server-notifications, voip-tracking, builtin-oidc) as
    missing, which buries the real omissions in ~100 entries of noise. Measured
    against the union, the pre-fix residual was 22 — every one a genuine
    omission, each closed by declaring it in the owning manifest.
    """
    router_set = {t for rs in per.values() for t in rs}
    golden = ledger_fixture_tuples("ledger_export")
    sdk = ledger_fixture_tuples("ledger_export_sdk")
    ledger_all = golden | sdk
    if not ledger_all:
        print("  skip positive contract check (fixtures absent)")
        return

    undeclared = ex.undeclared_routes(router_set, ledger_all)
    check(
        "every served route is declared in some ledger lane (S-14)",
        not undeclared,
        f"{len(undeclared)} undeclared, e.g. {undeclared[:3]}",
    )

    # Guard the guard: the predicate must actually discriminate. If
    # `undeclared_routes` were `return []` (or the lanes were loaded empty) the
    # assertion above would pass vacuously and lock nothing in place.
    probe = ("PATCH", "/_matrix/does-not-exist/probe")
    check(
        "the S-14 predicate flags a route that no lane declares (not vacuous)",
        ex.undeclared_routes(router_set | {probe}, ledger_all) == [probe],
        "expected exactly the injected probe to be flagged",
    )


def check_non_namespace_bucket(per: dict) -> None:
    """The non-namespace bucket must be exactly the intentional root surface.

    This used to assert the opposite — that `threepid.rs`'s `/requestToken` and
    `/submitToken` showed up as an orphan. Pinning a defect in place as if it
    were the contract is how an anomaly becomes permanent: the assertion
    documented the problem instead of forcing the decision the plan asked for.

    B5-4 took the decision (the router was dead code from birth — never merged,
    non-spec paths, and the real endpoints live in `account_compat.rs`), so the
    guard now pins the *invariant*: every derived route either sits under a
    Matrix namespace, or is one of the known intentional host-root
    registrations. Any addition is a new non-Matrix surface or a resurrected
    unwired router, and both require an explicit decision rather than quietly
    appearing in ROUTE_CONTRACT.md.

    The bucket is recomputed here from the in-process parse rather than read
    from `artifacts/registered_routes.json`, because this guard runs *before*
    the extractor in `check_route_contract.sh` and would otherwise assert
    against the previous revision's output.
    """
    expected = {
        # liveness probes registered directly on the root router
        ("GET", "/"),
        ("GET", "/health"),
        ("GET", "/_health"),
        # CAS is a host-root protocol: its paths are not Matrix paths
        ("GET", "/login"),
        ("GET", "/logout"),
        ("GET", "/serviceValidate"),
        ("GET", "/proxyValidate"),
        ("GET", "/p3/serviceValidate"),
        ("GET", "/proxy"),
        ("GET", "/admin/services"),
        ("POST", "/admin/services"),
        ("DELETE", "/admin/services/{service_id}"),
        ("GET", "/admin/users/{user_id}/attributes"),
        ("POST", "/admin/users/{user_id}/attributes"),
    }
    actual = {
        (meth, path)
        for routes in per.values()
        for meth, path in routes
        if not path.startswith(("/_matrix/", "/_synapse/", "/.well-known/"))
    }
    check(
        "non-namespace bucket is exactly the intentional root surface",
        actual == expected,
        f"unexpected={sorted(actual - expected)} missing={sorted(expected - actual)}",
    )


def check_lane_profile_modeling() -> None:
    """B2-1: the extractor must reproduce all six (lane x profile) fixture sets.

    Before this the loader collapsed both axes into one union, which is blind to
    the two interesting failures: a route promised in a *lane* that cannot
    compile it, and a route promised in a *profile* whose router is never merged.
    The fixtures on the other side come from the hand-written
    `*_route_manifest()` functions, so agreement is a two-implementation
    cross-check rather than a self-proving assertion.
    """
    lanes = ex.load_lanes()
    check(
        "Cargo.toml yields exactly the two feature lanes",
        set(lanes) == {"ledger_export", "ledger_export_sdk"},
        f"got {sorted(lanes)}",
    )
    if not lanes:
        return
    golden, sdk = lanes["ledger_export"], lanes["ledger_export_sdk"]
    check("golden lane is a strict subset of the SDK lane", golden < sdk, "the lanes must nest")
    check(
        "the lanes really differ (voice-extended is not in `default`)",
        "voice-extended" not in golden and "voice-extended" in sdk,
        f"golden={sorted(golden)}",
    )

    # The predicate has to discriminate in both directions, otherwise the six
    # comparisons below pass vacuously.
    check(
        'cfg(feature = "voice-extended") is off in golden, on in sdk',
        ex.cfg_allows('feature = "voice-extended"', golden) is False
        and ex.cfg_allows('feature = "voice-extended"', sdk) is True,
    )
    check(
        'cfg(not(feature = "friends")) is off in both lanes (friends is in `default`)',
        ex.cfg_allows('not(feature = "friends")', golden) is False
        and ex.cfg_allows('not(feature = "friends")', sdk) is False,
    )
    check(
        "cfg(all(..) / any(..) / not(..)) compose exactly as Rust does",
        ex.cfg_allows('all(feature = "widgets", not(feature = "voice-extended"))', golden) is True
        and ex.cfg_allows('any(feature = "nope", feature = "cas-sso")', golden) is False
        and ex.cfg_allows('any(feature = "nope", feature = "cas-sso")', sdk) is True,
    )
    check(
        "an unknown bare cfg flag evaluates to off (no cfg(test) code survives extraction)",
        ex.cfg_allows("test", sdk) is False,
    )
    check("union mode satisfies every predicate (the historic behaviour)", ex.cfg_allows('feature = "nope"', None) is True)

    # Runtime profile guards must be *read from the assembly*, not guessed.
    gated = ex.gated_router_builders(ex.load_sources())
    check(
        "runtime profile guards are read out of merge_into (exactly two gated routers)",
        gated == {"create_oidc_router": "oidc_enabled", "create_worker_body_router": "worker_enabled"},
        f"got {gated}",
    )

    for lane_name, feats in sorted(lanes.items()):
        res = ex.Resolver(ex.load_sources(feats), feats)
        sets = ex.profile_sets(res)
        check(
            f"{lane_name}: every derived route carries a guard record",
            all(res.guards.get(r) for r in sets["all"]),
            "an unrecorded route would be silently classified as always-on",
        )
        check(f"{lane_name}: default ⊆ worker ⊆ all", sets["default"] <= sets["worker"] <= sets["all"])
        for prof, got in sorted(sets.items()):
            fp = os.path.join(ROOT, "tests", "unit", "fixtures", lane_name, f"{prof}.json")
            if not os.path.exists(fp):
                print(f"  skip {lane_name}/{prof} (fixture absent)")
                continue
            with open(fp) as fh:
                want = {(e["method"], e["path"]) for e in json.load(fh)["entries"]}
            check(
                f"{lane_name}/{prof}: derived == fixture ({len(want)} routes)",
                got == want,
                f"missing={sorted(want - got)[:3]} extra={sorted(got - want)[:3]}",
            )


def check_emitted_gates() -> None:
    """B2-1 step 2b: the emitted `#[cfg]` must reproduce each lane on its own.

    The generated table replaces the hand-written manifests, and the manifests
    are `#[cfg]`-gated *by being inside a gated module* — the compiler does the
    filtering. A generated table has to reproduce that with explicit predicates
    on the rows, so the predicate per row is now load-bearing in a way it never
    was before: too narrow and a build silently loses endpoints, too wide and
    the 405-probe test reports a route the router does not serve.
    """
    gates = ex.mod_gated_files(ex.raw_sources())
    lanes = ex.load_lanes()
    if not lanes:
        return
    union = ex.Resolver(ex.load_sources(), None, gates)
    rows = ex.profile_sets(union)["all"]

    distinct = {union.gate_of(r) for r in rows}
    non_empty = {g for g in distinct if g}
    check(
        "every route carries a gate, and the gate set is not vacuous",
        len(rows) == len({r for r in rows if union.gate_of(r) is not None}) and len(distinct) >= 8 and len(non_empty) >= 6,
        f"{len(distinct)} distinct gates, {len(non_empty)} of them non-empty",
    )
    check(
        "every gate predicate is a plain `feature = \"..\"`",
        all(p.startswith('feature = "') and p.endswith('"') for g in distinct for p in g),
        f"odd predicates: {sorted({p for g in distinct for p in g if not p.startswith('feature = ')})[:5]}",
    )

    # The in-function `#[cfg]` block is the case module gates cannot explain:
    # `voip-tracking` gates *part* of `create_voip_compat_router`, a file with no
    # gate of its own. If the gate ignores the recorded scope, those 5 rows get
    # admitted into the default lane.
    voip = ("GET", "/_matrix/client/v3/rooms/{room_id}/call/{call_id}")
    check(
        "a `#[cfg]` block inside an ungated file is still gated",
        voip in rows and union.gate_of(voip) == frozenset({'feature = "voip-tracking"'}),
        f"gate={sorted(union.gate_of(voip)) if voip in rows else 'row missing'}",
    )
    # ...and the reverse: a route whose relative path is also registered by an
    # ungated router must not inherit the gated one's condition, nor drop it.
    root_login = ("GET", "/login")
    check(
        "a path shared by a gated and an ungated router keeps the gated condition",
        root_login in rows and union.gate_of(root_login) == frozenset({'feature = "cas-sso"'}),
        f"gate={sorted(union.gate_of(root_login)) if root_login in rows else 'row missing'}",
    )

    for lane_name, feats in sorted(lanes.items()):
        fp = os.path.join(ROOT, "tests", "unit", "fixtures", lane_name, "all.json")
        if not os.path.exists(fp):
            continue
        with open(fp) as fh:
            want = {(e["method"], e["path"]) for e in json.load(fh)["entries"]}
        got = {r for r in rows if ex.cfg_all_allow(list(union.gate_of(r)), feats)}
        check(f"gate-filtered union reproduces {lane_name}/all", got == want, f"{len(got)} vs {len(want)}")


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

    # The other direction. `extract_registered.py` under `EXTRACT_STRICT=1`
    # already fails on an allowlist entry that matches nothing (see the
    # `stale_allowed` branch in its `main()`), because such an entry keeps
    # "covering" a blind spot that no longer exists and lets the file only ever
    # grow. Until now this harness enforced only the growth direction, so a
    # stale entry passed here while the real gate went red — the guard test was
    # weaker than the thing it guards (铁律 8). Mirror the extractor exactly.
    stale = sorted(allowed - res.unresolved)
    check(
        "no stale unresolved-allowlist entry (bidirectional ratchet)",
        not stale,
        f"stale: {stale[:5]} — prune it from extract_unresolved_allowlist.txt",
    )


def check_ledger_origins() -> None:
    """B2-1 step 2: `registered_by` must stay derivable, not just transcribed.

    The point of the rule table is that deleting the hand-written manifests does
    not lose the ledger's module names. That claim is only worth anything if the
    rules are (a) complete, (b) anchored to functions that still exist, and (c)
    actually load-bearing — a rule table that is consulted but never decisive
    would let a rename slip through. Each is checked here.
    """
    origins = ex.load_ledger_origins()
    check("ledger_origins.txt parses and is non-trivial", len(origins) >= 20, f"{len(origins)} rules")
    check(
        "every rule carries a registered_by and a file",
        all(origin and owner for owner, _who, _qual, origin in origins),
    )

    # (b) no stale rule: every rule must name a file that exists and, unless it
    # is a file-level `*` rule, a function defined in it. A rule left behind by
    # a rename would keep matching nothing while looking authoritative.
    files = ex.load_sources()
    stale = []
    for owner, who, _qual, _origin in origins:
        src = files.get(owner)
        if src is None:
            stale.append(f"{owner} (file not found)")
            continue
        if who == "*":
            continue
        if not any(name == who for name, _body, _preds in ex.iter_fns(src)):
            stale.append(f"{owner}::{who} (fn not found)")
    check("no ledger_origins rule is stale", not stale, f"stale: {stale[:5]}")

    # (c) the table is decisive: reordering two rules that target one route must
    # change the answer. `/.well-known/jwks.json` is the real case — the full
    # OIDC router is listed above the fallback one precisely so the
    # all-extensions lane reads `oidc` while the default lane reads
    # `oidc_fallback`.
    probe = "/.well-known/jwks.json"
    ra = ex.resolve_label(probe, {("oidc/mod.rs", "create_oidc_router"), ("oidc/mod.rs", "create_oidc_fallback_router")}, origins)
    rb = ex.resolve_label(probe, {("oidc/mod.rs", "create_oidc_fallback_router")}, origins)
    check(
        "the two OIDC registrars resolve to different ledger names",
        ra == "oidc" and rb == "oidc_fallback",
        f"both-registrars={ra!r}, fallback-only={rb!r}",
    )
    flipped = [(o, w, q, v) for (o, w, q, v) in origins]
    i = next(n for n, (o, w, q, _v) in enumerate(flipped) if (o, w, q) == ("oidc/mod.rs", "create_oidc_router", "/.well-known/"))
    j = next(n for n, (o, w, q, _v) in enumerate(flipped) if (o, w, q) == ("oidc/mod.rs", "create_oidc_fallback_router", "/.well-known/"))
    flipped[i], flipped[j] = flipped[j], flipped[i]
    swapped = ex.resolve_label(probe, {("oidc/mod.rs", "create_oidc_router"), ("oidc/mod.rs", "create_oidc_fallback_router")}, flipped)
    check("rule order is what decides, not an accident of the set", swapped == "oidc_fallback", f"got {swapped!r}")

    # Ambiguity must fail loudly rather than pick. Two registrars with two
    # defaults and no rule to separate them is exactly the state that used to be
    # resolved by "whatever the manifest happened to say".
    check(
        "an unruled registrar conflict resolves to None, not a guess",
        ex.resolve_label("/whatever", {("room.rs", "create_room_router"), ("media/mod.rs", "create_media_router")}, origins) is None,
    )
    check(
        "a single unruled registrar still falls through to the path rule",
        ex.resolve_label("/whatever", {("room.rs", "create_room_router")}, origins) == "room",
    )


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

    def fixture(lane: str, prof: str) -> set:
        fp = os.path.join(ROOT, "tests", "unit", "fixtures", lane, f"{prof}.json")
        with open(fp) as fh:
            return {(e["method"], e["path"]) for e in json.load(fh)["entries"]}

    # Mutation 3 — `#[cfg]` gates ignored, i.e. back to the pre-B2-1 union
    # behaviour. The golden lane then compiles code it cannot compile and must
    # over-report against its own fixtures.
    orig_cfg = ex.cfg_allows
    ex.cfg_allows = lambda predicate, features: True
    try:
        lanes = ex.load_lanes()
        res = ex.Resolver(ex.load_sources(lanes["ledger_export"]), lanes["ledger_export"])
        got = ex.profile_sets(res)["all"]
        want = fixture("ledger_export", "all")
        if got != want:
            print(f"  ok   mutation#3 (cfg gates ignored) turns the suite RED: golden lane reports {len(got)} vs {len(want)}")
        else:
            print("  FAIL mutation#3 did NOT turn the suite red — the lane guard is self-proving")
            bad += 1
    finally:
        ex.cfg_allows = orig_cfg

    # Mutation 4 — profile guards forgotten. Worker + OIDC routers then look
    # always-merged, so `default` over-reports by exactly those 19 routes.
    orig_gated = ex.gated_router_builders
    ex.gated_router_builders = lambda files: {}
    try:
        lanes = ex.load_lanes()
        res = ex.Resolver(ex.load_sources(lanes["ledger_export_sdk"]), lanes["ledger_export_sdk"])
        got = ex.profile_sets(res)["default"]
        want = fixture("ledger_export_sdk", "default")
        if got != want:
            print(f"  ok   mutation#4 (profile guards dropped) turns the suite RED: default reports {len(got)} vs {len(want)}")
        else:
            print("  FAIL mutation#4 did NOT turn the suite red — the profile guard is self-proving")
            bad += 1
    finally:
        ex.gated_router_builders = orig_gated

    # Mutation 5 — a registered_by rule goes stale (step 2a fidelity). Drop the
    # two `@/.well-known/` swimlane rules: both OIDC registrars then resolve to
    # whatever file-level rule remains, and the labels drift against the fixture.
    orig_load = ex.load_ledger_origins

    def without_swimlane_rules():
        return [r for r in orig_load() if "/.well-known/" not in (r[2] or "")]

    ex.load_ledger_origins = without_swimlane_rules
    try:
        lanes = ex.load_lanes()
        drift = 0
        for lane_name, feats in sorted(lanes.items()):
            fp = os.path.join(ROOT, "tests", "unit", "fixtures", lane_name, "all.json")
            if not os.path.exists(fp):
                continue
            res_lane = ex.Resolver(ex.load_sources(feats), feats)
            ex.profile_sets(res_lane)
            with open(fp) as fh:
                for e in json.load(fh)["entries"]:
                    got = ex.resolve_label(
                        e["path"], res_lane.registrars.get((e["method"], e["path"]), set()),
                        ex.load_ledger_origins(),
                    )
                    if got != e["registered_by"]:
                        drift += 1
        if drift:
            print(f"  ok   mutation#5 (swimlane rules dropped) turns the suite RED: {drift} label(s) drift")
        else:
            print("  FAIL mutation#5 did NOT turn the suite red — the label guard is self-proving")
            bad += 1
    finally:
        ex.load_ledger_origins = orig_load

    # Mutation 6 — the generated gate forgets the module gates (step 2b). If
    # `gate_of` returns only the in-function scope, the golden lane admits the
    # extension-feature routes it cannot compile, and the gate-filtered union
    # no longer reproduces either fixture.
    orig_gate_of = ex.Resolver.gate_of

    def scope_only(self, row):
        contexts = self.cfg_of.get(row)
        if not contexts:
            return frozenset()
        return min(contexts, key=len)

    ex.Resolver.gate_of = scope_only
    try:
        lanes = ex.load_lanes()
        union = ex.Resolver(ex.load_sources(), None, ex.mod_gated_files(ex.raw_sources()))
        union_rows = ex.profile_sets(union)["all"]
        drifted = []
        for lane_name, feats in sorted(lanes.items()):
            fp = os.path.join(ROOT, "tests", "unit", "fixtures", lane_name, "all.json")
            if not os.path.exists(fp):
                continue
            with open(fp) as fh:
                want = {(e["method"], e["path"]) for e in json.load(fh)["entries"]}
            got = {r for r in union_rows if ex.cfg_all_allow(list(union.gate_of(r)), feats)}
            if got != want:
                drifted.append(f"{lane_name}:{len(got)}vs{len(want)}")
        if drifted:
            print(f"  ok   mutation#6 (module gates dropped) turns the suite RED: {drifted}")
        else:
            print("  FAIL mutation#6 did NOT turn the suite red — the cfg gate guard is self-proving")
            bad += 1
    finally:
        ex.Resolver.gate_of = orig_gate_of

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
    print("== positive contract (S-14) ==")
    check_positive_contract(per)
    print("== non-namespace surface ==")
    check_non_namespace_bucket(per)
    print("== compile lanes and runtime profiles (B2-1) ==")
    check_lane_profile_modeling()
    print("== unresolved ratchet ==")
    check_ratchet(res)
    print("== ledger origins (B2-1 step 2) ==")
    check_ledger_origins()
    print("== emitted cfg gates (B2-1 step 2b) ==")
    check_emitted_gates()

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
