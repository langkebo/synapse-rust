// Re-export from canonical `synapse-common` crate, plus `CacheHealthCheck`
// (which lives in `synapse-cache` because it wraps `CacheManager`).
pub use synapse_cache::CacheHealthCheck;
pub use synapse_common::health::*;
