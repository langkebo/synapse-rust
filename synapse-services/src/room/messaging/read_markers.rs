//! Room read marker operations (MSC2654, MSC4446).

use crate::common::error::{ApiError, ApiResult};

use super::service::MessagingService;

impl MessagingService {
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
            .map_err(|e| ApiError::internal_with_context(&format!("Failed to set {marker_type} marker"), &e))
    }

    /// Set read markers (MSC2654) with MSC4446 backward-move support.
    ///
    /// The optional `allow_backward` body flag allows the `m.fully_read`
    /// marker to move backwards in time. Read receipts (`m.read`) always
    /// enforce monotonicity regardless of the flag.
    pub async fn set_read_markers(&self, room_id: &str, user_id: &str, body: &serde_json::Value) -> ApiResult<()> {
        let allow_backward = body.get("allow_backward").and_then(|v| v.as_bool()).unwrap_or(false);

        if let Some(event_id) = body.get("m.fully_read").and_then(|v| v.as_str()) {
            if event_id.starts_with('$') {
                // MSC4446: m.fully_read respects allow_backward flag
                self.room_storage
                    .update_read_marker_monotonic(room_id, user_id, event_id, "m.fully_read", allow_backward)
                    .await
                    .map_err(|e| ApiError::internal_with_context("Failed to set m.fully_read marker", &e))?;
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
                // MSC4446: m.read always enforces monotonicity (allow_backward=false)
                self.room_storage
                    .update_read_marker_monotonic(room_id, user_id, event_id, "m.fully_read", false)
                    .await
                    .map_err(|e| ApiError::internal_with_context("Failed to set m.read marker", &e))?;
            }
        }

        Ok(())
    }
}
