use crate::account::UserService;
use std::sync::Arc;
use synapse_common::time::current_timestamp_millis;
use synapse_common::ApiError;
pub use synapse_storage::QuarantinedMediaChange;
pub use synapse_storage::{
    decode_media_cursor, encode_media_cursor, AdminMediaInfo, AdminMediaPage, AdminMediaQuotaSummary, MediaCursor,
};
use synapse_storage::{AdminMediaStoreApi, QuarantinedMediaChangeStoreApi};
use tracing::instrument;

/// The `AdminMediaService` struct.
pub struct AdminMediaService {
    storage: Arc<dyn AdminMediaStoreApi>,
    quarantine_change_storage: Arc<dyn QuarantinedMediaChangeStoreApi>,
    user_service: Arc<UserService>,
    /// Local server name, used to attribute quarantine-change audit rows.
    server_name: String,
}

impl AdminMediaService {
    /// See [`new`].
    pub fn new(
        storage: Arc<dyn AdminMediaStoreApi>,
        quarantine_change_storage: Arc<dyn QuarantinedMediaChangeStoreApi>,
        user_service: Arc<UserService>,
        server_name: String,
    ) -> Self {
        Self { storage, quarantine_change_storage, user_service, server_name }
    }

    /// See [`get_all_media`].
    #[instrument(skip(self))]
    pub async fn get_all_media(&self, limit: i64, cursor: Option<MediaCursor>) -> Result<AdminMediaPage, ApiError> {
        self.storage.get_all_media(limit, cursor).await
    }

    /// See [`get_media_info`].
    #[instrument(skip(self))]
    pub async fn get_media_info(&self, media_id: &str) -> Result<Option<AdminMediaInfo>, ApiError> {
        self.storage.get_media_info(media_id).await
    }

    /// See [`delete_media`].
    #[instrument(skip(self))]
    pub async fn delete_media(&self, media_id: &str) -> Result<(), ApiError> {
        if !self.storage.delete_media(media_id).await? {
            return Err(ApiError::not_found("Media not found".to_string()));
        }

        Ok(())
    }

    /// See [`get_media_quota`].
    #[instrument(skip(self))]
    pub async fn get_media_quota(&self) -> Result<AdminMediaQuotaSummary, ApiError> {
        self.storage.get_media_quota().await
    }

    /// See [`get_user_media`].
    #[instrument(skip(self))]
    pub async fn get_user_media(&self, identifier: &str) -> Result<(String, Vec<AdminMediaInfo>), ApiError> {
        let user = self.user_service.get_user_or_not_found(identifier).await?;
        let media = self.storage.get_user_media(&user.user_id).await?;
        Ok((user.user_id, media))
    }

    /// See [`delete_user_media`].
    #[instrument(skip(self))]
    pub async fn delete_user_media(&self, identifier: &str) -> Result<u64, ApiError> {
        let user = self.user_service.get_user_or_not_found(identifier).await?;
        self.storage.delete_user_media(&user.user_id).await
    }

    /// List media in a room.
    ///
    /// Backs `GET /_synapse/admin/v1/rooms/{room_id}/media`.
    /// Returns paginated media list with cursor support.
    #[instrument(skip(self))]
    pub async fn get_room_media(
        &self,
        room_id: &str,
        limit: i64,
        cursor: Option<MediaCursor>,
    ) -> Result<AdminMediaPage, ApiError> {
        self.storage.get_room_media(room_id, limit, cursor).await
    }

    /// Delete media from a room.
    ///
    /// Backs `DELETE /_synapse/admin/v1/rooms/{room_id}/media/{media_id}`.
    /// Removes the media from the room index and deletes it entirely if no other rooms reference it.
    #[instrument(skip(self))]
    pub async fn delete_room_media(&self, room_id: &str, media_id: &str) -> Result<(), ApiError> {
        if !self.storage.delete_room_media(room_id, media_id).await? {
            return Err(ApiError::not_found("Media not found in room".to_string()));
        }
        Ok(())
    }

    /// Query quarantine change history for a specific media item.
    ///
    /// Backs the `GET /_synapse/admin/v1/quarantine_media/{media_id}/changes`
    /// admin endpoint. Returns changes with `stream_id > since_stream_id`,
    /// ordered ascending, capped by `limit`.
    #[instrument(skip(self))]
    pub async fn get_media_quarantine_changes(
        &self,
        media_id: &str,
        since_stream_id: i64,
        limit: i64,
    ) -> Result<Vec<QuarantinedMediaChange>, ApiError> {
        self.quarantine_change_storage.get_changes_by_media(media_id, since_stream_id, limit).await
    }

    /// List quarantine changes across all media (global stream).
    ///
    /// Backs the `GET /_synapse/admin/v1/media/quarantine_changes` admin
    /// endpoint. Returns changes with `stream_id > since_stream_id`, ordered
    /// ascending, capped by `limit`.
    #[instrument(skip(self))]
    pub async fn get_global_media_quarantine_changes(
        &self,
        since_stream_id: i64,
        limit: i64,
    ) -> Result<Vec<QuarantinedMediaChange>, ApiError> {
        self.quarantine_change_storage.get_quarantined_media_changes(since_stream_id, limit).await
    }

    // ───────────────────────────────────────────────────────────────────────────
    // Admin quarantine management endpoints
    // ───────────────────────────────────────────────────────────────────────────

    /// Quarantine a media item for a specific server.
    ///
    /// Backs `POST /_synapse/admin/v1/media/quarantine/{server_name}/{media_id}`.
    /// Returns the stream_id of the quarantine change record.
    #[instrument(skip(self))]
    pub async fn quarantine_media(&self, server_name: &str, media_id: &str, changed_by: &str) -> Result<i64, ApiError> {
        let now_ts = current_timestamp_millis();

        // Record the quarantine change in the stream
        let stream_id = self
            .quarantine_change_storage
            .record_media_quarantine_change(media_id, server_name, "quarantine", changed_by, now_ts)
            .await?;

        // Update the actual quarantine status on the media record
        self.quarantine_change_storage.set_media_quarantine_status(media_id, server_name, "quarantined").await?;

        Ok(stream_id)
    }

    /// Unquarantine a media item for a specific server.
    ///
    /// Backs `POST /_synapse/admin/v1/media/unquarantine/{server_name}/{media_id}`.
    /// Returns the stream_id of the unquarantine change record.
    #[instrument(skip(self))]
    pub async fn unquarantine_media(
        &self,
        server_name: &str,
        media_id: &str,
        changed_by: &str,
    ) -> Result<i64, ApiError> {
        let now_ts = current_timestamp_millis();

        // Record the unquarantine change in the stream
        let stream_id = self
            .quarantine_change_storage
            .record_media_quarantine_change(media_id, server_name, "unquarantine", changed_by, now_ts)
            .await?;

        // Update the actual quarantine status on the media record
        self.quarantine_change_storage.set_media_quarantine_status(media_id, server_name, "").await?;

        Ok(stream_id)
    }

    /// Quarantine media in a room.
    ///
    /// First verifies that the media is in the room (by checking room_events),
    /// then records the quarantine change and updates the status.
    ///
    /// Backs `POST /_synapse/admin/v1/rooms/{roomId}/media/quarantine`.
    /// If `user_id` is provided, only that user's media in the room is affected.
    #[instrument(skip(self), fields(room_id, user_id))]
    pub async fn quarantine_room_media(
        &self,
        room_id: &str,
        user_id: Option<&str>,
        changed_by: &str,
    ) -> Result<i64, ApiError> {
        // Get all media in the room
        let page = self.storage.get_room_media(room_id, 1000, None).await?;

        let mut last_stream_id = 0i64;
        let now_ts = current_timestamp_millis();

        for media in &page.media {
            // Filter by user_id if specified
            if let Some(uid) = user_id {
                if media.uploader_user_id.as_deref() != Some(uid) {
                    continue;
                }
            }

            let server_name = self.server_name.as_str();
            let stream_id = self
                .quarantine_change_storage
                .record_media_quarantine_change(&media.media_id, server_name, "quarantine", changed_by, now_ts)
                .await?;

            self.quarantine_change_storage
                .set_media_quarantine_status(&media.media_id, server_name, "quarantined")
                .await?;

            last_stream_id = stream_id;
        }

        if last_stream_id == 0 {
            return Err(ApiError::not_found("No media found in room".to_string()));
        }

        Ok(last_stream_id)
    }

    /// Unquarantine media in a room.
    ///
    /// First verifies that the media is in the room (by checking room_events),
    /// then records the unquarantine change and clears the quarantine status.
    ///
    /// Backs `POST /_synapse/admin/v1/rooms/{roomId}/media/unquarantine`.
    /// If `user_id` is provided, only that user's media in the room is affected.
    #[instrument(skip(self), fields(room_id, user_id))]
    pub async fn unquarantine_room_media(
        &self,
        room_id: &str,
        user_id: Option<&str>,
        changed_by: &str,
    ) -> Result<i64, ApiError> {
        // Get all media in the room
        let page = self.storage.get_room_media(room_id, 1000, None).await?;

        let mut last_stream_id = 0i64;
        let now_ts = current_timestamp_millis();

        for media in &page.media {
            // Filter by user_id if specified
            if let Some(uid) = user_id {
                if media.uploader_user_id.as_deref() != Some(uid) {
                    continue;
                }
            }

            let server_name = self.server_name.as_str();
            let stream_id = self
                .quarantine_change_storage
                .record_media_quarantine_change(&media.media_id, server_name, "unquarantine", changed_by, now_ts)
                .await?;

            self.quarantine_change_storage.set_media_quarantine_status(&media.media_id, server_name, "").await?;

            last_stream_id = stream_id;
        }

        if last_stream_id == 0 {
            return Err(ApiError::not_found("No media found in room".to_string()));
        }

        Ok(last_stream_id)
    }

    /// Protect media from automatic quarantine.
    ///
    /// Sets the quarantine_status to "protected" which prevents automatic re-quarantine.
    ///
    /// Backs `POST /_synapse/admin/v1/media/protect/{serverName}/{mediaId}`.
    #[instrument(skip(self), fields(media_id))]
    pub async fn protect_media(&self, server_name: &str, media_id: &str, changed_by: &str) -> Result<i64, ApiError> {
        let now_ts = current_timestamp_millis();
        let stream_id = self
            .quarantine_change_storage
            .record_media_quarantine_change(media_id, server_name, "protect", changed_by, now_ts)
            .await?;

        self.quarantine_change_storage.set_media_quarantine_status(media_id, server_name, "protected").await?;

        Ok(stream_id)
    }

    // ───────────────────────────────────────────────────────────────────────────
    // U-5 missing endpoint implementations
    // ───────────────────────────────────────────────────────────────────────────

    /// Quarantine all local media uploaded by a given user.
    ///
    /// Backs `POST /_synapse/admin/v1/user/{user_id}/media/quarantine`.
    /// Records the quarantine action in the audit stream.
    #[instrument(skip(self), fields(user_id))]
    pub async fn quarantine_user_media(&self, user_id: &str, changed_by: &str) -> Result<i64, ApiError> {
        let now_ts = current_timestamp_millis();

        // Get canonical user ID first (validates user exists)
        let user = self.user_service.get_user_or_not_found(user_id).await?;

        // Quarantine all media for this user
        let count = self.storage.quarantine_user_media(&user.user_id).await?;

        if count == 0 {
            return Err(ApiError::not_found(format!("No media found for user {}", user_id)));
        }

        // Record the quarantine change in the audit stream
        let stream_id = self
            .quarantine_change_storage
            .record_media_quarantine_change(
                &format!("user:{}/*", user.user_id),
                self.server_name.as_str(),
                "quarantine_by_user",
                changed_by,
                now_ts,
            )
            .await?;

        Ok(stream_id)
    }

    /// Batch-delete local media by policy: created before `before_ts` OR larger than `max_size`.
    ///
    /// Backs `POST /_synapse/admin/v1/media/delete`.
    /// Both parameters are optional; a value of `0` means "no limit on that dimension".
    /// Protected and quarantined rows are skipped.
    #[instrument(skip(self))]
    pub async fn delete_media_by_policy(&self, before_ts: i64, max_size: i64) -> Result<u64, ApiError> {
        let deleted = self.storage.delete_media_by_policy(before_ts, max_size).await?;
        Ok(deleted)
    }

    /// Purge cached remote media that has not been accessed since `before_ts`.
    ///
    /// Backs `POST /_synapse/admin/v1/purge_media_cache`.
    /// In this implementation only local media exists, so this degrades to
    /// deleting local media that matches the access-time policy.
    #[instrument(skip(self))]
    pub async fn purge_media_cache(&self, before_ts: i64) -> Result<u64, ApiError> {
        let purged = self.storage.purge_media_cache(before_ts).await?;
        Ok(purged)
    }

    /// Clear the `protected` status on a media row so it can be quarantined
    /// or deleted by policy again.
    ///
    /// Backs `POST /_synapse/admin/v1/media/unprotect/{media_id}`.
    #[instrument(skip(self), fields(media_id))]
    pub async fn unprotect_media(&self, media_id: &str, changed_by: &str) -> Result<i64, ApiError> {
        let now_ts = current_timestamp_millis();

        // First verify the media exists
        let media = self.storage.get_media_info(media_id).await?;
        if media.is_none() {
            return Err(ApiError::not_found(format!("Media {} not found", media_id)));
        }

        // Unprotect the media
        let result = self.storage.unprotect_media(media_id, changed_by).await?;

        if result == 0 {
            return Err(ApiError::not_found(format!("Media {} was not protected", media_id)));
        }

        // Record the unprotect change in the audit stream
        let server_name = self.server_name.as_str();
        let stream_id = self
            .quarantine_change_storage
            .record_media_quarantine_change(media_id, server_name, "unprotect", changed_by, now_ts)
            .await?;

        Ok(stream_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use synapse_storage::test_mocks::{
        shared_fake_user_store, InMemoryAdminMediaStore, InMemoryQuarantineMediaChangeStore,
    };

    fn test_service() -> (AdminMediaService, Arc<InMemoryAdminMediaStore>, Arc<InMemoryQuarantineMediaChangeStore>) {
        let store = Arc::new(InMemoryAdminMediaStore::new());
        let quarantine_store = Arc::new(InMemoryQuarantineMediaChangeStore::new());
        let user_store = shared_fake_user_store();
        let user_service = Arc::new(crate::account::UserService::new(user_store.clone()));
        let svc =
            AdminMediaService::new(store.clone(), quarantine_store.clone(), user_service, "example.com".to_string());
        (svc, store, quarantine_store)
    }

    fn sample_media(id: &str, uploader: &str) -> AdminMediaInfo {
        AdminMediaInfo {
            media_id: id.into(),
            content_type: Some("image/png".into()),
            file_name: Some("test.png".into()),
            size: 1024,
            uploader_user_id: Some(uploader.into()),
            created_ts: 1_700_000_000_000,
            last_accessed_at: None,
            quarantined: false,
        }
    }

    #[tokio::test]
    async fn delete_media_removes_existing() {
        let (svc, store, _q) = test_service();
        store.insert_media(sample_media("media-1", "@alice:example.com")).await;
        svc.delete_media("media-1").await.unwrap();
        assert!(store.get_media_info("media-1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn delete_media_returns_not_found_for_missing() {
        let (svc, _store, _q) = test_service();
        let err = svc.delete_media("nonexistent").await.unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[tokio::test]
    async fn get_media_info_returns_media() {
        let (svc, store, _q) = test_service();
        store.insert_media(sample_media("media-1", "@alice:example.com")).await;
        let info = svc.get_media_info("media-1").await.unwrap();
        assert!(info.is_some());
        assert_eq!(info.unwrap().media_id, "media-1");
    }

    #[tokio::test]
    async fn get_media_info_returns_none() {
        let (svc, _store, _q) = test_service();
        assert!(svc.get_media_info("nonexistent").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn get_all_media_returns_results() {
        let (svc, store, _q) = test_service();
        store.insert_media(sample_media("media-1", "@alice:example.com")).await;
        store.insert_media(sample_media("media-2", "@bob:example.com")).await;
        let page = svc.get_all_media(100, None).await.unwrap();
        assert_eq!(page.media.len(), 2);
    }

    #[tokio::test]
    async fn get_media_quota_returns_summary() {
        let (svc, store, _q) = test_service();
        store.insert_media(sample_media("media-1", "@alice:example.com")).await;
        let mut m2 = sample_media("media-2", "@bob:example.com");
        m2.size = 2048;
        store.insert_media(m2).await;
        let quota = svc.get_media_quota().await.unwrap();
        assert_eq!(quota.total_count, 2);
        assert_eq!(quota.total_size, 3072);
    }

    #[tokio::test]
    async fn get_media_quota_empty() {
        let (svc, _store, _q) = test_service();
        let quota = svc.get_media_quota().await.unwrap();
        assert_eq!(quota.total_count, 0);
        assert_eq!(quota.total_size, 0);
    }

    #[tokio::test]
    async fn get_user_media_user_not_found() {
        let (svc, _store, _q) = test_service();
        let err = svc.get_user_media("@unknown:example.com").await.unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[tokio::test]
    async fn delete_user_media_user_not_found() {
        let (svc, _store, _q) = test_service();
        let err = svc.delete_user_media("@unknown:example.com").await.unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    // ── P0.2 TDD: quarantine change history (RED → GREEN) ──

    fn sample_change(stream_id: i64, media_id: &str, change_type: &str) -> QuarantinedMediaChange {
        QuarantinedMediaChange {
            stream_id,
            media_id: media_id.to_string(),
            server_name: "example.com".to_string(),
            change_type: change_type.to_string(),
            changed_by: "@admin:example.com".to_string(),
            created_ts: 1_700_000_000_000 + stream_id * 1_000,
        }
    }

    #[tokio::test]
    async fn get_media_quarantine_changes_returns_seeded_changes_for_target_media() {
        let (svc, _store, q_store) = test_service();
        q_store.seed_change(sample_change(1, "media-A", "quarantine")).await;
        q_store.seed_change(sample_change(2, "media-B", "quarantine")).await;
        q_store.seed_change(sample_change(3, "media-A", "unquarantine")).await;

        let changes = svc.get_media_quarantine_changes("media-A", 0, 100).await.unwrap();
        assert_eq!(changes.len(), 2, "only media-A changes should be returned");
        assert_eq!(changes[0].stream_id, 1);
        assert_eq!(changes[0].change_type, "quarantine");
        assert_eq!(changes[1].stream_id, 3);
        assert_eq!(changes[1].change_type, "unquarantine");
    }

    #[tokio::test]
    async fn get_media_quarantine_changes_respects_since_stream_id() {
        let (svc, _store, q_store) = test_service();
        q_store.seed_change(sample_change(1, "media-A", "quarantine")).await;
        q_store.seed_change(sample_change(2, "media-A", "unquarantine")).await;
        q_store.seed_change(sample_change(3, "media-A", "quarantine")).await;

        let changes = svc.get_media_quarantine_changes("media-A", 1, 100).await.unwrap();
        assert_eq!(changes.len(), 2, "should skip stream_id <= since_stream_id");
        assert_eq!(changes[0].stream_id, 2);
        assert_eq!(changes[1].stream_id, 3);
    }

    #[tokio::test]
    async fn get_media_quarantine_changes_respects_limit() {
        let (svc, _store, q_store) = test_service();
        for i in 1..=5 {
            q_store.seed_change(sample_change(i, "media-A", "quarantine")).await;
        }

        let changes = svc.get_media_quarantine_changes("media-A", 0, 3).await.unwrap();
        assert_eq!(changes.len(), 3, "limit should cap the result count");
        assert_eq!(changes[0].stream_id, 1);
        assert_eq!(changes[2].stream_id, 3);
    }

    #[tokio::test]
    async fn get_media_quarantine_changes_empty_when_no_history() {
        let (svc, _store, _q_store) = test_service();
        let changes = svc.get_media_quarantine_changes("nonexistent", 0, 100).await.unwrap();
        assert!(changes.is_empty());
    }
}
