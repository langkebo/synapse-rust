//! Infrastructure services domain group.
//!
//! Re-exports infrastructure-related service modules (background_update_service,
//! database_initializer, telemetry_service, feature_flag_service,
//! federation_blacklist_service, federation_key_rotation_service,
//! user_lock_service) under a single namespace so that new infrastructure
//! services can be added here without touching `lib.rs`.
//!
//! Consumers may use either:
//! - `synapse_services::infra::FeatureFlagService` (preferred, grouped path)
//! - `synapse_services::FeatureFlagService` (legacy flat path, via `pub use infra::*` in lib.rs)

#[allow(ambiguous_glob_reexports)]
pub use crate::database_initializer::{
    initialize_database, DatabaseInitMode, DatabaseInitService, Environment, InitializationReport,
};
pub use crate::feature_flag_service::FeatureFlagService;
pub use crate::federation_key_rotation_service::FederationKeyRotationService;

// P7.4 — additional infra-domain service re-exports (previously flat in lib.rs).
pub use crate::background_update_service::*;
pub use crate::e2ee_audit::*;
#[cfg(feature = "external-services")]
pub use crate::external_service_integration::{
    ExternalServiceConfig, ExternalServiceIntegration, ExternalServiceType, ServiceHealthStatus, TrendRadarConfig,
    TrendRadarPayload, WebhookAuthInput, WebhookPayload,
};
#[cfg(all(feature = "external-services", feature = "openclaw-routes"))]
pub use crate::external_service_integration::{OpenClawConfig, OpenClawPayload};
pub use crate::telemetry_service::*;
pub use crate::translation_service::*;
