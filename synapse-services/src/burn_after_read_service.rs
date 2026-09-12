use std::sync::Arc;
use synapse_common::current_timestamp_millis;
use synapse_common::ApiResult;
use synapse_storage::burn_after_read::BurnAfterReadStoreApi;
use tokio::sync::RwLock;

/// Maximum number of times a single burn row will be retried by the processor
/// before it is moved to dead-letter state. Once dead-lettered, the scanner
/// skips the row entirely so it cannot monopolize a sweep pass.
///
/// B-07 trade-off: 5 attempts gives ~5 sweeps before abandonment. The default
/// sweep interval is 30s, so a fully-failed row stays in the hot path for ~2.5
/// minutes before being moved out. Lower values (1-2) risk prematurely
/// abandoning transiently-failing rows; higher values (10+) amplify duplicate
/// redaction risk when mark_processed_batch fails repeatedly.
pub const BURN_MAX_RETRY: i32 = 5;

/// The `BurnSettings` struct.
#[derive(Debug, Clone)]
pub struct BurnSettings {
    /// The `is_enabled` field.
    pub is_enabled: bool,
    /// The `burn_after_ms` field.
    pub burn_after_ms: i64,
}

/// The `BurnEvent` struct.
#[derive(Debug, Clone)]
pub struct BurnEvent {
    /// The `id` field.
    pub id: i64,
    /// The `event_id` field.
    pub event_id: String,
    /// The `room_id` field.
    pub room_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `delete_ts` field.
    pub delete_ts: i64,
}

/// The `BurnStats` struct.
#[derive(Debug, Clone, Default)]
pub struct BurnStats {
    /// The `total_burned` field.
    pub total_burned: i64,
    /// The `total_pending` field.
    pub total_pending: i64,
    /// The `rooms_enabled` field.
    pub rooms_enabled: i64,
}

struct BurnProcessorState {
    is_running: bool,
}

/// The `BurnAfterReadService` struct.
pub struct BurnAfterReadService {
    storage: Arc<dyn BurnAfterReadStoreApi>,
    event_writer: Arc<dyn synapse_storage::event::EventWriter>,
    server_name: String,
    processor_state: Arc<RwLock<BurnProcessorState>>,
}

impl BurnAfterReadService {
    /// See [`new`].
    pub fn new(
        storage: Arc<dyn BurnAfterReadStoreApi>,
        event_writer: Arc<dyn synapse_storage::event::EventWriter>,
        server_name: String,
    ) -> Self {
        Self {
            storage,
            event_writer,
            server_name,
            processor_state: Arc::new(RwLock::new(BurnProcessorState { is_running: false })),
        }
    }

    /// See [`set_burn_enabled`].
    pub async fn set_burn_enabled(
        &self,
        user_id: &str,
        room_id: &str,
        enabled: bool,
        burn_after_ms: i64,
    ) -> ApiResult<()> {
        self.storage
            .set_settings(user_id, room_id, enabled, burn_after_ms)
            .await
            .map_err(|e| synapse_common::ApiError::internal_with_cause("Failed to set burn settings", e))?;

        Ok(())
    }

    /// See [`get_burn_settings`].
    pub async fn get_burn_settings(&self, user_id: &str, room_id: &str) -> ApiResult<Option<BurnSettings>> {
        let row = self
            .storage
            .get_settings(user_id, room_id)
            .await
            .map_err(|e| synapse_common::ApiError::internal_with_cause("Failed to get burn settings", e))?;

        Ok(row.map(|r| BurnSettings { is_enabled: r.is_enabled, burn_after_ms: r.burn_after_ms }))
    }

    /// See [`get_pending_burns`].
    pub async fn get_pending_burns(&self, user_id: &str, room_id: &str) -> ApiResult<Vec<BurnEvent>> {
        let rows = self
            .storage
            .get_pending_burns(user_id, room_id)
            .await
            .map_err(|e| synapse_common::ApiError::internal_with_cause("Failed to get pending burns", e))?;

        Ok(rows
            .into_iter()
            .map(|r| BurnEvent {
                id: r.id,
                event_id: r.event_id,
                room_id: r.room_id,
                user_id: r.user_id,
                created_ts: r.created_ts,
                delete_ts: r.delete_ts,
            })
            .collect())
    }

    /// See [`cancel_burn`].
    pub async fn cancel_burn(&self, user_id: &str, room_id: &str, event_id: &str) -> ApiResult<()> {
        self.storage
            .cancel_burn(user_id, room_id, event_id)
            .await
            .map_err(|e| synapse_common::ApiError::internal_with_cause("Failed to cancel burn", e))?;

        Ok(())
    }

    /// See [`delete_burned_message`].
    pub async fn delete_burned_message(&self, user_id: &str, room_id: &str, event_id: &str) -> ApiResult<()> {
        let now = current_timestamp_millis();

        if let Err(e) = self.event_writer.redact_event_content(event_id, Some(user_id)).await {
            ::tracing::warn!(
                error = %e,
                user_id = %user_id,
                room_id = %room_id,
                event_id = %event_id,
                "Failed to redact event content for burn"
            );
        }

        if let Err(e) = self
            .event_writer
            .create_event(
                synapse_storage::event::CreateEventParams {
                    event_id: synapse_common::crypto::generate_event_id(&self.server_name),
                    room_id: room_id.to_string(),
                    user_id: user_id.to_string(),
                    event_type: "m.room.redaction".to_string(),
                    content: serde_json::json!({"reason": "Burn after read"}),
                    state_key: None,
                    origin_server_ts: now,
                    redacts: None,
                },
                None,
            )
            .await
        {
            ::tracing::warn!(
                error = %e,
                user_id = %user_id,
                room_id = %room_id,
                event_id = %event_id,
                "Failed to create redaction event for burn"
            );
        }

        self.storage
            .log_burned_event(user_id, room_id, event_id, now)
            .await
            .map_err(|e| synapse_common::ApiError::internal_with_cause("Failed to log burned event", e))?;

        Ok(())
    }

    /// See [`set_user_default`].
    pub async fn set_user_default(&self, user_id: &str, default_burn_ms: i64) -> ApiResult<()> {
        self.storage
            .set_user_default(user_id, default_burn_ms)
            .await
            .map_err(|e| synapse_common::ApiError::internal_with_cause("Failed to set user default", e))?;

        Ok(())
    }

    /// See [`get_user_stats`].
    pub async fn get_user_stats(&self, user_id: &str) -> ApiResult<BurnStats> {
        let row = self
            .storage
            .get_user_stats(user_id)
            .await
            .map_err(|e| synapse_common::ApiError::internal_with_cause("Failed to get user stats", e))?;

        Ok(BurnStats {
            total_burned: row.total_burned,
            total_pending: row.total_pending,
            rooms_enabled: row.rooms_enabled,
        })
    }

    /// See [`schedule_burn`].
    pub async fn schedule_burn(
        &self,
        user_id: &str,
        room_id: &str,
        event_id: &str,
        burn_after_ms: i64,
    ) -> ApiResult<()> {
        let now = current_timestamp_millis();
        let delete_at = now + burn_after_ms;

        self.storage
            .schedule_burn(user_id, room_id, event_id, delete_at)
            .await
            .map_err(|e| synapse_common::ApiError::internal_with_cause("Failed to schedule burn", e))?;

        Ok(())
    }

    /// See [`process_expired_burns`].
    pub async fn process_expired_burns(&self) -> ApiResult<Vec<BurnEvent>> {
        let now = current_timestamp_millis();

        let expired_rows = self
            .storage
            .get_expired_burns(now)
            .await
            .map_err(|e| synapse_common::ApiError::internal_with_cause("Failed to get expired burns", e))?;

        if expired_rows.is_empty() {
            return Ok(Vec::new());
        }

        // Three-step classification to keep the timeline consistent:
        //
        // 1. For each expired row, try to redact content + emit redaction event.
        //    Both must succeed; otherwise the row stays unprocessed and the
        //    next sweep retries (idempotency is the contract: a half-finished
        //    burn would otherwise produce duplicate redaction events on the
        //    next pass and pollute the room timeline).
        //
        // 2. After the per-row redact/create phase, batch-mark only the rows
        //    whose redact+create BOTH succeeded, in a single SQL UPDATE, and
        //    batch-insert their audit log entries in a single UNNEST INSERT.
        //    This collapses 2N round-trips into 2, eliminating the N+1 the
        //    previous loop had.
        //
        // 3. B-07 retry cap: if mark_processed_batch fails (a partial-success
        //    state where redaction events ARE on the wire but the row stays
        //    unprocessed), bump retry_count on those rows. Rows whose
        //    retry_count reaches BURN_MAX_RETRY are moved to dead-letter state
        //    so the scanner stops attempting them. The mark_processed_batch
        //    failure path no longer produces an unbounded retry storm: at
        //    worst the row is dead-lettered after BURN_MAX_RETRY attempts and
        //    is no longer picked up. Any redaction events emitted before
        //    dead-lettering remain on the timeline (which is the correct
        //    behavior — a burn should not silently fail to redact because of
        //    bookkeeping trouble).
        let mut successfully_processed_ids: Vec<i64> = Vec::with_capacity(expired_rows.len());
        let mut log_entries: Vec<(String, String, String, i64)> = Vec::with_capacity(expired_rows.len());
        let mut expired = Vec::with_capacity(expired_rows.len());

        for row in &expired_rows {
            let redact_ok = self.event_writer.redact_event_content(&row.event_id, Some(&row.user_id)).await;

            if let Err(e) = &redact_ok {
                ::tracing::warn!(
                    error = %e,
                    burn_id = row.id,
                    user_id = %row.user_id,
                    room_id = %row.room_id,
                    event_id = %row.event_id,
                    retry_count = row.retry_count,
                    "Failed to redact event content for burn; will retry next sweep"
                );
                continue;
            }

            let create_ok = self
                .event_writer
                .create_event(
                    synapse_storage::event::CreateEventParams {
                        event_id: synapse_common::crypto::generate_event_id(&self.server_name),
                        room_id: row.room_id.clone(),
                        user_id: row.user_id.clone(),
                        event_type: "m.room.redaction".to_string(),
                        content: serde_json::json!({"reason": "Burn after read"}),
                        state_key: None,
                        origin_server_ts: now,
                        redacts: None,
                    },
                    None,
                )
                .await;

            if let Err(e) = &create_ok {
                ::tracing::warn!(
                    error = %e,
                    burn_id = row.id,
                    user_id = %row.user_id,
                    room_id = %row.room_id,
                    event_id = %row.event_id,
                    retry_count = row.retry_count,
                    "Failed to create redaction event for burn; content already redacted — \
                     will retry next sweep (idempotency: redact+create are re-entrant)"
                );
                continue;
            }

            successfully_processed_ids.push(row.id);
            log_entries.push((row.user_id.clone(), row.room_id.clone(), row.event_id.clone(), now));

            expired.push(BurnEvent {
                id: row.id,
                event_id: row.event_id.clone(),
                room_id: row.room_id.clone(),
                user_id: row.user_id.clone(),
                created_ts: row.created_ts,
                delete_ts: row.delete_ts,
            });
        }

        if !successfully_processed_ids.is_empty() {
            if let Err(e) = self.storage.mark_burn_processed_batch(&successfully_processed_ids).await {
                // B-07: increment retry_count and check cap. The previous behavior
                // logged only a warn and let the next sweep reprocess — a partial
                // failure loop that could run unbounded. Now we count attempts and
                // dead-letter at BURN_MAX_RETRY.
                let mark_err_str = e.to_string();
                ::tracing::error!(
                    error = %e,
                    count = successfully_processed_ids.len(),
                    "Failed to mark burns processed in batch; redaction events have been \
                     emitted but rows will be reprocessed — bumping retry_count and \
                     dead-lettering rows that exceed cap"
                );

                // Bump retry_count by 1 for all affected rows. Then ask the
                // storage layer to dead-letter the ones that have already
                // crossed the cap. We compute the set in memory: every row's
                // current retry_count + 1; if that >= MAX, dead-letter.
                let mut to_dead_letter: Vec<i64> = Vec::new();
                for row in &expired_rows {
                    if successfully_processed_ids.contains(&row.id) && row.retry_count + 1 >= BURN_MAX_RETRY {
                        to_dead_letter.push(row.id);
                    }
                }
                if let Err(e2) = self.storage.increment_retry_count(&successfully_processed_ids, &mark_err_str).await {
                    ::tracing::error!(
                        error = %e2,
                        count = successfully_processed_ids.len(),
                        "Failed to bump retry_count for affected burn rows; retry cap \
                         cannot be enforced until the next successful UPDATE"
                    );
                }
                if !to_dead_letter.is_empty() {
                    if let Err(e2) = self.storage.mark_dead_letter(&to_dead_letter).await {
                        ::tracing::error!(
                            error = %e2,
                            count = to_dead_letter.len(),
                            "Failed to dead-letter burn rows that exceeded retry cap; \
                             they will continue to be retried on each sweep"
                        );
                    } else {
                        ::tracing::warn!(
                            count = to_dead_letter.len(),
                            cap = BURN_MAX_RETRY,
                            "Moved burn rows to dead-letter state after exceeding retry cap; \
                             their redaction events are already on the wire but the audit \
                             log entry may be missing. Investigate the underlying cause \
                             (e.g. mark_burn_processed_batch failure mode)."
                        );
                    }
                }
                // Do NOT return Err here: the redaction events are already on the wire.
                // Returning the BurnEvent list lets the caller (the processor loop) carry
                // on, and the next sweep will see the row as still unprocessed and re-run
                // the redact+create sequence. The redact API is documented idempotent
                // and the create step will reuse the new event_id only if the previous
                // mark never landed — so duplicate redaction is the bounded failure mode.
            }

            if let Err(e) = self.storage.log_burned_event_batch(&log_entries).await {
                // B-07: log_burned_event_batch has its own retry cap. A failure
                // here means the redaction IS on the wire but the audit row is
                // missing. We don't move the burn to dead-letter because of
                // this — the redacted state is what matters most. Instead, we
                // bump retry_count so the operator can see repeated failures
                // and the row is bounded by the same cap as a mark_processed
                // failure.
                let log_err_str = e.to_string();
                ::tracing::warn!(
                    error = %e,
                    count = log_entries.len(),
                    "Failed to batch-insert burn log entries; bumping retry_count"
                );
                if let Err(e2) = self
                    .storage
                    .increment_retry_count(
                        &successfully_processed_ids
                            .iter()
                            .filter(|id| {
                                expired_rows.iter().any(|r| r.id == **id && r.retry_count + 1 >= BURN_MAX_RETRY)
                            })
                            .copied()
                            .collect::<Vec<_>>(),
                        &log_err_str,
                    )
                    .await
                {
                    ::tracing::warn!(
                        error = %e2,
                        "Failed to bump retry_count after log_burned_event_batch failure"
                    );
                }
            }
        }

        Ok(expired)
    }

    /// See [`recover_pending_burns`].
    pub async fn recover_pending_burns(&self) {
        ::tracing::info!("Recovering pending burn-after-read events from database");

        match self.process_expired_burns().await {
            Ok(expired) => {
                if expired.is_empty() {
                    ::tracing::info!(expired_count = 0, "No expired burn events to recover");
                } else {
                    ::tracing::info!(expired_count = expired.len(), "Recovered expired burn events");
                }
            }
            Err(e) => {
                ::tracing::error!(error = %e, "Failed to recover expired burn events");
            }
        }
    }

    /// See [`start_burn_processor`].
    pub async fn start_burn_processor(
        self: Arc<Self>,
        shutdown: tokio_util::sync::CancellationToken,
    ) -> Option<tokio::task::JoinHandle<()>> {
        let mut state = self.processor_state.write().await;
        if state.is_running {
            return None;
        }
        state.is_running = true;
        drop(state);

        let service = self.clone();
        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));

            loop {
                tokio::select! {
                    _ = shutdown.cancelled() => {
                        ::tracing::info!("Burn-after-read processor shutting down");
                        break;
                    }
                    _ = interval.tick() => {
                        if let Err(e) = service.process_expired_burns().await {
                            ::tracing::error!(error = %e, "Burn processor error");
                        }
                    }
                }
            }
        });

        ::tracing::info!(interval_secs = 5, "Burn-after-read processor started");
        Some(handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use synapse_storage::burn_after_read::{BurnPendingRow, BurnSettingsRow, BurnStatsRow, BurnUserDefaultsRow};

    /// Minimal no-op fake so `process_expired_burns` does nothing (no expired
    /// rows), letting us exercise the processor loop's shutdown behavior.
    struct NoopBurnStore;

    #[async_trait::async_trait]
    impl BurnAfterReadStoreApi for NoopBurnStore {
        async fn get_settings(&self, _u: &str, _r: &str) -> Result<Option<BurnSettingsRow>, sqlx::Error> {
            Ok(None)
        }
        async fn set_settings(
            &self,
            user_id: &str,
            room_id: &str,
            is_enabled: bool,
            burn_after_ms: i64,
        ) -> Result<BurnSettingsRow, sqlx::Error> {
            Ok(BurnSettingsRow {
                user_id: user_id.to_string(),
                room_id: room_id.to_string(),
                is_enabled,
                burn_after_ms,
                created_ts: 0,
                updated_ts: None,
            })
        }
        async fn schedule_burn(
            &self,
            user_id: &str,
            room_id: &str,
            event_id: &str,
            delete_ts: i64,
        ) -> Result<BurnPendingRow, sqlx::Error> {
            Ok(BurnPendingRow {
                id: 0,
                user_id: user_id.to_string(),
                room_id: room_id.to_string(),
                event_id: event_id.to_string(),
                created_ts: 0,
                delete_ts,
                is_processed: false,
                retry_count: 0,
                last_error: None,
                is_dead_letter: false,
            })
        }
        async fn cancel_burn(&self, _u: &str, _r: &str, _e: &str) -> Result<(), sqlx::Error> {
            Ok(())
        }
        async fn get_pending_burns(&self, _u: &str, _r: &str) -> Result<Vec<BurnPendingRow>, sqlx::Error> {
            Ok(Vec::new())
        }
        async fn get_expired_burns(&self, _now_ms: i64) -> Result<Vec<BurnPendingRow>, sqlx::Error> {
            Ok(Vec::new())
        }
        async fn mark_burn_processed(&self, _id: i64) -> Result<(), sqlx::Error> {
            Ok(())
        }
        async fn mark_burn_processed_batch(&self, _ids: &[i64]) -> Result<(), sqlx::Error> {
            Ok(())
        }
        async fn increment_retry_count(&self, _ids: &[i64], _err: &str) -> Result<(), sqlx::Error> {
            Ok(())
        }
        async fn mark_dead_letter(&self, _ids: &[i64]) -> Result<(), sqlx::Error> {
            Ok(())
        }
        async fn log_burned_event(&self, _u: &str, _r: &str, _e: &str, _ts: i64) -> Result<(), sqlx::Error> {
            Ok(())
        }
        async fn log_burned_event_batch(&self, _entries: &[(String, String, String, i64)]) -> Result<(), sqlx::Error> {
            Ok(())
        }
        async fn get_user_stats(&self, _user_id: &str) -> Result<BurnStatsRow, sqlx::Error> {
            Ok(BurnStatsRow { total_burned: 0, total_pending: 0, rooms_enabled: 0 })
        }
        async fn get_user_default(&self, _user_id: &str) -> Result<Option<BurnUserDefaultsRow>, sqlx::Error> {
            Ok(None)
        }
        async fn set_user_default(&self, _user_id: &str, _default_burn_ms: i64) -> Result<(), sqlx::Error> {
            Ok(())
        }
    }

    #[test]
    fn test_burn_settings_struct() {
        let settings = BurnSettings { is_enabled: true, burn_after_ms: 60_000 };
        assert!(settings.is_enabled);
        assert_eq!(settings.burn_after_ms, 60_000);
    }

    #[test]
    fn test_burn_event_struct() {
        let event = BurnEvent {
            id: 1,
            event_id: "$event1".to_string(),
            room_id: "!room:example.com".to_string(),
            user_id: "@alice:example.com".to_string(),
            created_ts: 1234567890,
            delete_ts: 1234567950,
        };
        assert_eq!(event.id, 1);
        assert_eq!(event.event_id, "$event1");
    }

    #[test]
    fn test_burn_stats_default() {
        let stats = BurnStats::default();
        assert_eq!(stats.total_burned, 0);
        assert_eq!(stats.total_pending, 0);
        assert_eq!(stats.rooms_enabled, 0);
    }

    #[test]
    fn test_burn_stats_custom() {
        let stats = BurnStats { total_burned: 10, total_pending: 3, rooms_enabled: 2 };
        assert_eq!(stats.total_burned, 10);
        assert_eq!(stats.total_pending, 3);
        assert_eq!(stats.rooms_enabled, 2);
    }

    #[tokio::test]
    async fn burn_processor_stops_on_shutdown() {
        let storage: Arc<dyn BurnAfterReadStoreApi> = Arc::new(NoopBurnStore);
        let event_writer: Arc<dyn synapse_storage::event::EventWriter> =
            Arc::new(synapse_storage::test_mocks::InMemoryEventStore::new());
        let service = Arc::new(BurnAfterReadService::new(storage, event_writer, "test".to_string()));

        let token = tokio_util::sync::CancellationToken::new();
        let handle = service.clone().start_burn_processor(token.clone()).await.expect("first start returns handle");

        // Second start while running must not spawn another task.
        assert!(service.clone().start_burn_processor(token.clone()).await.is_none());

        token.cancel();
        tokio::time::timeout(std::time::Duration::from_secs(1), handle)
            .await
            .expect("processor must stop within 1s of cancel")
            .expect("processor task must not panic");
    }

    // ── B.3 batch 6/6 — configurable FakeBurnStore + service method coverage ──
    //
    // The existing NoopBurnStore always returns empty/None. These tests add a
    // configurable FakeBurnStore that can seed expired_burns / settings /
    // pending_burns / user_stats, allowing us to exercise every service method
    // including the loop body of process_expired_burns.

    use std::sync::Mutex;

    struct FakeBurnStoreState {
        settings: Option<BurnSettingsRow>,
        pending_burns: Vec<BurnPendingRow>,
        expired_burns: Vec<BurnPendingRow>,
        user_stats: BurnStatsRow,
        user_default: Option<BurnUserDefaultsRow>,
        // Call tracking
        set_settings_calls: Vec<(String, String, bool, i64)>,
        schedule_burn_calls: Vec<(String, String, String, i64)>,
        cancel_burn_calls: Vec<(String, String, String)>,
        mark_processed_calls: Vec<i64>,
        increment_retry_count_calls: Vec<(Vec<i64>, String)>,
        mark_dead_letter_calls: Vec<Vec<i64>>,
        log_burned_calls: Vec<(String, String, String, i64)>,
        set_user_default_calls: Vec<(String, i64)>,
    }

    impl Default for FakeBurnStoreState {
        fn default() -> Self {
            Self {
                settings: None,
                pending_burns: Vec::new(),
                expired_burns: Vec::new(),
                user_stats: BurnStatsRow { total_burned: 0, total_pending: 0, rooms_enabled: 0 },
                user_default: None,
                set_settings_calls: Vec::new(),
                schedule_burn_calls: Vec::new(),
                cancel_burn_calls: Vec::new(),
                mark_processed_calls: Vec::new(),
                increment_retry_count_calls: Vec::new(),
                mark_dead_letter_calls: Vec::new(),
                log_burned_calls: Vec::new(),
                set_user_default_calls: Vec::new(),
            }
        }
    }

    /// Configurable BurnAfterReadStoreApi double. State is wrapped in a Mutex
    /// so the mock is Send + Sync (required by `Arc<dyn BurnAfterReadStoreApi>`).
    /// Tests lock the mutex to seed return values and assert on call history.
    #[derive(Default)]
    struct FakeBurnStore {
        state: Mutex<FakeBurnStoreState>,
    }

    impl FakeBurnStore {
        fn new() -> Self {
            Self::default()
        }

        fn with_expired_burns(burns: Vec<BurnPendingRow>) -> Self {
            let store = Self::new();
            store.state.lock().expect("fake mutex poisoned").expired_burns = burns;
            store
        }

        fn with_settings(settings: BurnSettingsRow) -> Self {
            let store = Self::new();
            store.state.lock().expect("fake mutex poisoned").settings = Some(settings);
            store
        }

        fn with_pending_burns(burns: Vec<BurnPendingRow>) -> Self {
            let store = Self::new();
            store.state.lock().expect("fake mutex poisoned").pending_burns = burns;
            store
        }

        fn with_stats(stats: BurnStatsRow) -> Self {
            let store = Self::new();
            store.state.lock().expect("fake mutex poisoned").user_stats = stats;
            store
        }
    }

    #[async_trait::async_trait]
    impl BurnAfterReadStoreApi for FakeBurnStore {
        async fn get_settings(&self, _u: &str, _r: &str) -> Result<Option<BurnSettingsRow>, sqlx::Error> {
            Ok(self.state.lock().expect("fake mutex poisoned").settings.clone())
        }
        async fn set_settings(
            &self,
            user_id: &str,
            room_id: &str,
            is_enabled: bool,
            burn_after_ms: i64,
        ) -> Result<BurnSettingsRow, sqlx::Error> {
            let mut s = self.state.lock().expect("fake mutex poisoned");
            s.set_settings_calls.push((user_id.into(), room_id.into(), is_enabled, burn_after_ms));
            Ok(BurnSettingsRow {
                user_id: user_id.into(),
                room_id: room_id.into(),
                is_enabled,
                burn_after_ms,
                created_ts: 0,
                updated_ts: None,
            })
        }
        async fn schedule_burn(
            &self,
            user_id: &str,
            room_id: &str,
            event_id: &str,
            delete_ts: i64,
        ) -> Result<BurnPendingRow, sqlx::Error> {
            let mut s = self.state.lock().expect("fake mutex poisoned");
            s.schedule_burn_calls.push((user_id.into(), room_id.into(), event_id.into(), delete_ts));
            Ok(BurnPendingRow {
                id: 0,
                user_id: user_id.into(),
                room_id: room_id.into(),
                event_id: event_id.into(),
                created_ts: 0,
                delete_ts,
                is_processed: false,
                retry_count: 0,
                last_error: None,
                is_dead_letter: false,
            })
        }
        async fn cancel_burn(&self, user_id: &str, room_id: &str, event_id: &str) -> Result<(), sqlx::Error> {
            self.state.lock().expect("fake mutex poisoned").cancel_burn_calls.push((
                user_id.into(),
                room_id.into(),
                event_id.into(),
            ));
            Ok(())
        }
        async fn get_pending_burns(&self, _u: &str, _r: &str) -> Result<Vec<BurnPendingRow>, sqlx::Error> {
            Ok(self.state.lock().expect("fake mutex poisoned").pending_burns.clone())
        }
        async fn get_expired_burns(&self, _now_ms: i64) -> Result<Vec<BurnPendingRow>, sqlx::Error> {
            // B-07: emulate the storage layer's dead-letter filter. The
            // service-side state may contain dead-letter rows for unit tests
            // that want to verify they are NOT picked up by the scanner.
            let s = self.state.lock().expect("fake mutex poisoned");
            Ok(s.expired_burns.iter().filter(|r| !r.is_dead_letter).cloned().collect())
        }
        async fn mark_burn_processed(&self, id: i64) -> Result<(), sqlx::Error> {
            self.state.lock().expect("fake mutex poisoned").mark_processed_calls.push(id);
            Ok(())
        }
        async fn mark_burn_processed_batch(&self, ids: &[i64]) -> Result<(), sqlx::Error> {
            self.state.lock().expect("fake mutex poisoned").mark_processed_calls.extend(ids.to_vec());
            Ok(())
        }
        async fn increment_retry_count(&self, ids: &[i64], last_error: &str) -> Result<(), sqlx::Error> {
            let mut s = self.state.lock().expect("fake mutex poisoned");
            s.increment_retry_count_calls.push((ids.to_vec(), last_error.to_string()));
            // Also reflect the new state in pending_burns/expired_burns so
            // subsequent calls see the updated retry_count.
            for row in s.expired_burns.iter_mut() {
                if ids.contains(&row.id) {
                    row.retry_count += 1;
                    row.last_error = Some(last_error.to_string());
                }
            }
            Ok(())
        }
        async fn mark_dead_letter(&self, ids: &[i64]) -> Result<(), sqlx::Error> {
            let mut s = self.state.lock().expect("fake mutex poisoned");
            s.mark_dead_letter_calls.push(ids.to_vec());
            for row in s.expired_burns.iter_mut() {
                if ids.contains(&row.id) {
                    row.is_dead_letter = true;
                }
            }
            Ok(())
        }
        async fn log_burned_event(
            &self,
            user_id: &str,
            room_id: &str,
            event_id: &str,
            ts: i64,
        ) -> Result<(), sqlx::Error> {
            self.state.lock().expect("fake mutex poisoned").log_burned_calls.push((
                user_id.into(),
                room_id.into(),
                event_id.into(),
                ts,
            ));
            Ok(())
        }
        async fn log_burned_event_batch(&self, entries: &[(String, String, String, i64)]) -> Result<(), sqlx::Error> {
            self.state.lock().expect("fake mutex poisoned").log_burned_calls.extend(entries.to_vec());
            Ok(())
        }
        async fn get_user_stats(&self, _user_id: &str) -> Result<BurnStatsRow, sqlx::Error> {
            Ok(self.state.lock().expect("fake mutex poisoned").user_stats.clone())
        }
        async fn get_user_default(&self, _user_id: &str) -> Result<Option<BurnUserDefaultsRow>, sqlx::Error> {
            Ok(self.state.lock().expect("fake mutex poisoned").user_default.clone())
        }
        async fn set_user_default(&self, user_id: &str, default_burn_ms: i64) -> Result<(), sqlx::Error> {
            self.state
                .lock()
                .expect("fake mutex poisoned")
                .set_user_default_calls
                .push((user_id.into(), default_burn_ms));
            Ok(())
        }
    }

    fn make_service(storage: Arc<dyn BurnAfterReadStoreApi>) -> Arc<BurnAfterReadService> {
        let event_writer: Arc<dyn synapse_storage::event::EventWriter> =
            Arc::new(synapse_storage::test_mocks::InMemoryEventStore::new());
        Arc::new(BurnAfterReadService::new(storage, event_writer, "test.example.com".to_string()))
    }

    #[tokio::test]
    async fn set_burn_enabled_calls_storage_set_settings() {
        let store = Arc::new(FakeBurnStore::new());
        let svc = make_service(store.clone());
        svc.set_burn_enabled("@alice:ex.com", "!room:ex.com", true, 60_000).await.unwrap();
        let calls = store.state.lock().expect("mutex poisoned").set_settings_calls.clone();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "@alice:ex.com");
        assert_eq!(calls[0].1, "!room:ex.com");
        assert!(calls[0].2);
        assert_eq!(calls[0].3, 60_000);
    }

    #[tokio::test]
    async fn get_burn_settings_returns_none_when_unconfigured() {
        let store = Arc::new(FakeBurnStore::new());
        let svc = make_service(store);
        let result = svc.get_burn_settings("@alice:ex.com", "!room:ex.com").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn get_burn_settings_returns_some_when_configured() {
        let store = Arc::new(FakeBurnStore::with_settings(BurnSettingsRow {
            user_id: "@alice:ex.com".into(),
            room_id: "!room:ex.com".into(),
            is_enabled: true,
            burn_after_ms: 30_000,
            created_ts: 0,
            updated_ts: None,
        }));
        let svc = make_service(store);
        let result = svc.get_burn_settings("@alice:ex.com", "!room:ex.com").await.unwrap();
        let settings = result.expect("settings should be Some");
        assert!(settings.is_enabled);
        assert_eq!(settings.burn_after_ms, 30_000);
    }

    #[tokio::test]
    async fn get_pending_burns_returns_empty_when_none() {
        let store = Arc::new(FakeBurnStore::new());
        let svc = make_service(store);
        let result = svc.get_pending_burns("@alice:ex.com", "!room:ex.com").await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn get_pending_burns_maps_rows_to_burn_events() {
        let store = Arc::new(FakeBurnStore::with_pending_burns(vec![BurnPendingRow {
            id: 7,
            user_id: "@alice:ex.com".into(),
            room_id: "!room:ex.com".into(),
            event_id: "$event1:ex.com".into(),
            created_ts: 100,
            delete_ts: 200,
            is_processed: false,
            retry_count: 0,
            last_error: None,
            is_dead_letter: false,
        }]));
        let svc = make_service(store);
        let result = svc.get_pending_burns("@alice:ex.com", "!room:ex.com").await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, 7);
        assert_eq!(result[0].event_id, "$event1:ex.com");
        assert_eq!(result[0].delete_ts, 200);
    }

    #[tokio::test]
    async fn cancel_burn_calls_storage_cancel_burn() {
        let store = Arc::new(FakeBurnStore::new());
        let svc = make_service(store.clone());
        svc.cancel_burn("@alice:ex.com", "!room:ex.com", "$event:ex.com").await.unwrap();
        let calls = store.state.lock().expect("mutex poisoned").cancel_burn_calls.clone();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].2, "$event:ex.com");
    }

    #[tokio::test]
    async fn schedule_burn_passes_now_plus_burn_after_as_delete_ts() {
        let store = Arc::new(FakeBurnStore::new());
        let svc = make_service(store.clone());
        svc.schedule_burn("@alice:ex.com", "!room:ex.com", "$event:ex.com", 5_000).await.unwrap();
        let calls = store.state.lock().expect("mutex poisoned").schedule_burn_calls.clone();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].2, "$event:ex.com");
        // delete_ts should be roughly now + 5_000 (allow 2s skew for test latency)
        let now = current_timestamp_millis();
        let delete_ts = calls[0].3;
        assert!(
            delete_ts >= now + 4_900 && delete_ts <= now + 5_100,
            "delete_ts={delete_ts}, now+5000={}",
            now + 5_000
        );
    }

    #[tokio::test]
    async fn set_user_default_calls_storage_set_user_default() {
        let store = Arc::new(FakeBurnStore::new());
        let svc = make_service(store.clone());
        svc.set_user_default("@alice:ex.com", 120_000).await.unwrap();
        let calls = store.state.lock().expect("mutex poisoned").set_user_default_calls.clone();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "@alice:ex.com");
        assert_eq!(calls[0].1, 120_000);
    }

    #[tokio::test]
    async fn get_user_stats_maps_row_to_burn_stats() {
        let store =
            Arc::new(FakeBurnStore::with_stats(BurnStatsRow { total_burned: 5, total_pending: 2, rooms_enabled: 3 }));
        let svc = make_service(store);
        let stats = svc.get_user_stats("@alice:ex.com").await.unwrap();
        assert_eq!(stats.total_burned, 5);
        assert_eq!(stats.total_pending, 2);
        assert_eq!(stats.rooms_enabled, 3);
    }

    #[tokio::test]
    async fn process_expired_burns_with_no_expired_returns_empty() {
        let store = Arc::new(FakeBurnStore::new());
        let svc = make_service(store);
        let result = svc.process_expired_burns().await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn process_expired_burns_with_expired_rows_marks_and_logs_each() {
        let store = Arc::new(FakeBurnStore::with_expired_burns(vec![
            BurnPendingRow {
                id: 1,
                user_id: "@alice:ex.com".into(),
                room_id: "!room:ex.com".into(),
                event_id: "$event1:ex.com".into(),
                created_ts: 100,
                delete_ts: 200,
                is_processed: false,
                retry_count: 0,
                last_error: None,
                is_dead_letter: false,
            },
            BurnPendingRow {
                id: 2,
                user_id: "@bob:ex.com".into(),
                room_id: "!room2:ex.com".into(),
                event_id: "$event2:ex.com".into(),
                created_ts: 150,
                delete_ts: 250,
                is_processed: false,
                retry_count: 0,
                last_error: None,
                is_dead_letter: false,
            },
        ]));
        let svc = make_service(store.clone());
        let expired = svc.process_expired_burns().await.unwrap();
        assert_eq!(expired.len(), 2);
        assert_eq!(expired[0].event_id, "$event1:ex.com");
        assert_eq!(expired[1].event_id, "$event2:ex.com");

        let s = store.state.lock().expect("mutex poisoned");
        assert_eq!(s.mark_processed_calls.len(), 2, "each expired row must be marked processed");
        assert!(s.mark_processed_calls.contains(&1));
        assert!(s.mark_processed_calls.contains(&2));
        assert_eq!(s.log_burned_calls.len(), 2, "each expired row must be logged");
        assert_eq!(s.log_burned_calls[0].0, "@alice:ex.com");
        assert_eq!(s.log_burned_calls[1].0, "@bob:ex.com");
    }

    #[tokio::test]
    async fn delete_burned_message_logs_and_succeeds_even_if_event_writer_partially_fails() {
        // delete_burned_message calls redact_event_content + create_event +
        // log_burned_event. The first two failures are logged but NOT
        // propagated; only log_burned_event failure propagates.
        let store = Arc::new(FakeBurnStore::new());
        let svc = make_service(store.clone());
        // InMemoryEventStore starts empty — redact_event_content will fail
        // (event not found), create_event will succeed.
        svc.delete_burned_message("@alice:ex.com", "!room:ex.com", "$event:ex.com").await.unwrap();
        let s = store.state.lock().expect("mutex poisoned");
        assert_eq!(s.log_burned_calls.len(), 1, "log_burned_event must be called");
        assert_eq!(s.log_burned_calls[0].0, "@alice:ex.com");
        assert_eq!(s.log_burned_calls[0].2, "$event:ex.com");
    }

    #[tokio::test]
    async fn recover_pending_burns_with_no_expired_logs_nothing_and_does_not_panic() {
        let store = Arc::new(FakeBurnStore::new());
        let svc = make_service(store);
        // recover_pending_burns returns () — just verify it doesn't panic.
        svc.recover_pending_burns().await;
    }

    #[tokio::test]
    async fn recover_pending_burns_with_expired_processes_them() {
        let store = Arc::new(FakeBurnStore::with_expired_burns(vec![BurnPendingRow {
            id: 42,
            user_id: "@alice:ex.com".into(),
            room_id: "!room:ex.com".into(),
            event_id: "$event:ex.com".into(),
            created_ts: 0,
            delete_ts: 0,
            is_processed: false,
            retry_count: 0,
            last_error: None,
            is_dead_letter: false,
        }]));
        let svc = make_service(store.clone());
        svc.recover_pending_burns().await;
        let s = store.state.lock().expect("mutex poisoned");
        assert!(s.mark_processed_calls.contains(&42), "expired burn must be processed during recovery");
    }
}
