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
