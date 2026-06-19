use sqlx::PgPool;
use std::sync::Arc;
use synapse_common::ApiError;
pub use synapse_storage::{
    decode_media_cursor, encode_media_cursor, AdminMediaInfo, AdminMediaPage, AdminMediaQuotaSummary, MediaCursor,
};
use synapse_storage::{AdminMediaStorage, UserStorage};
use tracing::instrument;

pub struct AdminMediaService {
    storage: AdminMediaStorage,
    user_storage: UserStorage,
}

impl AdminMediaService {
    pub fn new(pool: &Arc<PgPool>, user_storage: UserStorage) -> Self {
        Self { storage: AdminMediaStorage::new(pool), user_storage }
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
