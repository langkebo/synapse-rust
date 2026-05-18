use crate::common::ApiResult;
use crate::storage::burn_after_read::BurnAfterReadStorage;
use crate::storage::event::EventStorage;
use chrono::Utc;
use std::sync::Arc;
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
    pub delete_at: i64,
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
    storage: Arc<BurnAfterReadStorage>,
    event_storage: Arc<EventStorage>,
    server_name: String,
    processor_state: Arc<RwLock<BurnProcessorState>>,
}

impl BurnAfterReadService {
    pub fn new(
        storage: BurnAfterReadStorage,
        event_storage: EventStorage,
        server_name: String,
    ) -> Self {
        Self {
            storage: Arc::new(storage),
            event_storage: Arc::new(event_storage),
            server_name,
            processor_state: Arc::new(RwLock::new(BurnProcessorState {
                is_running: false,
            })),
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
            .map_err(|e| crate::common::ApiError::internal(format!("Failed to set burn settings: {e}")))?;

        Ok(())
    }

    pub async fn get_burn_settings(
        &self,
        user_id: &str,
        room_id: &str,
    ) -> ApiResult<Option<BurnSettings>> {
        let row = self
            .storage
            .get_settings(user_id, room_id)
            .await
            .map_err(|e| crate::common::ApiError::internal(format!("Failed to get burn settings: {e}")))?;

        Ok(row.map(|r| BurnSettings {
            is_enabled: r.is_enabled,
            burn_after_ms: r.burn_after_ms,
        }))
    }

    pub async fn get_pending_burns(
        &self,
        user_id: &str,
        room_id: &str,
    ) -> ApiResult<Vec<BurnEvent>> {
        let rows = self
            .storage
            .get_pending_burns(user_id, room_id)
            .await
            .map_err(|e| crate::common::ApiError::internal(format!("Failed to get pending burns: {e}")))?;

        Ok(rows
            .into_iter()
            .map(|r| BurnEvent {
                id: r.id,
                event_id: r.event_id,
                room_id: r.room_id,
                user_id: r.user_id,
                created_ts: r.created_ts,
                delete_at: r.delete_at,
            })
            .collect())
    }

    pub async fn cancel_burn(
        &self,
        user_id: &str,
        room_id: &str,
        event_id: &str,
    ) -> ApiResult<()> {
        self.storage
            .cancel_burn(user_id, room_id, event_id)
            .await
            .map_err(|e| crate::common::ApiError::internal(format!("Failed to cancel burn: {e}")))?;

        Ok(())
    }

    pub async fn delete_burned_message(
        &self,
        user_id: &str,
        room_id: &str,
        event_id: &str,
    ) -> ApiResult<()> {
        let now = Utc::now().timestamp_millis();

        if let Err(e) = self
            .event_storage
            .redact_event_content(event_id)
            .await
        {
            ::tracing::warn!(
                "Failed to redact event content for burn {}: {}",
                event_id, e
            );
        }

        if let Err(e) = self
            .event_storage
            .create_event(
                crate::storage::event::CreateEventParams {
                    event_id: crate::common::crypto::generate_event_id(&self.server_name),
                    room_id: room_id.to_string(),
                    user_id: user_id.to_string(),
                    event_type: "m.room.redaction".to_string(),
                    content: serde_json::json!({"reason": "Burn after read"}),
                    state_key: None,
                    origin_server_ts: now,
                },
                None,
            )
            .await
        {
            ::tracing::warn!(
                "Failed to create redaction event for burn {}: {}",
                event_id, e
            );
        }

        self.storage
            .log_burned_event(user_id, room_id, event_id, now)
            .await
            .map_err(|e| crate::common::ApiError::internal(format!("Failed to log burned event: {e}")))?;

        Ok(())
    }

    pub async fn set_user_default(
        &self,
        user_id: &str,
        default_burn_ms: i64,
    ) -> ApiResult<()> {
        self.storage
            .set_user_default(user_id, default_burn_ms)
            .await
            .map_err(|e| crate::common::ApiError::internal(format!("Failed to set user default: {e}")))?;

        Ok(())
    }

    pub async fn get_user_stats(&self, user_id: &str) -> ApiResult<BurnStats> {
        let row = self
            .storage
            .get_user_stats(user_id)
            .await
            .map_err(|e| crate::common::ApiError::internal(format!("Failed to get user stats: {e}")))?;

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
            .map_err(|e| crate::common::ApiError::internal(format!("Failed to schedule burn: {e}")))?;

        Ok(())
    }

    pub async fn process_expired_burns(&self) -> ApiResult<Vec<BurnEvent>> {
        let now = Utc::now().timestamp_millis();

        let expired_rows = self
            .storage
            .get_expired_burns(now)
            .await
            .map_err(|e| crate::common::ApiError::internal(format!("Failed to get expired burns: {e}")))?;

        let mut expired = Vec::new();

        for row in &expired_rows {
            if let Err(e) = self
                .event_storage
                .redact_event_content(&row.event_id)
                .await
            {
                ::tracing::warn!(
                    "Failed to redact event content for burn {}: {}",
                    row.event_id, e
                );
            }

            if let Err(e) = self
                .event_storage
                .create_event(
                    crate::storage::event::CreateEventParams {
                        event_id: crate::common::crypto::generate_event_id(&self.server_name),
                        room_id: row.room_id.clone(),
                        user_id: row.user_id.clone(),
                        event_type: "m.room.redaction".to_string(),
                        content: serde_json::json!({"reason": "Burn after read"}),
                        state_key: None,
                        origin_server_ts: now,
                    },
                    None,
                )
                .await
            {
                ::tracing::warn!(
                    "Failed to create redaction event for burn {}: {}",
                    row.event_id, e
                );
            }

            if let Err(e) = self.storage.mark_burn_processed(row.id).await {
                ::tracing::warn!("Failed to mark burn processed {}: {}", row.id, e);
            }

            if let Err(e) = self
                .storage
                .log_burned_event(&row.user_id, &row.room_id, &row.event_id, now)
                .await
            {
                ::tracing::warn!("Failed to log burned event {}: {}", row.event_id, e);
            }

            expired.push(BurnEvent {
                id: row.id,
                event_id: row.event_id.clone(),
                room_id: row.room_id.clone(),
                user_id: row.user_id.clone(),
                created_ts: row.created_ts,
                delete_at: row.delete_at,
            });
        }

        Ok(expired)
    }

    pub async fn recover_pending_burns(&self) {
        ::tracing::info!("Recovering pending burn-after-read events from database...");

        match self.process_expired_burns().await {
            Ok(expired) => {
                if expired.is_empty() {
                    ::tracing::info!("No expired burn events to recover");
                } else {
                    ::tracing::info!(
                        "Recovered and processed {} expired burn events",
                        expired.len()
                    );
                }
            }
            Err(e) => {
                ::tracing::error!("Failed to recover expired burn events: {}", e);
            }
        }
    }

    pub async fn start_burn_processor(self: Arc<Self>) {
        let mut state = self.processor_state.write().await;
        if state.is_running {
            return;
        }
        state.is_running = true;
        drop(state);

        let service = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));

            loop {
                interval.tick().await;

                if let Err(e) = service.process_expired_burns().await {
                    ::tracing::error!("Burn processor error: {}", e);
                }
            }
        });

        ::tracing::info!("Burn-after-read processor started (5s interval)");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_burn_settings_struct() {
        let settings = BurnSettings {
            is_enabled: true,
            burn_after_ms: 60_000,
        };
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
            delete_at: 1234567950,
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
        let stats = BurnStats {
            total_burned: 10,
            total_pending: 3,
            rooms_enabled: 2,
        };
        assert_eq!(stats.total_burned, 10);
        assert_eq!(stats.total_pending, 3);
        assert_eq!(stats.rooms_enabled, 2);
    }
}
