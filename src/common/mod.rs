pub mod argon2_config;
pub mod background_job;
pub mod collections;
pub mod concurrency;
pub mod config;
pub mod constants;
pub mod crypto;
pub mod early_exit;
pub mod error;
pub mod error_context;
pub mod event_utils;
pub mod federation_test_keys;
pub mod health;
pub mod logging;
pub mod macros;
pub mod metrics;
pub mod server_metrics;
pub mod password_hash_pool;
pub mod rate_limit;
pub mod rate_limit_config;
pub mod regex_cache;
pub mod sanitizer;
pub mod sanitizer_v2;
pub mod security;
pub mod task_queue;
pub mod telemetry_config;
pub mod time;
pub mod tracing;
pub mod transaction;
pub mod types;
pub mod validation;
pub mod xml_parser;

// HTML/text sanitizer: v2 (ammonia-based) is the canonical implementation.
// The regex-based v1 (`crate::common::sanitizer`) is retained only for
// historical reference and should NOT be used for new code — its blocklist
// approach misses obfuscation vectors that ammonia's whitelist correctly
// strips. Reach for v1 explicitly via its module path if absolutely needed.
#[allow(ambiguous_glob_reexports)]
pub use sanitizer_v2::*;

#[allow(ambiguous_glob_reexports)]
pub use argon2_config::*;
#[allow(ambiguous_glob_reexports)]
pub use background_job::*;
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
pub use error_context::*;
#[allow(ambiguous_glob_reexports)]
pub use event_utils::*;
#[allow(ambiguous_glob_reexports)]
pub use health::*;
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
pub use security::*;
#[allow(ambiguous_glob_reexports)]
pub use task_queue::*;
#[allow(ambiguous_glob_reexports)]
pub use telemetry_config::*;
#[allow(ambiguous_glob_reexports)]
pub use time::*;
#[allow(ambiguous_glob_reexports)]
pub use tracing::*;
#[allow(ambiguous_glob_reexports)]
pub use types::*;
#[allow(ambiguous_glob_reexports)]
pub use xml_parser::*;
