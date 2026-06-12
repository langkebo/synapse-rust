//! Room read marker operations (MSC2654).

use crate::common::error::{ApiError, ApiResult};

use super::service::RoomService;

impl RoomService {
    pub async fn update_read_marker(
        &self,
        room_id: &str,
        user_id: &str,
        event_id: &str,
        marker_type: &str,
    ) -> ApiResult<()> {
        self.room_storage
            .update_read_marker_with_type(room_id, user_id, event_id, marker_type)
            .await
            .map_err(|e| ApiError::internal_with_log(&format!("Failed to set {marker_type} marker"), &e))
    }

    pub async fn set_read_markers(
        &self,
        room_id: &str,
        user_id: &str,
        body: &serde_json::Value,
    ) -> ApiResult<()> {
        if let Some(event_id) = body.get("m.fully_read").and_then(|v| v.as_str()) {
            if event_id.starts_with('$') {
                self.update_read_marker(room_id, user_id, event_id, "m.fully_read").await?;
            }
        }

        if let Some(event_id) = body.get("m.private_read").and_then(|v| v.as_str()) {
            if event_id.starts_with('$') {
                self.update_read_marker(room_id, user_id, event_id, "m.private_read").await?;
            }
        }

        if let Some(marked_unread) = body.get("m.marked_unread").and_then(|v| v.as_object()) {
            if let Some(events) = marked_unread.get("events").and_then(|v| v.as_array()) {
                for event in events {
                    if let Some(event_id) = event.as_str() {
                        if event_id.starts_with('$') {
                            self.update_read_marker(room_id, user_id, event_id, "m.marked_unread").await?;
                        }
                    }
                }
            }
        }

        if let Some(event_id) = body.get("m.read").and_then(|v| v.as_str()) {
            if event_id.starts_with('$') {
                self.update_read_marker(room_id, user_id, event_id, "m.fully_read").await?;
            }
        }

        Ok(())
    }
}
