use crate::common::ApiError;
use crate::storage::retention::*;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::{debug, error, info, instrument, warn};

pub struct RetentionService {
    storage: Arc<RetentionStorage>,
    pool: Arc<PgPool>,
}

impl RetentionService {
    pub fn new(storage: Arc<RetentionStorage>, pool: Arc<PgPool>) -> Self {
        Self { storage, pool }
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
}
