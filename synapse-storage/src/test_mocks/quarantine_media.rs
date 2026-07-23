use super::*;
use crate::media::models::QuarantinedMediaChange;
use crate::media::QuarantinedMediaChangeStoreApi;

#[derive(Clone, Default)]
pub struct InMemoryQuarantineMediaChangeStore {
    changes: Arc<RwLock<Vec<QuarantinedMediaChange>>>,
    next_stream_id: Arc<RwLock<i64>>,
}

impl InMemoryQuarantineMediaChangeStore {
    pub fn new() -> Self {
        Self { changes: Arc::new(RwLock::new(Vec::new())), next_stream_id: Arc::new(RwLock::new(0)) }
    }

    /// Test helper: seed a change record directly with a fixed stream_id.
    pub async fn seed_change(&self, change: QuarantinedMediaChange) {
        let mut changes = self.changes.write().await;
        let mut next = self.next_stream_id.write().await;
        if *next < change.stream_id {
            *next = change.stream_id;
        }
        changes.push(change);
        changes.sort_by_key(|c| c.stream_id);
    }
}

#[async_trait::async_trait]
impl QuarantinedMediaChangeStoreApi for InMemoryQuarantineMediaChangeStore {
    async fn record_media_quarantine_change(
        &self,
        media_id: &str,
        server_name: &str,
        change_type: &str,
        changed_by: &str,
        now_ts: i64,
    ) -> Result<i64, ApiError> {
        let mut next = self.next_stream_id.write().await;
        *next += 1;
        let stream_id = *next;
        drop(next);

        let change = QuarantinedMediaChange {
            stream_id,
            media_id: media_id.to_string(),
            server_name: server_name.to_string(),
            change_type: change_type.to_string(),
            changed_by: changed_by.to_string(),
            created_ts: now_ts,
        };
        self.changes.write().await.push(change);
        Ok(stream_id)
    }

    async fn get_quarantined_media_changes(
        &self,
        since_stream_id: i64,
        limit: i64,
    ) -> Result<Vec<QuarantinedMediaChange>, ApiError> {
        let changes = self.changes.read().await;
        let mut results: Vec<QuarantinedMediaChange> =
            changes.iter().filter(|c| c.stream_id > since_stream_id).cloned().collect();
        results.sort_by_key(|c| c.stream_id);
        results.truncate(limit as usize);
        Ok(results)
    }

    async fn set_media_quarantine_status(
        &self,
        _media_id: &str,
        _server_name: &str,
        _quarantine_status: &str,
    ) -> Result<bool, ApiError> {
        // No media_metadata table in this in-memory double; report as no-op.
        Ok(false)
    }

    async fn get_current_stream_id(&self) -> Result<i64, ApiError> {
        Ok(*self.next_stream_id.read().await)
    }

    async fn get_changes_by_media(
        &self,
        media_id: &str,
        since_stream_id: i64,
        limit: i64,
    ) -> Result<Vec<QuarantinedMediaChange>, ApiError> {
        let changes = self.changes.read().await;
        let mut results: Vec<QuarantinedMediaChange> =
            changes.iter().filter(|c| c.media_id == media_id && c.stream_id > since_stream_id).cloned().collect();
        results.sort_by_key(|c| c.stream_id);
        results.truncate(limit as usize);
        Ok(results)
    }
}
