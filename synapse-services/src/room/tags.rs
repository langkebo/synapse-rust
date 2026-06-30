use crate::common::error::ApiError;

use super::state::service::RoomStateService;

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

impl RoomStateService {
    #[tracing::instrument(skip(self))]
    pub async fn get_all_tags(&self, user_id: &str) -> Result<Vec<synapse_storage::room_tag::RoomTag>, TagsError> {
        self.room_tag_storage.get_all_tags(user_id).await.map_err(|e| {
            tracing::error!("Failed to get all tags: {e}");
            TagsError::NotFound
        })
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_tags(
        &self,
        user_id: &str,
        room_id: &str,
    ) -> Result<Vec<synapse_storage::room_tag::RoomTag>, TagsError> {
        self.room_tag_storage.get_tags(user_id, room_id).await.map_err(|e| {
            tracing::error!("Failed to get tags: {e}");
            TagsError::NotFound
        })
    }

    #[tracing::instrument(skip(self))]
    pub async fn add_tag(&self, user_id: &str, room_id: &str, tag: &str, order: Option<f64>) -> Result<(), TagsError> {
        self.room_tag_storage.add_tag(user_id, room_id, tag, order).await.map_err(|e| {
            tracing::error!("Failed to add tag: {e}");
            TagsError::Duplicate
        })
    }

    #[tracing::instrument(skip(self))]
    pub async fn remove_tag(&self, user_id: &str, room_id: &str, tag: &str) -> Result<(), TagsError> {
        self.room_tag_storage.remove_tag(user_id, room_id, tag).await.map_err(|e| {
            tracing::error!("Failed to remove tag: {e}");
            TagsError::NotFound
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::error::ApiError;

    #[test]
    fn test_tags_error_not_found_maps_to_404() {
        let api_err: ApiError = TagsError::NotFound.into();
        assert_eq!(api_err.http_status().as_u16(), 404);
        assert!(api_err.message().contains("Tag not found"));
    }

    #[test]
    fn test_tags_error_duplicate_maps_to_409() {
        let api_err: ApiError = TagsError::Duplicate.into();
        assert_eq!(api_err.http_status().as_u16(), 409);
        assert!(api_err.message().contains("Tag already exists"));
    }

    #[test]
    fn test_tags_error_display() {
        assert_eq!(TagsError::NotFound.to_string(), "Tag not found");
        assert_eq!(TagsError::Duplicate.to_string(), "Tag already exists");
    }

    #[test]
    fn test_tags_error_debug() {
        let debug_str = format!("{:?}", TagsError::NotFound);
        assert!(debug_str.contains("NotFound"));
    }
}
