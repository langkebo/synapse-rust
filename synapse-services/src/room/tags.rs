use crate::common::error::{ApiError, ApiResult};

use super::service::RoomService;

impl RoomService {
    #[tracing::instrument(skip(self))]
    pub async fn get_all_tags(&self, user_id: &str) -> ApiResult<Vec<synapse_storage::room_tag::RoomTag>> {
        self.room_tag_storage
            .get_all_tags(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get all tags", &e))
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_tags(&self, user_id: &str, room_id: &str) -> ApiResult<Vec<synapse_storage::room_tag::RoomTag>> {
        self.room_tag_storage
            .get_tags(user_id, room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get tags", &e))
    }

    #[tracing::instrument(skip(self))]
    pub async fn add_tag(&self, user_id: &str, room_id: &str, tag: &str, order: Option<f64>) -> ApiResult<()> {
        self.room_tag_storage
            .add_tag(user_id, room_id, tag, order)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to add tag", &e))
    }

    #[tracing::instrument(skip(self))]
    pub async fn remove_tag(&self, user_id: &str, room_id: &str, tag: &str) -> ApiResult<()> {
        self.room_tag_storage
            .remove_tag(user_id, room_id, tag)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to remove tag", &e))
    }
}
