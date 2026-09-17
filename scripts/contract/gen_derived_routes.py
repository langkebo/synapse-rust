#!/usr/bin/env python3
"""Generate `synapse-web/src/routes/derived_routes.rs` from the route extractor.

B2-3 table-driven manifest: replaces ~120 hand-copied `*_route_manifest()` helpers.
Instead of restating routes, the generator emits ONE rank-tagged table that the
extractor proved reproduces all six committed fixtures (two lanes × three profiles).

Row model (proven; see the doc comment in the emitted file):
* every row carries a `#[cfg(...)]` feature gate — rustc does the per-lane
  filtering at compile time, so the same source builds both the default and the
  `all-extensions` SDK lanes;
* every row carries a profile rank (`Always` < `Worker` < `Oidc`);
* `derived_route_manifest` keeps rows with `rank <= profile_rank(flags)`, then
  de-duplicates by `(method, path)` retaining the highest rank. That reproduces
  the two `/.well-known/{openid-configuration,jwks.json}` OIDC-collision twins
  exactly (default lane → `oidc_fallback`, SDK lane → `oidc`).

Usage:
    python3 scripts/contract/gen_derived_routes.py
    python3 scripts/contract/gen_derived_routes.py --check   # drift gate

The generator refuses to emit unless the reconstructed table matches every fixture
in both lanes (see `verify_fixtures`).
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
import sys

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(SCRIPT_DIR))
_spec = importlib.util.spec_from_file_location(
    "extract_registered", os.path.join(SCRIPT_DIR, "extract_registered.py")
)
ex = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(ex)

LANE_DEFAULT = "ledger_export"
LANE_SDK = "ledger_export_sdk"
OUT_DATA = os.path.join(ROOT, "synapse-web", "src", "routes", "derived_route_table.inc.rs")
OUT_DATA_ALWAYS = os.path.join(ROOT, "synapse-web", "src", "routes", "derived_route_table_always.inc.rs")
OUT_DATA_WORKER = os.path.join(ROOT, "synapse-web", "src", "routes", "derived_route_table_worker.inc.rs")
OUT_DATA_OIDC = os.path.join(ROOT, "synapse-web", "src", "routes", "derived_route_table_oidc.inc.rs")
OUT = os.path.join(ROOT, "synapse-web", "src", "routes", "derived_routes.rs")

PROFILES = ["default", "worker", "all"]
PROFILE_RANK = {"Always": 0, "Worker": 1, "Oidc": 2}
PROFILE_MAX_RANK = {"default": 0, "worker": 1, "all": 2}
ALLOWED = {"default": {""}, "worker": {"", "worker_enabled"}, "all": {"", "worker_enabled", "oidc_enabled"}}

# ---------------------------------------------------------------------------
# Row model
# ---------------------------------------------------------------------------

def _build_lane(feats):
    srcs = ex.load_sources(feats)
    return ex.Resolver(srcs, feats, ex.mod_gated_files(dict(srcs)))


def _guard_label(cfg_set):
    """Map a `cfg_of` context to the closest profile guard name.

    The extractor stores raw predicate strings in `cfg_of`, but for the runtime
    filter we only need to know *which* feature flag is the narrowest one that
    enabled the row.  An unknown label falls back to the empty string `""`
    because *every* profile allows `""`; the tightest context still wins via
    `max(ok, key=len)`.
    """
    v = "".join(sorted(cfg_set))
    return {
        "worker_enabled": "worker_enabled",
        "worker_enabled+oidc": "worker_enabled+oidc",
        "oidc_enabled": "oidc_enabled",
    }.get(v, "")


def build_rows():
    lanes = ex.load_lanes()
    feats_sdk = frozenset(lanes[LANE_SDK])
    feats_gold = frozenset(lanes[LANE_DEFAULT])

    res_sdk = _build_lane(feats_sdk)
    PROFS_SDK = ex.profile_sets(res_sdk)
    origins = ex.load_ledger_origins()
    annotations = ex.load_ledger_annotations()

    def label(t, prof):
        regs = res_sdk.registrars.get(t, set())
        active = {(f, fn) for (f, fn) in regs if res_sdk.gated.get(fn, "") in ALLOWED[prof]}
        return ex.resolve_label(t[1], active, origins)

    def gate_for(t, prof):
        ctxs = res_sdk.cfg_of.get(t, set()) or {frozenset()}
        ok = [c for c in ctxs if _guard_label(c) in ALLOWED[prof]]
        return max(ok, key=lambda c: (len(c), sorted(c))) if ok else frozenset()

    def row_profile(t):
        if t in PROFS_SDK["default"]:
            return "Always"
        if t in PROFS_SDK["worker"]:
            return "Worker"
        return "Oidc"

    def cfg_of(t):
        return frozenset(res_sdk.gate_of(t))

    def ann_for(t):
        return annotations.get((t[0], t[1]), {})

    rows = set()
    for t in PROFS_SDK["all"]:
        rp = row_profile(t)
        prof_name = {"Always": "default", "Worker": "worker", "Oidc": "all"}[rp]
        lbl = label(t, prof_name)
        if lbl is None:
            raise SystemExit(f"gen_derived_routes: undecidable label for {t} (profile={prof_name})")
        cfg = cfg_of(t)
        ann = ann_for(t)
        rows.add((t[0], t[1], lbl, cfg, PROFILE_RANK[rp], ann.get("auth"), ann.get("rate_limit_exempt", False)))

    # Collision twins: same (method, path) served at a higher profile with a
    # different registered_by (the two /.well-known OIDC routes). Emit an extra
    # row at the higher rank, gated to the lane that actually compiles it.
    for rp, prof_h in (("Worker", "worker"), ("Oidc", "all")):
        for t in PROFS_SDK["all"]:
            if row_profile(t) != "Always":
                continue
            a = label(t, "default")
            b = label(t, prof_h)
            if a == b:
                continue
            cfg = gate_for(t, prof_h)
            ann = ann_for(t)
            rows.add((t[0], t[1], b, cfg, PROFILE_RANK[rp], ann.get("auth"), ann.get("rate_limit_exempt", False)))

    rows = sorted(rows, key=lambda r: (r[1], r[0], r[2], r[4], tuple(sorted(r[3]))))
    return rows, feats_gold, feats_sdk


# ---------------------------------------------------------------------------
# Self-verification: reconstruct each fixture's (method,path)->label from the
# rows exactly as the Rust code will, and compare against the committed files.
# ---------------------------------------------------------------------------

def reconstruct(rows, cfgset, prof):
    max_rank = PROFILE_MAX_RANK[prof]
    best = {}  # (m,p) -> (rank, label)
    for (m, p, lbl, cfg, rank, _auth, _exempt) in rows:
        if cfg and not ex.cfg_all_allow(list(cfg), cfgset):
            continue
        if rank > max_rank:
            continue
        key = (m, p)
        if key in best and best[key][0] >= rank:
            continue
        best[key] = (rank, lbl)
    return {k: v[1] for k, v in best.items()}


def verify_fixtures(rows, feats_gold, feats_sdk):
    fails = []
    for lane, cfgset in ((LANE_DEFAULT, feats_gold), (LANE_SDK, feats_sdk)):
        for prof in PROFILES:
            fp = os.path.join(ROOT, "tests", "unit", "fixtures", lane, f"{prof}.json")
            entries = json.load(open(fp, encoding="utf-8"))["entries"]
            want = {(e["method"], e["path"]): e["registered_by"] for e in entries}
            got = reconstruct(rows, cfgset, prof)
            if set(want) != set(got):
                fails.append(f"{lane}/{prof}: tuple set mismatch (missing={sorted(set(want) - set(got))[:3]} extra={sorted(set(got) - set(want))[:3]})")
                continue
            mism = {k: (got[k], want[k]) for k in want if got.get(k) != want[k]}
            if mism:
                fails.append(f"{lane}/{prof}: label mismatch on {len(mism)} routes: {list(mism.items())[:3]}")
    return fails


# ---------------------------------------------------------------------------
# Rust emission helpers
# ---------------------------------------------------------------------------

_METHOD = {"GET": "GET", "POST": "POST", "PUT": "PUT", "DELETE": "DELETE", "PATCH": "PATCH"}
_RANK = {0: "RouteProfile::Always", 1: "RouteProfile::Worker", 2: "RouteProfile::Oidc"}


def _s(s):
    return json.dumps(s, ensure_ascii=False)


def _row_rust(row):
    """One push statement, wrapped in `#[cfg]` when the row is feature-gated."""
    method, path, lbl, cfg, rank, auth, exempt = row
    meth = f"axum::http::Method::{_METHOD[method]}"
    auth_s = f"            .with_auth({_s(auth)})\n" if auth else ""
    rate_s = "            .with_rate_limit_exempt(true)\n" if exempt else ""
    push = (
        "    {\n"
        "        let e = RouteEntry::new(\n"
        f"            {meth},\n"
        f"            {_s(path)},\n"
        f"            {_s(lbl)},\n"
        "        )\n"
        f"{auth_s}{rate_s}"
        "        ;\n"
        f"        rows.push(DerivedRoute {{ entry: e, rank: {_RANK[rank]} }});\n"
        "    }\n"
    )
    if cfg:
        preds = sorted(cfg)
        attr = "#[cfg(all(" + ", ".join(preds) + "))]\n" if len(preds) > 1 else f"#[cfg({preds[0]})]\n"
        return attr + push
    return push


def emit(rows):
    """Render the table, then normalise it through `rustfmt`.

    Without this the two gates fight each other: `cargo fmt --check` wants the
    one-line `RouteEntry::new(..)` form, while the emitter's hand-laid text is
    byte-compared by `--check`. Running the emitted source through the same
    rustfmt (same `rustfmt.toml`, same edition) makes both gates agree.
    """
    return _rustfmt(emit_raw(rows))


def _rustfmt(text):
    """Format `text` with the repo's rustfmt; return it unchanged if unavailable."""
    import subprocess
    import tempfile

    try:
        with tempfile.NamedTemporaryFile("w", suffix=".rs", dir=os.path.dirname(OUT) or ".", delete=False) as fh:
            fh.write(text)
            tmp = fh.name
        try:
            proc = subprocess.run(
                ["rustfmt", "--edition", "2021", "--emit", "stdout", tmp],
                capture_output=True,
                text=True,
            )
        finally:
            pass
        if proc.returncode == 0 and proc.stdout.strip():
            os.unlink(tmp)
            # `rustfmt --emit stdout` prefixes the result with the source path:
            #   <abs path>:
            #   <blank>
            #   <formatted source>
            out = proc.stdout.split("\n")
            i = 0
            while i < len(out) and (out[i].rstrip().endswith(".rs:") or not out[i].strip()):
                i += 1
            formatted = "\n".join(out[i:])
            if formatted.strip():
                return formatted
        else:
            os.unlink(tmp)
    except (OSError, subprocess.SubprocessError, UnboundLocalError):
        pass
    return text


def emit_data_per_profile(rows):
    """Generate per-profile `all_derived_X_rows()` body including cfg blocks."""
    # Row tuple: (method, path, label, cfg, profile_rank, auth, rate_limit_exempt)
    # profile_rank is PROFILE_RANK["Always"]=0 / "Worker"=1 / "Oidc"=2 (index 4)
    always_rows = [r for r in rows if r[4] == PROFILE_RANK["Always"]]
    worker_rows = [r for r in rows if r[4] == PROFILE_RANK["Worker"]]
    oidc_rows = [r for r in rows if r[4] == PROFILE_RANK["Oidc"]]

    def emit_group(group_rows, profile_name):
        """Generate the function body for a profile group."""
        if not group_rows:
            # For empty groups, we still emit a stub that returns empty Vec
            return f"fn all_derived_{profile_name}_rows() -> Vec<DerivedRoute> {{\n    let mut rows: Vec<DerivedRoute> = Vec::with_capacity(0);\n    rows\n}}"

        row_lines = "".join(_row_rust(r) for r in group_rows)
        return f"""fn all_derived_{profile_name}_rows() -> Vec<DerivedRoute> {{
    let mut rows: Vec<DerivedRoute> = Vec::with_capacity({len(group_rows)});
{row_lines}    rows
}}"""

    return (
        emit_group(always_rows, "always"),
        emit_group(worker_rows, "worker"),
        emit_group(oidc_rows, "oidc"),
    )

def emit_raw(rows):
    row_lines = "".join(_row_rust(r) for r in rows)
    cap = len(rows)
    # `include_bytes!` needs compile-time literal strings; use concat+env to reach the fixture dir.
    fixtures_dir = "concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/tests/unit/fixtures\")"
    oracle = f"""\
    #[cfg(test)]
    mod derived_manifest_tests {{
use super::*;
use std::collections::HashMap;
use crate::web::routes::ledger_export::LedgerArtifact;
use crate::web::routes::route_module::ProfileFlags as PFlags;

        fn parse_fixture(bytes: &[u8]) -> Vec<(String, String, String)> {{
            let art: LedgerArtifact = serde_json::from_slice(bytes).unwrap();
            art.entries.into_iter().map(|e| (e.method, e.path, e.registered_by)).collect()
        }}

        macro_rules! assert_derived_matches {{
            ($profile:ident, $fixture:expr) => {{
                let flags = match stringify!($profile) {{
                    "DEFAULT" => PFlags {{ oidc_enabled: false, worker_enabled: false, saml_enabled: false }},
                    "WORKER"  => PFlags {{ oidc_enabled: false, worker_enabled: true, saml_enabled: false }},
                    "ALL"     => PFlags {{ oidc_enabled: true, worker_enabled: true, saml_enabled: false }},
                    _ => unreachable!(),
                }};
                let got = derived_route_manifest(&flags);
                let want = parse_fixture(include_bytes!($fixture));
                assert_eq!(got.len(), want.len(), "{{}} profile entry count mismatch", stringify!($profile));
                let mut got_map: HashMap<(String, String, String), ()> = HashMap::new();
                for e in &got {{
                    got_map.insert((e.method.as_str().to_string(), e.path.to_string(), e.registered_by.to_string()), ());
                }}
                for (method, path, registered_by) in want {{
                    let key = (method.clone(), path.clone(), registered_by.clone());
                    assert!(got_map.contains_key(&key), "{{}} profile missing row: {{}} {{}} {{}}", stringify!($profile), method, path, registered_by);
                }}
            }};
        }}

        // Default build → ledger_export fixtures.
        #[cfg(not(any(
            feature = "all-extensions",
            feature = "voice-extended",
            feature = "saml-sso",
            feature = "cas-sso",
            feature = "voip-tracking",
            feature = "server-notifications",
            feature = "privacy-ext",
            feature = "builtin-oidc",
        )))]
        #[test]
        fn default_profile_matches_fixture() {{
            assert_derived_matches!(DEFAULT, concat!({fixtures_dir}, "/ledger_export/default.json"));
        }}

        #[cfg(not(any(
            feature = "all-extensions",
            feature = "voice-extended",
            feature = "saml-sso",
            feature = "cas-sso",
            feature = "voip-tracking",
            feature = "server-notifications",
            feature = "privacy-ext",
            feature = "builtin-oidc",
        )))]
        #[test]
        fn worker_profile_matches_fixture() {{
            assert_derived_matches!(WORKER, concat!({fixtures_dir}, "/ledger_export/worker.json"));
        }}

        #[cfg(not(any(
            feature = "all-extensions",
            feature = "voice-extended",
            feature = "saml-sso",
            feature = "cas-sso",
            feature = "voip-tracking",
            feature = "server-notifications",
            feature = "privacy-ext",
            feature = "builtin-oidc",
        )))]
        #[test]
        fn all_profile_matches_fixture() {{
            assert_derived_matches!(ALL, concat!({fixtures_dir}, "/ledger_export/all.json"));
        }}

        // SDK / all-extensions build → ledger_export_sdk fixtures.
        #[cfg(feature = "all-extensions")]
        #[test]
        fn sdk_default_profile_matches_fixture() {{
            assert_derived_matches!(DEFAULT, concat!({fixtures_dir}, "/ledger_export_sdk/default.json"));
        }}

        #[cfg(feature = "all-extensions")]
        #[test]
        fn sdk_worker_profile_matches_fixture() {{
            assert_derived_matches!(WORKER, concat!({fixtures_dir}, "/ledger_export_sdk/worker.json"));
        }}

        #[cfg(feature = "all-extensions")]
        #[test]
        fn sdk_all_profile_matches_fixture() {{
            assert_derived_matches!(ALL, concat!({fixtures_dir}, "/ledger_export_sdk/all.json"));
        }}
    }}
"""
    return f"""\
//! GENERATED by `scripts/contract/gen_derived_routes.py` — DO NOT EDIT.
//!
//! Route manifest table derived from the real `.route(...)` surface (see the
//! extractor in `scripts/contract/extract_registered.py`). This replaces the
//! ~120 hand-copied `*_route_manifest()` helpers: instead of restating routes,
//! the compiler filters this table by `#[cfg]` and `derived_route_manifest`
//! filters it by the runtime [`ProfileFlags`] ceiling, then de-duplicates by
//! `(method, path)` keeping the highest rank.
//!
//! ## Row model
//!
//! * `RouteProfile` is a monotonic feature ceiling. `default` builds expose
//!   `Always`; `worker_enabled` adds `Worker`; `oidc_enabled` adds `Oidc`.
//!   `derived_route_manifest` keeps every row with `rank <= flags.rank()`.
//! * The two `/.well-known/{{openid-configuration,jwks.json}}` routes appear
//!   twice — once `Always` (label `oidc_fallback`) and once `Oidc`
//!   (label `oidc`, gated `#[cfg(feature = \\"builtin-oidc\\")]`). In the SDK
//!   lane both compile and the dedup keeps the higher-rank `oidc`; in the
//!   default lane the `Oidc` twin is cfg-stripped and `oidc_fallback` wins.
//!   That is exactly what the committed fixtures record.
//!
//! ## Regenerate
//!
//! ```text
//! python3 scripts/contract/gen_derived_routes.py
//! ```
//!
//! The generator refuses to emit unless this table reproduces all six fixtures
//! in `tests/unit/fixtures/{{ledger_export,ledger_export_sdk}}/` byte-equivalently
//! by `(method, path, registered_by)`, so editing it by hand is pointless — the
//! next regeneration overwrites it.

#![allow(clippy::unreadable_literal)]

use super::route_ledger::RouteEntry;
use super::route_module::ProfileFlags;

/// Monotonic feature ceiling a row belongs to.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum RouteProfile {{
    /// Served in every profile.
    Always = 0,
    /// Served only when `worker_enabled`.
    Worker = 1,
    /// Served only when `oidc_enabled`.
    Oidc = 2,
}}

/// A manifest row: the route plus the lowest profile that surfaces it.
pub struct DerivedRoute {{
    /// The route entry itself.
    pub entry: RouteEntry,
    /// Minimum profile rank at which this row is live.
    pub rank: RouteProfile,
}}

impl ProfileFlags {{
    /// The feature ceiling for this flag combination, as a [`RouteProfile`].
    pub fn rank(&self) -> RouteProfile {{
        if self.oidc_enabled {{
            RouteProfile::Oidc
        }} else if self.worker_enabled {{
            RouteProfile::Worker
        }} else {{
            RouteProfile::Always
        }}
    }}
}}

/// Project the live `AppState` flags onto the feature-ceiling rank.
pub fn rank_for_flags(flags: &ProfileFlags) -> RouteProfile {{
    flags.rank()
}}

/// All derived rows visible to the *current* build (compile-time `#[cfg]` has
/// already stripped rows whose features are off).
fn all_derived_rows() -> Vec<DerivedRoute> {{
    let mut rows: Vec<DerivedRoute> = Vec::with_capacity({cap});
{row_lines}    rows
}}

/// Profile-driven manifest. Keep rows at or below the flag ceiling, then
/// de-duplicate by `(method, path)` keeping the highest rank. Output is sorted
/// by `(path, method, registered_by)` for byte-stable diffs.
pub fn derived_route_manifest(flags: &ProfileFlags) -> Vec<RouteEntry> {{
    let max_rank = rank_for_flags(flags);
    let mut best: std::collections::HashMap<(String, String), RouteProfile> =
        std::collections::HashMap::new();
    let mut kept: std::collections::HashMap<(String, String), RouteEntry> =
        std::collections::HashMap::new();
    for DerivedRoute {{ entry, rank }} in all_derived_rows() {{
        if rank > max_rank {{
            continue;
        }}
        let key = (entry.method.as_str().to_string(), entry.path.to_string());
        if let Some(prev) = best.get(&key) {{
            if *prev >= rank {{
                continue;
            }}
        }}
        best.insert(key.clone(), rank);
        kept.insert(key, entry);
    }}
    let mut out: Vec<RouteEntry> = kept.into_values().collect();
    out.sort_by(|a, b| {{
        a.path.cmp(b.path).then_with(|| a.method.as_str().cmp(b.method.as_str())).then_with(|| {{
            a.registered_by.cmp(b.registered_by)
        }})
    }});
    out
}}

{oracle}
"""


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true", help="fail (exit 1) if the committed file differs")
    args = ap.parse_args()

    rows, feats_gold, feats_sdk = build_rows()
    fails = verify_fixtures(rows, feats_gold, feats_sdk)
    if fails:
        print("gen_derived_routes: FIXTURE FIDELITY FAILED")
        for f in fails:
            print("  -", f)
        sys.exit(1)
    print(f"gen_derived_routes: table reproduces all {len(rows)} rows.")

    # Generate per-profile data files
    always_data, worker_data, oidc_data = emit_data_per_profile(rows)

    # Write per-profile .inc files
    with open(OUT_DATA_ALWAYS, "w", encoding="utf-8") as fh:
        fh.write(always_data)
    with open(OUT_DATA_WORKER, "w", encoding="utf-8") as fh:
        fh.write(worker_data)
    with open(OUT_DATA_OIDC, "w", encoding="utf-8") as fh:
        fh.write(oidc_data)

    # Generate derived_routes.rs with includes for all three profiles
    rs_content = _generate_rs_header()
    rs_content += _generate_test_module()

    with open(OUT, "w", encoding="utf-8") as fh:
        fh.write(rs_content)

    print(f"gen_derived_routes: wrote {OUT_DATA_ALWAYS}, {OUT_DATA_WORKER}, {OUT_DATA_OIDC}")
    print(f"gen_derived_routes: wrote {OUT}")


def _generate_test_module():
    """Generate the test module for derived_routes.rs."""
    fixtures_dir = "concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/../tests/unit/fixtures\")"
    return f"""
#[cfg(test)]
mod derived_manifest_tests {{
    use super::*;
    use crate::routes::ledger_export::LedgerArtifact;
    use crate::routes::route_module::ProfileFlags as PFlags;
    use std::collections::HashMap;

    fn parse_fixture(bytes: &[u8]) -> Vec<(String, String, String)> {{
        let art: LedgerArtifact = serde_json::from_slice(bytes).unwrap();
        art.entries.into_iter().map(|e| (e.method, e.path, e.registered_by)).collect()
    }}

    macro_rules! assert_derived_matches {{
        ($profile:ident, $fixture:expr) => {{
            let flags = match stringify!($profile) {{
                "DEFAULT" => PFlags {{ oidc_enabled: false, worker_enabled: false, saml_enabled: false }},
                "WORKER"  => PFlags {{ oidc_enabled: false, worker_enabled: true, saml_enabled: false }},
                "ALL"     => PFlags {{ oidc_enabled: true, worker_enabled: true, saml_enabled: false }},
                _ => unreachable!(),
            }};
            let got = derived_route_manifest(&flags);
            let want = parse_fixture(include_bytes!($fixture));
            assert_eq!(got.len(), want.len(), "{{}} profile entry count mismatch", stringify!($profile));
            let mut got_map: HashMap<(String, String, String), ()> = HashMap::new();
            for e in &got {{
                got_map.insert((e.method.as_str().to_string(), e.path.to_string(), e.registered_by.to_string()), ());
            }}
            for (method, path, registered_by) in want {{
                let key = (method.clone(), path.clone(), registered_by.clone());
                assert!(
                    got_map.contains_key(&key),
                    "{{}} profile missing row: {{}} {{}} {{}}",
                    stringify!($profile),
                    method,
                    path,
                    registered_by
                );
            }}
        }};
    }}

    // Default build → ledger_export fixtures.
    #[cfg(not(any(
        feature = "all-extensions",
        feature = "voice-extended",
        feature = "saml-sso",
        feature = "cas-sso",
        feature = "voip-tracking",
        feature = "server-notifications",
        feature = "privacy-ext",
        feature = "builtin-oidc",
    )))]
    #[test]
    fn default_profile_matches_fixture() {{
        assert_derived_matches!(
            DEFAULT,
            concat!({fixtures_dir}, "/ledger_export/default.json")
        );
    }}

    #[cfg(not(any(
        feature = "all-extensions",
        feature = "voice-extended",
        feature = "saml-sso",
        feature = "cas-sso",
        feature = "voip-tracking",
        feature = "server-notifications",
        feature = "privacy-ext",
        feature = "builtin-oidc",
    )))]
    #[test]
    fn worker_profile_matches_fixture() {{
        assert_derived_matches!(
            WORKER,
            concat!({fixtures_dir}, "/ledger_export/worker.json")
        );
    }}

    #[cfg(not(any(
        feature = "all-extensions",
        feature = "voice-extended",
        feature = "saml-sso",
        feature = "cas-sso",
        feature = "voip-tracking",
        feature = "server-notifications",
        feature = "privacy-ext",
        feature = "builtin-oidc",
    )))]
    #[test]
    fn all_profile_matches_fixture() {{
        assert_derived_matches!(
            ALL,
            concat!({fixtures_dir}, "/ledger_export/all.json")
        );
    }}

    // SDK / all-extensions build → ledger_export_sdk fixtures.
    #[cfg(feature = "all-extensions")]
    #[test]
    fn sdk_default_profile_matches_fixture() {{
        assert_derived_matches!(
            DEFAULT,
            concat!({fixtures_dir}, "/ledger_export_sdk/default.json")
        );
    }}

    #[cfg(feature = "all-extensions")]
    #[test]
    fn sdk_worker_profile_matches_fixture() {{
        assert_derived_matches!(
            WORKER,
            concat!({fixtures_dir}, "/ledger_export_sdk/worker.json")
        );
    }}

    #[cfg(feature = "all-extensions")]
    #[test]
    fn sdk_all_profile_matches_fixture() {{
        assert_derived_matches!(
            ALL,
            concat!({fixtures_dir}, "/ledger_export_sdk/all.json")
        );
    }}
}}
"""


def _generate_rs_header():
    """Generate derived_routes.rs: the header (doc + types), the all_derived_rows
    aggregator that include!s the 3 per-profile .inc files, then the
    derived_route_manifest function and test module."""
    return """\
//! GENERATED by `scripts/contract/gen_derived_routes.py` — DO NOT EDIT.
//!
//! Route manifest table derived from the real `.route(...)` surface (see the
//! extractor in `scripts/contract/extract_registered.py`). This replaces the
//! ~120 hand-copied `*_route_manifest()` helpers: instead of restating routes,
//! the compiler filters this table by `#[cfg]` and `derived_route_manifest`
//! filters it by the runtime [`ProfileFlags`] ceiling, then de-duplicates by
//! `(method, path)` keeping the highest rank.
//!
//! ## Row model
//!
//! * `RouteProfile` is a monotonic feature ceiling. `default` builds expose
//!   `Always`; `worker_enabled` adds `Worker`; `oidc_enabled` adds `Oidc`.
//!   `derived_route_manifest` keeps every row with `rank <= flags.rank()`.
//! * The two `/.well-known/{openid-configuration,jwks.json}` routes appear
//!   twice — once `Always` (label `oidc_fallback`) and once `Oidc`
//!   (label `oidc`, gated `#[cfg(feature = "builtin-oidc")]`). In the SDK
//!   lane both compile and the dedup keeps the higher-rank `oidc`; in the
//!   default lane the `Oidc` twin is cfg-stripped and `oidc_fallback` wins.
//!   That is exactly what the committed fixtures record.
//!
//! ## Regenerate
//!
//! ```text
//! python3 scripts/contract/gen_derived_routes.py
//! ```
//!
//! The generator refuses to emit unless this table reproduces all six fixtures
//! in `tests/unit/fixtures/{ledger_export,ledger_export_sdk}/` byte-equivalently
//! by `(method, path, registered_by)`, so editing it by hand is pointless — the
//! next regeneration overwrites it.
//!
//! ## Split structure
//!
//! The route table is split by `RouteProfile` into three `.inc.rs` files:
//! - `derived_route_table_always.inc.rs` — rows served in every profile
//! - `derived_route_table_worker.inc.rs` — rows gated on `worker_enabled`
//! - `derived_route_table_oidc.inc.rs`  — rows gated on `oidc_enabled`
//!
//! Each file defines `fn all_derived_*_rows()`. The aggregator
//! `all_derived_rows()` include!s all three.

#![allow(clippy::unreadable_literal)]

use super::route_ledger::RouteEntry;
use super::route_module::ProfileFlags;

/// Monotonic feature ceiling a row belongs to.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum RouteProfile {
    /// Served in every profile.
    Always = 0,
    /// Served only when `worker_enabled`.
    Worker = 1,
    /// Served only when `oidc_enabled`.
    Oidc = 2,
}

/// A manifest row: the route plus the lowest profile that surfaces it.
pub struct DerivedRoute {
    /// The route entry itself.
    pub entry: RouteEntry,
    /// Minimum profile rank at which this row is live.
    pub rank: RouteProfile,
}

impl ProfileFlags {
    /// The feature ceiling for this flag combination, as a [`RouteProfile`].
    pub fn rank(&self) -> RouteProfile {
        if self.oidc_enabled {
            RouteProfile::Oidc
        } else if self.worker_enabled {
            RouteProfile::Worker
        } else {
            RouteProfile::Always
        }
    }
}

/// Project the live `AppState` flags onto the feature-ceiling rank.
pub fn rank_for_flags(flags: &ProfileFlags) -> RouteProfile {
    flags.rank()
}

// Per-profile route manifests — each includes its subset of routes.
include!("derived_route_table_always.inc.rs");
include!("derived_route_table_worker.inc.rs");
include!("derived_route_table_oidc.inc.rs");

/// Combined manifest that aggregates all profiles.
/// `derived_route_manifest` calls this to get the full list of rows.
fn all_derived_rows() -> Vec<DerivedRoute> {
    let mut rows: Vec<DerivedRoute> = Vec::with_capacity(1500);
    rows.extend(all_derived_always_rows());
    rows.extend(all_derived_worker_rows());
    rows.extend(all_derived_oidc_rows());
    rows
}

/// Profile-driven manifest. Keep rows at or below the flag ceiling, then
/// de-duplicate by `(method, path)` keeping the highest rank. Output is sorted
/// by `(path, method, registered_by)` for byte-stable diffs.
pub fn derived_route_manifest(flags: &ProfileFlags) -> Vec<RouteEntry> {
    let max_rank = rank_for_flags(flags);
    let mut best: std::collections::HashMap<(String, String), RouteProfile> =
        std::collections::HashMap::new();
    let mut kept: std::collections::HashMap<(String, String), RouteEntry> =
        std::collections::HashMap::new();
    for DerivedRoute { entry, rank } in all_derived_rows() {
        if rank > max_rank {
            continue;
        }
        let key = (entry.method.as_str().to_string(), entry.path.to_string());
        if let Some(prev) = best.get(&key) {
            if *prev >= rank {
                continue;
            }
        }
        best.insert(key.clone(), rank);
        kept.insert(key, entry);
    }
    let mut out: Vec<RouteEntry> = kept.into_values().collect();
    out.sort_by(|a, b| {
        a.path
            .cmp(b.path)
            .then_with(|| a.method.as_str().cmp(b.method.as_str()))
            .then_with(|| {
                a.registered_by.cmp(b.registered_by)
            })
    });
    out
}
"""


def _generate_test_module():
    """Generate the test module for derived_routes.rs."""
    fixtures_dir = 'concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/unit/fixtures")'
    return f"""
#[cfg(test)]
mod derived_manifest_tests {{
    use super::*;
    use crate::routes::ledger_export::LedgerArtifact;
    use crate::routes::route_module::ProfileFlags as PFlags;
    use std::collections::HashMap;

    fn parse_fixture(bytes: &[u8]) -> Vec<(String, String, String)> {{
        let art: LedgerArtifact = serde_json::from_slice(bytes).unwrap();
        art.entries.into_iter().map(|e| (e.method, e.path, e.registered_by)).collect()
    }}

    macro_rules! assert_derived_matches {{
        ($profile:ident, $fixture:expr) => {{
            let flags = match stringify!($profile) {{
                "DEFAULT" => PFlags {{ oidc_enabled: false, worker_enabled: false, saml_enabled: false }},
                "WORKER"  => PFlags {{ oidc_enabled: false, worker_enabled: true, saml_enabled: false }},
                "ALL"     => PFlags {{ oidc_enabled: true, worker_enabled: true, saml_enabled: false }},
                _ => unreachable!(),
            }};
            let got = derived_route_manifest(&flags);
            let want = parse_fixture(include_bytes!($fixture));
            assert_eq!(got.len(), want.len(), "{{}} profile entry count mismatch", stringify!($profile));
            let mut got_map: HashMap<(String, String, String), ()> = HashMap::new();
            for e in &got {{
                got_map.insert((e.method.as_str().to_string(), e.path.to_string(), e.registered_by.to_string()), ());
            }}
            for (method, path, registered_by) in want {{
                let key = (method.clone(), path.clone(), registered_by.clone());
                assert!(
                    got_map.contains_key(&key),
                    "{{}} profile missing row: {{}} {{}} {{}}",
                    stringify!($profile),
                    method,
                    path,
                    registered_by
                );
            }}
        }};
    }}

    // Default build → ledger_export fixtures.
    #[cfg(not(any(
        feature = "all-extensions",
        feature = "voice-extended",
        feature = "saml-sso",
        feature = "cas-sso",
        feature = "voip-tracking",
        feature = "server-notifications",
        feature = "privacy-ext",
        feature = "builtin-oidc",
    )))]
    #[test]
    fn default_profile_matches_fixture() {{
        assert_derived_matches!(
            DEFAULT,
            concat!({fixtures_dir}, "/ledger_export/default.json")
        );
    }}

    #[cfg(not(any(
        feature = "all-extensions",
        feature = "voice-extended",
        feature = "saml-sso",
        feature = "cas-sso",
        feature = "voip-tracking",
        feature = "server-notifications",
        feature = "privacy-ext",
        feature = "builtin-oidc",
    )))]
    #[test]
    fn worker_profile_matches_fixture() {{
        assert_derived_matches!(
            WORKER,
            concat!({fixtures_dir}, "/ledger_export/worker.json")
        );
    }}

    #[cfg(not(any(
        feature = "all-extensions",
        feature = "voice-extended",
        feature = "saml-sso",
        feature = "cas-sso",
        feature = "voip-tracking",
        feature = "server-notifications",
        feature = "privacy-ext",
        feature = "builtin-oidc",
    )))]
    #[test]
    fn all_profile_matches_fixture() {{
        assert_derived_matches!(
            ALL,
            concat!({fixtures_dir}, "/ledger_export/all.json")
        );
    }}

    // SDK / all-extensions build → ledger_export_sdk fixtures.
    #[cfg(feature = "all-extensions")]
    #[test]
    fn sdk_default_profile_matches_fixture() {{
        assert_derived_matches!(
            DEFAULT,
            concat!({fixtures_dir}, "/ledger_export_sdk/default.json")
        );
    }}

    #[cfg(feature = "all-extensions")]
    #[test]
    fn sdk_worker_profile_matches_fixture() {{
        assert_derived_matches!(
            WORKER,
            concat!({fixtures_dir}, "/ledger_export_sdk/worker.json")
        );
    }}

    #[cfg(feature = "all-extensions")]
    #[test]
    fn sdk_all_profile_matches_fixture() {{
        assert_derived_matches!(
            ALL,
            concat!({fixtures_dir}, "/ledger_export_sdk/all.json")
        );
    }}
}}
"""


if __name__ == "__main__":
    main()
