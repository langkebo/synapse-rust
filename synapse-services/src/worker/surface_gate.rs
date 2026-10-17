//! The worker HTTP surface, made executable (W-F).
//!
//! `WorkerType::owned_route_prefixes` has always been metadata: nothing on the
//! request path consulted it, so any instance served *every* route and the answer
//! to "who owns this prefix" in `GET /_synapse/worker/v1/topology` could not be
//! checked, let alone enforced. This module turns that table into a predicate the
//! HTTP layer can apply.
//!
//! Three deliberate rules:
//!
//! * **`master` owns everything.** The gate is never installed for the master
//!   role, so single-process deployments are byte-for-byte unchanged; the
//!   predicate still answers `true` for it, so a caller cannot accidentally
//!   create a half-open surface by asking the question in the other direction.
//! * **A few paths are instance-agnostic** ([`INSTANCE_AGNOSTIC_PATHS`]). Every
//!   instance behind a load balancer can be asked for `/health`, `/versions` or
//!   `/.well-known/matrix/client`; answering 404 there would look like an outage
//!   and would make a health check pull a perfectly healthy worker out of the
//!   pool. The list is deliberately short and explicit rather than "anything that
//!   smells like infrastructure".
//! * **`*` matches one or more segments.** That is what makes
//!   `/_matrix/client/*/sync` cover `/_matrix/client/v3/sync`,
//!   `/_matrix/client/v3/rooms/{room_id}/sync` and the sliding-sync spellings
//!   (`/_matrix/client/unstable/org.matrix.msc3575/sync`) from a single entry —
//!   the shapes a single-segment wildcard would silently drop, turning a
//!   correctly routed request into a 404.
//!
//! Two entries in the table are **load-bearing in a way their names do not
//! suggest**, which is why they are called out here rather than in the enum:
//!
//! * `frontend` owns `/_matrix/vendor/v1/*` as well as `/_matrix/client/*`. The
//!   vendor prefix is where this server's client-facing extensions live (185
//!   routes at the time of writing, including `rooms/{id}/sync`), so a "client
//!   reader" with only the client prefix would 404 a third of the client API.
//! * `synchrotron` also owns `/_matrix/vendor/v1/rooms/*/sync`: the room-scoped
//!   sync route is registered under the vendor prefix, not under
//!   `/_matrix/client/*`.
//!
//! Both were found by comparing the table against the extracted route ledger;
//! `tests/unit/worker_surface_gate_tests.rs` keeps that comparison running.

use crate::worker::types::WorkerType;

/// Paths every instance answers regardless of its role.
///
/// Kept short on purpose: each entry is a promise that a 404 there would be a
/// false alarm for an operator or a client, not a routing mistake.
pub const INSTANCE_AGNOSTIC_PATHS: &[&str] = &[
    // Liveness/readiness for the orchestrator and the reverse proxy.
    "/health",
    "/_health",
    "/metrics",
    // Capability discovery: clients ask this before anything else, and the
    // answer is identical on every instance.
    "/_matrix/client/versions",
    "/_matrix/client/v3/versions",
    "/_matrix/server_version",
    // Well-known documents are served by whichever instance the LB picks.
    "/.well-known/matrix/server",
    "/.well-known/matrix/client",
    "/.well-known/matrix/support",
];

/// Does `role` own `path`?
///
/// `path` may carry a query string and a trailing slash; both are ignored.
pub fn role_owns_path(role: WorkerType, path: &str) -> bool {
    // Never narrow the master: its prefixes list is not meant to describe the
    // whole surface (it has no entry for `/health`, for instance).
    if role == WorkerType::Master {
        return true;
    }

    let path = normalize(path);
    if INSTANCE_AGNOSTIC_PATHS.contains(&path.as_str()) {
        return true;
    }

    role.owned_route_prefixes().iter().any(|pattern| prefix_matches(pattern, &path))
}

/// Does `pattern` cover `path`?
///
/// `*` matches one or more whole segments; every other segment must be equal.
pub fn prefix_matches(pattern: &str, path: &str) -> bool {
    let pattern: Vec<&str> = segments(pattern).collect();
    let path: Vec<&str> = segments(path).collect();
    matches_segments(&pattern, &path)
}

/// Strip the query string and any trailing slash so `/a/b/` and `/a/b` agree.
fn normalize(path: &str) -> String {
    let without_query = path.split('?').next().unwrap_or(path);
    let trimmed = without_query.trim_end_matches('/');
    if trimmed.is_empty() {
        "/".to_string()
    } else {
        trimmed.to_string()
    }
}

/// The non-empty segments of a path.
fn segments(path: &str) -> impl Iterator<Item = &str> {
    path.split('/').filter(|segment| !segment.is_empty())
}

fn matches_segments(pattern: &[&str], path: &[&str]) -> bool {
    match (pattern.split_first(), path.first()) {
        // A wildcard consumes one or more segments; try every split and let the
        // remainder decide, so `/_matrix/client/*/sync` also matches
        // `/_matrix/client/v3/rooms/{room_id}/sync`.
        (Some((&"*", rest)), Some(_)) => (1..=path.len()).any(|taken| matches_segments(rest, &path[taken..])),
        (Some((&"*", _)), None) => false,
        (Some((expected, rest)), Some(actual)) => expected == actual && matches_segments(rest, &path[1..]),
        (None, None) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owns(role: WorkerType, path: &str) -> bool {
        role_owns_path(role, path)
    }

    #[test]
    fn a_wildcard_covers_one_or_more_segments() {
        assert!(prefix_matches("/_matrix/client/*/sync", "/_matrix/client/v3/sync"));
        assert!(
            prefix_matches("/_matrix/client/*/sync", "/_matrix/client/v3/rooms/!r:localhost/sync"),
            "the wildcard must be able to span several segments, or room sync is lost"
        );
        assert!(
            prefix_matches("/_matrix/client/*/sync", "/_matrix/client/unstable/org.matrix.msc3575/sync"),
            "sliding sync is `client/unstable/<msc>/sync`"
        );
        assert!(!prefix_matches("/_matrix/client/*/sync", "/_matrix/client/v3/sync/extra"));
        assert!(!prefix_matches("/_matrix/client/*/sync", "/_matrix/client/sync"));
    }

    #[test]
    fn literal_segments_must_match_exactly() {
        assert!(prefix_matches("/_matrix/media/*", "/_matrix/media/v3/download/a/b"));
        assert!(!prefix_matches("/_matrix/media/*", "/_matrix/federation/v1/send/1"));
        assert!(!prefix_matches("/_matrix/client/*/sync", "/_matrix/client/v3/rooms/!r/send/m.room.message/1"));
    }

    #[test]
    fn the_prefix_table_matches_the_roles_it_claims() {
        // `frontend` is the catch-all client reader.
        assert!(owns(WorkerType::Frontend, "/_matrix/client/v3/rooms/!r/send/m.room.message/1"));
        assert!(owns(WorkerType::Frontend, "/_matrix/client/v3/sync"));
        assert!(!owns(WorkerType::Frontend, "/_matrix/federation/v1/send/1"));
        assert!(!owns(WorkerType::Frontend, "/_synapse/admin/v1/users"));

        // `event_persister` only answers the replication control plane.
        assert!(owns(WorkerType::EventPersister, "/_synapse/worker/v1/replication/events"));
        assert!(!owns(WorkerType::EventPersister, "/_matrix/client/v3/sync"));

        assert!(owns(WorkerType::FederationReader, "/_matrix/federation/v1/event/!e"));
        assert!(!owns(WorkerType::FederationReader, "/_matrix/client/v3/sync"));

        assert!(owns(WorkerType::MediaRepository, "/_matrix/media/v3/download/a"));
        assert!(!owns(WorkerType::MediaRepository, "/_matrix/client/v3/sync"));

        assert!(owns(WorkerType::AppService, "/_matrix/app/v1/transactions/1"));
        assert!(!owns(WorkerType::AppService, "/_matrix/client/v3/sync"));

        // The background worker owns the worker control plane, not the client API.
        assert!(owns(WorkerType::Background, "/_synapse/worker/v1/workers/w1/heartbeat"));
        assert!(!owns(WorkerType::Background, "/_matrix/client/v3/rooms/!r/send/m.room.message/1"));
    }

    /// Every sync shape the router actually registers must be owned by
    /// `synchrotron` — this is the list a single-segment wildcard would break.
    ///
    /// The paths mirror `synapse-web/src/routes/sync.rs` and
    /// `synapse-web/src/routes/sliding_sync.rs`; if a new sync route is added
    /// there, add it here too (the gate will 404 it otherwise).
    #[test]
    fn synchrotron_owns_every_sync_route_shape() {
        let sync_paths = [
            "/_matrix/client/v3/sync",
            "/_matrix/client/v1/sync",
            "/_matrix/client/v4/sync",
            "/_matrix/client/v3/rooms/!r:localhost/sync",
            "/_matrix/client/unstable/org.matrix.msc3575/sync",
            "/_matrix/client/unstable/org.matrix.simplified_msc3575/sync",
        ];
        for path in sync_paths {
            assert!(owns(WorkerType::Synchrotron, path), "synchrotron must own {path}");
        }

        assert!(
            !owns(WorkerType::Synchrotron, "/_matrix/client/v3/rooms/!r/send/m.room.message/1"),
            "owning sync must not imply owning sends"
        );
        assert!(!owns(WorkerType::Synchrotron, "/_matrix/media/v3/download/a"));
    }

    #[test]
    fn a_role_with_no_prefixes_owns_only_the_agnostic_paths() {
        for role in [WorkerType::FederationSender, WorkerType::Pusher] {
            for path in INSTANCE_AGNOSTIC_PATHS {
                assert!(owns(role, path), "{role:?} must still answer {path}");
            }
            assert!(!owns(role, "/_matrix/client/v3/sync"));
            assert!(!owns(role, "/_matrix/federation/v1/send/1"));
        }
    }

    #[test]
    fn master_owns_everything() {
        for path in ["/", "/health", "/_matrix/client/v3/sync", "/anything/else"] {
            assert!(owns(WorkerType::Master, path), "master must keep the whole surface ({path})");
        }
    }

    #[test]
    fn trailing_slash_and_query_string_do_not_change_ownership() {
        assert!(owns(WorkerType::Frontend, "/_matrix/client/v3/sync/"));
        assert!(owns(WorkerType::Frontend, "/_matrix/client/v3/sync?since=s1"));
        assert!(owns(WorkerType::Frontend, "/health?x=1"));
    }

    #[test]
    fn every_declared_prefix_is_a_narrowing_pattern() {
        // A bare `*` or an empty pattern would make a role own the whole surface
        // by accident; catch that at test time instead of in production.
        for role in [
            WorkerType::Frontend,
            WorkerType::Background,
            WorkerType::EventPersister,
            WorkerType::Synchrotron,
            WorkerType::FederationReader,
            WorkerType::MediaRepository,
            WorkerType::AppService,
        ] {
            for pattern in role.owned_route_prefixes() {
                let pattern: &str = pattern;
                assert!(pattern.starts_with('/'), "{role:?}: prefix must be absolute ({pattern})");
                assert!(pattern != "*" && pattern != "/*", "{role:?}: `{pattern}` would own everything");
            }
        }
    }
}
