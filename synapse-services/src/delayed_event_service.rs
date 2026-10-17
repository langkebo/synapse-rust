//! MSC4140 — cancellable delayed events: scheduling and client-driven management.
//!
//! Owns the *policy* for delayed events (ownership fail-closed, "no longer
//! pending" transitions) so the HTTP layer only parses requests and renders
//! responses. The storage seam stays in `synapse-storage`; the background
//! dispatcher in `src/server/` keeps talking to the storage handle directly
//! because it has no HTTP-facing policy to apply.

use std::sync::Arc;
use synapse_common::ApiError;

// `DelayedEvent` is part of this module's **public** API — `schedule`, `get`,
// `list_for_user` and `manage` all return or describe it — so callers must be
// able to name it. Re-exported here because `synapse-web` does not (and should
// not) depend on `synapse-storage` directly: the route layer speaks the service
// seam, and a route that has to add a storage dependency just to name a return
// type is the seam leaking. `CreateDelayedEventRequest` / `DelayedEventAction`
// are already re-exported for the same reason.
use synapse_storage::delayed_events::DelayedEventStorageApi;
pub use synapse_storage::delayed_events::{CreateDelayedEventRequest, DelayedEvent, DelayedEventAction};

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

    /// Fetch a single *pending* delayed event for `caller_id` (MSC4140 `GET`).
    ///
    /// Mirrors [`manage`]'s fail-closed ownership rule so the endpoint never
    /// leaks the existence of another user's delayed event: a missing event, a
    /// foreign event, and an already-settled (`non-pending`) event all surface
    /// as "not found". Upstream only returns rows where `is_processed = False`,
    /// which this project tracks as `status == "pending"`.
    pub async fn get(&self, delay_id: i64, caller_id: &str, request_id: &str) -> Result<DelayedEvent, ApiError> {
        let event = self
            .storage
            .get_delayed_event(delay_id)
            .await?
            .ok_or_else(|| ApiError::not_found("Delayed event not found".to_string()))?;

        if event.user_id != caller_id || event.status != "pending" {
            tracing::warn!(
                request_id = %request_id,
                delay_id,
                owner = %event.user_id,
                caller = %caller_id,
                status = %event.status,
                "MSC4140 single-event lookup denied: not a pending event owned by the caller"
            );
            return Err(ApiError::not_found("Delayed event not found".to_string()));
        }

        Ok(event)
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

    /// List the caller's **pending** delayed events, oldest first.
    ///
    /// Backs `GET /_matrix/client/unstable/org.matrix.msc4140/delayed_events`.
    ///
    /// The `user_id` filter is pushed **into the storage query** (see
    /// [`DelayedEventStorageApi::list_pending_delayed_events_for_user`]) rather
    /// than applied here: unlike [`Self::get`]/[`Self::manage`], there is no
    /// single `delay_id` to check ownership against, so "only my rows" has to be
    /// a property of the query itself.
    pub async fn list_for_user(&self, caller_id: &str) -> Result<Vec<DelayedEvent>, ApiError> {
        let events = self.storage.list_pending_delayed_events_for_user(caller_id).await?;
        tracing::debug!(user_id = %caller_id, count = events.len(), "MSC4140 pending delayed events listed");
        Ok(events)
    }

    /// Cancel **every pending** delayed event owned by `user_id`; returns how
    /// many were actually cancelled.
    ///
    /// Account deactivation calls this **first**, mirroring upstream Synapse
    /// v1.163.0 (`deactivate_account.py`: "Cancel the user's delayed events
    /// (MSC4140). Do this first, so that none of them get sent while the rest of
    /// the deactivation is in progress."). Without it a user who scheduled a
    /// message and then deactivated their account would still have it delivered
    /// — the deactivation would be a security hole rather than a state change.
    ///
    /// Deliberately composed from the existing list + per-row cancel instead of a
    /// bulk `UPDATE`: the single-row path is the one the client-facing `cancel`
    /// action already exercises (status transition, "no longer pending" rule), so
    /// there is exactly one place where "what it means to cancel" is expressed. A
    /// second bulk statement could drift on which statuses count as pending.
    ///
    /// A row that settles between the list and the cancel is skipped rather than
    /// treated as an error: `cancel_delayed_event` returns `false` for it, and
    /// "someone else already settled it" is not a deactivation failure.
    pub async fn cancel_all_for_user(&self, user_id: &str) -> Result<usize, ApiError> {
        let pending = self.storage.list_pending_delayed_events_for_user(user_id).await?;
        let total = pending.len();
        let mut cancelled = 0usize;

        for event in pending {
            if self.storage.cancel_delayed_event(event.id).await? {
                cancelled += 1;
            } else {
                tracing::debug!(
                    user_id = %user_id,
                    delay_id = event.id,
                    "MSC4140 delayed event settled before deactivation could cancel it; skipping"
                );
            }
        }

        tracing::info!(
            user_id = %user_id,
            cancelled,
            pending = total,
            "MSC4140 pending delayed events cancelled for account deactivation"
        );
        Ok(cancelled)
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
            sticky_duration_ms: None,
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

    #[tokio::test]
    async fn owner_can_fetch_their_pending_delayed_event() {
        let (service, _, id) = seeded().await;
        let event = service.get(id, "@alice:example.com", "req-7").await.expect("get");
        assert_eq!(event.id, id);
        assert_eq!(event.user_id, "@alice:example.com");
        assert_eq!(event.room_id, "!room:example.com");
        assert_eq!(event.event_type, "m.room.message");
        assert_eq!(event.delay_ms, 5_000);
    }

    #[tokio::test]
    async fn fetching_a_foreign_event_is_not_found() {
        let (service, _, id) = seeded().await;
        let err = service.get(id, "@mallory:example.com", "req-8").await.expect_err("foreign caller must be hidden");
        assert_eq!(err.kind, ApiErrorKind::NotFound, "must hide existence, not 403");
    }

    #[tokio::test]
    async fn fetching_a_missing_event_is_not_found() {
        let (service, _, _) = seeded().await;
        let err = service.get(9_999, "@alice:example.com", "req-9").await.expect_err("missing event must 404");
        assert_eq!(err.kind, ApiErrorKind::NotFound);
    }

    #[tokio::test]
    async fn fetching_a_settled_event_is_not_found() {
        let (service, _, id) = seeded().await;
        service.manage(id, DelayedEventAction::Send, "@alice:example.com", "req-10").await.expect("send");
        let err = service
            .get(id, "@alice:example.com", "req-11")
            .await
            .expect_err("settled event must be hidden (upstream filters is_processed=false)");
        assert_eq!(err.kind, ApiErrorKind::NotFound);
    }

    // ── cancel_all_for_user (account deactivation) ────────────────────────

    #[tokio::test]
    async fn cancel_all_for_user_cancels_every_pending_row_of_that_user() {
        let store = Arc::new(InMemoryDelayedEventStore::new());
        let service = Arc::new(DelayedEventService::new(store.clone()));
        let first = service.schedule(request("@alice:example.com")).await.expect("schedule 1").id;
        let second = service.schedule(request("@alice:example.com")).await.expect("schedule 2").id;

        let cancelled = service.cancel_all_for_user("@alice:example.com").await.expect("cancel all");

        assert_eq!(cancelled, 2, "both pending rows must be cancelled");
        assert_eq!(store.status_of(first).await.as_deref(), Some("cancelled"));
        assert_eq!(store.status_of(second).await.as_deref(), Some("cancelled"));
        assert!(
            service.list_for_user("@alice:example.com").await.expect("list").is_empty(),
            "nothing may stay pending after deactivation"
        );
    }

    #[tokio::test]
    async fn cancel_all_for_user_leaves_other_users_alone() {
        let store = Arc::new(InMemoryDelayedEventStore::new());
        let service = Arc::new(DelayedEventService::new(store.clone()));
        let alice = service.schedule(request("@alice:example.com")).await.expect("alice").id;
        let bob = service.schedule(request("@bob:example.com")).await.expect("bob").id;

        let cancelled = service.cancel_all_for_user("@alice:example.com").await.expect("cancel all");

        assert_eq!(cancelled, 1);
        assert_eq!(store.status_of(alice).await.as_deref(), Some("cancelled"));
        assert_eq!(
            store.status_of(bob).await.as_deref(),
            Some("pending"),
            "another user's delayed event must survive — the filter is per-owner, not global"
        );
    }

    /// A row that settles between the list and the cancel must be **skipped**, not
    /// turned into a deactivation failure: `cancel_delayed_event` reports `false`
    /// for it, and the count must reflect only what this call actually cancelled.
    #[tokio::test]
    async fn cancel_all_for_user_counts_only_rows_it_actually_cancelled() {
        let store = Arc::new(InMemoryDelayedEventStore::new());
        let service = Arc::new(DelayedEventService::new(store.clone()));
        let already_sent = service.schedule(request("@alice:example.com")).await.expect("schedule").id;
        service.manage(already_sent, DelayedEventAction::Send, "@alice:example.com", "req").await.expect("send");

        let cancelled = service.cancel_all_for_user("@alice:example.com").await.expect("cancel all");

        assert_eq!(cancelled, 0, "a settled row is not a cancellation");
        assert_eq!(store.status_of(already_sent).await.as_deref(), Some("sent"));
    }

    #[tokio::test]
    async fn cancel_all_for_user_is_a_noop_when_nothing_is_pending() {
        let store = Arc::new(InMemoryDelayedEventStore::new());
        let service = Arc::new(DelayedEventService::new(store.clone()));
        assert_eq!(service.cancel_all_for_user("@nobody:example.com").await.expect("cancel all"), 0);
    }
}
