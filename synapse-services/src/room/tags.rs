use crate::common::error::ApiError;

use super::service::RoomService;

/// Domain errors for tag operations.
#[derive(Debug, thiserror::Error)]
pub enum TagsError {
    #[error("Tag not found")]
    NotFound,
    #[error("Tag already exists")]
    Duplicate,
}

impl From<TagsError> for ApiError {
    fn from(e: TagsError) -> Self {
        match e {
            TagsError::NotFound => ApiError::not_found(e.to_string()),
            TagsError::Duplicate => ApiError::conflict(e.to_string()),
        }
    }
}

impl RoomService {
    #[tracing::instrument(skip(self))]
    pub async fn get_all_tags(&self, user_id: &str) -> Result<Vec<synapse_storage::room_tag::RoomTag>, TagsError> {
        self.room_tag_storage
            .get_all_tags(user_id)
            .await
            .map_err(|e| {
                tracing::error!("Failed to get all tags: {e}");
                TagsError::NotFound
            })
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_tags(&self, user_id: &str, room_id: &str) -> Result<Vec<synapse_storage::room_tag::RoomTag>, TagsError> {
        self.room_tag_storage
            .get_tags(user_id, room_id)
            .await
            .map_err(|e| {
                tracing::error!("Failed to get tags: {e}");
                TagsError::NotFound
            })
    }

    #[tracing::instrument(skip(self))]
    pub async fn add_tag(&self, user_id: &str, room_id: &str, tag: &str, order: Option<f64>) -> Result<(), TagsError> {
        self.room_tag_storage
            .add_tag(user_id, room_id, tag, order)
            .await
            .map_err(|e| {
                tracing::error!("Failed to add tag: {e}");
                TagsError::Duplicate
            })
    }

    #[tracing::instrument(skip(self))]
    pub async fn remove_tag(&self, user_id: &str, room_id: &str, tag: &str) -> Result<(), TagsError> {
        self.room_tag_storage
            .remove_tag(user_id, room_id, tag)
            .await
            .map_err(|e| {
                tracing::error!("Failed to remove tag: {e}");
                TagsError::NotFound
            })
    }
}
