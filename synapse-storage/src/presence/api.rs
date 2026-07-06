use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

/// Storage-agnostic API for presence persistence.
#[async_trait]
pub trait PresenceStoreApi: Send + Sync {
    fn pool(&self) -> &Arc<sqlx::PgPool>;

    async fn set_presence(&self, user_id: &str, presence: &str, status_msg: Option<&str>) -> Result<(), sqlx::Error>;

    async fn set_typing(&self, room_id: &str, user_id: &str, typing: bool) -> Result<(), sqlx::Error>;

    async fn get_presences(
        &self,
        user_ids: &[String],
    ) -> Result<HashMap<String, (String, Option<String>)>, sqlx::Error>;

    async fn get_presence_with_meta(
        &self,
        user_id: &str,
    ) -> Result<Option<(String, Option<String>, Option<i64>)>, sqlx::Error>;

    async fn remove_subscription(&self, subscriber_id: &str, target_id: &str) -> Result<(), sqlx::Error>;

    async fn add_subscription(&self, subscriber_id: &str, target_id: &str) -> Result<(), sqlx::Error>;

    async fn get_subscriptions(&self, subscriber_id: &str) -> Result<Vec<String>, sqlx::Error>;

    async fn get_presence_batch_with_meta(
        &self,
        user_ids: &[String],
    ) -> Result<Vec<(String, String, Option<String>, Option<i64>)>, sqlx::Error>;

    async fn get_presence_snapshots(
        &self,
        user_ids: &[String],
    ) -> Result<HashMap<String, super::PresenceSnapshot>, sqlx::Error>;
}

// ── Delegation impl for Postgres PresenceStorage ─────────────────────

#[async_trait]
impl PresenceStoreApi for super::PresenceStorage {
    fn pool(&self) -> &Arc<sqlx::PgPool> {
        &self.pool
    }

    async fn set_presence(&self, user_id: &str, presence: &str, status_msg: Option<&str>) -> Result<(), sqlx::Error> {
        self.set_presence(user_id, presence, status_msg).await
    }

    async fn set_typing(&self, room_id: &str, user_id: &str, typing: bool) -> Result<(), sqlx::Error> {
        self.set_typing(room_id, user_id, typing).await
    }

    async fn get_presences(
        &self,
        user_ids: &[String],
    ) -> Result<HashMap<String, (String, Option<String>)>, sqlx::Error> {
        self.get_presences(user_ids).await
    }

    async fn get_presence_with_meta(
        &self,
        user_id: &str,
    ) -> Result<Option<(String, Option<String>, Option<i64>)>, sqlx::Error> {
        self.get_presence_with_meta(user_id).await
    }

    async fn remove_subscription(&self, subscriber_id: &str, target_id: &str) -> Result<(), sqlx::Error> {
        self.remove_subscription(subscriber_id, target_id).await
    }

    async fn add_subscription(&self, subscriber_id: &str, target_id: &str) -> Result<(), sqlx::Error> {
        self.add_subscription(subscriber_id, target_id).await
    }

    async fn get_subscriptions(&self, subscriber_id: &str) -> Result<Vec<String>, sqlx::Error> {
        self.get_subscriptions(subscriber_id).await
    }

    async fn get_presence_batch_with_meta(
        &self,
        user_ids: &[String],
    ) -> Result<Vec<(String, String, Option<String>, Option<i64>)>, sqlx::Error> {
        self.get_presence_batch_with_meta(user_ids).await
    }

    async fn get_presence_snapshots(
        &self,
        user_ids: &[String],
    ) -> Result<HashMap<String, super::PresenceSnapshot>, sqlx::Error> {
        self.get_presence_snapshots(user_ids).await
    }
}
