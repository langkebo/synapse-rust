//! Admin event-redaction queries and batch redaction.
//!
//! `find_event_ids_for_redaction` / `batch_redact_events` are inherent methods
//! on the persistence-layer `EventStorage`; the admin route needs them, so this
//! narrow service is the seam that keeps `synapse-web` free of storage types
//! (B4-5c).

use std::sync::Arc;
use synapse_common::error::ApiError;
use synapse_storage::event::EventStorage;

/// Narrow service over `EventStorage`'s redaction surface.
pub struct EventRedactionService {
    storage: Arc<EventStorage>,
}

impl EventRedactionService {
    /// See [`new`].
    pub fn new(storage: Arc<EventStorage>) -> Self {
        Self { storage }
    }

    /// Event ids in `room_id` whose `origin_server_ts` falls inside the
    /// requested window, oldest first.
    pub async fn find_event_ids_for_redaction(
        &self,
        room_id: &str,
        before_ts: Option<i64>,
        after_ts: Option<i64>,
        limit: i64,
    ) -> Result<Vec<String>, ApiError> {
        self.storage
            .find_event_ids_for_redaction(room_id, before_ts, after_ts, limit)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to query events for redaction", e))
    }

    /// Redact the given events, returning how many rows were redacted.
    pub async fn batch_redact_events(&self, event_ids: &[String], redacted_by: Option<&str>) -> Result<u64, ApiError> {
        self.storage
            .batch_redact_events(event_ids, redacted_by)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to batch redact events", e))
    }
}
