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

// P7.3: worker, pruning, schema_health_check, trigram_ranking, and
// server_notification are infrastructure-related storage modules — group them
// under `infra::` so they are flat-re-exported via `pub use infra::*;` rather
// than via explicit flat re-exports in lib.rs.
pub use crate::pruning::*;
pub use crate::schema_health_check::*;
#[cfg(feature = "server-notifications")]
pub use crate::server_notification::{
    decode_server_notification_cursor, encode_server_notification_cursor, CreateNotificationRequest,
    CreateTemplateRequest, NotificationDeliveryLog, NotificationTemplate, NotificationWithStatus,
    ScheduledNotification, ServerNotification, ServerNotificationCursor, ServerNotificationStorage,
    ServerNotificationStoreApi, UserNotificationStatus,
};
pub use crate::trigram_ranking::*;
pub use crate::worker::{
    AssignTaskRequest, HeartbeatRequest, RdataEvent, RdataPosition, RegisterWorkerRequest, ReplicationPosition,
    SendCommandRequest, StreamPosition, UpdateConnectionStatsRequest, WorkerCapabilities, WorkerCommand,
    WorkerCommandRow, WorkerConnection, WorkerEvent, WorkerEventRow, WorkerInfo, WorkerLoadStats,
    WorkerLoadStatsUpdate, WorkerResponsibilitySummary, WorkerRow, WorkerRuntimeConfig, WorkerStatus, WorkerStorage,
    WorkerStoreApi, WorkerTaskAssignment, WorkerTopologyEntry, WorkerTopologyPreset, WorkerTopologyPresetInstance,
    WorkerTopologySummary, WorkerType,
};
