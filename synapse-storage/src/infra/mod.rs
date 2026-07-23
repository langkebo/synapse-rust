//! Infrastructure storage domain group.
//!
//! Re-exports infrastructure-related storage modules (background updates,
//! maintenance, monitoring, performance, schema validation, federation
//! blacklist/queue, rate limiting, feature flags) under a single namespace so
//! that new infra storage modules can be added here without touching `lib.rs`.
//!
//! Consumers should prefer `synapse_storage::infra::BackgroundUpdateStorage`
//! over the flat `synapse_storage::BackgroundUpdateStorage`.

pub use crate::background_update::{
    BackgroundUpdate, BackgroundUpdateHistory, BackgroundUpdateLock, BackgroundUpdateStats, BackgroundUpdateStorage,
    BackgroundUpdateStoreApi, CreateBackgroundUpdateRequest, UpdateBackgroundUpdateRequest,
};
pub use crate::feature_flags::{
    CreateFeatureFlagRequest, FeatureFlag, FeatureFlagFilters, FeatureFlagRecord, FeatureFlagStorage,
    FeatureFlagStoreApi, FeatureFlagTargetInput, FeatureFlagTargetRecord, UpdateFeatureFlagRequest,
};
pub use crate::federation_blacklist::{
    decode_federation_blacklist_cursor, encode_federation_blacklist_cursor, AddBlacklistRequest, CreateLogRequest,
    CreateRuleRequest, FederationAccessStats, FederationBlacklist, FederationBlacklistCursor, FederationBlacklistLog,
    FederationBlacklistRule, FederationBlacklistStorage, FederationBlacklistStoreApi, UpdateStatsRequest,
};
pub use crate::federation_queue::FederationQueueStoreApi;
pub use crate::maintenance::{DatabaseMaintenance, MaintenanceReport, TableStats, VacuumResult};
pub use crate::monitoring::{
    ConnectionPoolStatus, DataIntegrityReport, DatabaseHealthStatus, DatabaseMonitor, DuplicateEntry,
    ForeignKeyViolation, NullConstraintViolation, OrphanedRecord, PerformanceMetrics,
};
pub use crate::performance::{time_query, PerformanceMonitor, PoolStatistics, QueryMetrics};
pub use crate::rate_limit::{RateLimitRecord, RateLimitStorage, RateLimitStoreApi};
pub use crate::schema_validator::{SchemaValidationResult, SchemaValidator, TableSchemaInfo};
