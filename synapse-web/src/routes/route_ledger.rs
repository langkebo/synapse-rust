//! Explicit registry of the HTTP routes the assembled Axum [`axum::Router`] is
//! supposed to expose.
//!
//! ## Why this exists
//!
//! `axum::Router` does not offer a public way to walk the routes it has been
//! configured with. That has historically let silent-merge bugs slip through
//! in this codebase — most notably the `key_backup` regression documented in
//! the SPEC_ALIGNMENT_PLAN doc (see §1.1 and items R4 / O2). Two routers
//! registered overlapping `(method, path)` tuples, `Router::merge` let the
//! first wins through, and the breakage was only visible from an Element-side
//! 405.
//!
//! Upstream Python synapse avoids this class of bug because every REST
//! servlet module registers itself through `register_servlets(hs, http_server)`
//! and the integration harness exercises every documented endpoint. The
//! ledger here is the analogous construct for this codebase: the route table
//! in `derived_routes.rs` declares, verbatim, the `(method, absolute_path)`
//! tuples the `.route(...)` / `.nest(...)` sites register on the live server.
//! It is *generated*, not hand-written — `scripts/contract/extract_registered.py`
//! scrapes the real registration surface and
//! `scripts/contract/gen_derived_routes.py` materialises it, so a hand-copied
//! manifest can no longer drift away from the router it describes.
//! `assembly::declared_ledger_for(&AppState)` filters that table by the runtime
//! [`ProfileFlags`](crate::routes::route_module::ProfileFlags) before
//! `create_router` calls the validate method on this ledger. Duplicates (same
//! method +
//! same path, from any combination of routers) abort startup with a diagnostic
//! that lists every offending entry. The final count is logged as
//! `route manifest validated: N declared (method, path) tuples, 0 duplicates`,
//! satisfying the [§6 verification] requirement.
//!
//! The manifest is *also* the source of truth for the integration tests that
//! PATCH-probe every declared entry against the assembled router and assert a
//! 405 with the expected method in the `Allow` header. That catches the reverse
//! drift — a manifest entry that no longer has a real route behind it.
//!
//! ## Contributor rule
//!
//! Any PR that adds or changes a feature-gated route must update the ledger in
//! the same change. Concretely, that means: register the route (`.route(...)` /
//! `.nest(...)` as usual) and then regenerate the derived table so the ledger
//! picks it up:
//!
//! ```text
//! python3 scripts/contract/gen_derived_routes.py
//! ```
//!
//! `scripts/contract/check_route_contract.sh` fails if the derived table or
//! `docs/synapse-rust/ROUTE_CONTRACT.md` has drifted, so a new route cannot
//! silently stay invisible to the ledger.
//!
//! Annotations the `.route()` source cannot express (`rate_limit_exempt`,
//! `auth`) live in `scripts/contract/ledger_annotations.txt`; `registered_by`
//! labels live in `scripts/contract/ledger_origins.txt`.
//!
//! Do not merge a new runtime/compile-time gated route that is only wired in
//! Axum assembly code. If the route surface changes, the ledger, startup log,
//! and route-ledger snapshot tests must change in the same PR.

use axum::http::Method;
use std::collections::HashMap;
use std::fmt;

/// A single `(method, path)` tuple that some router promises to register.
///
/// `path` is the *absolute* HTTP path as it is reachable from the outside
/// world — including nest prefixes like `/_matrix/client/v3`. Keeping the
/// absolute form (rather than the relative path the router itself uses) is
/// deliberate: the same relative path is typically nested under multiple
/// version prefixes, and the duplicate detector must see those as distinct
/// entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteEntry {
    /// The `method` field.
    pub method: Method,
    /// The `path` field.
    pub path: &'static str,
    /// Human-readable name of the router module that registers this entry —
    /// e.g. `"key_backup"`. Surfaced in duplicate diagnostics so the offending
    /// source files are immediately obvious.
    pub registered_by: &'static str,
    /// Optional query parameters recognized by this endpoint.
    pub query_params: &'static [&'static str],
    /// Optional auth requirement: "user", "admin", "optional", "federation", or "none".
    pub auth: Option<&'static str>,
    /// B-4: When `true`, the IP-level rate limit middleware skips this route.
    /// Used for sync/sliding-sync endpoints that implement their own
    /// per-user+device rate limiting inside the handler to avoid double
    /// limiting. Auto-collected by `create_router` from the route ledger.
    pub rate_limit_exempt: bool,
}

impl RouteEntry {
    /// Creates a new [`RouteEntry`] instance.
    pub const fn new(method: Method, path: &'static str, registered_by: &'static str) -> Self {
        Self { method, path, registered_by, query_params: &[], auth: None, rate_limit_exempt: false }
    }

    /// Builder method to set the [`auth`](Self::auth) field.
    pub const fn with_auth(mut self, auth: &'static str) -> Self {
        self.auth = Some(auth);
        self
    }

    /// Builder method to set query parameter metadata.
    pub const fn with_query_params(mut self, query_params: &'static [&'static str]) -> Self {
        self.query_params = query_params;
        self
    }

    /// B-4: Mark this route as exempt from the IP-level rate limit middleware.
    pub const fn with_rate_limit_exempt(mut self, exempt: bool) -> Self {
        self.rate_limit_exempt = exempt;
        self
    }
}

/// Expand a set of `(Method, relative_path)` tuples across a list of nest
/// prefixes, producing owned [`RouteEntry`] values.
///
/// This is a convenience for the common pattern of nesting the same inner
/// router under `/_matrix/client/v1`, `/_matrix/client/v3`, and
/// `/_matrix/client/v3` — writing out all three copies by hand is noisy and
/// error-prone.
///
/// `paths` entries must start with `/`; `prefixes` entries must NOT end with
/// `/`. The implementation concatenates them verbatim and leaks the result
/// so that [`RouteEntry::path`] can remain `&'static str`. This leak is
/// strictly bounded — it happens once per manifest entry at startup, and the
/// manifest is fixed-size — so it's equivalent to a static allocation.
pub fn expand_under_prefixes(
    registered_by: &'static str,
    prefixes: &[&str],
    paths: &[(Method, &str)],
) -> Vec<RouteEntry> {
    let mut out = Vec::with_capacity(prefixes.len() * paths.len());
    for prefix in prefixes {
        debug_assert!(
            !prefix.ends_with('/'),
            "prefix {prefix:?} must not end with '/' — expand_under_prefixes will add it"
        );
        for (method, relative) in paths {
            debug_assert!(relative.starts_with('/'), "relative path {relative:?} must start with '/'");
            let full: &'static str = Box::leak(format!("{prefix}{relative}").into_boxed_str());
            out.push(RouteEntry::new(method.clone(), full, registered_by));
        }
    }
    out
}

/// Collection of [`RouteEntry`] values assembled from every router manifest.
#[derive(Debug, Default, Clone)]
pub struct RouteLedger {
    entries: Vec<RouteEntry>,
}

/// The `RegisteredByCount` struct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredByCount {
    /// The `registered_by` field.
    pub registered_by: &'static str,
    /// The `entries` field.
    pub entries: usize,
}

impl RouteLedger {
    /// Creates a new [`RouteLedger`] instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a manifest produced by a single router module.
    pub fn extend<I>(&mut self, entries: I)
    where
        I: IntoIterator<Item = RouteEntry>,
    {
        self.entries.extend(entries);
    }

    /// Return an iterator over the route entries.
    pub fn iter(&self) -> impl Iterator<Item = &RouteEntry> {
        self.entries.iter()
    }

    /// Borrow the entries as a slice — for consumers that need `&[RouteEntry]`
    /// (e.g. `collect::<Vec<_>>()`-free scanning in tests).
    pub fn as_slice(&self) -> &[RouteEntry] {
        &self.entries
    }

    /// Return the number of route entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Return whether the ledger has no route entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Aggregate declared routes by their `registered_by` namespace.
    ///
    /// The result is sorted by descending entry count, then namespace name, so
    /// startup logs and snapshots remain stable across runs.
    pub fn registered_by_counts(&self) -> Vec<RegisteredByCount> {
        let mut counts: HashMap<&'static str, usize> = HashMap::new();
        for entry in &self.entries {
            *counts.entry(entry.registered_by).or_insert(0) += 1;
        }

        let mut out: Vec<RegisteredByCount> =
            counts.into_iter().map(|(registered_by, entries)| RegisteredByCount { registered_by, entries }).collect();
        out.sort_by(|a, b| b.entries.cmp(&a.entries).then_with(|| a.registered_by.cmp(b.registered_by)));
        out
    }

    /// Verify that every `(method, path)` tuple appears at most once across
    /// all manifests.
    ///
    /// Two identical entries from the same module are still a duplicate —
    /// the router would panic anyway when handed two `.route(path, ...)` calls
    /// with overlapping methods, so we would rather fail cleanly at manifest
    /// validation than leave the panic to Axum's internals.
    pub fn validate(&self) -> Result<LedgerReport, DuplicateRouteError> {
        // `Method` doesn't implement `Ord`, so a `BTreeMap` is out — but its
        // `Hash + Eq` impls let us key a `HashMap` directly. We then sort the
        // duplicate report by `(method, path)` for stable diagnostics.
        let mut seen: HashMap<(Method, &'static str), Vec<&'static str>> = HashMap::new();
        for entry in &self.entries {
            seen.entry((entry.method.clone(), entry.path)).or_default().push(entry.registered_by);
        }

        let mut duplicates: Vec<DuplicateEntry> = seen
            .iter()
            .filter(|(_, owners)| owners.len() > 1)
            .map(|((method, path), owners)| DuplicateEntry {
                method: method.clone(),
                path,
                registered_by: owners.clone(),
            })
            .collect();
        duplicates.sort_by(|a, b| a.method.as_str().cmp(b.method.as_str()).then_with(|| a.path.cmp(b.path)));

        if !duplicates.is_empty() {
            return Err(DuplicateRouteError { duplicates });
        }

        Ok(LedgerReport { unique_tuples: seen.len(), total_entries: self.entries.len() })
    }
}

/// Summary of a successful RouteLedger::validate call.
#[derive(Debug, Clone, Copy)]
pub struct LedgerReport {
    /// Number of distinct `(method, path)` tuples — equal to the total
    /// manifest length when validation succeeds.
    pub unique_tuples: usize,
    /// Raw number of [`RouteEntry`] values across all manifests. Kept
    /// separately for future use; today it matches `unique_tuples` exactly.
    pub total_entries: usize,
}

/// The `DuplicateEntry` struct.
#[derive(Debug, Clone)]
pub struct DuplicateEntry {
    /// The `method` field.
    pub method: Method,
    /// The `path` field.
    pub path: &'static str,
    /// The `registered_by` field.
    pub registered_by: Vec<&'static str>,
}

/// The `DuplicateRouteError` struct.
#[derive(Debug, Clone)]
pub struct DuplicateRouteError {
    /// The `duplicates` field.
    pub duplicates: Vec<DuplicateEntry>,
}

impl fmt::Display for DuplicateRouteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "route manifest contains {} duplicate (method, path) tuple(s):", self.duplicates.len())?;
        for dup in &self.duplicates {
            writeln!(f, "  - {} {} registered by: {}", dup.method, dup.path, dup.registered_by.join(", "))?;
        }
        Ok(())
    }
}

impl std::error::Error for DuplicateRouteError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_accepts_non_overlapping_entries() {
        let mut ledger = RouteLedger::new();
        ledger.extend([
            RouteEntry::new(Method::GET, "/a", "m1"),
            RouteEntry::new(Method::POST, "/a", "m1"),
            RouteEntry::new(Method::GET, "/b", "m2"),
        ]);
        let report = ledger.validate().expect("no duplicates");
        assert_eq!(report.unique_tuples, 3);
    }

    #[test]
    fn validate_flags_cross_module_duplicates() {
        let mut ledger = RouteLedger::new();
        ledger
            .extend([RouteEntry::new(Method::GET, "/clash", "mod_a"), RouteEntry::new(Method::GET, "/clash", "mod_b")]);
        let err = ledger.validate().expect_err("duplicate must be reported");
        assert_eq!(err.duplicates.len(), 1);
        assert_eq!(err.duplicates[0].method, Method::GET);
        assert_eq!(err.duplicates[0].path, "/clash");
        assert_eq!(err.duplicates[0].registered_by, vec!["mod_a", "mod_b"]);
        let rendered = err.to_string();
        assert!(rendered.contains("mod_a"));
        assert!(rendered.contains("mod_b"));
        assert!(rendered.contains("/clash"));
    }

    #[test]
    fn expand_under_prefixes_produces_full_paths() {
        let entries =
            expand_under_prefixes("demo", &["/api/v1", "/api/v3"], &[(Method::GET, "/foo"), (Method::PUT, "/foo")]);
        let paths: Vec<_> = entries.iter().map(|e| e.path).collect();
        assert_eq!(paths, vec!["/api/v1/foo", "/api/v1/foo", "/api/v3/foo", "/api/v3/foo"]);
        assert!(entries.iter().all(|e| e.registered_by == "demo"));
    }

    #[test]
    fn registered_by_counts_are_sorted_and_grouped() {
        let mut ledger = RouteLedger::new();
        ledger.extend([
            RouteEntry::new(Method::GET, "/a", "mod_b"),
            RouteEntry::new(Method::POST, "/b", "mod_a"),
            RouteEntry::new(Method::GET, "/c", "mod_b"),
            RouteEntry::new(Method::PUT, "/d", "mod_c"),
            RouteEntry::new(Method::DELETE, "/e", "mod_a"),
        ]);

        let counts = ledger.registered_by_counts();
        assert_eq!(
            counts,
            vec![
                RegisteredByCount { registered_by: "mod_a", entries: 2 },
                RegisteredByCount { registered_by: "mod_b", entries: 2 },
                RegisteredByCount { registered_by: "mod_c", entries: 1 },
            ]
        );
    }

    // ------------------------------------------------------------------
    // B-4: Rate limit exemption auto-derivation tests
    // ------------------------------------------------------------------

    #[test]
    fn rate_limit_exempt_defaults_to_false() {
        let entry = RouteEntry::new(Method::GET, "/foo", "m1");
        assert!(!entry.rate_limit_exempt);
    }

    #[test]
    fn with_rate_limit_exempt_sets_flag() {
        let entry = RouteEntry::new(Method::GET, "/foo", "m1").with_rate_limit_exempt(true);
        assert!(entry.rate_limit_exempt);

        let entry = RouteEntry::new(Method::GET, "/foo", "m1").with_rate_limit_exempt(false);
        assert!(!entry.rate_limit_exempt);
    }

    #[test]
    fn collect_exempt_paths_from_ledger() {
        // B-4: Verify that only entries marked `rate_limit_exempt = true`
        // appear in the collected list, mirroring what `create_router` does.
        let mut ledger = RouteLedger::new();
        ledger.extend([
            RouteEntry::new(Method::GET, "/sync", "sync").with_rate_limit_exempt(true),
            RouteEntry::new(Method::GET, "/events", "sync"),
            RouteEntry::new(Method::POST, "/v1/sync", "sliding_sync").with_rate_limit_exempt(true),
            RouteEntry::new(Method::GET, "/rooms", "room"),
        ]);

        let exempt_paths: Vec<&'static str> = ledger.iter().filter(|e| e.rate_limit_exempt).map(|e| e.path).collect();

        assert_eq!(exempt_paths, vec!["/sync", "/v1/sync"]);
        // Non-exempt routes must NOT appear
        assert!(!exempt_paths.contains(&"/events"));
        assert!(!exempt_paths.contains(&"/rooms"));
    }

    #[test]
    fn collect_exempt_paths_from_real_manifests() {
        // B-4: Verify that the sync and sliding_sync routes produce the
        // expected exempt paths — the same 6 paths that were previously
        // hardcoded in `is_sync_rate_limit_exempt_path`. Sourced from the
        // derived route table rather than per-module manifests.
        let all_entries = crate::routes::assembly::declared_ledger_all();

        let exempt_paths: Vec<&str> = all_entries.iter().filter(|e| e.rate_limit_exempt).map(|e| e.path).collect();

        // All 6 sync/sliding-sync paths must be exempt
        let expected = [
            "/_matrix/client/v3/sync",
            "/_matrix/client/v3/sync",
            "/_matrix/client/v1/sync",
            "/_matrix/client/v4/sync",
            "/_matrix/client/unstable/org.matrix.msc3575/sync",
            "/_matrix/client/unstable/org.matrix.simplified_msc3575/sync",
        ];
        for path in &expected {
            assert!(
                exempt_paths.contains(path),
                "expected {path} to be rate-limit-exempt but it was not found in {:?}",
                exempt_paths
            );
        }

        // Non-sync routes must NOT be exempt
        let non_exempt: Vec<&str> = all_entries.iter().filter(|e| !e.rate_limit_exempt).map(|e| e.path).collect();
        assert!(non_exempt.contains(&"/_matrix/client/v3/events"), "events should not be exempt");
        assert!(non_exempt.contains(&"/_matrix/client/v3/joined_rooms"), "joined_rooms should not be exempt");
    }
}
