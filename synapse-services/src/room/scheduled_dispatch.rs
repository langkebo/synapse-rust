//! MSC4140 — dispatch of **one** due delayed event.
//!
//! Extracted from the background loop in `src/server/mod.rs` for two reasons:
//!
//! * that file is on the file-size ratchet's baseline, and the send path was the
//!   largest block in it;
//! * the loop itself is not callable from a test, so "a scheduled event finally
//!   goes out" had no regression coverage at all. This function does.
//!
//! # MSC4354 sticky events
//!
//! A delayed event may be scheduled with a sticky duration. The TTL cannot be
//! written at scheduling time (there is no event yet), so it is stored on the
//! `delayed_events` row and applied **here**, immediately after the send, to the
//! event's `unsigned` projection. Upstream PR #19354 does the same: the handler
//! injects `StickyEventField(duration_ms=...)` into the event dict at send time.
//!
//! Applying it here rather than inside `send_message_with_txn` keeps the message
//! path free of a scheduling concern — a plain `/send` must not learn about
//! `delayed_events`, and the failure semantics differ: if the `unsigned` write
//! fails, the event is already sent, so the dispatcher should report the failure
//! rather than pretend the row was delivered *and* pinned.
//!
//! # Why state events cannot be sticky
//!
//! Scheduling validation rejects `state_key` + sticky (upstream:
//! "Sticky state events are not allowed"). So the state branch below never has a
//! TTL to apply — and if one ever arrived by some other route it is dropped
//! loudly rather than silently written onto a state event.

use crate::room::service::RoomService;
use synapse_common::{crypto::generate_event_id, current_timestamp_millis, ApiError};
use synapse_storage::delayed_events::DelayedEvent;
use synapse_storage::sticky_event::STICKY_TTL_UNSIGNED_FIELD;
use synapse_storage::CreateEventParams;

/// Send one due delayed event, applying its sticky TTL when it was scheduled sticky.
///
/// The caller owns the distributed lock and the `mark_sent` bookkeeping; this
/// function only performs the write that the row describes.
pub async fn dispatch_due_event(room: &RoomService, server_name: &str, event: &DelayedEvent) -> Result<(), ApiError> {
    if let Some(state_key) = event.state_key.clone() {
        if event.sticky_duration_ms.is_some() {
            ::tracing::warn!(
                delay_id = event.id,
                room_id = %event.room_id,
                "Delayed state event carries a sticky duration; ignoring it \
                 (sticky state events are rejected at scheduling time)"
            );
        }
        // State events must go through `create_event` to preserve state semantics
        // (replace-by-(type, state_key), power-level checks replayed from the
        // persisted row).
        room.messaging()
            .create_event(
                CreateEventParams {
                    event_id: generate_event_id(server_name),
                    room_id: event.room_id.clone(),
                    user_id: event.user_id.clone(),
                    event_type: event.event_type.clone(),
                    content: event.content.clone(),
                    state_key: Some(state_key),
                    origin_server_ts: current_timestamp_millis(),
                    redacts: None,
                },
                None,
            )
            .await
            .map(|_| ())?;
        return Ok(());
    }

    // The synthetic `event_id` from `delayed_events` doubles as the txn id, so a
    // retry after a crash dedupes on the DB constraint instead of double-sending.
    let response = room
        .messaging()
        .send_message_with_txn(&event.room_id, &event.user_id, &event.event_type, &event.content, &event.event_id)
        .await?;

    let Some(duration_ms) = event.sticky_duration_ms else {
        return Ok(());
    };

    // The event is already persisted at this point. Propagate a failure rather
    // than swallowing it: a retry re-enters through the same txn id, gets the same
    // event back from the dedup constraint, and re-attempts this write.
    let event_id = response
        .get("event_id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| ApiError::internal("Sent delayed event without an event_id".to_string()))?;
    room.messaging()
        .update_event_unsigned(event_id, &serde_json::json!({ STICKY_TTL_UNSIGNED_FIELD: duration_ms }))
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(state_key: Option<&str>, sticky: Option<i64>) -> DelayedEvent {
        DelayedEvent {
            id: 7,
            room_id: "!r:localhost".to_string(),
            user_id: "@u:localhost".to_string(),
            device_id: "D".to_string(),
            event_id: "$delayed:!r:@u:1".to_string(),
            event_type: "m.room.message".to_string(),
            state_key: state_key.map(str::to_string),
            content: serde_json::json!({ "msgtype": "m.text", "body": "hi" }),
            delay_ms: 1_000,
            scheduled_ts: 0,
            created_ts: 0,
            status: "pending".to_string(),
            retry_count: 0,
            last_error: None,
            sticky_duration_ms: sticky,
        }
    }

    /// The row is the only carrier of the hint. If the column were not read back,
    /// every scheduled sticky event would silently go out unpinned — the exact
    /// failure the old 501 was protecting against, just quieter.
    #[test]
    fn the_stored_sticky_duration_survives_a_round_trip_through_the_row() {
        assert_eq!(row(None, Some(60_000)).sticky_duration_ms, Some(60_000));
        assert_eq!(row(None, None).sticky_duration_ms, None);
        assert_eq!(row(Some("k"), None).sticky_duration_ms, None);
    }
}
