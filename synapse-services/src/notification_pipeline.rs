//! Event → notification-inbox pipeline (D16).
//!
//! `GET /_matrix/client/v3/notifications` reads the `notifications` table, but
//! nothing on the message path used to write to it: the only producer was
//! `PushNotificationService::send_notification`, which is *delivery*-driven and
//! returns early when the recipient has no push device. A user who never
//! registered a pusher therefore had a structurally empty inbox.
//!
//! This module records the inbox entry when the event is persisted, independent
//! of whether there is anywhere to deliver a push. It is invoked from
//! [`crate::notifying_event_writer::NotifyingEventWriter`], the single decorator
//! every write path (local send, membership, federation) goes through, so no
//! handler has to remember to call it.
//!
//! The per-recipient evaluation itself is one `INSERT ... SELECT` in
//! [`PushStorage::record_room_event_notifications`]; see that method for the
//! suppression rules.
//!
//! # Not a delivery path
//!
//! Recording a row does **not** send a push notification. Delivery stays in
//! `PushNotificationService`, and the two are deliberately decoupled: an inbox
//! entry is durable state, a push is best-effort.

use async_trait::async_trait;
use std::sync::Arc;

use synapse_common::current_timestamp_millis;
use synapse_storage::event::RoomEvent;
use synapse_storage::push::PushStorage;
use tracing::{debug, info, warn};

/// Events older than this are treated as backfill and never recorded.
///
/// Federation backfill (`create_event_with_graph` / `create_outlier_event`) can
/// replay a room's history. Without a freshness check, joining a room with
/// thousands of historical messages would insert thousands of inbox rows for
/// events the client has already seen in the room timeline.
const MAX_EVENT_AGE_MS: i64 = 5 * 60 * 1000;

/// Called after an event is committed, to write the notification-inbox rows it
/// should produce.
///
/// Implementations must not fail the write: the event is already persisted, so
/// an error here is a missing inbox entry, not a failed request.
#[async_trait]
pub trait EventNotificationRecorder: Send + Sync {
    /// Records (or suppresses) the inbox rows for `event`.
    async fn record_for_event(&self, event: &RoomEvent);
}

/// The default [`EventNotificationRecorder`], backed by the `notifications` table.
pub struct NotificationPipeline {
    push_storage: Arc<PushStorage>,
    /// This server's name; users not ending in `:<server_name>` are remote and
    /// have no inbox here.
    local_server_name: String,
}

impl NotificationPipeline {
    /// Builds the pipeline for a server whose local users are `@localpart:<server_name>`.
    pub fn new(push_storage: Arc<PushStorage>, local_server_name: impl Into<String>) -> Self {
        Self { push_storage, local_server_name: local_server_name.into() }
    }
}

/// Whether an event is of a type the default push rules notify on.
///
/// Mirrors the two default rules that actually alert a user: `m.room.message`
/// (except `m.notice`, which the default `suppress_notices` rule silences) and
/// `m.room.encrypted`. Everything else — state events, redactions, receipts,
/// typing — never lands in `/notifications`.
fn is_notifiable(event_type: &str, content: &serde_json::Value) -> bool {
    match event_type {
        "m.room.encrypted" => true,
        "m.room.message" => content.get("msgtype").and_then(|v| v.as_str()) != Some("m.notice"),
        _ => false,
    }
}

/// Reads `content.m.mentions` (`user_ids` + `room`) from a message.
///
/// Encrypted events have opaque content, so they never carry mentions the
/// server can see — they degrade to `message`.
///
/// `m.mentions` is only one of the default highlight rules. The display-name /
/// local-user-name rules and user keyword (`content`) rules match the body text
/// instead; those are evaluated in the same SQL statement from
/// [`body_text`], so they are not handled here.
fn mentions(content: &serde_json::Value) -> (Vec<String>, bool) {
    let mentions = content.get("m.mentions");

    let user_ids = mentions
        .and_then(|m| m.get("user_ids"))
        .and_then(|v| v.as_array())
        .map(|ids| ids.iter().filter_map(|id| id.as_str().map(str::to_owned)).collect())
        .unwrap_or_default();

    let room = mentions.and_then(|m| m.get("room")).and_then(|v| v.as_bool()).unwrap_or(false);

    (user_ids, room)
}

/// The plaintext haystack for the body-text highlight rules.
///
/// `.m.rule.contains_display_name` / `.m.rule.contains_user_name` and any
/// user-authored keyword (`kind='content'`) rule match against `content.body`.
/// Encrypted events have no `body`, so this is `""` and only explicit
/// `m.mentions` can highlight.
fn body_text(content: &serde_json::Value) -> &str {
    content.get("body").and_then(|v| v.as_str()).unwrap_or("")
}

#[async_trait]
impl EventNotificationRecorder for NotificationPipeline {
    async fn record_for_event(&self, event: &RoomEvent) {
        if event.state_key.is_some() || !is_notifiable(&event.event_type, &event.content) {
            return;
        }

        let now = current_timestamp_millis();
        if now.saturating_sub(event.origin_server_ts) > MAX_EVENT_AGE_MS {
            debug!(
                event_id = %event.event_id,
                room_id = %event.room_id,
                age_ms = now.saturating_sub(event.origin_server_ts),
                "Skipping notification record for a backfilled event"
            );
            return;
        }

        let (highlight_user_ids, highlight_room) = mentions(&event.content);

        match self
            .push_storage
            .record_room_event_notifications(
                &event.room_id,
                &event.event_id,
                &event.user_id,
                &highlight_user_ids,
                highlight_room,
                event.origin_server_ts,
                &self.local_server_name,
                body_text(&event.content),
            )
            .await
        {
            Ok(0) => {}
            Ok(count) => {
                info!(
                    event_id = %event.event_id,
                    room_id = %event.room_id,
                    notified_members = count,
                    "Recorded event notifications"
                );
            }
            // The event is already committed: surfacing this as a request error
            // would report a failure for work that succeeded.
            Err(error) => {
                warn!(
                    event_id = %event.event_id,
                    room_id = %event.room_id,
                    error = %error,
                    "Failed to record event notifications"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn messages_are_notifiable_but_notices_are_not() {
        assert!(is_notifiable("m.room.message", &json!({"msgtype": "m.text", "body": "hi"})));
        // The default `suppress_notices` push rule silences `m.notice`.
        assert!(!is_notifiable("m.room.message", &json!({"msgtype": "m.notice", "body": "bot"})));
        assert!(is_notifiable("m.room.encrypted", &json!({"algorithm": "m.megolm.v1.aes-sha2"})));
        assert!(!is_notifiable("m.room.member", &json!({"membership": "join"})));
        assert!(!is_notifiable("m.receipt", &json!({})));
    }

    #[test]
    fn mentions_are_read_from_the_msc3952_block() {
        let (user_ids, room) = mentions(&json!({
            "msgtype": "m.text",
            "body": "hi @alice:test.com",
            "m.mentions": { "user_ids": ["@alice:test.com"], "room": true }
        }));
        assert_eq!(user_ids, vec!["@alice:test.com".to_owned()]);
        assert!(room);

        let (user_ids, room) = mentions(&json!({"msgtype": "m.text", "body": "hi"}));
        assert!(user_ids.is_empty(), "a message without m.mentions highlights nobody");
        assert!(!room);
    }

    #[test]
    fn body_text_is_the_plaintext_body_or_empty() {
        assert_eq!(body_text(&json!({"msgtype": "m.text", "body": "hi @alice:test.com"})), "hi @alice:test.com");
        // Encrypted events carry no `body`; display-name/keyword rules cannot fire.
        assert_eq!(body_text(&json!({"algorithm": "m.megolm.v1.aes-sha2", "ciphertext": "..."})), "");
    }
}
