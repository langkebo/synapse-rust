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
//!   registered route whose path contains `delayed_event` must be present in the
//!   set (catches a missing entry when a route is added).
//!
//! ⚠️ Guard B matches the **singular stem** `delayed_event` on purpose. The
//! family spells its two halves differently — the scheduling endpoint is
//! `…/rooms/{room_id}/delayed_event/{event_type}/{txn_id}` (singular, upstream
//! MSC4140's own spelling) while the management endpoints are
//! `…/delayed_events/{delay_id}` (plural) — so a `contains("delayed_events")`
//! scan skips the scheduling route entirely. That is exactly what happened when
//! `PUT …/rooms/{room_id}/delayed_event/…` was added: the old filter could not
//! see it, and the entry escaped the set. Matching the stem covers both
//! spellings, and the missing entry was then proven to fail (see red proof).
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
//!   assertion fires as well, since an empty set cannot fail Guard A otherwise);
//! * drop **only** the singular scheduling entry, leaving the plural one ⇒
//!   Guard B red with that one path named — which is also the red proof that the
//!   singular-stem filter is load-bearing rather than decorative.

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
/// Every registered route whose path contains `delayed_event` must be present in
/// `DELAYED_EVENTS_WORKER_PATHS`, so a newly added route cannot silently escape
/// the explicit set.
///
/// The stem is the **singular** `delayed_event`, which also matches the plural
/// `delayed_events` — the family uses both spellings and only the stem covers
/// them.
#[test]
fn registered_delayed_events_routes_are_all_listed() {
    let ledger = declared_ledger_all();
    let family: HashSet<&str> =
        ledger.iter().map(|entry| entry.path).filter(|path| path.contains("delayed_event")).collect();

    assert!(
        !family.is_empty(),
        "Guard B cannot run vacuously: the derived route ledger must contain at least one \
         delayed-events route"
    );
    assert!(
        family.len() >= 2,
        "both halves of the family must be in the ledger (the scheduling route is singular, the \
         management route plural); got {family:?}"
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
        // Same collection-ish shape, wrong namespace: the family lives under
        // `/_matrix/client/unstable/org.matrix.msc4140/`, never under `v3`.
        // (This replaced `…/msc4140/delayed_events`, which became a **real**
        // route on 2026-10-14 — the list endpoint, §11.7.1 — and therefore can no
        // longer serve as the "prefix-but-no-route" negative control.)
        "/_matrix/client/v3/delayed_events",
        // Singular scheduling path with a segment missing (the registered
        // template has `{event_type}/{txn_id}` after `delayed_event`).
        "/_matrix/client/unstable/org.matrix.msc4140/rooms/{room_id}/delayed_event/{event_type}",
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
