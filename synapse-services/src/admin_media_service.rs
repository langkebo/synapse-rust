use std::sync::Arc;
use synapse_common::ApiError;
pub use synapse_storage::{
    decode_media_cursor, encode_media_cursor, AdminMediaInfo, AdminMediaPage, AdminMediaQuotaSummary, MediaCursor,
};
use synapse_storage::{AdminMediaStoreApi, UserStore};
use tracing::instrument;

pub struct AdminMediaService {
    storage: Arc<dyn AdminMediaStoreApi>,
    user_storage: Arc<dyn UserStore>,
}

impl AdminMediaService {
    pub fn new(storage: Arc<dyn AdminMediaStoreApi>, user_storage: Arc<dyn UserStore>) -> Self {
        Self { storage, user_storage }
    }

    #[instrument(skip(self))]
    pub async fn get_all_media(&self, limit: i64, cursor: Option<MediaCursor>) -> Result<AdminMediaPage, ApiError> {
        self.storage.get_all_media(limit, cursor).await
    }

    #[instrument(skip(self))]
    pub async fn get_media_info(&self, media_id: &str) -> Result<Option<AdminMediaInfo>, ApiError> {
        self.storage.get_media_info(media_id).await
    }

    #[instrument(skip(self))]
    pub async fn delete_media(&self, media_id: &str) -> Result<(), ApiError> {
        if !self.storage.delete_media(media_id).await? {
            return Err(ApiError::not_found("Media not found".to_string()));
        }

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn get_media_quota(&self) -> Result<AdminMediaQuotaSummary, ApiError> {
        self.storage.get_media_quota().await
    }

    #[instrument(skip(self))]
    pub async fn get_user_media(&self, identifier: &str) -> Result<(String, Vec<AdminMediaInfo>), ApiError> {
        let user = self
            .user_storage
            .get_user_by_identifier(identifier)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?
            .ok_or_else(|| ApiError::not_found("User not found".to_string()))?;

        let media = self.storage.get_user_media(&user.user_id).await?;

        Ok((user.user_id, media))
    }

    #[instrument(skip(self))]
    pub async fn delete_user_media(&self, identifier: &str) -> Result<u64, ApiError> {
        let user = self
            .user_storage
            .get_user_by_identifier(identifier)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?
            .ok_or_else(|| ApiError::not_found("User not found".to_string()))?;

        self.storage.delete_user_media(&user.user_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use synapse_storage::test_mocks::{shared_fake_user_store, InMemoryAdminMediaStore};

    fn test_service() -> (AdminMediaService, Arc<InMemoryAdminMediaStore>) {
        let store = Arc::new(InMemoryAdminMediaStore::new());
        let svc = AdminMediaService::new(store.clone(), shared_fake_user_store());
        (svc, store)
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

    // ── delete_media ────────────────────────────────────────────────

    #[tokio::test]
    async fn delete_media_removes_existing() {
        let (svc, store) = test_service();
        store.insert_media(sample_media("media-1", "@alice:example.com")).await;
        svc.delete_media("media-1").await.unwrap();
        assert!(store.get_media_info("media-1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn delete_media_returns_not_found_for_missing() {
        let (svc, _store) = test_service();
        let err = svc.delete_media("nonexistent").await.unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    // ── get_media_info ──────────────────────────────────────────────

    #[tokio::test]
    async fn get_media_info_returns_media() {
        let (svc, store) = test_service();
        store.insert_media(sample_media("media-1", "@alice:example.com")).await;
        let info = svc.get_media_info("media-1").await.unwrap();
        assert!(info.is_some());
        assert_eq!(info.unwrap().media_id, "media-1");
    }

    #[tokio::test]
    async fn get_media_info_returns_none() {
        let (svc, _store) = test_service();
        assert!(svc.get_media_info("nonexistent").await.unwrap().is_none());
    }

    // ── get_all_media ───────────────────────────────────────────────

    #[tokio::test]
    async fn get_all_media_returns_results() {
        let (svc, store) = test_service();
        store.insert_media(sample_media("media-1", "@alice:example.com")).await;
        store.insert_media(sample_media("media-2", "@bob:example.com")).await;
        let page = svc.get_all_media(100, None).await.unwrap();
        assert_eq!(page.media.len(), 2);
    }

    // ── get_media_quota ─────────────────────────────────────────────

    #[tokio::test]
    async fn get_media_quota_returns_summary() {
        let (svc, store) = test_service();
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
        let (svc, _store) = test_service();
        let quota = svc.get_media_quota().await.unwrap();
        assert_eq!(quota.total_count, 0);
        assert_eq!(quota.total_size, 0);
    }

    // ── get_user_media ──────────────────────────────────────────────

    #[tokio::test]
    async fn get_user_media_user_not_found() {
        let (svc, _store) = test_service();
        let err = svc.get_user_media("@unknown:example.com").await.unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    // ── delete_user_media ───────────────────────────────────────────

    #[tokio::test]
    async fn delete_user_media_user_not_found() {
        let (svc, _store) = test_service();
        let err = svc.delete_user_media("@unknown:example.com").await.unwrap_err();
        assert!(err.to_string().contains("not found"));
    }
}
