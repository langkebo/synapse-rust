//! L-3 guards: the explicit MSC4140 delayed-events path set must stay pinned to
//! the real registered route table.
//!
//! Background: worker route ownership used to be expressed only as coarse
//! prefixes (`WorkerType::owned_route_prefixes()`, whose `/_matrix/client/*`
//! entry is far too broad for this family — it also admits `/login`,
//! `/register`, admin and stream-writer endpoints). `DELAYED_EVENTS_WORKER_PATHS`
//! is the explicit path set for the family; these two guards pin it to a real
//! source instead of a second hand-copied list.
//!
//! * Guard A — [`delayed_events_worker_paths_are_registered_routes`]: every entry
//!   of the set must be an actually registered route (catches typos / drift).
//! * Guard B — [`registered_delayed_events_routes_are_all_listed`]: every
//!   registered route whose path contains `delayed_events` must be present in the
//!   set (catches a missing entry when a route is added).
//!
//! The truth source is `synapse_web::routes::declared_ledger_all()` — the ledger
//! API backed by `synapse-web/src/routes/derived_route_table_always.inc.rs`,
//! which `scripts/contract/gen_derived_routes.py` extracts from the real
//! `.route(...)` registration sites and validates byte-equivalently against the
//! six checked-in fixtures in `tests/unit/fixtures/ledger_export{,_sdk}/`.
//!
//! # Red proof (AGENTS.md 铁律 8 / R11)
//!
//! Both guards were shown to fail on a deliberately broken set and then reverted:
//!
//! * typo the single entry (`.../delayed_events_typo/{delay_id}`) ⇒ Guard A red;
//! * drop the single entry (`&[]`) ⇒ Guard B red (and Guard A's non-vacuity
//!   assertion fires as well, since an empty set cannot fail Guard A otherwise).

use std::collections::HashSet;

use synapse_services::worker::topology_validator::{may_serve_delayed_events_route, DELAYED_EVENTS_WORKER_PATHS};
use synapse_web::routes::declared_ledger_all;

/// Guard A — no typos / drift.
///
/// Every path in `DELAYED_EVENTS_WORKER_PATHS` must correspond to at least one
/// registered route in the derived ledger, and the runtime gate must admit it.
#[test]
fn delayed_events_worker_paths_are_registered_routes() {
    assert!(
        !DELAYED_EVENTS_WORKER_PATHS.is_empty(),
        "Guard A cannot run vacuously: DELAYED_EVENTS_WORKER_PATHS must contain at least one path"
    );

    let ledger = declared_ledger_all();
    let registered: HashSet<&str> = ledger.iter().map(|entry| entry.path).collect();
    assert!(!registered.is_empty(), "derived route ledger must not be empty");

    for path in DELAYED_EVENTS_WORKER_PATHS {
        assert!(
            registered.contains(path),
            "Guard A failed: DELAYED_EVENTS_WORKER_PATHS entry {path:?} is not a registered route \
             (typo, or the route moved/renamed). The family's real routes live in \
             synapse-web/src/routes/delayed_events.rs and are nested at \
             synapse-web/src/routes/assembly.rs"
        );
        assert!(
            may_serve_delayed_events_route(path),
            "Guard A failed: {path:?} is in DELAYED_EVENTS_WORKER_PATHS but \
             may_serve_delayed_events_route rejected it"
        );
    }
}

/// Guard B — family coverage.
///
/// Every registered route whose path contains `delayed_events` must be present in
/// `DELAYED_EVENTS_WORKER_PATHS`, so a newly added route cannot silently escape
/// the explicit set.
#[test]
fn registered_delayed_events_routes_are_all_listed() {
    let ledger = declared_ledger_all();
    let family: HashSet<&str> =
        ledger.iter().map(|entry| entry.path).filter(|path| path.contains("delayed_events")).collect();

    assert!(
        !family.is_empty(),
        "Guard B cannot run vacuously: the derived route ledger must contain at least one \
         delayed_events route"
    );

    for path in family {
        assert!(
            DELAYED_EVENTS_WORKER_PATHS.contains(&path),
            "Guard B failed: registered delayed_events route {path:?} is missing from \
             DELAYED_EVENTS_WORKER_PATHS (add it to the explicit set in \
             synapse-services/src/worker/topology_validator.rs)"
        );
        assert!(
            may_serve_delayed_events_route(path),
            "Guard B failed: registered delayed_events route {path:?} is rejected by \
             may_serve_delayed_events_route"
        );
    }
}

/// The gate must be an **exact** match, never a `/_matrix/client/*` prefix match.
///
/// This is the negative control for the whole family: a prefix implementation
/// would turn these assertions red, which is exactly the failure mode the
/// explicit set exists to prevent.
#[test]
fn delayed_events_gate_rejects_non_family_paths() {
    for path in [
        "/_matrix/client/v3/login",
        "/_matrix/client/v3/register",
        "/_matrix/client/v3/sync",
        "/_synapse/admin/v1/users",
        "/_synapse/worker/v1/replication/stream",
        // Same family prefix, but not a registered route (no `{delay_id}`).
        "/_matrix/client/unstable/org.matrix.msc4140/delayed_events",
        // Concrete instance instead of the registered path template.
        "/_matrix/client/unstable/org.matrix.msc4140/delayed_events/42",
        // Extra segment past the registered template.
        "/_matrix/client/unstable/org.matrix.msc4140/delayed_events/{delay_id}/extra",
    ] {
        assert!(
            !may_serve_delayed_events_route(path),
            "may_serve_delayed_events_route must match the explicit family paths exactly, \
             but it admitted non-family path {path:?}"
        );
    }
}
