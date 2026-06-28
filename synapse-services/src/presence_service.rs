use crate::common::error::{ApiError, ApiResult};

pub struct PresenceService {
    storage: synapse_storage::PresenceStorage,
}

impl PresenceService {
    pub fn new(storage: synapse_storage::PresenceStorage) -> Self {
        Self { storage }
    }

    #[tracing::instrument(skip(self))]
    #[allow(clippy::type_complexity)]
    pub async fn get_presence_with_meta(
        &self,
        user_id: &str,
    ) -> ApiResult<Option<(String, Option<String>, Option<i64>)>> {
        self.storage
            .get_presence_with_meta(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get presence", &e))
    }

    #[tracing::instrument(skip(self))]
    pub async fn set_presence(
        &self,
        user_id: &str,
        presence: &str,
        status_msg: Option<&str>,
    ) -> ApiResult<()> {
        self.storage
            .set_presence(user_id, presence, status_msg)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to set presence", &e))
    }

    #[tracing::instrument(skip(self))]
    pub async fn add_subscription(&self, subscriber_id: &str, target_id: &str) -> ApiResult<()> {
        self.storage
            .add_subscription(subscriber_id, target_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to add presence subscription", &e))
    }

    #[tracing::instrument(skip(self))]
    pub async fn remove_subscription(&self, subscriber_id: &str, target_id: &str) -> ApiResult<()> {
        self.storage
            .remove_subscription(subscriber_id, target_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to remove presence subscription", &e))
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_subscriptions(&self, subscriber_id: &str) -> ApiResult<Vec<String>> {
        self.storage
            .get_subscriptions(subscriber_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get subscriptions", &e))
    }

    #[tracing::instrument(skip(self))]
    #[allow(clippy::type_complexity)]
    pub async fn get_presence_batch_with_meta(
        &self,
        user_ids: &[String],
    ) -> ApiResult<Vec<(String, String, Option<String>, Option<i64>)>> {
        self.storage
            .get_presence_batch_with_meta(user_ids)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get presence batch", &e))
    }
}
