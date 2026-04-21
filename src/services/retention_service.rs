use crate::common::config::RetentionConfig;
use crate::common::metrics::{Counter, Gauge, Histogram, MetricsCollector};
use crate::common::ApiError;
#[cfg(feature = "beacons")]
use crate::services::beacon_service::BeaconService;
use crate::services::media::chunked_upload::ChunkedUploadService;
use crate::storage::audit::AuditEventStorage;
use crate::storage::retention::*;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::{debug, error, info, instrument, warn};

#[derive(Debug, Clone, Default)]
pub struct DataLifecycleCleanupSummary {
    pub started_ts: i64,
    pub completed_ts: i64,
    pub duration_ms: i64,
    pub expired_events_deleted: u64,
    pub expired_beacons_deleted: u64,
    pub expired_uploads_deleted: u64,
    pub expired_audit_events_deleted: u64,
    pub cleanup_queue_items_processed: u64,
    pub cleanup_queue_rows_pruned: u64,
    pub failed_tasks: u64,
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
            cycles_failed_total: metrics
                .register_counter("retention_lifecycle_cycles_failed_total".to_string()),
            events_deleted_total: metrics
                .register_counter("retention_lifecycle_events_deleted_total".to_string()),
            beacons_deleted_total: metrics
                .register_counter("retention_lifecycle_beacons_deleted_total".to_string()),
            uploads_deleted_total: metrics
                .register_counter("retention_lifecycle_uploads_deleted_total".to_string()),
            audit_events_deleted_total: metrics
                .register_counter("retention_lifecycle_audit_events_deleted_total".to_string()),
            queue_processed_total: metrics
                .register_counter("retention_lifecycle_queue_processed_total".to_string()),
            queue_pruned_total: metrics
                .register_counter("retention_lifecycle_queue_pruned_total".to_string()),
            last_run_ts: metrics.register_gauge("retention_lifecycle_last_run_ts".to_string()),
            last_failure_ts: metrics
                .register_gauge("retention_lifecycle_last_failure_ts".to_string()),
            last_duration_ms: metrics
                .register_gauge("retention_lifecycle_last_duration_ms".to_string()),
            last_failed_tasks: metrics
                .register_gauge("retention_lifecycle_last_failed_tasks".to_string()),
            last_events_deleted: metrics
                .register_gauge("retention_lifecycle_last_events_deleted".to_string()),
            last_beacons_deleted: metrics
                .register_gauge("retention_lifecycle_last_beacons_deleted".to_string()),
            last_uploads_deleted: metrics
                .register_gauge("retention_lifecycle_last_uploads_deleted".to_string()),
            last_audit_events_deleted: metrics
                .register_gauge("retention_lifecycle_last_audit_events_deleted".to_string()),
            last_queue_processed: metrics
                .register_gauge("retention_lifecycle_last_queue_processed".to_string()),
            last_queue_pruned: metrics
                .register_gauge("retention_lifecycle_last_queue_pruned".to_string()),
            cycle_duration_ms: metrics
                .register_histogram("retention_lifecycle_cycle_duration_ms".to_string()),
        }
    }

    fn observe_cycle(&self, summary: &DataLifecycleCleanupSummary) {
        self.cycles_total.inc();
        self.events_deleted_total
            .inc_by(summary.expired_events_deleted);
        self.beacons_deleted_total
            .inc_by(summary.expired_beacons_deleted);
        self.uploads_deleted_total
            .inc_by(summary.expired_uploads_deleted);
        self.audit_events_deleted_total
            .inc_by(summary.expired_audit_events_deleted);
        self.queue_processed_total
            .inc_by(summary.cleanup_queue_items_processed);
        self.queue_pruned_total
            .inc_by(summary.cleanup_queue_rows_pruned);
        self.last_run_ts.set(summary.completed_ts as f64);
        self.last_duration_ms.set(summary.duration_ms as f64);
        self.last_failed_tasks.set(summary.failed_tasks as f64);
        self.last_events_deleted
            .set(summary.expired_events_deleted as f64);
        self.last_beacons_deleted
            .set(summary.expired_beacons_deleted as f64);
        self.last_uploads_deleted
            .set(summary.expired_uploads_deleted as f64);
        self.last_audit_events_deleted
            .set(summary.expired_audit_events_deleted as f64);
        self.last_queue_processed
            .set(summary.cleanup_queue_items_processed as f64);
        self.last_queue_pruned
            .set(summary.cleanup_queue_rows_pruned as f64);
        self.cycle_duration_ms.observe(summary.duration_ms as f64);

        if summary.failed_tasks > 0 {
            self.cycles_failed_total.inc();
            self.last_failure_ts.set(summary.completed_ts as f64);
        }
    }
}

pub struct RetentionService {
    storage: Arc<RetentionStorage>,
    pool: Arc<PgPool>,
    audit_storage: Arc<AuditEventStorage>,
    lifecycle_metrics: RetentionLifecycleMetrics,
    last_lifecycle_summary: Arc<RwLock<Option<DataLifecycleCleanupSummary>>>,
}

impl RetentionService {
    pub fn new(
        storage: Arc<RetentionStorage>,
        pool: Arc<PgPool>,
        metrics: Arc<MetricsCollector>,
        audit_storage: Arc<AuditEventStorage>,
    ) -> Self {
        Self {
            storage,
            pool,
            audit_storage,
            lifecycle_metrics: RetentionLifecycleMetrics::new(&metrics),
            last_lifecycle_summary: Arc::new(RwLock::new(None)),
        }
    }

    #[instrument(skip(self))]
    pub async fn get_room_policy(
        &self,
        room_id: &str,
    ) -> Result<Option<RoomRetentionPolicy>, ApiError> {
        let policy = self
            .storage
            .get_room_policy(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get room policy: {}", e)))?;

        Ok(policy)
    }

    #[instrument(skip(self))]
    pub async fn get_effective_policy(
        &self,
        room_id: &str,
    ) -> Result<EffectiveRetentionPolicy, ApiError> {
        let policy = self
            .storage
            .get_effective_policy(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get effective policy: {}", e)))?;

        Ok(policy)
    }

    #[instrument(skip(self))]
    pub async fn set_room_policy(
        &self,
        request: CreateRoomRetentionPolicyRequest,
    ) -> Result<RoomRetentionPolicy, ApiError> {
        info!("Setting retention policy for room: {}", request.room_id);

        if let Some(max_lifetime) = request.max_lifetime {
            if max_lifetime < 0 {
                return Err(ApiError::bad_request("max_lifetime cannot be negative"));
            }
        }

        let policy = self
            .storage
            .create_room_policy(request)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to create room policy: {}", e)))?;

        Ok(policy)
    }

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
            .map_err(|e| ApiError::internal(format!("Failed to update room policy: {}", e)))?;

        Ok(policy)
    }

    #[instrument(skip(self))]
    pub async fn delete_room_policy(&self, room_id: &str) -> Result<(), ApiError> {
        info!("Deleting retention policy for room: {}", room_id);

        self.storage
            .delete_room_policy(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to delete room policy: {}", e)))?;

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn get_server_policy(&self) -> Result<ServerRetentionPolicy, ApiError> {
        let policy = self
            .storage
            .get_server_policy()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get server policy: {}", e)))?;

        Ok(policy)
    }

    #[instrument(skip(self))]
    pub async fn update_server_policy(
        &self,
        request: UpdateServerRetentionPolicyRequest,
    ) -> Result<ServerRetentionPolicy, ApiError> {
        info!("Updating server retention policy");

        let policy = self
            .storage
            .update_server_policy(request)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update server policy: {}", e)))?;

        Ok(policy)
    }

    #[instrument(skip(self))]
    pub async fn run_cleanup(&self, room_id: &str) -> Result<RetentionCleanupLog, ApiError> {
        info!("Running retention cleanup for room: {}", room_id);

        let policy = self
            .storage
            .get_effective_policy(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get policy: {}", e)))?;

        if policy.max_lifetime.is_none() {
            return Err(ApiError::bad_request(
                "No retention policy configured for this room",
            ));
        }

        let log = self
            .storage
            .create_cleanup_log(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to create cleanup log: {}", e)))?;

        let max_lifetime = policy
            .max_lifetime
            .expect("max_lifetime already checked above");
        let cutoff_ts = chrono::Utc::now().timestamp_millis() - max_lifetime;

        match self.storage.delete_events_before(room_id, cutoff_ts).await {
            Ok(deleted_count) => {
                if let Err(e) = self
                    .storage
                    .complete_cleanup_log(log.id, deleted_count, 0, 0, 0)
                    .await
                {
                    warn!("Failed to complete cleanup log: {}", e);
                }

                if let Err(e) = self
                    .storage
                    .update_stats(room_id, 0, 0, deleted_count, None)
                    .await
                {
                    warn!("Failed to update stats: {}", e);
                }

                info!(
                    "Deleted {} expired events from room {}",
                    deleted_count, room_id
                );

                let completed_log = RetentionCleanupLog {
                    id: log.id,
                    room_id: room_id.to_string(),
                    events_deleted: deleted_count,
                    state_events_deleted: 0,
                    media_deleted: 0,
                    bytes_freed: 0,
                    started_ts: log.started_ts,
                    completed_ts: Some(chrono::Utc::now().timestamp_millis()),
                    status: "completed".to_string(),
                    error_message: None,
                };

                Ok(completed_log)
            }
            Err(e) => {
                let error_msg = format!("Failed to delete events: {}", e);
                error!("{}", error_msg);

                if let Err(err) = self.storage.fail_cleanup_log(log.id, &error_msg).await {
                    warn!("Failed to fail cleanup log: {}", err);
                }

                Err(ApiError::internal(error_msg))
            }
        }
    }

    #[instrument(skip(self))]
    pub async fn process_pending_cleanups(&self, limit: i64) -> Result<usize, ApiError> {
        let items = self
            .storage
            .get_pending_cleanups(limit)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get pending cleanups: {}", e)))?;

        let mut processed = 0;
        for item in items {
            match self.process_cleanup_item(&item).await {
                Ok(_) => {
                    if let Err(e) = self.storage.mark_cleanup_processed(item.id).await {
                        warn!("Failed to mark cleanup processed: {}", e);
                    }
                    processed += 1;
                }
                Err(e) => {
                    if let Err(err) = self
                        .storage
                        .mark_cleanup_failed(item.id, &e.to_string())
                        .await
                    {
                        warn!("Failed to mark cleanup failed: {}", err);
                    }
                }
            }
        }

        Ok(processed)
    }

    async fn process_cleanup_item(&self, item: &RetentionCleanupQueueItem) -> Result<(), ApiError> {
        if let (Some(event_id), Some(event_type)) = (&item.event_id, &item.event_type) {
            if Self::is_protected_event_type(event_type) {
                debug!("Skipping protected event type: {}", event_type);
                return Ok(());
            }

            sqlx::query("DELETE FROM events WHERE event_id = $1")
                .bind(event_id)
                .execute(&*self.pool)
                .await
                .map_err(|e| ApiError::internal(format!("Failed to delete event: {}", e)))?;

            self.storage
                .record_deleted_event(&item.room_id, event_id, "retention")
                .await
                .map_err(|e| {
                    ApiError::internal(format!("Failed to record deleted event: {}", e))
                })?;
        }

        Ok(())
    }

    fn is_protected_event_type(event_type: &str) -> bool {
        matches!(
            event_type,
            "m.room.create"
                | "m.room.power_levels"
                | "m.room.join_rules"
                | "m.room.history_visibility"
        )
    }

    #[instrument(skip(self))]
    pub async fn schedule_room_cleanup(&self, room_id: &str) -> Result<i64, ApiError> {
        info!("Scheduling retention cleanup for room: {}", room_id);

        let count = self
            .storage
            .schedule_room_cleanup(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to schedule cleanup: {}", e)))?;

        Ok(count)
    }

    #[instrument(skip(self))]
    pub async fn get_stats(&self, room_id: &str) -> Result<Option<RetentionStats>, ApiError> {
        let stats = self
            .storage
            .get_stats(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get stats: {}", e)))?;

        Ok(stats)
    }

    #[instrument(skip(self))]
    pub async fn get_cleanup_logs(
        &self,
        room_id: &str,
        limit: i64,
    ) -> Result<Vec<RetentionCleanupLog>, ApiError> {
        let logs = self
            .storage
            .get_cleanup_logs(room_id, limit)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get cleanup logs: {}", e)))?;

        Ok(logs)
    }

    #[instrument(skip(self))]
    pub async fn get_deleted_events(
        &self,
        room_id: &str,
        since_ts: i64,
    ) -> Result<Vec<DeletedEventIndex>, ApiError> {
        let events = self
            .storage
            .get_deleted_events(room_id, since_ts)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get deleted events: {}", e)))?;

        Ok(events)
    }

    #[instrument(skip(self))]
    pub async fn get_rooms_with_policies(&self) -> Result<Vec<RoomRetentionPolicy>, ApiError> {
        let policies =
            self.storage.get_rooms_with_policies().await.map_err(|e| {
                ApiError::internal(format!("Failed to get rooms with policies: {}", e))
            })?;

        Ok(policies)
    }

    #[instrument(skip(self))]
    pub async fn get_pending_cleanup_count(&self, room_id: &str) -> Result<i64, ApiError> {
        let count = self
            .storage
            .get_pending_cleanup_count(room_id)
            .await
            .map_err(|e| {
                ApiError::internal(format!("Failed to get pending cleanup count: {}", e))
            })?;

        Ok(count)
    }

    #[instrument(skip(self))]
    pub async fn is_event_expired(
        &self,
        room_id: &str,
        origin_server_ts: i64,
    ) -> Result<bool, ApiError> {
        let policy = self
            .storage
            .get_effective_policy(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get policy: {}", e)))?;

        if let Some(max_lifetime) = policy.max_lifetime {
            let cutoff_ts = chrono::Utc::now().timestamp_millis() - max_lifetime;
            Ok(origin_server_ts < cutoff_ts)
        } else {
            Ok(false)
        }
    }

    pub async fn run_scheduled_cleanups(&self) -> Result<usize, ApiError> {
        info!("Running scheduled retention cleanups");

        let policies = self
            .storage
            .get_rooms_with_policies()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get policies: {}", e)))?;

        let mut total_cleaned = 0;

        for policy in policies {
            if policy.max_lifetime.is_some() {
                match self.run_cleanup(&policy.room_id).await {
                    Ok(log) => {
                        total_cleaned += log.events_deleted as usize;
                    }
                    Err(e) => {
                        warn!("Failed to run cleanup for room {}: {}", policy.room_id, e);
                    }
                }
            }
        }

        Ok(total_cleaned)
    }

    pub async fn get_last_lifecycle_summary(&self) -> Option<DataLifecycleCleanupSummary> {
        self.last_lifecycle_summary.read().await.clone()
    }

    #[cfg(feature = "beacons")]
    #[instrument(skip(self, beacon_service, config))]
    pub async fn run_data_lifecycle_cycle(
        &self,
        beacon_service: &BeaconService,
        config: &RetentionConfig,
    ) -> DataLifecycleCleanupSummary {
        let started_ts = chrono::Utc::now().timestamp_millis();
        let started = Instant::now();
        let mut summary = DataLifecycleCleanupSummary {
            started_ts,
            ..Default::default()
        };

        match self.run_scheduled_cleanups().await {
            Ok(count) => {
                summary.expired_events_deleted = count as u64;
            }
            Err(error) => {
                summary.failed_tasks += 1;
                warn!("Failed to run scheduled retention cleanups: {}", error);
            }
        }

        match beacon_service.cleanup_expired_beacons().await {
            Ok(count) => {
                summary.expired_beacons_deleted = count;
            }
            Err(error) => {
                summary.failed_tasks += 1;
                warn!("Failed to cleanup expired beacons: {}", error);
            }
        }

        self.finish_lifecycle_cycle(&mut summary, config, started_ts, started)
            .await
    }

    #[cfg(not(feature = "beacons"))]
    #[instrument(skip(self, config))]
    pub async fn run_data_lifecycle_cycle_no_beacons(
        &self,
        config: &RetentionConfig,
    ) -> DataLifecycleCleanupSummary {
        let started_ts = chrono::Utc::now().timestamp_millis();
        let started = Instant::now();
        let mut summary = DataLifecycleCleanupSummary {
            started_ts,
            ..Default::default()
        };

        match self.run_scheduled_cleanups().await {
            Ok(count) => {
                summary.expired_events_deleted = count as u64;
            }
            Err(error) => {
                summary.failed_tasks += 1;
                warn!("Failed to run scheduled retention cleanups: {}", error);
            }
        }

        self.finish_lifecycle_cycle(&mut summary, config, started_ts, started)
            .await
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
                warn!("Failed to cleanup expired uploads: {}", error);
            }
        }

        match self
            .cleanup_audit_events(config.audit_retention_days, started_ts)
            .await
        {
            Ok(count) => {
                summary.expired_audit_events_deleted = count;
            }
            Err(error) => {
                summary.failed_tasks += 1;
                warn!("Failed to cleanup expired audit events: {}", error);
            }
        }

        match self
            .process_pending_cleanups(config.cleanup_batch_size as i64)
            .await
        {
            Ok(count) => {
                summary.cleanup_queue_items_processed = count as u64;
            }
            Err(error) => {
                summary.failed_tasks += 1;
                warn!("Failed to process retention cleanup queue: {}", error);
            }
        }

        match self
            .prune_finished_cleanup_queue(config.queue_retention_days, started_ts)
            .await
        {
            Ok(count) => {
                summary.cleanup_queue_rows_pruned = count;
            }
            Err(error) => {
                summary.failed_tasks += 1;
                warn!("Failed to prune retention cleanup queue: {}", error);
            }
        }

        summary.duration_ms = started.elapsed().as_millis() as i64;
        summary.completed_ts = chrono::Utc::now().timestamp_millis();
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
        ChunkedUploadService::new(self.pool.clone())
            .cleanup_expired()
            .await
    }

    async fn cleanup_audit_events(
        &self,
        retention_days: u64,
        now_ts: i64,
    ) -> Result<u64, ApiError> {
        let Some(cutoff_ts) = Self::cutoff_ts_from_days(now_ts, retention_days) else {
            return Ok(0);
        };

        self.audit_storage
            .delete_events_before(cutoff_ts)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to cleanup audit events: {}", e)))
    }

    async fn prune_finished_cleanup_queue(
        &self,
        retention_days: u64,
        now_ts: i64,
    ) -> Result<u64, ApiError> {
        let Some(cutoff_ts) = Self::cutoff_ts_from_days(now_ts, retention_days) else {
            return Ok(0);
        };

        self.storage
            .cleanup_finished_queue_items_before(cutoff_ts)
            .await
            .map(|count| count as u64)
            .map_err(|e| {
                ApiError::internal(format!("Failed to cleanup retention queue rows: {}", e))
            })
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

    #[test]
    fn test_is_protected_event_type_create() {
        assert!(RetentionService::is_protected_event_type("m.room.create"));
    }

    #[test]
    fn test_is_protected_event_type_power_levels() {
        assert!(RetentionService::is_protected_event_type(
            "m.room.power_levels"
        ));
    }

    #[test]
    fn test_is_protected_event_type_join_rules() {
        assert!(RetentionService::is_protected_event_type(
            "m.room.join_rules"
        ));
    }

    #[test]
    fn test_is_protected_event_type_history_visibility() {
        assert!(RetentionService::is_protected_event_type(
            "m.room.history_visibility"
        ));
    }

    #[test]
    fn test_is_not_protected_event_type_message() {
        assert!(!RetentionService::is_protected_event_type("m.room.message"));
    }

    #[test]
    fn test_is_not_protected_event_type_member() {
        assert!(!RetentionService::is_protected_event_type("m.room.member"));
    }

    #[test]
    fn test_create_room_retention_policy_request() {
        let request = crate::storage::retention::CreateRoomRetentionPolicyRequest {
            room_id: "!room:example.com".to_string(),
            max_lifetime: Some(86400000),
            min_lifetime: Some(0),
            expire_on_clients: Some(true),
        };
        assert_eq!(request.room_id, "!room:example.com");
        assert_eq!(request.max_lifetime, Some(86400000));
    }

    #[test]
    fn test_update_room_retention_policy_request() {
        let request = crate::storage::retention::UpdateRoomRetentionPolicyRequest {
            max_lifetime: Some(172800000),
            min_lifetime: None,
            expire_on_clients: Some(false),
        };
        assert_eq!(request.max_lifetime, Some(172800000));
        assert!(request.min_lifetime.is_none());
    }

    #[test]
    fn test_update_room_retention_policy_request_default() {
        let request = crate::storage::retention::UpdateRoomRetentionPolicyRequest::default();
        assert!(request.max_lifetime.is_none());
        assert!(request.min_lifetime.is_none());
        assert!(request.expire_on_clients.is_none());
    }

    #[test]
    fn test_room_retention_policy_structure() {
        let policy = crate::storage::retention::RoomRetentionPolicy {
            id: 1,
            room_id: "!room:example.com".to_string(),
            max_lifetime: Some(86400000),
            min_lifetime: 0,
            expire_on_clients: true,
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
        let policy = crate::storage::retention::ServerRetentionPolicy {
            id: 1,
            max_lifetime: Some(604800000),
            min_lifetime: 0,
            expire_on_clients: true,
            created_ts: 1234567890,
            updated_ts: 1234567890,
        };
        assert!(policy.max_lifetime.is_some());
        assert_eq!(policy.min_lifetime, 0);
    }

    #[test]
    fn test_retention_cleanup_log_structure() {
        let log = crate::storage::retention::RetentionCleanupLog {
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
        let log = crate::storage::retention::RetentionCleanupLog {
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
    fn test_retention_stats_structure() {
        let stats = crate::storage::retention::RetentionStats {
            id: 1,
            room_id: "!room:example.com".to_string(),
            total_events: 1000,
            events_in_retention: 800,
            events_expired: 200,
            last_cleanup_ts: Some(1234567890),
            next_cleanup_ts: Some(1234657890),
        };
        assert_eq!(stats.total_events, 1000);
        assert_eq!(stats.events_expired, 200);
    }

    #[test]
    fn test_deleted_event_index() {
        let deleted = crate::storage::retention::DeletedEventIndex {
            id: 1,
            room_id: "!room:example.com".to_string(),
            event_id: "$event:example.com".to_string(),
            deletion_ts: 1234567890,
            reason: "retention".to_string(),
        };
        assert_eq!(deleted.event_id, "$event:example.com");
        assert_eq!(deleted.reason, "retention");
    }

    #[test]
    fn test_retention_cleanup_queue_item() {
        let item = crate::storage::retention::RetentionCleanupQueueItem {
            id: 1,
            room_id: "!room:example.com".to_string(),
            event_id: Some("$event:example.com".to_string()),
            event_type: Some("m.room.message".to_string()),
            origin_server_ts: 1234567890,
            scheduled_ts: 1234567890,
            status: "pending".to_string(),
            created_ts: 1234567890,
            processed_ts: None,
            error_message: None,
            retry_count: 0,
        };
        assert_eq!(item.status, "pending");
        assert_eq!(item.retry_count, 0);
    }

    #[test]
    fn test_cutoff_ts_from_days_zero_disables_cleanup() {
        assert_eq!(RetentionService::cutoff_ts_from_days(1_000, 0), None);
    }

    #[test]
    fn test_cutoff_ts_from_days_positive_retention() {
        assert_eq!(
            RetentionService::cutoff_ts_from_days(172_800_000, 1),
            Some(86_400_000)
        );
    }
}
