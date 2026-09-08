use super::storage::{ToDeviceMessage, ToDeviceStorage, ToDeviceStorageApi};
use serde_json::Value;
use std::sync::Arc;
use synapse_common::map_database;
use synapse_common::ApiError;
use synapse_storage::UserStore;

const TRANSACTION_MAX_AGE_MS: i64 = 24 * 60 * 60 * 1000;

#[derive(Clone)]
/// The `ToDeviceService` type.
pub struct ToDeviceService {
    storage: Arc<dyn ToDeviceStorageApi>,
    user_storage: Option<Arc<dyn UserStore>>,
}

/// (see code)
impl ToDeviceService {
    /// See [`new`].
    pub fn new(storage: Arc<dyn ToDeviceStorageApi>) -> Self {
        Self { storage, user_storage: None }
    }

    /// See [`new_from_pool`].
    pub fn new_from_pool(pool: &Arc<sqlx::PgPool>) -> Self {
        Self::new(Arc::new(ToDeviceStorage::new(pool)))
    }

    /// See [`with_user_storage`].
    pub fn with_user_storage(mut self, user_storage: Arc<dyn UserStore>) -> Self {
        self.user_storage = Some(user_storage);
        self
    }

    /// See [`send_messages`].
    pub async fn send_messages(
        &self,
        sender_user_id: &str,
        sender_device_id: &str,
        event_type: &str,
        message_id: Option<&str>,
        messages: &Value,
    ) -> Result<(), ApiError> {
        if let Some(mid) = message_id {
            let is_first = self.storage.record_transaction(sender_user_id, sender_device_id, mid).await?;
            if !is_first {
                tracing::debug!("Duplicate to-device transaction {} from {}:{}", mid, sender_user_id, sender_device_id);
                return Ok(());
            }
            if let Err(e) = self.storage.cleanup_old_transactions(TRANSACTION_MAX_AGE_MS).await {
                tracing::warn!("Failed to clean up old to-device transactions: {e}");
            }
        }

        if let Some(msg_map) = messages.as_object() {
            // Stage 1: batch user-existence check. filter_existing_users runs
            // a single `WHERE user_id = ANY($1)` query and returns the subset
            // of user_ids that exist. This replaces the previous N-call
            // user_exists loop with one round-trip.

            // Pre-collect all user_ids before the device loop so we can batch
            // the existence check.
            let all_user_ids: Vec<String> = msg_map.keys().cloned().collect();

            // Stage 1a: filter to existing users in one query (if user_storage
            // is configured; otherwise assume all users exist).
            let existing_users: Option<std::collections::HashSet<String>> = if let Some(user_storage) =
                &self.user_storage
            {
                let existing =
                    user_storage.filter_existing_users(&all_user_ids).await.map_err(map_database!("send_messages"))?;
                Some(existing.into_iter().collect())
            } else {
                None
            };

            // Stage 1b: build ToDeviceMessage list, skipping entries whose
            // recipient user is known to not exist.
            let mut batch: Vec<ToDeviceMessage<'_>> = Vec::new();
            for (user_id, devices) in msg_map {
                if let Some(ref existing) = existing_users {
                    if !existing.contains(user_id) {
                        tracing::warn!("Skipping to-device message for non-existent user: {}", user_id);
                        continue;
                    }
                }

                if let Some(device_map) = devices.as_object() {
                    for (device_id, content) in device_map {
                        batch.push(ToDeviceMessage {
                            sender_user_id,
                            sender_device_id,
                            recipient_user_id: user_id,
                            recipient_device_id: device_id,
                            event_type,
                            message_id,
                            content: content.clone(),
                        });
                    }
                }
            }

            // Stage 2: one batched INSERT for all valid (user, device) pairs.
            // add_messages_batch still filters out non-existent devices via
            // device_exists() — but those checks are now done once per unique
            // recipient inside the batch implementation rather than N times
            // outside.
            let inserted = self.storage.add_messages_batch(&batch).await?;
            tracing::debug!(
                target_user_count = msg_map.len(),
                inserted_count = inserted,
                "Batched to-device message dispatch complete"
            );
        }
        Ok(())
    }

    /// See [`get_messages_for_sync`].
    pub async fn get_messages_for_sync(&self, user_id: &str, device_id: &str) -> Result<Vec<Value>, ApiError> {
        self.storage.get_and_delete_messages(user_id, device_id).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::ToDeviceService;
    use crate::test_mocks::InMemoryToDeviceStorage;
    use serde_json::json;

    #[test]
    fn test_to_device_message_structure() {
        let messages = json!({
            "@alice:example.com": {
                "DEVICE1": {
                    "type": "m.room_key",
                    "content": {
                        "algorithm": "m.megolm.v1.aes-sha2"
                    }
                }
            }
        });
        assert!(messages["@alice:example.com"]["DEVICE1"]["type"].is_string());
    }

    #[test]
    fn test_to_device_multiple_devices() {
        let messages = json!({
            "@bob:example.com": {
                "DEVICE1": {"type": "m.test"},
                "DEVICE2": {"type": "m.test"}
            }
        });
        let devices = messages["@bob:example.com"].as_object().unwrap();
        assert_eq!(devices.len(), 2);
    }

    #[test]
    fn test_to_device_empty_messages() {
        let messages = json!({});
        assert!(messages.as_object().unwrap().is_empty());
    }

    /// E2EE-10: serde_json::Map (backed by BTreeMap without `preserve_order`)
    /// iterates keys in deterministic alphabetical order. This means that
    /// within a single `send_messages` call, the iteration order over the
    /// `messages` JSON map is deterministic — but not the sender's intended
    /// order.
    ///
    /// This is acceptable because:
    /// 1. Each (recipient_user_id, recipient_device_id) pair appears at most
    ///    once per `send_messages` call (JSON map structure prevents duplicates).
    /// 2. Cross-call ordering is guaranteed by `stream_id` (assigned via
    ///    `nextval('to_device_stream_id_seq')`), which is monotonically
    ///    increasing and globally unique.
    /// 3. All retrieval paths (`get_messages`, `get_messages_since`,
    ///    `get_and_delete_messages`) ORDER BY stream_id ASC.
    #[test]
    fn test_to_device_message_map_iteration_is_deterministic() {
        let messages = json!({
            "@zoe:example.com": {"DEVICE_Z": {"type": "m.room_key", "content": {"seq": 3}}},
            "@alice:example.com": {"DEVICE_A": {"type": "m.room_key", "content": {"seq": 1}}},
            "@bob:example.com": {"DEVICE_B": {"type": "m.room_key", "content": {"seq": 2}}}
        });

        let map = messages.as_object().unwrap();
        let keys: Vec<&str> = map.keys().map(String::as_str).collect();

        // BTreeMap iterates in sorted (alphabetical) key order — deterministic
        // regardless of insertion order in the json! macro.
        assert_eq!(keys, vec!["@alice:example.com", "@bob:example.com", "@zoe:example.com"]);

        // Each recipient appears exactly once — no intra-call duplication.
        for (_, devices) in map {
            let device_map = devices.as_object().unwrap();
            // Each recipient has exactly one device in this test.
            assert_eq!(device_map.len(), 1);
        }
    }

    /// E2EE-10: Verify that the ToDeviceMessage struct carries all fields
    /// needed for correct delivery, including sender identity for stream_id
    /// assignment via `add_message`.
    #[test]
    fn test_to_device_message_struct_fields() {
        use super::super::storage::ToDeviceMessage;
        use serde_json::json;

        let msg = ToDeviceMessage {
            sender_user_id: "@alice:example.com",
            sender_device_id: "DEVICE_A",
            recipient_user_id: "@bob:example.com",
            recipient_device_id: "DEVICE_B",
            event_type: "m.room_key",
            message_id: Some("txn_001"),
            content: json!({"algorithm": "m.megolm.v1.aes-sha2"}),
        };

        assert_eq!(msg.sender_user_id, "@alice:example.com");
        assert_eq!(msg.recipient_user_id, "@bob:example.com");
        assert_eq!(msg.event_type, "m.room_key");
        assert_eq!(msg.message_id, Some("txn_001"));
        assert!(msg.content.is_object());
    }

    // ── TRANSACTION_MAX_AGE_MS constant ──────────────────────────

    #[test]
    fn test_transaction_max_age_is_24_hours() {
        // 24h = 86_400_000ms
        assert_eq!(super::TRANSACTION_MAX_AGE_MS, 24 * 60 * 60 * 1000);
    }

    #[test]
    fn test_to_device_message_with_no_message_id() {
        // When message_id is None, dedup is skipped — the message flows
        // through directly without recording a transaction.
        use super::super::storage::ToDeviceMessage;
        use serde_json::json;

        let msg = ToDeviceMessage {
            sender_user_id: "@alice:example.com",
            sender_device_id: "DEVICE_A",
            recipient_user_id: "@bob:example.com",
            recipient_device_id: "DEVICE_B",
            event_type: "m.room_key",
            message_id: None,
            content: json!({"algorithm": "m.megolm.v1.aes-sha2"}),
        };

        assert!(msg.message_id.is_none());
    }

    #[test]
    fn test_to_device_message_with_empty_content() {
        use super::super::storage::ToDeviceMessage;
        use serde_json::json;

        let msg = ToDeviceMessage {
            sender_user_id: "@alice:example.com",
            sender_device_id: "DEVICE_A",
            recipient_user_id: "@bob:example.com",
            recipient_device_id: "DEVICE_B",
            event_type: "m.forwarded_room_key",
            message_id: None,
            content: json!({}),
        };

        assert!(msg.content.is_object());
        assert!(msg.content.as_object().unwrap().is_empty());
    }

    // ── send_messages service logic tests (InMemory mock) ────────────────────────

    #[tokio::test]
    async fn test_send_messages_skip_duplicate_transaction() {
        use crate::test_mocks::InMemoryToDeviceStorage;

        let storage = Arc::new(InMemoryToDeviceStorage::new());
        let svc = ToDeviceService::new(storage.clone());

        let messages = json!({
            "@bob:example.com": {"DEVICE_B": {"type": "m.room_key"}}
        });

        // First call: should succeed (first time).
        svc.send_messages("@alice:example.com", "DEV_A", "m.room_key", Some("txn_001"), &messages)
            .await.unwrap();
        assert_eq!(storage.transaction_count().await, 1);

        // Second call with same txn_id: should be a dedup hit — no error, no message added.
        // InMemoryToDeviceStorage doesn't track devices, so add_messages_batch would
        // insert even non-existent recipients. The dedup itself is what we test here.
        svc.send_messages("@alice:example.com", "DEV_A", "m.room_key", Some("txn_001"), &messages)
            .await.unwrap();
        assert_eq!(storage.transaction_count().await, 1, "duplicate txn should not be recorded");
    }

    #[tokio::test]
    async fn test_send_messages_no_message_id_skips_dedup() {
        use crate::test_mocks::InMemoryToDeviceStorage;

        let storage = Arc::new(InMemoryToDeviceStorage::new());
        let svc = ToDeviceService::new(storage.clone());

        let messages = json!({
            "@bob:example.com": {"DEVICE_B": {"type": "m.room_key"}}
        });

        // No message_id → no transaction recorded, no dedup.
        svc.send_messages("@alice:example.com", "DEV_A", "m.room_key", None, &messages)
            .await.unwrap();
        assert_eq!(storage.transaction_count().await, 0, "no txn recorded when message_id is None");
    }
}
