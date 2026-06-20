use synapse_common::error::ApiError;
use synapse_storage::PresenceStorage;

#[derive(Clone)]
pub struct PresenceService {
    presence_storage: PresenceStorage,
}

impl PresenceService {
    pub fn new(presence_storage: PresenceStorage) -> Self {
        Self { presence_storage }
    }

    pub async fn get_presence_with_meta(
        &self,
        user_id: &str,
    ) -> Result<Option<(String, Option<String>, Option<i64>)>, ApiError> {
        self.presence_storage
            .get_presence_with_meta(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get presence", &e))
    }

    pub async fn set_presence(&self, user_id: &str, presence: &str, status_msg: Option<&str>) -> Result<(), ApiError> {
        self.presence_storage
            .set_presence(user_id, presence, status_msg)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to set presence", &e))
    }

    pub async fn add_subscription(&self, subscriber_id: &str, target_id: &str) -> Result<(), ApiError> {
        self.presence_storage
            .add_subscription(subscriber_id, target_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to add presence subscription", &e))
    }

    pub async fn remove_subscription(&self, subscriber_id: &str, target_id: &str) -> Result<(), ApiError> {
        self.presence_storage
            .remove_subscription(subscriber_id, target_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to remove presence subscription", &e))
    }

    pub async fn get_subscriptions(&self, subscriber_id: &str) -> Result<Vec<String>, ApiError> {
        self.presence_storage
            .get_subscriptions(subscriber_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get subscriptions", &e))
    }

    pub async fn get_presence_batch_with_meta(
        &self,
        user_ids: &[String],
    ) -> Result<Vec<(String, String, Option<String>, Option<i64>)>, ApiError> {
        self.presence_storage
            .get_presence_batch_with_meta(user_ids)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get presence batch", &e))
    }
}
