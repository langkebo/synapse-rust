//! MSC4140 — cancellable delayed events: scheduling and client-driven management.
//!
//! Owns the *policy* for delayed events (ownership fail-closed, "no longer
//! pending" transitions) so the HTTP layer only parses requests and renders
//! responses. The storage seam stays in `synapse-storage`; the background
//! dispatcher in `src/server/` keeps talking to the storage handle directly
//! because it has no HTTP-facing policy to apply.

use std::sync::Arc;
use synapse_common::ApiError;

pub use synapse_storage::delayed_events::{CreateDelayedEventRequest, DelayedEventAction};
use synapse_storage::delayed_events::{DelayedEvent, DelayedEventStorageApi};

/// Service for MSC4140 cancellable delayed events.
pub struct DelayedEventService {
    storage: Arc<dyn DelayedEventStorageApi>,
}

impl DelayedEventService {
    /// See [`new`].
    pub fn new(storage: Arc<dyn DelayedEventStorageApi>) -> Self {
        Self { storage }
    }

    /// Schedule a delayed event, returning the stored row (including its
    /// client-facing `delay_id`).
    pub async fn schedule(&self, request: CreateDelayedEventRequest) -> Result<DelayedEvent, ApiError> {
        self.storage.create_delayed_event(request).await
    }

    /// Apply a client-requested `action` to the delayed event `delay_id` on
    /// behalf of `caller_id`.
    ///
    /// Ownership is checked fail-closed *before* any state mutation, and a
    /// missing event and a foreign event both map to "not found" so the
    /// endpoint never leaks the existence of another user's delayed event.
    pub async fn manage(
        &self,
        delay_id: i64,
        action: DelayedEventAction,
        caller_id: &str,
        request_id: &str,
    ) -> Result<(), ApiError> {
        let event = self
            .storage
            .get_delayed_event(delay_id)
            .await?
            .ok_or_else(|| ApiError::not_found("Delayed event not found".to_string()))?;

        if event.user_id != caller_id {
            tracing::warn!(
                request_id = %request_id,
                delay_id,
                owner = %event.user_id,
                caller = %caller_id,
                "MSC4140 management denied: caller does not own delayed event"
            );
            return Err(ApiError::not_found("Delayed event not found".to_string()));
        }

        let (updated, verb) = match action {
            DelayedEventAction::Cancel => (self.storage.cancel_delayed_event(delay_id).await?, "cancelled"),
            DelayedEventAction::Restart => (self.storage.restart_delayed_event(delay_id).await?, "restarted"),
            DelayedEventAction::Send => (self.storage.mark_sent(delay_id).await?, "sent"),
        };
        if !updated {
            return Err(ApiError::bad_request(format!("Delayed event is no longer pending and cannot be {verb}")));
        }

        tracing::info!(
            request_id = %request_id,
            delay_id,
            action = action.as_str(),
            user_id = %caller_id,
            "MSC4140 delayed event managed"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use synapse_common::ApiErrorKind;
    use synapse_storage::test_mocks::delayed_event::InMemoryDelayedEventStore;

    fn request(user_id: &str) -> CreateDelayedEventRequest {
        CreateDelayedEventRequest {
            room_id: "!room:example.com".to_string(),
            user_id: user_id.to_string(),
            device_id: "DEVICE".to_string(),
            event_type: "m.room.message".to_string(),
            state_key: None,
            content: serde_json::json!({ "body": "later" }),
            delay_ms: 5_000,
        }
    }

    async fn seeded() -> (Arc<DelayedEventService>, Arc<InMemoryDelayedEventStore>, i64) {
        let store = Arc::new(InMemoryDelayedEventStore::new());
        let service = Arc::new(DelayedEventService::new(store.clone()));
        let scheduled = service.schedule(request("@alice:example.com")).await.expect("schedule");
        (service, store, scheduled.id)
    }

    #[tokio::test]
    async fn owner_can_cancel_their_delayed_event() {
        let (service, store, id) = seeded().await;
        service.manage(id, DelayedEventAction::Cancel, "@alice:example.com", "req-1").await.expect("cancel");
        assert_eq!(store.status_of(id).await.as_deref(), Some("cancelled"));
    }

    #[tokio::test]
    async fn foreign_caller_gets_not_found_and_no_mutation() {
        let (service, store, id) = seeded().await;
        let err = service
            .manage(id, DelayedEventAction::Cancel, "@mallory:example.com", "req-2")
            .await
            .expect_err("foreign caller must be rejected");
        assert_eq!(err.kind, ApiErrorKind::NotFound, "must hide existence, not 403");
        assert_eq!(store.status_of(id).await.as_deref(), Some("pending"), "fail-closed: no mutation");
    }

    #[tokio::test]
    async fn managing_a_missing_event_is_not_found() {
        let (service, _, _) = seeded().await;
        let err = service
            .manage(9_999, DelayedEventAction::Send, "@alice:example.com", "req-3")
            .await
            .expect_err("missing event must 404");
        assert_eq!(err.kind, ApiErrorKind::NotFound);
    }

    #[tokio::test]
    async fn acting_on_a_settled_event_is_a_bad_request() {
        let (service, _, id) = seeded().await;
        service.manage(id, DelayedEventAction::Send, "@alice:example.com", "req-4").await.expect("send");
        let err = service
            .manage(id, DelayedEventAction::Cancel, "@alice:example.com", "req-5")
            .await
            .expect_err("settled event cannot be cancelled");
        assert_eq!(err.kind, ApiErrorKind::BadRequest);
        assert!(err.message.contains("cannot be cancelled"), "got: {}", err.message);
    }

    #[tokio::test]
    async fn restart_keeps_the_event_pending() {
        let (service, store, id) = seeded().await;
        service.manage(id, DelayedEventAction::Restart, "@alice:example.com", "req-6").await.expect("restart");
        assert_eq!(store.status_of(id).await.as_deref(), Some("pending"));
    }
}
