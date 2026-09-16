//! HTTP boundary for Synapse Rust.
//!
//! Extracted from the root crate's `src/web/` (plus the federation glue that
//! depends on the HTTP context) so the root crate is reduced to composition and
//! bootstrap (B4-5b).

// Test code may use unwrap/expect/panic per Rust testing idiom.
// Production lib code is still held to the strict clippy config in [lints].
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

/// The `federation` module — federation glue that depends on the HTTP context
/// (`EduDispatcher` and friends), so it lives with the HTTP layer.
pub mod federation;
/// The `middleware` module.
pub mod middleware;
/// The `routes` module.
pub mod routes;
/// The `utils` module.
pub(crate) mod utils;

pub use middleware::{payload_too_large_json_middleware, request_debug_middleware, request_timeout_middleware};
pub use routes::{
    admin, create_router, declared_ledger_all, declared_ledger_for, declared_ledger_for_profile, media, AppState,
    AuthenticatedUser, OptionalAuthenticatedUser,
};
