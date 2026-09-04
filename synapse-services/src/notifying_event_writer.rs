//! Wake-up decorator around [`EventWriter`].
//!
//! # Why this exists
//!
//! Sliding-sync clients park in a long-poll for up to 30 seconds when a sync
//! produces no data (see `sliding_sync_service`). Parking is only correct if
//! *every* write path releases the parked waiters; a write that forgets to
//! notify turns into a user-visible delay of up to the full poll timeout.
//!
//! Sprinkling `notify_room()` calls across the ~15 mutating HTTP handlers is
//! not a durable answer — each new handler is one forgotten call away from
//! reintroducing the bug, and nothing in the type system flags the omission.
//!
//! Instead we wrap the storage-layer writer. Every service (messaging,
//! membership, lifecycle, moderation, federation backfill) persists through
//! `Arc<dyn EventWriter>`, so decorating that one trait object at the single
//! wiring site covers all of them, including handlers that do not exist yet.
//!
//! # Transaction safety
//!
//! `create_event` accepts an optional transaction. When a caller passes
//! `Some(tx)` the row is not committed yet, so waking a waiter would be
//! actively harmful: the waiter re-reads the database, cannot see the
//! uncommitted event, and returns an empty response. We therefore only notify
//! on the auto-commit path (`tx.is_none()`). Callers that batch events inside a
//! transaction (room creation) are responsible for notifying after commit.
//!
//! # Known gaps
//!
//! - `redact_event_content` receives only an `event_id`, with no `room_id` to
//!   address a notification to. Redactions surface on the next poll.
//! - `delete_events_before` is a retention/purge path; it is not latency
//!   sensitive and is deliberately silent.
//!
//! Both gaps degrade to "picked up within the poll timeout", never to a lost
//! update, because waiters always re-read the database when they wake.

use async_trait::async_trait;
use std::sync::Arc;

use synapse_storage::event::{CreateEventParams, EventWriter, RoomEvent};

use crate::event_notifier::EventNotifier;

/// Decorates an [`EventWriter`], publishing a wake-up after each successful
/// write so that long-polling sliding-sync clients return immediately.
pub struct NotifyingEventWriter {
    inner: Arc<dyn EventWriter>,
    notifier: EventNotifier,
}

impl NotifyingEventWriter {
    /// Wraps `inner`, routing wake-ups through `notifier`.
    pub fn new(inner: Arc<dyn EventWriter>, notifier: EventNotifier) -> Self {
        Self { inner, notifier }
    }

    /// Publishes the wake-up for a freshly persisted event.
    ///
    /// Beyond the room slot we also poke the *target* user of an
    /// `m.room.member` event. An invited user has not joined the room yet, so
    /// they are not subscribed to the room slot and a room-only notification
    /// would never reach them — their invite would sit unseen until the poll
    /// timed out.
    fn publish(&self, room_id: &str, event_type: &str, state_key: Option<&str>) {
        self.notifier.notify_room(room_id);

        if event_type == "m.room.member" {
            if let Some(target) = state_key {
                if !target.is_empty() {
                    self.notifier.notify_user(target);
                }
            }
        }
    }
}

impl std::fmt::Debug for NotifyingEventWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NotifyingEventWriter").finish_non_exhaustive()
    }
}

#[async_trait]
impl EventWriter for NotifyingEventWriter {
    fn pool(&self) -> &Arc<sqlx::PgPool> {
        self.inner.pool()
    }

    async fn create_event(
        &self,
        params: CreateEventParams,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<RoomEvent, sqlx::Error> {
        // Capture before `params` is moved into the inner writer.
        let room_id = params.room_id.clone();
        let event_type = params.event_type.clone();
        let state_key = params.state_key.clone();
        let autocommit = tx.is_none();

        let event = self.inner.create_event(params, tx).await?;

        if autocommit {
            self.publish(&room_id, &event_type, state_key.as_deref());
        }

        Ok(event)
    }

    async fn update_event_signatures_and_hashes(
        &self,
        event_id: &str,
        signatures: &serde_json::Value,
        hashes: &serde_json::Value,
    ) -> Result<(), sqlx::Error> {
        self.inner.update_event_signatures_and_hashes(event_id, signatures, hashes).await
    }

    async fn redact_event_content(&self, event_id: &str, redacted_by: Option<&str>) -> Result<(), sqlx::Error> {
        // No room_id in scope — see "Known gaps" in the module docs.
        self.inner.redact_event_content(event_id, redacted_by).await
    }

    async fn create_event_with_graph(
        &self,
        params: CreateEventParams,
        prev_events: &[String],
        auth_events: &[String],
        depth: i64,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<RoomEvent, sqlx::Error> {
        let room_id = params.room_id.clone();
        let event_type = params.event_type.clone();
        let state_key = params.state_key.clone();
        let autocommit = tx.is_none();

        let event = self.inner.create_event_with_graph(params, prev_events, auth_events, depth, tx).await?;

        if autocommit {
            self.publish(&room_id, &event_type, state_key.as_deref());
        }

        Ok(event)
    }

    async fn save_event_signature(
        &self,
        event_id: &str,
        user_id: &str,
        device_id: &str,
        signature: &str,
        key_id: &str,
        algorithm: &str,
        created_ts: i64,
    ) -> Result<(), sqlx::Error> {
        self.inner.save_event_signature(event_id, user_id, device_id, signature, key_id, algorithm, created_ts).await
    }

    async fn report_event(
        &self,
        event_id: &str,
        room_id: &str,
        reported_user_id: &str,
        reporter_user_id: &str,
        reason: Option<&str>,
        score: i32,
    ) -> Result<i64, sqlx::Error> {
        // Moderation reports are not timeline events; nothing to wake for.
        self.inner.report_event(event_id, room_id, reported_user_id, reporter_user_id, reason, score).await
    }

    async fn add_ephemeral_event(
        &self,
        room_id: &str,
        user_id: &str,
        event_type: &str,
        content: &serde_json::Value,
        stream_id: i64,
    ) -> Result<(), sqlx::Error> {
        self.inner.add_ephemeral_event(room_id, user_id, event_type, content, stream_id).await?;
        // Typing and receipts are only useful while they are fresh.
        self.notifier.notify_room(room_id);
        Ok(())
    }

    async fn upsert_ephemeral_event(
        &self,
        room_id: &str,
        user_id: &str,
        event_type: &str,
        content: &serde_json::Value,
        stream_id: i64,
        created_ts: i64,
        expires_at: Option<i64>,
    ) -> Result<(), sqlx::Error> {
        self.inner
            .upsert_ephemeral_event(room_id, user_id, event_type, content, stream_id, created_ts, expires_at)
            .await?;
        self.notifier.notify_room(room_id);
        Ok(())
    }

    async fn delete_ephemeral_event(&self, room_id: &str, event_type: &str, user_id: &str) -> Result<(), sqlx::Error> {
        self.inner.delete_ephemeral_event(room_id, event_type, user_id).await?;
        // Clears the typing indicator promptly instead of letting it linger.
        self.notifier.notify_room(room_id);
        Ok(())
    }

    async fn delete_events_before(&self, room_id: &str, timestamp: i64, dry_run: bool) -> Result<u64, sqlx::Error> {
        // Retention purge — see "Known gaps" in the module docs.
        self.inner.delete_events_before(room_id, timestamp, dry_run).await
    }

    async fn upsert_power_levels_event(
        &self,
        event_id: &str,
        room_id: &str,
        user_id: &str,
        content: serde_json::Value,
        origin_server_ts: i64,
        sender: &str,
    ) -> Result<(), sqlx::Error> {
        self.inner.upsert_power_levels_event(event_id, room_id, user_id, content, origin_server_ts, sender).await?;
        self.notifier.notify_room(room_id);
        Ok(())
    }

    async fn record_event_txn(
        &self,
        user_id: &str,
        room_id: &str,
        txn_id: &str,
        event_id: &str,
    ) -> Result<bool, sqlx::Error> {
        self.inner.record_event_txn(user_id, room_id, txn_id, event_id).await
    }

    async fn delete_event_by_id(&self, event_id: &str) -> Result<(), sqlx::Error> {
        self.inner.delete_event_by_id(event_id).await
    }
}

#[cfg(all(test, feature = "test-utils"))]
mod tests {
    use super::*;
    use std::time::Duration;
    use synapse_storage::test_mocks::InMemoryEventStore;

    const ROOM: &str = "!room:example.com";
    const SENDER: &str = "@alice:example.com";
    const TARGET: &str = "@bob:example.com";
    const STRANGER: &str = "@carol:example.com";

    fn build() -> (NotifyingEventWriter, EventNotifier) {
        let notifier = EventNotifier::new();
        let inner: Arc<dyn EventWriter> = Arc::new(InMemoryEventStore::new());
        (NotifyingEventWriter::new(inner, notifier.clone()), notifier)
    }

    fn params(event_type: &str, state_key: Option<&str>) -> CreateEventParams {
        CreateEventParams {
            event_id: format!("${}:example.com", uuid::Uuid::new_v4()),
            room_id: ROOM.to_string(),
            user_id: SENDER.to_string(),
            event_type: event_type.to_string(),
            content: serde_json::json!({ "body": "hi" }),
            state_key: state_key.map(str::to_string),
            origin_server_ts: 0,
            redacts: None,
        }
    }

    /// Registers a waiter the same way `sliding_sync_service` does: eagerly,
    /// *before* the write happens. Without `enable()` the notification fires
    /// into the void and the waiter hangs until its timeout.
    fn arm(slot: &Arc<tokio::sync::Notify>) -> std::pin::Pin<Box<tokio::sync::futures::Notified<'_>>> {
        let mut waiter = Box::pin(slot.notified());
        waiter.as_mut().enable();
        waiter
    }

    async fn woke(waiter: std::pin::Pin<Box<tokio::sync::futures::Notified<'_>>>) -> bool {
        tokio::time::timeout(Duration::from_millis(200), waiter).await.is_ok()
    }

    #[tokio::test]
    async fn persisting_an_event_wakes_room_waiters() {
        let (writer, notifier) = build();
        let slots = notifier.slots_for(STRANGER, &[ROOM.to_string()]);
        let room_waiter = arm(&slots[1]);

        writer.create_event(params("m.room.message", None), None).await.unwrap();

        assert!(woke(room_waiter).await, "a persisted message must release clients parked on that room");
    }

    #[tokio::test]
    async fn membership_event_also_wakes_the_target_user() {
        // An invitee has not joined yet, so they hold no room slot. Only the
        // user-slot notification can deliver the invite before the poll times
        // out.
        let (writer, notifier) = build();
        let slots = notifier.slots_for(TARGET, &[]);
        let user_waiter = arm(&slots[0]);

        writer.create_event(params("m.room.member", Some(TARGET)), None).await.unwrap();

        assert!(woke(user_waiter).await, "an invite must reach the invited user, who holds no room slot yet");
    }

    #[tokio::test]
    async fn membership_event_does_not_wake_unrelated_users() {
        let (writer, notifier) = build();
        let slots = notifier.slots_for(STRANGER, &[]);
        let stranger_waiter = arm(&slots[0]);

        writer.create_event(params("m.room.member", Some(TARGET)), None).await.unwrap();

        assert!(!woke(stranger_waiter).await, "membership changes must not fan out to uninvolved users");
    }

    #[tokio::test]
    async fn plain_message_does_not_wake_user_slots() {
        let (writer, notifier) = build();
        let slots = notifier.slots_for(TARGET, &[]);
        let user_waiter = arm(&slots[0]);

        writer.create_event(params("m.room.message", None), None).await.unwrap();

        assert!(!woke(user_waiter).await, "messages are addressed to the room slot, not to individual user slots");
    }

    #[tokio::test]
    async fn ephemeral_events_wake_room_waiters() {
        // Typing and receipts are worthless if they arrive 30 seconds late.
        let (writer, notifier) = build();
        let slots = notifier.slots_for(STRANGER, &[ROOM.to_string()]);
        let room_waiter = arm(&slots[1]);

        writer.add_ephemeral_event(ROOM, SENDER, "m.typing", &serde_json::json!({ "typing": true }), 1).await.unwrap();

        assert!(woke(room_waiter).await, "typing indicators must be delivered promptly");
    }

    #[tokio::test]
    async fn clearing_an_ephemeral_event_wakes_room_waiters() {
        let (writer, notifier) = build();
        let slots = notifier.slots_for(STRANGER, &[ROOM.to_string()]);
        let room_waiter = arm(&slots[1]);

        writer.delete_ephemeral_event(ROOM, "m.typing", SENDER).await.unwrap();

        assert!(woke(room_waiter).await, "a stale typing indicator should clear without waiting for the timeout");
    }

    // ── 覆盖剩余 impl EventWriter 方法 ─────────────────────────────────────

    /// `create_event_with_graph` is the federation backfill path; it must also
    /// wake room waiters on autocommit so that federated events are visible
    /// in sliding-sync immediately.
    #[tokio::test]
    async fn create_event_with_graph_wakes_room_waiters() {
        let (writer, notifier) = build();
        let slots = notifier.slots_for(STRANGER, &[ROOM.to_string()]);
        let room_waiter = arm(&slots[1]);

        let event = writer
            .create_event_with_graph(
                params("m.room.message", None),
                &["$prev:example.com".to_string()],
                &["$auth:example.com".to_string()],
                10,
                None,
            )
            .await
            .unwrap();

        assert!(woke(room_waiter).await, "federation backfill events must release room waiters");
        assert_eq!(event.room_id, ROOM);
    }

    /// `upsert_ephemeral_event` is used for ephemeral events that expire
    /// (e.g. typing with a TTL). It must still notify so clients do not miss
    /// the typing start while it is still valid.
    #[tokio::test]
    async fn upsert_ephemeral_event_wakes_room_waiters() {
        let (writer, notifier) = build();
        let slots = notifier.slots_for(STRANGER, &[ROOM.to_string()]);
        let room_waiter = arm(&slots[1]);

        // expires_at = now + 30s: typing indicator that auto-expires.
        let expires_at = chrono::Utc::now().timestamp_millis() + 30_000;
        writer
            .upsert_ephemeral_event(
                ROOM,
                SENDER,
                "m.typing",
                &serde_json::json!({ "typing": true }),
                2,
                expires_at,
                Some(expires_at),
            )
            .await
            .unwrap();

        assert!(woke(room_waiter).await, "ephemeral events with TTL must also release room waiters");
    }

    /// `upsert_power_levels_event` is a room-administration path: admin changes
    /// user power levels. Clients must see this immediately in sliding-sync.
    #[tokio::test]
    async fn upsert_power_levels_event_wakes_room_waiters() {
        let (writer, notifier) = build();
        let slots = notifier.slots_for(STRANGER, &[ROOM.to_string()]);
        let room_waiter = arm(&slots[1]);

        let params = params("m.room.power_levels", Some(""));
        writer
            .upsert_power_levels_event(
                &params.event_id,
                ROOM,
                SENDER,
                serde_json::json!({
                    "users": { "@alice:example.com": 50 }
                }),
                0,
                SENDER,
            )
            .await
            .unwrap();

        assert!(woke(room_waiter).await, "power level changes must release room waiters");
    }

    /// `update_event_signatures_and_hashes` is a pure delegation to inner
    /// storage. Coverage confirms the delegation does not panic.
    #[tokio::test]
    async fn update_event_signatures_and_hashes_is_passed_to_inner() {
        let (writer, _notifier) = build();

        // InMemoryEventStore::update_event_signatures_and_hashes is a no-op,
        // so this must succeed without panic.
        writer
            .update_event_signatures_and_hashes(
                "$event:example.com",
                &serde_json::json!({}),
                &serde_json::json!({}),
            )
            .await
            .unwrap();
    }

    /// `report_event` is a moderation path with no timeline event; it must not
    /// wake any waiters (confirmed by delegation to inner only).
    #[tokio::test]
    async fn report_event_is_passed_to_inner() {
        let (writer, _notifier) = build();

        writer
            .report_event("$event:example.com", ROOM, "@bad:example.com", SENDER, Some("spam"), -100)
            .await
            .unwrap();
    }

    /// `record_event_txn` deduplicates /records a sent-txn-id → event-id mapping.
    /// No notification expected; covered to confirm delegation.
    #[tokio::test]
    async fn record_event_txn_is_passed_to_inner() {
        let (writer, _notifier) = build();

        writer
            .record_event_txn(SENDER, ROOM, "txn-abc123", "$event:example.com")
            .await
            .unwrap();
    }

    /// `delete_event_by_id` is a hard-delete path; no notification expected.
    #[tokio::test]
    async fn delete_event_by_id_is_passed_to_inner() {
        let (writer, _notifier) = build();

        writer.delete_event_by_id("$event:example.com").await.unwrap();
    }

    /// `delete_events_before` is a retention purge path — deliberately silent
    /// per module docs. Covered to confirm it passes through without panic.
    #[tokio::test]
    async fn delete_events_before_is_passed_to_inner() {
        let (writer, _notifier) = build();

        writer.delete_events_before(ROOM, 1_700_000_000_000, false).await.unwrap();
    }

    /// `save_event_signature` stores a device key signature; no room timeline
    /// event so no notification. Covered to confirm delegation.
    #[tokio::test]
    async fn save_event_signature_is_passed_to_inner() {
        let (writer, _notifier) = build();

        writer
            .save_event_signature(
                "$event:example.com",
                SENDER,
                "DEVICE0",
                "sig",
                "ed25519:0",
                "ed25519",
                1_700_000_000_000,
            )
            .await
            .unwrap();
    }
}
