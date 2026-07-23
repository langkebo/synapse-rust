// Single consolidated re-export from synapse_common.
//
// All items re-exported at synapse_common's crate root are available here.
// No glob-ambiguity suppression is needed because synapse-common/src/lib.rs
// now uses explicit `pub use module::{...}` lists instead of
// `pub use module::*` globs, eliminating glob-vs-glob ambiguity.
pub use synapse_common::*;

// Explicit macro re-exports (#[macro_export] macros at synapse_common root).
// Also covered by the glob above; kept explicit for discoverability.
pub use synapse_common::{
    impl_api_error, map_bad_request, map_forbidden, map_internal, map_not_found, map_unauthorized,
};

// Re-export entire modules from synapse_common (for module-path access, e.g.
// `common::metrics::Counter` in addition to `common::Counter`).
pub use synapse_common::metrics;
pub use synapse_common::server_metrics;

// Local genuine modules (non-facade). These shadow the same-named modules
// that the glob would re-export from synapse_common; local definitions win.
pub mod config;
pub mod crypto;
pub mod error;
#[cfg(any(test, feature = "test-utils"))]
pub mod federation_test_keys;
pub mod health;
pub mod logging;
pub mod rate_limit;

// Re-exports of local-only items from genuine modules.
//
// Pure-facade modules (config, crypto, federation_test_keys, rate_limit) need
// no explicit re-export here — their items are covered by `pub use
// synapse_common::*` above, and the local modules just re-export from
// synapse_common internally.
//
// `logging::init_logging` is re-exported explicitly because the local
// implementation differs from synapse_common's (adds RequestIdPropagationLayer);
// the explicit re-export shadows the glob's `synapse_common::init_logging`.
pub use error::{crypto_error_to_api_error, ed25519_error_to_api_error};
pub use health::CacheHealthCheck;
pub use logging::init_logging;
