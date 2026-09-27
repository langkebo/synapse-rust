//! Admin event-redaction queries and batch redaction.
//!
//! `find_event_ids_for_redaction` / `batch_redact_events` are inherent methods
//! on the persistence-layer `EventStorage`; the admin route needs them, so this
//! narrow service is the seam that keeps `synapse-web` free of storage types
//! (B4-5c).
//!
//! It also owns the MSC3912 single-layer cascade: the storage layer can find the
//! related events and redact one, but only this layer holds the `RoomAuth` needed
//! to decide, per related event, whether the requesting user may redact it.

use std::sync::Arc;
use synapse_common::error::ApiError;
use synapse_storage::event::EventStorage;

use crate::auth::RoomAuth;

/// Narrow service over `EventStorage`'s redaction surface.
pub struct EventRedactionService {
    storage: Arc<EventStorage>,
    room_auth: Arc<dyn RoomAuth>,
}

impl EventRedactionService {
    /// See [`new`].
    pub fn new(storage: Arc<EventStorage>, room_auth: Arc<dyn RoomAuth>) -> Self {
        Self { storage, room_auth }
    }

    /// Event ids in `room_id` whose `origin_server_ts` falls inside the
    /// requested window, oldest first.
    pub async fn find_event_ids_for_redaction(
        &self,
        room_id: &str,
        before_ts: Option<i64>,
        after_ts: Option<i64>,
        limit: i64,
    ) -> Result<Vec<String>, ApiError> {
        self.storage
            .find_event_ids_for_redaction(room_id, before_ts, after_ts, limit)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to query events for redaction", e))
    }

    /// Redact the given events, returning how many rows were redacted.
    ///
    /// `redaction_event_id` is the id of the `m.room.redaction` event that
    /// caused this redaction (a self-referential FK to `events.event_id`), or
    /// `None` when there is no causing event (a server/operator action).
    pub async fn batch_redact_events(
        &self,
        event_ids: &[String],
        redaction_event_id: Option<&str>,
    ) -> Result<u64, ApiError> {
        self.storage
            .batch_redact_events(event_ids, redaction_event_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to batch redact events", e))
    }

    /// MSC3912: Cascade redact an event and all related events.
    ///
    /// Finds all events that reference the target event via relationship fields
    /// (m.in_reply_to, m.relates_to, m.replace) and redacts them recursively.
    ///
    /// # Arguments
    /// * `event_id` - The event to cascade redact from
    /// * `redaction_event_id` - The id of the `m.room.redaction` event that
    ///   caused this redaction (a self-referential FK to `events.event_id`), or
    ///   `None` when there is no causing event (the admin cascade endpoint is an
    ///   operator action and persists no redaction event)
    /// * `max_depth` - Maximum recursion depth (default 5)
    ///
    /// # Returns
    /// Number of events successfully redacted
    pub async fn cascade_redact_event(
        &self,
        event_id: &str,
        redaction_event_id: Option<&str>,
        max_depth: u32,
    ) -> Result<u64, ApiError> {
        self.storage
            .cascade_redact_event(event_id, redaction_event_id, max_depth)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to cascade redact event", e))
    }

    /// MSC3912: Single-layer cascade redaction for client-side.
    ///
    /// Finds all events that reference the target event via relationship fields
    /// (m.in_reply_to, m.relates_to, m.replace) and redacts them at a single
    /// level (no recursion).
    ///
    /// **Authorization.** Each related event is checked individually with
    /// [`RoomAuth::can_redact_event`] — the same rule the non-cascade redaction
    /// path uses. Without that check `with_rel_types` is a privilege-escalation
    /// primitive: the requester only has to be allowed to redact the *target*
    /// event to also wipe every related event, including other users'. Denied
    /// events are skipped (never redacted) and logged with structured fields.
    ///
    /// **Audit tracking.** The `redaction_event_id` parameter carries the ID
    /// of the `m.room.redaction` event that was already persisted by the caller
    /// (the redaction handler in `events.rs`). This is what satisfies the
    /// self-referential FK `events.redacted_by -> events.event_id`, and it is
    /// what lets an auditor reconstruct who redacted what without joining back
    /// to a user ID.
    ///
    /// # Arguments
    /// * `room_id` - Room to search in
    /// * `event_id` - Target event ID
    /// * `rel_types` - List of relationship types to match (use `["*"]` for all)
    /// * `actor_user_id` - The user requesting the redaction, whose permissions
    ///   are evaluated against every related event
    /// * `redaction_event_id` - The ID of the persisted `m.room.redaction`
    ///   event; passed to [`EventStorage::redact_event_content`] so that
    ///   `events.redacted_by` on every cascaded target carries a valid event ID
    ///
    /// # Returns
    /// Number of events actually redacted (denied/skipped events are not counted)
    pub async fn cascade_redact_related_events(
        &self,
        room_id: &str,
        event_id: &str,
        rel_types: &[String],
        actor_user_id: &str,
        redaction_event_id: &str,
    ) -> Result<u64, ApiError> {
        let related = self
            .storage
            .find_related_events_single_layer(room_id, event_id, rel_types)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to query related events for redaction", e))?;

        let mut redacted = 0u64;
        for target_id in related {
            // The related event's sender decides whether the actor may redact it.
            // Look it up the same way the non-cascade path does; a missing row is
            // fail-closed (skip) rather than an implicit authorization.
            let Some(target_event) = self
                .storage
                .get_event(&target_id)
                .await
                .map_err(|e| ApiError::internal_with_cause("Failed to load related event for redaction", e))?
            else {
                ::tracing::warn!(
                    target: "security_audit",
                    event = "cascade_redaction_target_missing",
                    room_id = %room_id,
                    event_id = %event_id,
                    target_event_id = %target_id,
                    "Related event disappeared before cascade redaction; skipping"
                );
                continue;
            };

            if let Err(error) = self.room_auth.can_redact_event(room_id, actor_user_id, &target_event.user_id).await {
                ::tracing::warn!(
                    target: "security_audit",
                    event = "cascade_redaction_denied",
                    room_id = %room_id,
                    event_id = %event_id,
                    target_event_id = %target_id,
                    actor_user_id = %actor_user_id,
                    target_sender_id = %target_event.user_id,
                    error = %error,
                    "Skipping related event: actor is not allowed to redact it"
                );
                continue;
            }

            if let Err(error) = self.storage.redact_event_content(&target_id, Some(redaction_event_id)).await {
                // Keep going: one bad row must not silently drop the rest of the
                // cascade, but it must not disappear either.
                ::tracing::error!(
                    target: "security_audit",
                    event = "cascade_redaction_target_failed",
                    room_id = %room_id,
                    event_id = %event_id,
                    target_event_id = %target_id,
                    actor_user_id = %actor_user_id,
                    redaction_event_id = %redaction_event_id,
                    error = %error,
                    "Failed to redact a related event during cascade"
                );
                continue;
            }

            redacted += 1;
        }

        Ok(redacted)
    }
}
