//! W-F guards: the worker HTTP surface gate must agree with the **real** route
//! table.
//!
//! `worker.surface_gate` turns `WorkerType::owned_route_prefixes` into an
//! enforced surface. That converts a harmless-looking table into a routing
//! contract: every missing entry is a 404 behind the load balancer, and a 404 on
//! `/sync` looks exactly like an outage. Hand-maintained prefix lists and
//! machine-extracted route tables drift apart, so these tests compare the two.
//!
//! The truth source is [`declared_ledger_all`] — the same derived ledger the
//! route-contract gate validates (`scripts/contract/gen_derived_routes.py`),
//! not a second hand-copied list.
//!
//! What the guards already caught (2026-10-17, when the gate was first written):
//!
//! * `frontend` claimed only `/_matrix/client/*`, but **185** client-facing
//!   routes live under `/_matrix/vendor/v1/*` ⇒ a client reader would have 404'd
//!   a third of the client API;
//! * `synchrotron` claimed `/_matrix/client/*/sync`, but the room-scoped sync
//!   route is registered as `/_matrix/vendor/v1/rooms/{room_id}/sync` ⇒ room sync
//!   would have 404'd.

use synapse_services::worker::surface_gate::role_owns_path;
use synapse_services::worker::types::WorkerType;
use synapse_web::routes::declared_ledger_all;

/// Turn a route template (`/_matrix/vendor/v1/rooms/{room_id}/sync`) into a
/// request path (`/_matrix/vendor/v1/rooms/x/sync`).
///
/// The gate compares against what a client actually sends; a `{param}` segment
/// still occupies one segment, so substituting a literal keeps the comparison
/// honest without teaching the gate about templates.
fn as_request_path(template: &str) -> String {
    template
        .split('/')
        .map(|segment| if segment.starts_with('{') && segment.ends_with('}') { "x" } else { segment })
        .collect::<Vec<_>>()
        .join("/")
}

fn ledger_paths() -> Vec<String> {
    let ledger = declared_ledger_all();
    let paths: Vec<String> = ledger.iter().map(|entry| entry.path.to_string()).collect();
    assert!(paths.len() > 500, "the derived route ledger must not look empty ({})", paths.len());
    paths
}

fn unowned_by(role: WorkerType, predicate: impl Fn(&str) -> bool) -> Vec<String> {
    ledger_paths()
        .into_iter()
        .filter(|path| predicate(path))
        .filter(|path| !role_owns_path(role, &as_request_path(path)))
        .collect()
}

#[test]
fn frontend_owns_the_entire_client_surface() {
    let missing = unowned_by(WorkerType::Frontend, |path| {
        path.starts_with("/_matrix/client/") || path.starts_with("/_matrix/vendor/v1/")
    });

    assert!(
        missing.is_empty(),
        "these client-facing routes are NOT owned by `frontend` ⇒ a client_reader behind the \
         load balancer would answer 404 for them. Add the missing prefix to \
         `WorkerType::owned_route_prefixes` (or, if the route is not client-facing, move it out of \
         these namespaces):\n  {}",
        missing.join("\n  ")
    );
}

#[test]
fn synchrotron_owns_every_sync_route() {
    // `…/summary/sync` is a room-summary refresh (POST), not the client sync loop;
    // it belongs to whichever instance serves the room-summary surface.
    let missing = unowned_by(WorkerType::Synchrotron, |path| path.ends_with("/sync") && !path.contains("/summary/"));

    assert!(
        missing.is_empty(),
        "these sync routes are NOT owned by `synchrotron`; pointing the proxy's `/sync` rule at a \
         synchrotron would 404 them:\n  {}",
        missing.join("\n  ")
    );
}

#[test]
fn media_repository_and_federation_reader_own_their_namespaces() {
    let media = unowned_by(WorkerType::MediaRepository, |path| path.starts_with("/_matrix/media/"));
    assert!(media.is_empty(), "media routes not owned by `media_repository`:\n  {}", media.join("\n  "));

    // Keys are how a remote server authenticates us: a federation reader that
    // cannot serve `/_matrix/key/*` makes every inbound request from it fail.
    let federation = unowned_by(WorkerType::FederationReader, |path| {
        path.starts_with("/_matrix/federation/") || path.starts_with("/_matrix/key/")
    });
    assert!(
        federation.is_empty(),
        "federation/key routes not owned by `federation_reader`:\n  {}",
        federation.join("\n  ")
    );
}

/// The admin surface must stay master-only: widening a data-plane role to the
/// admin API would let a proxy mistake expose it.
#[test]
fn no_non_master_role_owns_the_admin_surface() {
    for role in [
        WorkerType::Frontend,
        WorkerType::Background,
        WorkerType::EventPersister,
        WorkerType::Synchrotron,
        WorkerType::FederationSender,
        WorkerType::FederationReader,
        WorkerType::MediaRepository,
        WorkerType::Pusher,
        WorkerType::AppService,
    ] {
        for path in ["/_synapse/admin/v1/users", "/_synapse/admin/v1/rooms/x/delete"] {
            assert!(!role_owns_path(role, path), "{role:?} must not own the admin surface ({path})");
        }
    }
}

/// A path that does not exist must be refused for every role but master — the
/// gate is a *narrowing*, so "not in the table" can never become "allowed".
#[test]
fn unknown_paths_are_refused_for_every_non_master_role() {
    for role in [
        WorkerType::Frontend,
        WorkerType::Background,
        WorkerType::EventPersister,
        WorkerType::Synchrotron,
        WorkerType::FederationSender,
        WorkerType::FederationReader,
        WorkerType::MediaRepository,
        WorkerType::Pusher,
        WorkerType::AppService,
    ] {
        assert!(!role_owns_path(role, "/definitely/not/a/route"));
    }
    assert!(role_owns_path(WorkerType::Master, "/definitely/not/a/route"));
}
