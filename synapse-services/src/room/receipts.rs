//! Room receipt operations: send and query read receipts.

use crate::common::error::{ApiError, ApiResult};
use serde_json::json;
use synapse_storage::Receipt;

use super::service::RoomService;

impl RoomService {
    pub async fn send_receipt(
        &self,
        room_id: &str,
        user_id: &str,
        event_id: &str,
        receipt_type: &str,
        body: &serde_json::Value,
    ) -> ApiResult<()> {
        self.room_storage
            .add_receipt(user_id, user_id, room_id, event_id, receipt_type, body)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to store receipt", &e))?;

        let now_ts = chrono::Utc::now().timestamp_millis();
        let mut receipt_entry = body.as_object().cloned().unwrap_or_default();
        receipt_entry.insert("ts".to_string(), json!(now_ts));
        let receipt_content = json!({
            event_id: {
                receipt_type: {
                    user_id: receipt_entry
                }
            }
        });

        self.event_storage
            .add_ephemeral_event(room_id, user_id, "m.receipt", &receipt_content, now_ts)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to store ephemeral receipt", &e))?;

        if let Some(event_broadcaster) = self.event_broadcaster.read().await.clone() {
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
            .map_err(|e| ApiError::internal_with_log("Failed to get receipts", &e))
    }
}
