pub mod argon2_config;
pub mod backpressure;
pub mod collections;
pub mod concurrency;
pub mod constants;
pub mod crypto;
pub mod early_exit;
pub mod error;
pub mod event_utils;
pub mod feature_flags;
pub mod federation_test_keys;
pub mod health;
pub mod key_encryption;
pub mod logging;
pub mod macros;
pub mod media_link_signer;
pub mod media_locator;
pub mod nonce_cache;
pub mod password_hash_pool;
pub mod rate_limit;
pub mod rate_limit_config;
pub mod regex_cache;
pub mod room_versions;
pub mod sanitizer;
pub mod security;
pub mod time;
pub mod tracing;
pub mod traits;
pub mod transaction;
pub mod types;
pub mod xml_parser;

pub use synapse_common::metrics;
pub use synapse_common::server_metrics;

pub mod config;

// HTML/text sanitizer: ammonia-based whitelist implementation (the only implementation).
#[allow(ambiguous_glob_reexports)]
pub use sanitizer::*;

#[allow(ambiguous_glob_reexports)]
pub use argon2_config::*;
#[allow(ambiguous_glob_reexports)]
pub use collections::*;
#[allow(ambiguous_glob_reexports)]
pub use concurrency::*;
#[allow(ambiguous_glob_reexports)]
pub use config::*;
#[allow(ambiguous_glob_reexports)]
pub use constants::*;
#[allow(ambiguous_glob_reexports)]
pub use crypto::*;
#[allow(ambiguous_glob_reexports)]
pub use early_exit::*;
#[allow(ambiguous_glob_reexports)]
pub use error::*;
#[allow(ambiguous_glob_reexports)]
pub use event_utils::*;
#[allow(ambiguous_glob_reexports)]
pub use feature_flags::*;
#[cfg(any(test, feature = "test-utils"))]
#[allow(ambiguous_glob_reexports)]
pub use federation_test_keys::*;
#[allow(ambiguous_glob_reexports)]
pub use health::*;
#[allow(ambiguous_glob_reexports)]
pub use key_encryption::*;
#[allow(ambiguous_glob_reexports)]
pub use logging::*;
#[allow(ambiguous_glob_reexports)]
pub use metrics::*;
#[allow(ambiguous_glob_reexports)]
pub use password_hash_pool::*;
#[allow(ambiguous_glob_reexports)]
pub use rate_limit::*;
#[allow(ambiguous_glob_reexports)]
pub use rate_limit_config::*;
#[allow(ambiguous_glob_reexports)]
pub use regex_cache::*;
#[allow(ambiguous_glob_reexports)]
pub use room_versions::*;
#[allow(ambiguous_glob_reexports)]
pub use security::*;
#[allow(ambiguous_glob_reexports)]
pub use synapse_common::background_job::*;
#[allow(ambiguous_glob_reexports)]
pub use synapse_common::task_queue::*;
#[allow(ambiguous_glob_reexports)]
pub use synapse_common::telemetry_config::*;
#[allow(ambiguous_glob_reexports)]
pub use synapse_common::validation::*;
#[allow(ambiguous_glob_reexports)]
pub use time::*;
#[allow(ambiguous_glob_reexports)]
pub use tracing::*;
#[allow(ambiguous_glob_reexports)]
pub use types::*;
#[allow(ambiguous_glob_reexports)]
pub use xml_parser::*;
