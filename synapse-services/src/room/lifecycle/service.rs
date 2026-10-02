//! Domain service for room lifecycle operations — create, upgrade, and
//! migration.
//!
//! Extracted from RoomService as part of the domain split plan (Task 4).

use crate::common::error::{ApiError, ApiResult};
use crate::policy_service::PolicyService;
use std::sync::Arc;
use synapse_cache::CacheManager;
use synapse_common::validation::Validator;
use synapse_storage::{MemberStoreApi, RoomStoreApi, UserStore};

/// Domain service for room lifecycle operations — create, upgrade, and
/// migration.
#[derive(Clone)]
pub struct LifecycleService {
    pub(crate) room_storage: Arc<dyn RoomStoreApi>,
    pub(crate) member_storage: Arc<dyn MemberStoreApi>,
    pub(crate) event_reader: Arc<dyn synapse_storage::event::EventReader>,
    pub(crate) event_writer: Arc<dyn synapse_storage::event::EventWriter>,
    pub(crate) user_storage: Arc<dyn UserStore>,
    pub(crate) validator: Arc<Validator>,
    pub(crate) server_name: String,
    /// Direct reference to RoomSummaryService, injected during construction
    /// instead of via a back-reference to RoomService.
    pub(crate) room_summary_service: Option<Arc<crate::room::summary::RoomSummaryService>>,
    pub(crate) cache: Arc<CacheManager>,
    /// Optional application-service manager. When present, room lifecycle
    /// events (create, upgrade) are enqueued for matching application
    /// services after the transaction commits.
    pub(crate) app_service_manager: Option<Arc<crate::application_service::ApplicationServiceManager>>,
    /// MSC4284 — Policy server service. When present, room creation
    /// consults the policy server before persisting. `None` in
    /// test setups or when the policy server is not configured.
    pub(crate) policy_service: Option<Arc<PolicyService>>,
    /// The third-party event admission gate (Synapse `check_event_allowed`).
    /// Every event of the room-creation sequence consults it before the row is
    /// persisted, so a rule can refuse the whole create with `403` and leave no
    /// partial room behind. Required, not optional — a gate that can be absent
    /// is a gate that can be skipped.
    pub(crate) event_admission_gate: Arc<dyn crate::module_service::EventAdmissionGate>,
}

/// Configuration for constructing a [`LifecycleService`].
pub struct LifecycleServiceConfig {
    /// The `room_storage` field.
    pub room_storage: Arc<dyn RoomStoreApi>,
    /// The `member_storage` field.
    pub member_storage: Arc<dyn MemberStoreApi>,
    /// The `event_reader` field.
    pub event_reader: Arc<dyn synapse_storage::event::EventReader>,
    /// The `event_writer` field.
    pub event_writer: Arc<dyn synapse_storage::event::EventWriter>,
    /// The `user_storage` field.
    pub user_storage: Arc<dyn UserStore>,
    /// The `validator` field.
    pub validator: Arc<Validator>,
    /// The `server_name` field.
    pub server_name: String,
    /// The `room_summary_service` field.
    pub room_summary_service: Option<Arc<crate::room::summary::RoomSummaryService>>,
    /// The `cache` field.
    pub cache: Arc<CacheManager>,
    /// The `app_service_manager` field.
    pub app_service_manager: Option<Arc<crate::application_service::ApplicationServiceManager>>,
    /// MSC4284 — Policy server service. `None` in test setups or when
    /// the policy server is not configured.
    pub policy_service: Option<Arc<PolicyService>>,
    /// The event admission gate (Synapse `check_event_allowed`). See
    /// [`LifecycleService::event_admission_gate`].
    pub event_admission_gate: Arc<dyn crate::module_service::EventAdmissionGate>,
}

impl LifecycleService {
    /// See [`new`].
    pub fn new(config: LifecycleServiceConfig) -> Self {
        Self {
            room_storage: config.room_storage,
            member_storage: config.member_storage,
            event_reader: config.event_reader,
            event_writer: config.event_writer,
            user_storage: config.user_storage,
            validator: config.validator,
            server_name: config.server_name,
            room_summary_service: config.room_summary_service,
            cache: config.cache,
            app_service_manager: config.app_service_manager,
            policy_service: config.policy_service,
            event_admission_gate: config.event_admission_gate,
        }
    }

    /// MSC4284: Check policy for room creation.
    /// Returns `Ok(())` if allowed, or `Err(Forbidden)` if denied by the
    /// policy server. No-op when no policy service is configured.
    pub(crate) async fn check_create_policy(&self, room_id: &str, creator: &str) -> ApiResult<()> {
        let Some(policy) = &self.policy_service else {
            return Ok(());
        };
        match policy.check_room_create(room_id, creator).await {
            crate::policy_service::PolicyResult::Allow => Ok(()),
            crate::policy_service::PolicyResult::Deny(reason) => {
                ::tracing::warn!(
                    room_id = %room_id,
                    creator = %creator,
                    reason = %reason,
                    "Room creation denied by policy server"
                );
                Err(ApiError::forbidden(format!("Denied by policy server: {}", reason)))
            }
        }
    }
}
