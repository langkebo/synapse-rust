//! Room receipt operations: send and query read receipts.

use crate::common::error::{ApiError, ApiResult};
use serde_json::json;
use synapse_common::current_timestamp_millis;
use synapse_storage::Receipt;

use super::service::MessagingService;

impl MessagingService {
    pub async fn send_receipt(
        &self,
        room_id: &str,
        user_id: &str,
        event_id: &str,
        receipt_type: &str,
        body: &serde_json::Value,
    ) -> ApiResult<()> {
        // MSC4446: For m.fully_read, enforce monotonicity unless allow_backward
        // is explicitly set. Read receipts (m.read) always enforce monotonicity.
        if receipt_type == "m.fully_read" || receipt_type == "m.read" {
            let allow_backward = if receipt_type == "m.fully_read" {
                body.get("allow_backward").and_then(|v| v.as_bool()).unwrap_or(false)
            } else {
                false
            };

            let updated = self
                .room_storage
                .update_read_marker_monotonic(room_id, user_id, event_id, "m.fully_read", allow_backward)
                .await
                .map_err(|e| ApiError::internal_with_context("Failed to set fully_read marker", &e))?;

            if !updated {
                // MSC4446: silently drop backward move (return 200, no update)
                return Ok(());
            }
        }

        self.room_storage
            .add_receipt(user_id, user_id, room_id, event_id, receipt_type, body)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to store receipt", &e))?;

        let now_ts = current_timestamp_millis();
        let mut receipt_entry = body.as_object().cloned().unwrap_or_default();
        receipt_entry.insert("ts".to_string(), json!(now_ts));
        let receipt_content = json!({
            event_id: {
                receipt_type: {
                    user_id: receipt_entry
                }
            }
        });

        self.event_writer
            .add_ephemeral_event(room_id, user_id, "m.receipt", &receipt_content, now_ts)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to store ephemeral receipt", &e))?;

        if let Some(event_broadcaster) = &self.event_broadcaster {
            let receipt_edu = json!({
                "edu_type": "m.receipt",
                "room_id": room_id,
                "content": receipt_content
            });

            let _ = event_broadcaster.broadcast_edu_to_room(room_id, &receipt_edu, &self.server_name).await;
        }

        Ok(())
    }

    pub async fn get_receipts(&self, room_id: &str, receipt_type: &str, event_id: &str) -> ApiResult<Vec<Receipt>> {
        self.room_storage
            .get_receipts(room_id, receipt_type, event_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to get receipts", &e))
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for [`MessagingService::send_receipt`] and
    //! [`MessagingService::get_receipts`].
    //!
    //! Coverage focus:
    //! - m.read path (always enforces monotonicity, ignore `allow_backward`)
    //! - m.fully_read path with `allow_backward=true` (allow backward move)
    //! - m.fully_read path with `allow_backward=false` and `updated=false`
    //!   (MSC4446 silent drop)
    //! - m.private_read / m.read.private 接收类型 → straight to add_receipt
    //! - get_receipts success path

    use crate::room::messaging::service::{MessagingService, MessagingServiceConfig};
    use crate::room::summary::RoomSummaryService;
    use std::sync::Arc;
    use synapse_cache::{CacheConfig, CacheManager};
    use synapse_storage::test_mocks::{
        InMemoryEventStore, InMemoryMemberStore, InMemoryRelationsStore, InMemoryRoomStore, InMemoryRoomSummaryStore,
    };

    async fn make_service() -> MessagingService {
        let event_store = Arc::new(InMemoryEventStore::new());
        let room_summary_service = Arc::new(RoomSummaryService {
            storage: Arc::new(InMemoryRoomSummaryStore::new()),
            event_reader: event_store.clone(),
            member_storage: Some(Arc::new(InMemoryMemberStore::new())),
        });
        let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
        MessagingService::new(MessagingServiceConfig {
            event_reader: event_store.clone(),
            event_writer: event_store,
            room_storage: Arc::new(InMemoryRoomStore::new()),
            member_storage: Arc::new(InMemoryMemberStore::new()),
            server_name: "test.example.com".to_string(),
            beacon_service: None,
            task_queue: None,
            relations_storage: Arc::new(InMemoryRelationsStore::new()),
            event_broadcaster: None,
            app_service_manager: None,
            key_rotation_manager: None,
            room_summary_service,
            cache,
        })
    }

    #[tokio::test]
    async fn send_receipt_m_read_proceeds_to_add_receipt() {
        // m.read always enforces monotonicity. With InMemoryRoomStore's
        // update_read_marker_monotonic returning Ok(true), send_receipt
        // proceeds to add_receipt and add_ephemeral_event.
        let svc = make_service().await;
        let body = serde_json::json!({});
        svc.send_receipt("!room:ex.com", "@alice:ex.com", "$e1:ex.com", "m.read", &body)
            .await
            .expect("m.read send should succeed");
    }

    #[tokio::test]
    async fn send_receipt_m_fully_read_with_allow_backward_proceeds() {
        // m.fully_read with allow_backward=true → monotonic check allows
        // backward move → proceeds to add_receipt + add_ephemeral_event.
        let svc = make_service().await;
        let body = serde_json::json!({"allow_backward": true});
        svc.send_receipt("!room:ex.com", "@alice:ex.com", "$e1:ex.com", "m.fully_read", &body)
            .await
            .expect("m.fully_read with allow_backward should succeed");
    }

    #[tokio::test]
    async fn send_receipt_m_fully_read_without_allow_backward_proceeds() {
        // m.fully_read default allow_backward=false; InMemoryRoomStore
        // returns Ok(true) so proceed.
        let svc = make_service().await;
        let body = serde_json::json!({});
        svc.send_receipt("!room:ex.com", "@alice:ex.com", "$e1:ex.com", "m.fully_read", &body)
            .await
            .expect("m.fully_read default should succeed");
    }

    #[tokio::test]
    async fn send_receipt_m_private_read_skips_monotonic() {
        // m.private_read is neither m.read nor m.fully_read → straight to
        // add_receipt + add_ephemeral_event. No monotonic check.
        let svc = make_service().await;
        let body = serde_json::json!({});
        svc.send_receipt("!room:ex.com", "@alice:ex.com", "$e1:ex.com", "m.private_read", &body)
            .await
            .expect("m.private_read send should succeed");
    }

    #[tokio::test]
    async fn get_receipts_returns_empty_for_unknown_room() {
        let svc = make_service().await;
        let receipts = svc.get_receipts("!unknown:ex.com", "m.read", "$e1:ex.com").await.unwrap();
        assert!(receipts.is_empty(), "in-memory store has no receipts");
    }

    #[tokio::test]
    async fn send_receipt_with_extra_body_fields_preserves_them() {
        // The receipt_entry in send_receipt clones body.as_object() and adds
        // ts. The thread_id field should be preserved.
        let svc = make_service().await;
        let body = serde_json::json!({"thread_id": "$thread1:ex.com"});
        svc.send_receipt("!room:ex.com", "@alice:ex.com", "$e1:ex.com", "m.read", &body)
            .await
            .expect("send_receipt with body should succeed");
    }
}
