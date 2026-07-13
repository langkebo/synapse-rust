use chrono::Utc;
use std::sync::Arc;
use synapse_common::ApiResult;
use synapse_storage::burn_after_read::BurnAfterReadStoreApi;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct BurnSettings {
    pub is_enabled: bool,
    pub burn_after_ms: i64,
}

#[derive(Debug, Clone)]
pub struct BurnEvent {
    pub id: i64,
    pub event_id: String,
    pub room_id: String,
    pub user_id: String,
    pub created_ts: i64,
    pub delete_ts: i64,
}

#[derive(Debug, Clone, Default)]
pub struct BurnStats {
    pub total_burned: i64,
    pub total_pending: i64,
    pub rooms_enabled: i64,
}

struct BurnProcessorState {
    is_running: bool,
}

pub struct BurnAfterReadService {
    storage: Arc<dyn BurnAfterReadStoreApi>,
    event_writer: Arc<dyn synapse_storage::event::EventWriter>,
    server_name: String,
    processor_state: Arc<RwLock<BurnProcessorState>>,
}

impl BurnAfterReadService {
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
            .map_err(|e| synapse_common::ApiError::internal_with_log("Failed to set burn settings", &e))?;

        Ok(())
    }

    pub async fn get_burn_settings(&self, user_id: &str, room_id: &str) -> ApiResult<Option<BurnSettings>> {
        let row = self
            .storage
            .get_settings(user_id, room_id)
            .await
            .map_err(|e| synapse_common::ApiError::internal_with_log("Failed to get burn settings", &e))?;

        Ok(row.map(|r| BurnSettings { is_enabled: r.is_enabled, burn_after_ms: r.burn_after_ms }))
    }

    pub async fn get_pending_burns(&self, user_id: &str, room_id: &str) -> ApiResult<Vec<BurnEvent>> {
        let rows = self
            .storage
            .get_pending_burns(user_id, room_id)
            .await
            .map_err(|e| synapse_common::ApiError::internal_with_log("Failed to get pending burns", &e))?;

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

    pub async fn cancel_burn(&self, user_id: &str, room_id: &str, event_id: &str) -> ApiResult<()> {
        self.storage
            .cancel_burn(user_id, room_id, event_id)
            .await
            .map_err(|e| synapse_common::ApiError::internal_with_log("Failed to cancel burn", &e))?;

        Ok(())
    }

    pub async fn delete_burned_message(&self, user_id: &str, room_id: &str, event_id: &str) -> ApiResult<()> {
        let now = Utc::now().timestamp_millis();

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
            .map_err(|e| synapse_common::ApiError::internal_with_log("Failed to log burned event", &e))?;

        Ok(())
    }

    pub async fn set_user_default(&self, user_id: &str, default_burn_ms: i64) -> ApiResult<()> {
        self.storage
            .set_user_default(user_id, default_burn_ms)
            .await
            .map_err(|e| synapse_common::ApiError::internal_with_log("Failed to set user default", &e))?;

        Ok(())
    }

    pub async fn get_user_stats(&self, user_id: &str) -> ApiResult<BurnStats> {
        let row = self
            .storage
            .get_user_stats(user_id)
            .await
            .map_err(|e| synapse_common::ApiError::internal_with_log("Failed to get user stats", &e))?;

        Ok(BurnStats {
            total_burned: row.total_burned,
            total_pending: row.total_pending,
            rooms_enabled: row.rooms_enabled,
        })
    }

    pub async fn schedule_burn(
        &self,
        user_id: &str,
        room_id: &str,
        event_id: &str,
        burn_after_ms: i64,
    ) -> ApiResult<()> {
        let now = Utc::now().timestamp_millis();
        let delete_at = now + burn_after_ms;

        self.storage
            .schedule_burn(user_id, room_id, event_id, delete_at)
            .await
            .map_err(|e| synapse_common::ApiError::internal_with_log("Failed to schedule burn", &e))?;

        Ok(())
    }

    pub async fn process_expired_burns(&self) -> ApiResult<Vec<BurnEvent>> {
        let now = Utc::now().timestamp_millis();

        let expired_rows = self
            .storage
            .get_expired_burns(now)
            .await
            .map_err(|e| synapse_common::ApiError::internal_with_log("Failed to get expired burns", &e))?;

        let mut expired = Vec::new();

        for row in &expired_rows {
            if let Err(e) = self.event_writer.redact_event_content(&row.event_id, Some(&row.user_id)).await {
                ::tracing::warn!(
                    error = %e,
                    burn_id = row.id,
                    user_id = %row.user_id,
                    room_id = %row.room_id,
                    event_id = %row.event_id,
                    "Failed to redact event content for burn"
                );
            }

            if let Err(e) = self
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
                .await
            {
                ::tracing::warn!(
                    error = %e,
                    burn_id = row.id,
                    user_id = %row.user_id,
                    room_id = %row.room_id,
                    event_id = %row.event_id,
                    "Failed to create redaction event for burn"
                );
            }

            if let Err(e) = self.storage.mark_burn_processed(row.id).await {
                ::tracing::warn!(error = %e, burn_id = row.id, event_id = %row.event_id, "Failed to mark burn processed");
            }

            if let Err(e) = self.storage.log_burned_event(&row.user_id, &row.room_id, &row.event_id, now).await {
                ::tracing::warn!(
                    error = %e,
                    burn_id = row.id,
                    user_id = %row.user_id,
                    room_id = %row.room_id,
                    event_id = %row.event_id,
                    "Failed to log burned event"
                );
            }

            expired.push(BurnEvent {
                id: row.id,
                event_id: row.event_id.clone(),
                room_id: row.room_id.clone(),
                user_id: row.user_id.clone(),
                created_ts: row.created_ts,
                delete_ts: row.delete_ts,
            });
        }

        Ok(expired)
    }

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
        async fn log_burned_event(&self, _u: &str, _r: &str, _e: &str, _ts: i64) -> Result<(), sqlx::Error> {
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
        let event_storage: Arc<dyn synapse_storage::event::EventStoreApi> =
            Arc::new(synapse_storage::test_mocks::InMemoryEventStore::new());
        let service = Arc::new(BurnAfterReadService::new(storage, event_storage, "test".to_string()));

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
}
