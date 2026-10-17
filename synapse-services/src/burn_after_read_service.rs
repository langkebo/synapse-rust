use std::sync::Arc;
use synapse_common::current_timestamp_millis;
use synapse_common::ApiResult;
use synapse_storage::burn_after_read::BurnAfterReadStoreApi;
use tokio::sync::RwLock;

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
///
/// This service is a **per-user time registry**, not a deleter. `burn_after_ms`
/// is a client-side preference: the server records when a given reader's copy
/// should disappear, and the reader's own client does the local deletion. The
/// server never redacts the event, because a room event has exactly one copy
/// shared by every member — redacting it would delete the message for *all*
/// members the moment *one* member reads it.
///
/// Server-side physical deletion remains the job of the retention service
/// (`m.room.retention`), which is a room-wide policy and therefore not an
/// escalation.
pub struct BurnAfterReadService {
    storage: Arc<dyn BurnAfterReadStoreApi>,
    processor_state: Arc<RwLock<BurnProcessorState>>,
}

impl BurnAfterReadService {
    /// See [`new`].
    pub fn new(storage: Arc<dyn BurnAfterReadStoreApi>) -> Self {
        Self { storage, processor_state: Arc::new(RwLock::new(BurnProcessorState { is_running: false })) }
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
    ///
    /// Records that `user_id`'s copy of `event_id` has burned. The event itself
    /// is left untouched: see the struct docs for why the server must not
    /// redact it.
    pub async fn delete_burned_message(&self, user_id: &str, room_id: &str, event_id: &str) -> ApiResult<()> {
        let now = current_timestamp_millis();

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
    ///
    /// Returns the effective deletion deadline. Re-scheduling the same event
    /// never pushes an existing deadline back: the storage layer keeps the
    /// earlier of the two, so a client re-reading a message cannot extend its
    /// lifetime indefinitely.
    pub async fn schedule_burn(
        &self,
        user_id: &str,
        room_id: &str,
        event_id: &str,
        burn_after_ms: i64,
    ) -> ApiResult<i64> {
        let now = current_timestamp_millis();
        let delete_at = now + burn_after_ms;

        let row = self
            .storage
            .schedule_burn(user_id, room_id, event_id, delete_at)
            .await
            .map_err(|e| synapse_common::ApiError::internal_with_cause("Failed to schedule burn", e))?;

        Ok(row.delete_ts)
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

        // Plan B: the server is a per-user *time registry*, not a deleter. Each
        // row belongs to exactly one reader; marking it processed tells that
        // reader's client its deadline has passed. Redacting the event here
        // would delete the single shared room event for *every* member, which
        // is precisely the "one reader deletes for everyone" escalation this
        // path must not have. Physical deletion stays with the retention
        // service, where it is a room-wide policy.
        let log_entries: Vec<(String, String, String, i64)> =
            expired_rows.iter().map(|r| (r.user_id.clone(), r.room_id.clone(), r.event_id.clone(), now)).collect();
        let ids: Vec<i64> = expired_rows.iter().map(|r| r.id).collect();
        let expired = expired_rows
            .into_iter()
            .map(|r| BurnEvent {
                id: r.id,
                event_id: r.event_id,
                room_id: r.room_id,
                user_id: r.user_id,
                created_ts: r.created_ts,
                delete_ts: r.delete_ts,
            })
            .collect::<Vec<_>>();

        // Log first, mark second: `log_burned_event_batch` is idempotent
        // (`ON CONFLICT (user_id, event_id) DO NOTHING`), so a failed mark is
        // retried on the next sweep without duplicating audit rows. The reverse
        // order would lose the audit entry for good, because the mark already
        // hides the row from `get_expired_burns`.
        if let Err(e) = self.storage.log_burned_event_batch(&log_entries).await {
            ::tracing::error!(
                error = %e,
                count = log_entries.len(),
                "Failed to batch-insert burn log entries; rows stay unprocessed and the next sweep retries"
            );
        } else if let Err(e) = self.storage.mark_burn_processed_batch(&ids).await {
            ::tracing::error!(
                error = %e,
                count = ids.len(),
                "Failed to mark burns processed in batch; rows will be retried next sweep (audit log is idempotent)"
            );
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
        let service = Arc::new(BurnAfterReadService::new(storage));

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
        Arc::new(BurnAfterReadService::new(storage))
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
        let result = svc.schedule_burn("@alice:ex.com", "!room:ex.com", "$event:ex.com", 5_000).await.unwrap();
        let calls = store.state.lock().expect("mutex poisoned").schedule_burn_calls.clone();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].2, "$event:ex.com");
        // The service must return the *stored* delete_ts so the client can show
        // the real deadline (which may be earlier than now + burn_after_ms when
        // an existing row is kept).
        assert_eq!(result, calls[0].3);
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
    async fn delete_burned_message_only_writes_audit_and_never_redacts_the_event() {
        // Plan B: a burn is a per-reader deadline, not a room-wide deletion.
        // The service must record the burn in the audit log and leave the
        // shared room event untouched.
        let store = Arc::new(FakeBurnStore::new());
        let svc = make_service(store.clone());
        svc.delete_burned_message("@alice:ex.com", "!room:ex.com", "$event:ex.com").await.unwrap();
        let s = store.state.lock().expect("mutex poisoned");
        assert_eq!(s.log_burned_calls.len(), 1, "log_burned_event must be called");
        assert_eq!(s.log_burned_calls[0].0, "@alice:ex.com");
        assert_eq!(s.log_burned_calls[0].2, "$event:ex.com");
        assert!(s.mark_processed_calls.is_empty(), "delete_burned_message must not touch the pending burn row");
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
