//! Room read marker operations (MSC2654, MSC4446).

use crate::common::error::{ApiError, ApiResult};

use super::service::MessagingService;

impl MessagingService {
    /// See [`update_read_marker`].
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
            .map_err(|e| ApiError::internal_with_cause(&format!("Failed to set {marker_type} marker"), e))
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
                    .map_err(|e| ApiError::internal_with_cause("Failed to set m.fully_read marker", e))?;
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
                    .map_err(|e| ApiError::internal_with_cause("Failed to set m.read marker", e))?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for [`MessagingService::update_read_marker`] and
    //! [`MessagingService::set_read_markers`].
    //!
    //! Coverage focus:
    //! - update_read_marker happy path
    //! - set_read_markers: m.fully_read with allow_backward=true/false
    //! - set_read_markers: m.private_read (non-monotonic marker type)
    //! - set_read_markers: m.marked_unread.events[] (array, multiple values)
    //! - set_read_markers: m.read (always allow_backward=false)
    //! - set_read_markers: empty body → Ok(()) (all branches skipped)
    //! - set_read_markers: invalid event_id (no '$' prefix) → skipped silently

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
    async fn update_read_marker_succeeds() {
        let svc = make_service().await;
        svc.update_read_marker("!room:ex.com", "@alice:ex.com", "$e1:ex.com", "m.fully_read")
            .await
            .expect("update_read_marker should succeed");
    }

    #[tokio::test]
    async fn set_read_markers_empty_body_returns_ok() {
        // All branches are conditional on body fields. Empty body → no-op.
        let svc = make_service().await;
        let body = serde_json::json!({});
        svc.set_read_markers("!room:ex.com", "@alice:ex.com", &body).await.expect("empty body should be Ok");
    }

    #[tokio::test]
    async fn set_read_markers_fully_read_allow_backward_true() {
        let svc = make_service().await;
        let body = serde_json::json!({"m.fully_read": "$e1:ex.com", "allow_backward": true});
        svc.set_read_markers("!room:ex.com", "@alice:ex.com", &body)
            .await
            .expect("m.fully_read with allow_backward should succeed");
    }

    #[tokio::test]
    async fn set_read_markers_fully_read_allow_backward_false() {
        let svc = make_service().await;
        let body = serde_json::json!({"m.fully_read": "$e1:ex.com"});
        svc.set_read_markers("!room:ex.com", "@alice:ex.com", &body)
            .await
            .expect("m.fully_read default allow_backward should succeed");
    }

    #[tokio::test]
    async fn set_read_markers_private_read() {
        // m.private_read → calls update_read_marker (not monotonic)
        let svc = make_service().await;
        let body = serde_json::json!({"m.private_read": "$e1:ex.com"});
        svc.set_read_markers("!room:ex.com", "@alice:ex.com", &body).await.expect("m.private_read should succeed");
    }

    #[tokio::test]
    async fn set_read_markers_marked_unread_multiple_events() {
        // m.marked_unread.events[] → calls update_read_marker per event
        let svc = make_service().await;
        let body = serde_json::json!({
            "m.marked_unread": {
                "events": ["$e1:ex.com", "$e2:ex.com"]
            }
        });
        svc.set_read_markers("!room:ex.com", "@alice:ex.com", &body)
            .await
            .expect("m.marked_unread with multiple events should succeed");
    }

    #[tokio::test]
    async fn set_read_markers_m_read_always_false() {
        // m.read → update_read_marker_monotonic with allow_backward=false
        let svc = make_service().await;
        let body = serde_json::json!({"m.read": "$e1:ex.com"});
        svc.set_read_markers("!room:ex.com", "@alice:ex.com", &body).await.expect("m.read should succeed");
    }

    #[tokio::test]
    async fn set_read_markers_invalid_event_id_skipped() {
        // Event IDs without '$' prefix are silently skipped
        let svc = make_service().await;
        let body = serde_json::json!({
            "m.fully_read": "not_an_event_id",
            "m.private_read": "also_invalid",
            "m.read": "nope"
        });
        // All get skipped, result is still Ok
        svc.set_read_markers("!room:ex.com", "@alice:ex.com", &body)
            .await
            .expect("invalid event IDs should be silently skipped");
    }

    #[tokio::test]
    async fn set_read_markers_combined_markers() {
        // Multiple marker types in one body — all processed independently
        let svc = make_service().await;
        let body = serde_json::json!({
            "m.fully_read": "$e1:ex.com",
            "m.private_read": "$e2:ex.com",
            "m.read": "$e3:ex.com",
            "m.marked_unread": {"events": ["$e4:ex.com"]}
        });
        svc.set_read_markers("!room:ex.com", "@alice:ex.com", &body)
            .await
            .expect("combined markers should all succeed");
    }
}
