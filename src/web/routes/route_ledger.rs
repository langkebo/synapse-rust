//! Explicit registry of the HTTP routes the assembled Axum [`Router`] is
//! supposed to expose.
//!
//! ## Why this exists
//!
//! `axum::Router` does not offer a public way to walk the routes it has been
//! configured with. That has historically let silent-merge bugs slip through
//! in this codebase — most notably the `key_backup` regression documented
//! in [`docs/synapse-rust/SPEC_ALIGNMENT_PLAN_2026-05-01.md`] (see §1.1 and
//! items R4 / O2). Two routers registered overlapping `(method, path)`
//! tuples, `Router::merge` let the first wins through, and the breakage was
//! only visible from an Element-side 405.
//!
//! Upstream Python synapse avoids this class of bug because every REST
//! servlet module registers itself through `register_servlets(hs, http_server)`
//! and the integration harness exercises every documented endpoint. The
//! ledger here is the analogous construct for this codebase: each router
//! module exports a `*_manifest()` function that declares, verbatim, the
//! `(method, absolute_path)` tuples it will register on the live server.
//! `assembly::declared_route_manifest_for(&AppState)` combines the always-on
//! manifests with state-aware route modules before `create_router` calls
//! [`RouteLedger::validate`]. Duplicates (same method + same path, from any
//! combination of routers) abort startup with a diagnostic that lists every
//! offending entry. The final count is logged as `route manifest validated:
//! N declared (method, path) tuples, 0 duplicates`, satisfying the
//! [§6 verification] requirement.
//!
//! The manifest is *also* the source of truth for
//! [`tests/integration/api_route_ledger_tests.rs`]: that test PATCH-probes
//! every declared entry against the assembled router and asserts a 405 with
//! the expected method in the `Allow` header. That catches the reverse
//! drift — a manifest entry that no longer has a real route behind it.
//!
//! ## Contributor rule
//!
//! Any PR that adds or changes a feature-gated route must update the ledger in
//! the same change. Concretely, that means either:
//!
//! - wire the feature through [`crate::web::routes::route_module::RouteModule`]
//!   and return the new entries from `manifest_for(&AppState)`, or
//! - extend the owning module's explicit `*_route_manifest()` /
//!   `assembly_compat_manifest()` output when the route is intentionally kept
//!   outside `route_module`.
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
    pub method: Method,
    pub path: &'static str,
    /// Human-readable name of the router module that registers this entry —
    /// e.g. `"key_backup"`. Surfaced in duplicate diagnostics so the offending
    /// source files are immediately obvious.
    pub registered_by: &'static str,
    /// Optional query parameters recognized by this endpoint.
    pub query_params: &'static [&'static str],
    /// Optional auth requirement: "user", "admin", "optional", "federation", or "none".
    pub auth: Option<&'static str>,
}

impl RouteEntry {
    pub const fn new(method: Method, path: &'static str, registered_by: &'static str) -> Self {
        Self { method, path, registered_by, query_params: &[], auth: None }
    }

    pub const fn with_auth(mut self, auth: &'static str) -> Self {
        self.auth = Some(auth);
        self
    }

    pub const fn with_query_params(mut self, query_params: &'static [&'static str]) -> Self {
        self.query_params = query_params;
        self
    }
}

/// Expand a set of `(Method, relative_path)` tuples across a list of nest
/// prefixes, producing owned [`RouteEntry`] values.
///
/// This is a convenience for the common pattern of nesting the same inner
/// router under `/_matrix/client/v1`, `/_matrix/client/r0`, and
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredByCount {
    pub registered_by: &'static str,
    pub entries: usize,
}

impl RouteLedger {
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

    pub fn iter(&self) -> impl Iterator<Item = &RouteEntry> {
        self.entries.iter()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

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
        // Emit a deprecation warning when any r0 routes remain registered.
        // r0 is a legacy Matrix API version — clients should migrate to /v3/.
        let r0_count = self.entries.iter().filter(|e| e.path.contains("/r0/")).count();
        if r0_count > 0 {
            ::tracing::warn!(
                r0_routes = r0_count,
                "{} r0 route(s) are deprecated and scheduled for removal. \
                 Clients should migrate to /v3/ paths.",
                r0_count,
            );
        }

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

/// Summary of a successful [`RouteLedger::validate`] call.
#[derive(Debug, Clone, Copy)]
pub struct LedgerReport {
    /// Number of distinct `(method, path)` tuples — equal to the total
    /// manifest length when validation succeeds.
    pub unique_tuples: usize,
    /// Raw number of [`RouteEntry`] values across all manifests. Kept
    /// separately for future use; today it matches `unique_tuples` exactly.
    pub total_entries: usize,
}

#[derive(Debug, Clone)]
pub struct DuplicateEntry {
    pub method: Method,
    pub path: &'static str,
    pub registered_by: Vec<&'static str>,
}

#[derive(Debug, Clone)]
pub struct DuplicateRouteError {
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
}
