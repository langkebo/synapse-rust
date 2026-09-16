// Single consolidated re-export from synapse_common.
//
// All items re-exported at synapse_common's crate root are available here.
// No glob-ambiguity suppression is needed because synapse-common/src/lib.rs
// now uses explicit `pub use module::{...}` lists instead of
// `pub use module::*` globs, eliminating glob-vs-glob ambiguity.
pub use synapse_common::*;

// Explicit macro re-exports (#[macro_export] macros at synapse_common root).
// Also covered by the glob above; kept explicit for discoverability.
pub use synapse_common::{map_database, map_internal};

// Re-export entire modules from synapse_common (for module-path access, e.g.
// `common::metrics::Counter` in addition to `common::Counter`).
pub use synapse_common::metrics;
pub use synapse_common::server_metrics;

// Local genuine modules (non-facade). These shadow the same-named modules
// that the glob would re-export from synapse_common; local definitions win.
/// The `config` module.
pub mod config;
/// The `crypto` module.
pub mod crypto;
/// The `error` module.
pub mod error;
/// The `federation_test_keys` module.
#[cfg(any(test, feature = "test-utils"))]
pub mod federation_test_keys;
/// The `health` module.
pub mod health;

// Re-exports of local-only items from genuine modules.
//
// Pure-facade modules (config, crypto, federation_test_keys) need
// no explicit re-export here — their items are covered by `pub use
// synapse_common::*` above, and the local modules just re-export from
// synapse_common internally.
// (`rate_limit` facade removed 2026-08-09 together with the dead legacy
// in-memory RateLimiter; the authoritative limiter is web/middleware/rate_limit.rs.)
//
pub use error::{crypto_error_to_api_error, ed25519_error_to_api_error};
pub use health::CacheHealthCheck;
