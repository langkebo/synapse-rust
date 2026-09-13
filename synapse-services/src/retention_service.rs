#[cfg(feature = "beacons")]
use crate::beacon_service::BeaconService;
use std::sync::Arc;
use std::time::Instant;
use synapse_common::config::RetentionConfig;
use synapse_common::current_timestamp_millis;
use synapse_common::metrics::{Counter, Gauge, Histogram, MetricsCollector};
use synapse_common::ApiError;

use synapse_storage::media::ChunkedUploadStoreApi;
use synapse_storage::retention::*;
use tokio::sync::RwLock;
use tracing::{error, info, instrument, warn};

/// The `DataLifecycleCleanupSummary` struct.
#[derive(Debug, Clone, Default)]
pub struct DataLifecycleCleanupSummary {
    /// The `started_ts` field.
    pub started_ts: i64,
    /// The `completed_ts` field.
    pub completed_ts: i64,
    /// The `duration_ms` field.
    pub duration_ms: i64,
    /// The `expired_events_deleted` field.
    pub expired_events_deleted: u64,
    /// The `expired_beacons_deleted` field.
    pub expired_beacons_deleted: u64,
    /// The `expired_uploads_deleted` field.
    pub expired_uploads_deleted: u64,
    /// The `expired_audit_events_deleted` field.
    pub expired_audit_events_deleted: u64,
    /// The `cleanup_queue_items_processed` field.
    pub cleanup_queue_items_processed: u64,
    /// The `cleanup_queue_rows_pruned` field.
    pub cleanup_queue_rows_pruned: u64,
    /// The `failed_tasks` field.
    pub failed_tasks: u64,
}

/// The `RetentionStatusSummary` struct.
#[derive(Debug, Clone)]
pub struct RetentionStatusSummary {
    /// The `rooms_with_custom_policy` field.
    pub rooms_with_custom_policy: i64,
    /// The `server_policy_enabled` field.
    pub server_policy_enabled: bool,
    /// The `last_run` field.
    pub last_run: Option<DataLifecycleCleanupSummary>,
}

#[derive(Clone)]
struct RetentionLifecycleMetrics {
    cycles_total: Counter,
    cycles_failed_total: Counter,
    events_deleted_total: Counter,
    beacons_deleted_total: Counter,
    uploads_deleted_total: Counter,
    audit_events_deleted_total: Counter,
    queue_processed_total: Counter,
    queue_pruned_total: Counter,
    last_run_ts: Gauge,
    last_failure_ts: Gauge,
    last_duration_ms: Gauge,
    last_failed_tasks: Gauge,
    last_events_deleted: Gauge,
    last_beacons_deleted: Gauge,
    last_uploads_deleted: Gauge,
    last_audit_events_deleted: Gauge,
    last_queue_processed: Gauge,
    last_queue_pruned: Gauge,
    cycle_duration_ms: Histogram,
}

impl RetentionLifecycleMetrics {
    fn new(metrics: &Arc<MetricsCollector>) -> Self {
        Self {
            cycles_total: metrics.register_counter("retention_lifecycle_cycles_total".to_string()),
            cycles_failed_total: metrics.register_counter("retention_lifecycle_cycles_failed_total".to_string()),
            events_deleted_total: metrics.register_counter("retention_lifecycle_events_deleted_total".to_string()),
            beacons_deleted_total: metrics.register_counter("retention_lifecycle_beacons_deleted_total".to_string()),
            uploads_deleted_total: metrics.register_counter("retention_lifecycle_uploads_deleted_total".to_string()),
            audit_events_deleted_total: metrics
                .register_counter("retention_lifecycle_audit_events_deleted_total".to_string()),
            queue_processed_total: metrics.register_counter("retention_lifecycle_queue_processed_total".to_string()),
            queue_pruned_total: metrics.register_counter("retention_lifecycle_queue_pruned_total".to_string()),
            last_run_ts: metrics.register_gauge("retention_lifecycle_last_run_ts".to_string()),
            last_failure_ts: metrics.register_gauge("retention_lifecycle_last_failure_ts".to_string()),
            last_duration_ms: metrics.register_gauge("retention_lifecycle_last_duration_ms".to_string()),
            last_failed_tasks: metrics.register_gauge("retention_lifecycle_last_failed_tasks".to_string()),
            last_events_deleted: metrics.register_gauge("retention_lifecycle_last_events_deleted".to_string()),
            last_beacons_deleted: metrics.register_gauge("retention_lifecycle_last_beacons_deleted".to_string()),
            last_uploads_deleted: metrics.register_gauge("retention_lifecycle_last_uploads_deleted".to_string()),
            last_audit_events_deleted: metrics
                .register_gauge("retention_lifecycle_last_audit_events_deleted".to_string()),
            last_queue_processed: metrics.register_gauge("retention_lifecycle_last_queue_processed".to_string()),
            last_queue_pruned: metrics.register_gauge("retention_lifecycle_last_queue_pruned".to_string()),
            cycle_duration_ms: metrics.register_histogram("retention_lifecycle_cycle_duration_ms".to_string()),
        }
    }

    fn observe_cycle(&self, summary: &DataLifecycleCleanupSummary) {
        self.cycles_total.inc();
        self.events_deleted_total.inc_by(summary.expired_events_deleted);
        self.beacons_deleted_total.inc_by(summary.expired_beacons_deleted);
        self.uploads_deleted_total.inc_by(summary.expired_uploads_deleted);
        self.audit_events_deleted_total.inc_by(summary.expired_audit_events_deleted);
        self.queue_processed_total.inc_by(summary.cleanup_queue_items_processed);
        self.queue_pruned_total.inc_by(summary.cleanup_queue_rows_pruned);
        self.last_run_ts.set(summary.completed_ts as f64);
        self.last_duration_ms.set(summary.duration_ms as f64);
        self.last_failed_tasks.set(summary.failed_tasks as f64);
        self.last_events_deleted.set(summary.expired_events_deleted as f64);
        self.last_beacons_deleted.set(summary.expired_beacons_deleted as f64);
        self.last_uploads_deleted.set(summary.expired_uploads_deleted as f64);
        self.last_audit_events_deleted.set(summary.expired_audit_events_deleted as f64);
        self.last_queue_processed.set(summary.cleanup_queue_items_processed as f64);
        self.last_queue_pruned.set(summary.cleanup_queue_rows_pruned as f64);
        self.cycle_duration_ms.observe(summary.duration_ms as f64);

        if summary.failed_tasks > 0 {
            self.cycles_failed_total.inc();
            self.last_failure_ts.set(summary.completed_ts as f64);
        }
    }
}

/// The `RetentionService` struct.
pub struct RetentionService {
    storage: Arc<dyn synapse_storage::retention::RetentionStoreApi>,
    chunked_upload_storage: Arc<dyn ChunkedUploadStoreApi>,
    audit_storage: Arc<dyn synapse_storage::audit::AuditEventStoreApi>,
    lifecycle_metrics: RetentionLifecycleMetrics,
    last_lifecycle_summary: Arc<RwLock<Option<DataLifecycleCleanupSummary>>>,
}

impl RetentionService {
    /// See [`new`].
    pub fn new(
        storage: Arc<dyn synapse_storage::retention::RetentionStoreApi>,
        chunked_upload_storage: Arc<dyn ChunkedUploadStoreApi>,
        metrics: &Arc<MetricsCollector>,
        audit_storage: Arc<dyn synapse_storage::audit::AuditEventStoreApi>,
    ) -> Self {
        Self {
            storage,
            chunked_upload_storage,
            audit_storage,
            lifecycle_metrics: RetentionLifecycleMetrics::new(metrics),
            last_lifecycle_summary: Arc::new(RwLock::new(None)),
        }
    }

    /// See [`get_room_policy`].
    #[instrument(skip(self))]
    pub async fn get_room_policy(&self, room_id: &str) -> Result<Option<RoomRetentionPolicy>, ApiError> {
        let policy = self
            .storage
            .get_room_policy(room_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get room policy", e))?;

        Ok(policy)
    }

    /// See [`get_effective_policy`].
    #[instrument(skip(self))]
    pub async fn get_effective_policy(&self, room_id: &str) -> Result<EffectiveRetentionPolicy, ApiError> {
        let policy = self
            .storage
            .get_effective_policy(room_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get effective policy", e))?;

        Ok(policy)
    }

    /// See [`set_room_policy`].
    #[instrument(skip(self))]
    pub async fn set_room_policy(
        &self,
        request: CreateRoomRetentionPolicyRequest,
    ) -> Result<RoomRetentionPolicy, ApiError> {
        info!(
            room_id = %request.room_id,
            max_lifetime = ?request.max_lifetime,
            min_lifetime = ?request.min_lifetime,
            expire_on_clients = ?request.is_expire_on_clients,
            "Setting retention policy for room"
        );

        if let Some(max_lifetime) = request.max_lifetime {
            if max_lifetime < 0 {
                return Err(ApiError::bad_request("max_lifetime cannot be negative"));
            }
        }

        let policy = self
            .storage
            .create_room_policy(request)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to create room policy", e))?;

        Ok(policy)
    }

    /// See [`update_room_policy`].
    #[instrument(skip(self))]
    pub async fn update_room_policy(
        &self,
        room_id: &str,
        request: UpdateRoomRetentionPolicyRequest,
    ) -> Result<RoomRetentionPolicy, ApiError> {
        let policy = self
            .storage
            .update_room_policy(room_id, request)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to update room policy", e))?;

        Ok(policy)
    }

    /// See [`delete_room_policy`].
    #[instrument(skip(self))]
    pub async fn delete_room_policy(&self, room_id: &str) -> Result<(), ApiError> {
        info!(room_id = %room_id, "Deleting retention policy for room");

        self.storage
            .delete_room_policy(room_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to delete room policy", e))?;

        Ok(())
    }

    /// See [`get_server_policy`].
    #[instrument(skip(self))]
    pub async fn get_server_policy(&self) -> Result<ServerRetentionPolicy, ApiError> {
        let policy = self
            .storage
            .get_server_policy()
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get server policy", e))?;

        Ok(policy)
    }

    /// See [`get_server_policy_optional`].
    #[instrument(skip(self))]
    pub async fn get_server_policy_optional(&self) -> Result<Option<ServerRetentionPolicy>, ApiError> {
        let policy = self
            .storage
            .get_server_policy_optional()
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get server policy", e))?;

        Ok(policy)
    }

    /// Resolve the effective retention policy for a room.
    ///
    /// Returns the room-level policy if one exists; otherwise falls back
    /// to the server-wide default policy, and finally to a hardcoded
    /// default with no max_lifetime.
    #[instrument(skip(self))]
    pub async fn resolve_effective_policy(&self, room_id: &str) -> Result<RoomRetentionPolicy, ApiError> {
        let room_policy = self.get_room_policy(room_id).await?;

        if let Some(policy) = room_policy {
            return Ok(policy);
        }

        let server_policy = self.get_server_policy_optional().await?;

        match server_policy {
            Some(sp) => Ok(RoomRetentionPolicy {
                room_id: room_id.to_string(),
                id: 0,
                max_lifetime: sp.max_lifetime,
                min_lifetime: sp.min_lifetime,
                is_expire_on_clients: sp.is_expire_on_clients,
                is_server_default: true,
                created_ts: sp.created_ts,
                updated_ts: sp.updated_ts,
            }),
            None => Ok(RoomRetentionPolicy {
                room_id: room_id.to_string(),
                id: 0,
                max_lifetime: None,
                min_lifetime: 0,
                is_expire_on_clients: false,
                is_server_default: true,
                created_ts: 0,
                updated_ts: 0,
            }),
        }
    }

    /// See [`update_server_policy`].
    #[instrument(skip(self))]
    pub async fn update_server_policy(
        &self,
        request: UpdateServerRetentionPolicyRequest,
    ) -> Result<ServerRetentionPolicy, ApiError> {
        info!(
            max_lifetime = ?request.max_lifetime,
            min_lifetime = ?request.min_lifetime,
            expire_on_clients = ?request.is_expire_on_clients,
            "Updating server retention policy"
        );

        let policy = self
            .storage
            .update_server_policy(request)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to update server policy", e))?;

        Ok(policy)
    }

    /// See [`upsert_server_policy`].
    #[instrument(skip(self))]
    pub async fn upsert_server_policy(
        &self,
        request: UpdateServerRetentionPolicyRequest,
    ) -> Result<ServerRetentionPolicy, ApiError> {
        info!(
            max_lifetime = ?request.max_lifetime,
            min_lifetime = ?request.min_lifetime,
            expire_on_clients = ?request.is_expire_on_clients,
            "Upserting server retention policy"
        );

        let policy = self
            .storage
            .upsert_server_policy(request)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to upsert server policy", e))?;

        Ok(policy)
    }

    /// See [`run_cleanup`].
    #[instrument(skip(self))]
    pub async fn run_cleanup(&self, room_id: &str) -> Result<RetentionCleanupLog, ApiError> {
        info!(room_id = %room_id, "Running retention cleanup for room");

        let policy = self
            .storage
            .get_effective_policy(room_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get policy", e))?;

        let max_lifetime =
            policy.max_lifetime.ok_or_else(|| ApiError::bad_request("No retention policy configured for this room"))?;
        let cutoff_ts = current_timestamp_millis() - max_lifetime;
        let started_ts = current_timestamp_millis();

        match self.storage.delete_events_before(room_id, cutoff_ts).await {
            Ok(deleted_count) => {
                info!(events_deleted = deleted_count, room_id = room_id, "Retention cleanup completed");

                Ok(RetentionCleanupLog {
                    id: 0,
                    room_id: room_id.to_string(),
                    events_deleted: deleted_count,
                    state_events_deleted: 0,
                    media_deleted: 0,
                    bytes_freed: 0,
                    started_ts,
                    completed_ts: Some(current_timestamp_millis()),
                    status: "completed".to_string(),
                    error_message: None,
                })
            }
            Err(e) => {
                let error_msg = format!("Failed to delete events: {e}");
                error!(room_id = room_id, error = %e, "Retention cleanup failed");
                Err(ApiError::internal(error_msg))
            }
        }
    }

    /// See [`process_pending_cleanups`].
    #[instrument(skip(self))]
    pub async fn process_pending_cleanups(&self, _limit: i64) -> Result<usize, ApiError> {
        // No-op: cleanup queue table has been removed
        Ok(0)
    }

    /// See [`schedule_room_cleanup`].
    #[instrument(skip(self))]
    pub async fn schedule_room_cleanup(&self, room_id: &str) -> Result<i64, ApiError> {
        info!(room_id = room_id, "Retention cleanup scheduled (no-op, queue table removed)");
        Ok(0)
    }

    /// See [`get_stats`].
    #[instrument(skip(self))]
    pub async fn get_stats(&self, _room_id: &str) -> Result<Option<RetentionStats>, ApiError> {
        // No-op: cleanup queue table has been removed
        Ok(None)
    }

    /// See [`get_cleanup_logs`].
    #[instrument(skip(self))]
    pub async fn get_cleanup_logs(&self, _room_id: &str, _limit: i64) -> Result<Vec<RetentionCleanupLog>, ApiError> {
        // No-op: cleanup queue table has been removed
        Ok(vec![])
    }

    /// See [`get_deleted_events`].
    #[instrument(skip(self))]
    pub async fn get_deleted_events(&self, _room_id: &str, _since_ts: i64) -> Result<Vec<DeletedEventIndex>, ApiError> {
        // No-op: cleanup queue table has been removed
        Ok(vec![])
    }

    /// See [`get_rooms_with_policies`].
    #[instrument(skip(self))]
    pub async fn get_rooms_with_policies(&self) -> Result<Vec<RoomRetentionPolicy>, ApiError> {
        let policies = self
            .storage
            .get_rooms_with_policies()
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get rooms with policies", e))?;

        Ok(policies)
    }

    /// See [`get_pending_cleanup_count`].
    #[instrument(skip(self))]
    pub async fn get_pending_cleanup_count(&self, _room_id: &str) -> Result<i64, ApiError> {
        // No-op: cleanup queue table has been removed
        Ok(0)
    }

    /// See [`is_event_expired`].
    #[instrument(skip(self))]
    pub async fn is_event_expired(&self, room_id: &str, origin_server_ts: i64) -> Result<bool, ApiError> {
        let policy = self
            .storage
            .get_effective_policy(room_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get policy", e))?;

        if let Some(max_lifetime) = policy.max_lifetime {
            let cutoff_ts = current_timestamp_millis() - max_lifetime;
            Ok(origin_server_ts < cutoff_ts)
        } else {
            Ok(false)
        }
    }

    /// See [`run_scheduled_cleanups`].
    pub async fn run_scheduled_cleanups(&self) -> Result<usize, ApiError> {
        info!(cleanup_scope = %"scheduled", "Running scheduled retention cleanups");

        let policies = self
            .storage
            .get_rooms_with_policies()
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get policies", e))?;

        info!(policy_count = policies.len(), "Loaded retention policies for scheduled cleanups");

        let mut total_cleaned = 0;

        for policy in policies {
            if policy.max_lifetime.is_some() {
                match self.run_cleanup(&policy.room_id).await {
                    Ok(log) => {
                        total_cleaned += log.events_deleted as usize;
                    }
                    Err(e) => {
                        warn!(error = %e, room_id = %policy.room_id, "Failed to run cleanup for room");
                    }
                }
            }
        }

        Ok(total_cleaned)
    }

    /// See [`get_last_lifecycle_summary`].
    pub async fn get_last_lifecycle_summary(&self) -> Option<DataLifecycleCleanupSummary> {
        self.last_lifecycle_summary.read().await.clone()
    }

    /// See [`get_status_summary`].
    #[instrument(skip(self))]
    pub async fn get_status_summary(&self) -> Result<RetentionStatusSummary, ApiError> {
        let rooms_with_custom_policy = self
            .storage
            .count_room_policies()
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to count room retention policies", e))?;
        let server_policy_enabled = self
            .storage
            .has_server_policy()
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to check server retention policy", e))?;
        let last_run = self.get_last_lifecycle_summary().await;

        Ok(RetentionStatusSummary { rooms_with_custom_policy, server_policy_enabled, last_run })
    }

    /// See [`run_data_lifecycle_cycle`].
    #[cfg(feature = "beacons")]
    #[instrument(skip(self, beacon_service, config))]
    pub async fn run_data_lifecycle_cycle(
        &self,
        beacon_service: &BeaconService,
        config: &RetentionConfig,
    ) -> DataLifecycleCleanupSummary {
        let started_ts = current_timestamp_millis();
        let started = Instant::now();
        let mut summary = DataLifecycleCleanupSummary { started_ts, ..Default::default() };

        match self.run_scheduled_cleanups().await {
            Ok(count) => {
                summary.expired_events_deleted = count as u64;
            }
            Err(error) => {
                summary.failed_tasks += 1;
                warn!(error = %error, started_ts, failed_tasks = summary.failed_tasks, "Failed to run scheduled retention cleanups");
            }
        }

        match beacon_service.cleanup_expired_beacons().await {
            Ok(count) => {
                summary.expired_beacons_deleted = count;
            }
            Err(error) => {
                summary.failed_tasks += 1;
                warn!(error = %error, started_ts, failed_tasks = summary.failed_tasks, "Failed to cleanup expired beacons");
            }
        }

        self.finish_lifecycle_cycle(&mut summary, config, started_ts, started).await
    }

    /// See [`run_data_lifecycle_cycle_no_beacons`].
    #[cfg(not(feature = "beacons"))]
    #[instrument(skip(self, config))]
    pub async fn run_data_lifecycle_cycle_no_beacons(&self, config: &RetentionConfig) -> DataLifecycleCleanupSummary {
        let started_ts = current_timestamp_millis();
        let started = Instant::now();
        let mut summary = DataLifecycleCleanupSummary { started_ts, ..Default::default() };

        match self.run_scheduled_cleanups().await {
            Ok(count) => {
                summary.expired_events_deleted = count as u64;
            }
            Err(error) => {
                summary.failed_tasks += 1;
                warn!(error = %error, started_ts, failed_tasks = summary.failed_tasks, "Failed to run scheduled retention cleanups");
            }
        }

        self.finish_lifecycle_cycle(&mut summary, config, started_ts, started).await
    }

    async fn finish_lifecycle_cycle(
        &self,
        summary: &mut DataLifecycleCleanupSummary,
        config: &RetentionConfig,
        started_ts: i64,
        started: Instant,
    ) -> DataLifecycleCleanupSummary {
        match self.cleanup_expired_uploads().await {
            Ok(count) => {
                summary.expired_uploads_deleted = count;
            }
            Err(error) => {
                summary.failed_tasks += 1;
                warn!(error = %error, started_ts, failed_tasks = summary.failed_tasks, "Failed to cleanup expired uploads");
            }
        }

        match self.cleanup_audit_events(config.audit_retention_days, started_ts).await {
            Ok(count) => {
                summary.expired_audit_events_deleted = count;
            }
            Err(error) => {
                summary.failed_tasks += 1;
                warn!(
                    error = %error,
                    started_ts,
                    audit_retention_days = config.audit_retention_days,
                    failed_tasks = summary.failed_tasks,
                    "Failed to cleanup expired audit events"
                );
            }
        }

        match self.process_pending_cleanups(config.cleanup_batch_size as i64).await {
            Ok(count) => {
                summary.cleanup_queue_items_processed = count as u64;
            }
            Err(error) => {
                summary.failed_tasks += 1;
                warn!(
                    error = %error,
                    started_ts,
                    cleanup_batch_size = config.cleanup_batch_size,
                    failed_tasks = summary.failed_tasks,
                    "Failed to process retention cleanup queue"
                );
            }
        }

        match self.prune_finished_cleanup_queue(config.queue_retention_days, started_ts) {
            Ok(count) => {
                summary.cleanup_queue_rows_pruned = count;
            }
            Err(error) => {
                summary.failed_tasks += 1;
                warn!(
                    error = %error,
                    started_ts,
                    queue_retention_days = config.queue_retention_days,
                    failed_tasks = summary.failed_tasks,
                    "Failed to prune retention cleanup queue"
                );
            }
        }

        summary.duration_ms = started.elapsed().as_millis() as i64;
        summary.completed_ts = current_timestamp_millis();
        self.lifecycle_metrics.observe_cycle(summary);
        let result = summary.clone();
        *self.last_lifecycle_summary.write().await = Some(result.clone());

        info!(
            expired_events_deleted = result.expired_events_deleted,
            expired_beacons_deleted = result.expired_beacons_deleted,
            expired_uploads_deleted = result.expired_uploads_deleted,
            expired_audit_events_deleted = result.expired_audit_events_deleted,
            cleanup_queue_items_processed = result.cleanup_queue_items_processed,
            cleanup_queue_rows_pruned = result.cleanup_queue_rows_pruned,
            failed_tasks = result.failed_tasks,
            duration_ms = result.duration_ms,
            "Completed data lifecycle cleanup cycle"
        );

        result
    }

    async fn cleanup_expired_uploads(&self) -> Result<u64, ApiError> {
        self.chunked_upload_storage.cleanup_expired().await
    }

    async fn cleanup_audit_events(&self, retention_days: u64, now_ts: i64) -> Result<u64, ApiError> {
        let Some(cutoff_ts) = Self::cutoff_ts_from_days(now_ts, retention_days) else {
            return Ok(0);
        };

        self.audit_storage
            .delete_events_before(cutoff_ts)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to cleanup audit events", e))
    }

    fn prune_finished_cleanup_queue(&self, _retention_days: u64, _now_ts: i64) -> Result<u64, ApiError> {
        // No-op: cleanup queue table has been removed
        Ok(0)
    }

    fn cutoff_ts_from_days(now_ts: i64, retention_days: u64) -> Option<i64> {
        if retention_days == 0 {
            return None;
        }

        let retention_ms = retention_days.saturating_mul(24 * 60 * 60 * 1000);
        Some(now_ts.saturating_sub(retention_ms.min(i64::MAX as u64) as i64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_protected_event_type(event_type: &str) -> bool {
        matches!(
            event_type,
            "m.room.create" | "m.room.power_levels" | "m.room.join_rules" | "m.room.history_visibility"
        )
    }

    #[test]
    fn test_is_protected_event_type_create() {
        assert!(is_protected_event_type("m.room.create"));
    }

    #[test]
    fn test_is_protected_event_type_power_levels() {
        assert!(is_protected_event_type("m.room.power_levels"));
    }

    #[test]
    fn test_is_protected_event_type_join_rules() {
        assert!(is_protected_event_type("m.room.join_rules"));
    }

    #[test]
    fn test_is_protected_event_type_history_visibility() {
        assert!(is_protected_event_type("m.room.history_visibility"));
    }

    #[test]
    fn test_is_not_protected_event_type_message() {
        assert!(!is_protected_event_type("m.room.message"));
    }

    #[test]
    fn test_is_not_protected_event_type_member() {
        assert!(!is_protected_event_type("m.room.member"));
    }

    #[test]
    fn test_create_room_retention_policy_request() {
        let request = synapse_storage::retention::CreateRoomRetentionPolicyRequest {
            room_id: "!room:example.com".to_string(),
            max_lifetime: Some(86_400_000),
            min_lifetime: Some(0),
            is_expire_on_clients: Some(true),
        };
        assert_eq!(request.room_id, "!room:example.com");
        assert_eq!(request.max_lifetime, Some(86_400_000));
    }

    #[test]
    fn test_update_room_retention_policy_request() {
        let request = synapse_storage::retention::UpdateRoomRetentionPolicyRequest {
            max_lifetime: Some(172800000),
            min_lifetime: None,
            is_expire_on_clients: Some(false),
        };
        assert_eq!(request.max_lifetime, Some(172800000));
        assert!(request.min_lifetime.is_none());
    }

    #[test]
    fn test_update_room_retention_policy_request_default() {
        let request = synapse_storage::retention::UpdateRoomRetentionPolicyRequest::default();
        assert!(request.max_lifetime.is_none());
        assert!(request.min_lifetime.is_none());
        assert!(request.is_expire_on_clients.is_none());
    }

    #[test]
    fn test_room_retention_policy_structure() {
        let policy = synapse_storage::retention::RoomRetentionPolicy {
            id: 1,
            room_id: "!room:example.com".to_string(),
            max_lifetime: Some(86_400_000),
            min_lifetime: 0,
            is_expire_on_clients: true,
            is_server_default: false,
            created_ts: 1234567890,
            updated_ts: 1234567890,
        };
        assert_eq!(policy.room_id, "!room:example.com");
        assert!(policy.max_lifetime.is_some());
        assert!(!policy.is_server_default);
    }

    #[test]
    fn test_server_retention_policy_structure() {
        let policy = synapse_storage::retention::ServerRetentionPolicy {
            id: 1,
            max_lifetime: Some(604800000),
            min_lifetime: 0,
            is_expire_on_clients: true,
            created_ts: 1234567890,
            updated_ts: 1234567890,
        };
        assert!(policy.max_lifetime.is_some());
        assert_eq!(policy.min_lifetime, 0);
    }

    #[test]
    fn test_retention_cleanup_log_structure() {
        let log = synapse_storage::retention::RetentionCleanupLog {
            id: 1,
            room_id: "!room:example.com".to_string(),
            events_deleted: 100,
            state_events_deleted: 5,
            media_deleted: 10,
            bytes_freed: 1024000,
            started_ts: 1234567890,
            completed_ts: Some(1234567999),
            status: "completed".to_string(),
            error_message: None,
        };
        assert_eq!(log.events_deleted, 100);
        assert_eq!(log.status, "completed");
        assert!(log.error_message.is_none());
    }

    #[test]
    fn test_retention_cleanup_log_failed() {
        let log = synapse_storage::retention::RetentionCleanupLog {
            id: 1,
            room_id: "!room:example.com".to_string(),
            events_deleted: 0,
            state_events_deleted: 0,
            media_deleted: 0,
            bytes_freed: 0,
            started_ts: 1234567890,
            completed_ts: Some(1234567999),
            status: "failed".to_string(),
            error_message: Some("Database error".to_string()),
        };
        assert_eq!(log.status, "failed");
        assert!(log.error_message.is_some());
    }

    #[test]
    fn test_cutoff_ts_from_days_zero_disables_cleanup() {
        assert_eq!(RetentionService::cutoff_ts_from_days(1_000, 0), None);
    }

    #[test]
    fn test_cutoff_ts_from_days_positive_retention() {
        assert_eq!(RetentionService::cutoff_ts_from_days(172_800_000, 1), Some(86_400_000));
    }
}

#[cfg(test)]
mod db_tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    use sqlx::postgres::PgPool;
    use synapse_common::metrics::MetricsCollector;
    use synapse_common::ApiErrorKind;
    use synapse_storage::audit::AuditEventStorage;
    use synapse_storage::media::ChunkedUploadStorage;
    use synapse_storage::retention::{CreateRoomRetentionPolicyRequest, UpdateServerRetentionPolicyRequest};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

    fn unique_test_suffix() -> String {
        let counter = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        format!("{counter}{nanos}")
    }

    /// Each test gets its own schema cloned from the full baseline template.
    ///
    /// These tests used to run against the shared `public` schema, which made
    /// them the canary for cross-test pollution: `test_run_cleanup_requires_room_policy`
    /// sets the server policy to `max_lifetime = NULL` and expects `run_cleanup`
    /// to reject the room, but a concurrently running test that had just set a
    /// server-wide policy (visible through `public`) made the cleanup succeed
    /// instead. `#[serial_test::serial]` does not help here — it is a
    /// process-local lock and nextest runs every test in its own process.
    ///
    /// The clone copies table *structure* (`CREATE TABLE ... (LIKE ... INCLUDING ALL)`),
    /// not rows, so the singleton `server_retention_policy` row that the v11
    /// baseline seeds has to be recreated here.
    async fn test_pool() -> Arc<PgPool> {
        let pool = crate::test_utils::prepare_isolated_test_pool().await.expect("Failed to prepare isolated test pool");
        sqlx::query(
            "INSERT INTO server_retention_policy
                 (id, max_lifetime, min_lifetime, is_expire_on_clients, created_ts, updated_ts)
             VALUES (1, NULL, 0, FALSE, 0, 0)
             ON CONFLICT (id) DO NOTHING",
        )
        .execute(&*pool)
        .await
        .expect("failed to seed server_retention_policy in the isolated schema");
        pool
    }

    async fn pool_schema_name(pool: &Arc<PgPool>) -> String {
        sqlx::query_scalar("SELECT current_schema()").fetch_one(&**pool).await.expect("current_schema")
    }

    fn build_retention_service(pool: Arc<PgPool>) -> super::RetentionService {
        let metrics = Arc::new(MetricsCollector::new());
        super::RetentionService::new(
            Arc::new(synapse_storage::retention::RetentionStorage::new(&pool)),
            Arc::new(ChunkedUploadStorage::new(&pool)),
            &metrics,
            Arc::new(AuditEventStorage::new(&pool)),
        )
    }

    /// Reset server_retention_policy row to its seeded default so tests
    /// that mutate it don't leak global state to subsequent tests.
    /// Uses raw SQL because `update_server_policy` COALESCEs NULL inputs and
    /// therefore cannot clear `max_lifetime` back to NULL.
    async fn reset_server_policy(pool: &Arc<PgPool>) {
        let _ = sqlx::query(
            "UPDATE server_retention_policy SET max_lifetime = NULL, min_lifetime = 0, is_expire_on_clients = false WHERE id = 1",
        )
        .execute(&**pool)
        .await;
    }

    /// Helper: ensure a minimal room row exists in the DB.
    async fn ensure_test_room(pool: &Arc<PgPool>, room_id: &str) {
        let now = synapse_common::current_timestamp_millis();
        sqlx::query("INSERT INTO rooms (room_id, creator, created_ts) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING")
            .bind(room_id)
            .bind("test_creator")
            .bind(now)
            .execute(&**pool)
            .await
            .expect("failed to create test room");
    }

    /// Helper: clear retention state for a room.
    async fn cleanup_test_room(pool: &Arc<PgPool>, room_id: &str) {
        let _ =
            sqlx::query("DELETE FROM room_retention_policies WHERE room_id = $1").bind(room_id).execute(&**pool).await;
        let _ = sqlx::query("DELETE FROM events WHERE room_id = $1").bind(room_id).execute(&**pool).await;
        let _ = sqlx::query("DELETE FROM rooms WHERE room_id = $1").bind(room_id).execute(&**pool).await;
    }

    // ------------------------------------------------------------------
    // 1. set_room_policy -> create_room_policy -> get_room_policy 一致性
    // ------------------------------------------------------------------
    #[tokio::test]
    async fn test_set_and_get_room_policy() {
        let pool = test_pool().await;
        let service = build_retention_service(pool.clone());
        let room_id = format!("!test_room_1:example.com{}", unique_test_suffix());
        ensure_test_room(&pool, &room_id).await;

        let request = CreateRoomRetentionPolicyRequest {
            room_id: room_id.clone(),
            max_lifetime: Some(86_400_000), // 1 day
            min_lifetime: Some(0),
            is_expire_on_clients: Some(true),
        };

        let policy = service.set_room_policy(request).await.expect("set_room_policy failed");
        assert_eq!(policy.room_id, room_id);
        assert_eq!(policy.max_lifetime, Some(86_400_000));
        assert!(!policy.is_server_default);

        let fetched = service.get_room_policy(&room_id).await.expect("get_room_policy failed");
        assert_eq!(fetched.as_ref().unwrap().max_lifetime, Some(86_400_000));

        cleanup_test_room(&pool, &room_id).await;
    }

    // ------------------------------------------------------------------
    // 2. get_effective_policy: room policy > server policy (serial: mutates global server state)
    // ------------------------------------------------------------------
    #[serial_test::serial]
    #[tokio::test]
    async fn test_effective_policy_room_over_server() {
        let pool = test_pool().await;
        reset_server_policy(&pool).await;
        let service = build_retention_service(pool.clone());
        let room_id = format!("!test_room_2:example.com{}", unique_test_suffix());
        ensure_test_room(&pool, &room_id).await;

        // Set a server-wide default policy (7-day retention).
        let server_req = UpdateServerRetentionPolicyRequest {
            max_lifetime: Some(604_800_000), // 7 days
            min_lifetime: Some(86_400_000),
            is_expire_on_clients: Some(true),
        };
        let server_policy = service.update_server_policy(server_req).await.expect("update_server_policy failed");
        assert_eq!(server_policy.max_lifetime, Some(604_800_000));

        // Room policy with 1-day retention should override the server default.
        let room_req = CreateRoomRetentionPolicyRequest {
            room_id: room_id.clone(),
            max_lifetime: Some(86_400_000),
            min_lifetime: Some(0),
            is_expire_on_clients: Some(true),
        };
        service.set_room_policy(room_req).await.expect("set_room_policy failed");

        let effective = service.get_effective_policy(&room_id).await.expect("get_effective_policy failed");
        // Room 1-day retention wins over the server 7-day default.
        assert_eq!(effective.max_lifetime, Some(86_400_000));
        // min_lifetime / is_expire_on_clients come from the room policy when one exists.
        assert_eq!(effective.min_lifetime, 0);
        assert!(effective.is_expire_on_clients);

        cleanup_test_room(&pool, &room_id).await;
    }

    // ------------------------------------------------------------------
    // 3. get_effective_policy: falls back to server policy when no room policy (serial)
    // ------------------------------------------------------------------
    #[serial_test::serial]
    #[tokio::test]
    async fn test_effective_policy_server_fallback() {
        let pool = test_pool().await;
        reset_server_policy(&pool).await;
        let service = build_retention_service(pool.clone());
        let room_id = format!("!test_room_3:example.com{}", unique_test_suffix());
        ensure_test_room(&pool, &room_id).await;

        // Server default policy (3-day retention).
        let server_req = UpdateServerRetentionPolicyRequest {
            max_lifetime: Some(259_200_000), // 3 days
            min_lifetime: Some(43_200_000),  // 12 hours
            is_expire_on_clients: Some(false),
        };
        service.update_server_policy(server_req).await.expect("update_server_policy failed");

        // No room policy → effective policy must inherit the server default.
        let effective = service.get_effective_policy(&room_id).await.expect("get_effective_policy failed");
        assert_eq!(effective.max_lifetime, Some(259_200_000));
        assert_eq!(effective.min_lifetime, 43_200_000);
        assert!(!effective.is_expire_on_clients); // matches server setting

        cleanup_test_room(&pool, &room_id).await;
    }

    // ------------------------------------------------------------------
    // 4. run_cleanup: no room policy + NULL server max_lifetime → bad_request (serial: depends on server state)
    // ------------------------------------------------------------------
    #[serial_test::serial]
    #[tokio::test]
    async fn test_run_cleanup_requires_room_policy() {
        let pool = test_pool().await;
        reset_server_policy(&pool).await;
        let service = build_retention_service(pool.clone());
        let room_id = format!("!test_room_4:example.com{}", unique_test_suffix());
        ensure_test_room(&pool, &room_id).await;

        // Ensure no room policy exists for this room
        let _ =
            sqlx::query("DELETE FROM room_retention_policies WHERE room_id = $1").bind(&room_id).execute(&*pool).await;

        // Without room policy AND server max_lifetime = NULL,
        // run_cleanup should fail with bad_request
        let result = service.run_cleanup(&room_id).await;
        assert!(result.is_err(), "run_cleanup without policy should error");
        let err = result.unwrap_err();
        assert_eq!(err.kind, ApiErrorKind::BadRequest);
        assert!(err.message.contains("No retention policy configured"));

        cleanup_test_room(&pool, &room_id).await;
    }

    // ------------------------------------------------------------------
    // 5. set_room_policy validates max_lifetime is non-negative
    // ------------------------------------------------------------------
    #[tokio::test]
    async fn test_set_room_policy_rejects_negative_max_lifetime() {
        let pool = test_pool().await;
        let service = build_retention_service(pool.clone());
        let room_id = format!("!test_room_5:example.com{}", unique_test_suffix());
        ensure_test_room(&pool, &room_id).await;

        let request = CreateRoomRetentionPolicyRequest {
            room_id: room_id.clone(),
            max_lifetime: Some(-1_000), // invalid: negative
            min_lifetime: Some(0),
            is_expire_on_clients: Some(false),
        };

        let result = service.set_room_policy(request).await;
        assert!(result.is_err(), "negative max_lifetime should be rejected");
        let err = result.unwrap_err();
        assert_eq!(err.kind, ApiErrorKind::BadRequest);
        assert!(err.message.contains("cannot be negative"));

        cleanup_test_room(&pool, &room_id).await;
    }

    // ------------------------------------------------------------------
    // 6. run_cleanup end-to-end: deletes expired events, preserves protected + fresh
    // ------------------------------------------------------------------
    #[tokio::test]
    async fn test_run_cleanup_deletes_expired_events() {
        let pool = test_pool().await;
        let service = build_retention_service(pool.clone());
        let room_id = format!("!test_room_6:example.com{}", unique_test_suffix());
        ensure_test_room(&pool, &room_id).await;

        // Set 1-day retention policy
        let policy_req = CreateRoomRetentionPolicyRequest {
            room_id: room_id.clone(),
            max_lifetime: Some(86_400_000), // 1 day in ms
            min_lifetime: Some(0),
            is_expire_on_clients: Some(true),
        };
        service.set_room_policy(policy_req).await.expect("set_room_policy failed");

        // Insert events with known timestamps
        let now = synapse_common::current_timestamp_millis();
        let expired_ts = now - 172_800_000; // ~2 days ago (beyond 1-day retention)
        let fresh_ts = now - 43_200_000; // ~12 hours ago (within retention)
        let suffix = unique_test_suffix();

        // Event that SHOULD be deleted (expired)
        sqlx::query(
            "INSERT INTO events (event_id, room_id, sender, event_type, content, origin_server_ts, state_key) VALUES ($1, $2, $3, 'm.room.message', '{}'::jsonb, $4, NULL)"
        )
        .bind(format!("$exp{}:example.com{}", suffix, unique_test_suffix())).bind(&room_id).bind("@alice:example.com").bind(expired_ts)
        .execute(&*pool)
        .await
        .expect("insert expired event failed");

        // Event that should NOT be deleted (fresh)
        sqlx::query(
            "INSERT INTO events (event_id, room_id, sender, event_type, content, origin_server_ts, state_key) VALUES ($1, $2, $3, 'm.room.message', '{}'::jsonb, $4, NULL)"
        )
        .bind(format!("$fresh{}:example.com{}", suffix, unique_test_suffix())).bind(&room_id).bind("@alice:example.com").bind(fresh_ts)
        .execute(&*pool)
        .await
        .expect("insert fresh event failed");

        // Protected event types that survive regardless of age
        sqlx::query(
            "INSERT INTO events (event_id, room_id, sender, event_type, content, origin_server_ts, state_key) VALUES ($1, $2, $3, 'm.room.create', '{}'::jsonb, $4, NULL) ON CONFLICT DO NOTHING"
        )
        .bind(format!("$create{}:example.com{}", suffix, unique_test_suffix())).bind(&room_id).bind("@alice:example.com").bind(now)
        .execute(&*pool)
        .await
        .expect("insert m.room.create failed");

        // Run cleanup
        let result = service.run_cleanup(&room_id).await;
        assert!(result.is_ok(), "run_cleanup should succeed");
        let log = result.unwrap();
        // Exactly 1 event deleted (the expired message event)
        assert_eq!(log.events_deleted, 1, "should delete exactly 1 expired event");

        // Verify remaining state
        let rows: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM events WHERE room_id = $1")
            .bind(&room_id)
            .fetch_one(&*pool)
            .await
            .expect("query event count failed");
        // Expected survivors: 1 fresh + 1 create (protected) = 2
        assert_eq!(rows.0, 2, "expired event deleted, protected create survives");

        cleanup_test_room(&pool, &room_id).await;
    }

    /// Regression: the runtime DatabaseInitService does not create the
    /// retention tables, so an isolated schema without the full baseline made
    /// `server_retention_policy` resolve to the shared `public` schema.
    #[tokio::test]
    async fn isolated_schema_contains_retention_tables() {
        let pool = test_pool().await;
        let (local_tables, resolved): (i64, Option<String>) = sqlx::query_as(
            r#"
            SELECT
              (SELECT count(*) FROM pg_tables
                WHERE schemaname = current_schema()
                  AND tablename IN ('server_retention_policy', 'room_retention_policies')),
              (SELECT n.nspname FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
                WHERE c.oid = 'server_retention_policy'::regclass)
            "#,
        )
        .fetch_one(&*pool)
        .await
        .expect("retention table probe");
        assert_eq!(local_tables, 2, "both retention tables must exist in the isolated schema");
        assert_eq!(
            resolved.as_deref(),
            Some(pool_schema_name(&pool).await.as_str()),
            "server_retention_policy must resolve inside the isolated schema, not public"
        );
    }
}
