// Direct re-exports from synapse_common (consolidated from 24 single-line facade files)
#[allow(ambiguous_glob_reexports)]
pub use synapse_common::argon2_config::*;
#[allow(ambiguous_glob_reexports)]
pub use synapse_common::collections::*;
#[allow(ambiguous_glob_reexports)]
pub use synapse_common::concurrency::*;
#[allow(ambiguous_glob_reexports)]
pub use synapse_common::constants::*;
#[allow(ambiguous_glob_reexports)]
pub use synapse_common::early_exit::*;
#[allow(ambiguous_glob_reexports)]
pub use synapse_common::event_utils::*;
#[allow(ambiguous_glob_reexports)]
pub use synapse_common::feature_flags::*;
#[allow(ambiguous_glob_reexports)]
pub use synapse_common::key_encryption::*;
#[allow(ambiguous_glob_reexports)]
pub use synapse_common::media_link_signer::*;
#[allow(ambiguous_glob_reexports)]
pub use synapse_common::media_locator::*;
#[allow(ambiguous_glob_reexports)]
pub use synapse_common::nonce_cache::*;
#[allow(ambiguous_glob_reexports)]
pub use synapse_common::password_hash_pool::*;
#[allow(ambiguous_glob_reexports)]
pub use synapse_common::rate_limit_config::*;
#[allow(ambiguous_glob_reexports)]
pub use synapse_common::regex_cache::*;
#[allow(ambiguous_glob_reexports)]
pub use synapse_common::room_versions::*;
pub use synapse_common::{
    impl_api_error, map_bad_request, map_forbidden, map_internal, map_not_found, map_unauthorized,
};
// HTML/text sanitizer: ammonia-based whitelist implementation (the only implementation).
#[allow(ambiguous_glob_reexports)]
pub use synapse_common::sanitizer::*;
#[allow(ambiguous_glob_reexports)]
pub use synapse_common::security::*;
#[allow(ambiguous_glob_reexports)]
pub use synapse_common::time::*;
#[allow(ambiguous_glob_reexports)]
pub use synapse_common::tracing::*;
#[allow(ambiguous_glob_reexports)]
pub use synapse_common::traits::*;
#[allow(ambiguous_glob_reexports)]
pub use synapse_common::transaction::*;
#[allow(ambiguous_glob_reexports)]
pub use synapse_common::types::*;
#[allow(ambiguous_glob_reexports)]
pub use synapse_common::xml_parser::*;

// Re-export entire modules from synapse_common
pub use synapse_common::metrics;
pub use synapse_common::server_metrics;

// Local genuine modules (non-facade, kept as files)
pub mod config;
pub mod crypto;
pub mod error;
#[cfg(any(test, feature = "test-utils"))]
pub mod federation_test_keys;
pub mod health;
pub mod logging;
pub mod rate_limit;

// Additional re-exports from synapse_common (no local facade existed)
#[allow(ambiguous_glob_reexports)]
pub use synapse_common::background_job::*;
#[allow(ambiguous_glob_reexports)]
pub use synapse_common::task_queue::*;
#[allow(ambiguous_glob_reexports)]
pub use synapse_common::telemetry_config::*;
#[allow(ambiguous_glob_reexports)]
pub use synapse_common::validation::*;

// Re-exports from local genuine modules
#[allow(ambiguous_glob_reexports)]
pub use config::*;
#[allow(ambiguous_glob_reexports)]
pub use crypto::*;
#[allow(ambiguous_glob_reexports)]
pub use error::*;
#[cfg(any(test, feature = "test-utils"))]
#[allow(ambiguous_glob_reexports)]
pub use federation_test_keys::*;
#[allow(ambiguous_glob_reexports)]
pub use health::*;
#[allow(ambiguous_glob_reexports)]
pub use logging::*;
#[allow(ambiguous_glob_reexports)]
pub use metrics::*;
#[allow(ambiguous_glob_reexports)]
pub use rate_limit::*;
