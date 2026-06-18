use sqlx::PgPool;
use std::sync::Arc;
use synapse_common::ApiError;
use synapse_storage::room_tag::{RoomTag, RoomTagStorage};
use tracing::instrument;

#[derive(Clone)]
pub struct RoomTagService {
    pool: Arc<PgPool>,
}

impl RoomTagService {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    #[instrument(skip(self))]
    pub async fn get_all_user_tags(&self, user_id: &str) -> Result<Vec<RoomTag>, ApiError> {
        RoomTagStorage::get_all_tags(&self.pool, user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get tags", &e))
    }

    #[instrument(skip(self))]
    pub async fn get_room_tags(&self, user_id: &str, room_id: &str) -> Result<Vec<RoomTag>, ApiError> {
        RoomTagStorage::get_tags(&self.pool, user_id, room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get tags", &e))
    }

    #[instrument(skip(self))]
    pub async fn put_room_tag(
        &self,
        user_id: &str,
        room_id: &str,
        tag: &str,
        order: Option<f64>,
    ) -> Result<(), ApiError> {
        RoomTagStorage::add_tag(&self.pool, user_id, room_id, tag, order)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to set tag", &e))
    }

    #[instrument(skip(self))]
    pub async fn delete_room_tag(&self, user_id: &str, room_id: &str, tag: &str) -> Result<(), ApiError> {
        RoomTagStorage::remove_tag(&self.pool, user_id, room_id, tag)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to delete tag", &e))
    }
}