use crate::common::error::{ApiError, ApiResult};
use std::sync::Arc;

use synapse_storage::presence::PresenceStoreApi;

/// Presence status tuple: (presence_state, status_msg, last_active_ts)
pub type PresenceRecord = (String, Option<String>, Option<i64>);
/// Batch presence tuple: (user_id, presence_state, status_msg, last_active_ts)
pub type PresenceBatchRecord = (String, String, Option<String>, Option<i64>);

pub struct PresenceService {
    storage: Arc<dyn PresenceStoreApi>,
}

impl PresenceService {
    pub fn new(storage: Arc<dyn PresenceStoreApi>) -> Self {
        Self { storage }
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_presence_with_meta(&self, user_id: &str) -> ApiResult<Option<PresenceRecord>> {
        self.storage
            .get_presence_with_meta(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get presence", &e))
    }

    #[tracing::instrument(skip(self))]
    pub async fn set_presence(&self, user_id: &str, presence: &str, status_msg: Option<&str>) -> ApiResult<()> {
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
    pub async fn get_presence_batch_with_meta(&self, user_ids: &[String]) -> ApiResult<Vec<PresenceBatchRecord>> {
        self.storage
            .get_presence_batch_with_meta(user_ids)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get presence batch", &e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use synapse_storage::test_mocks::InMemoryPresenceStore;

    fn test_service() -> PresenceService {
        PresenceService::new(Arc::new(InMemoryPresenceStore::new()))
    }

    #[test]
    fn test_presence_record_type_construction() {
        let record: PresenceRecord = ("online".to_string(), Some("at work".to_string()), Some(1719600000));
        assert_eq!(record.0, "online");
        assert_eq!(record.1, Some("at work".to_string()));
        assert_eq!(record.2, Some(1719600000));
    }

    #[test]
    fn test_presence_batch_record_type_construction() {
        let record: PresenceBatchRecord =
            ("@alice:localhost".to_string(), "online".to_string(), Some("available".to_string()), Some(1719600000));
        assert_eq!(record.0, "@alice:localhost");
        assert_eq!(record.1, "online");
    }

    #[test]
    fn test_presence_record_option_none_fields() {
        let record: PresenceRecord = ("offline".to_string(), None, None);
        assert_eq!(record.0, "offline");
        assert!(record.1.is_none());
        assert!(record.2.is_none());
    }

    // ── Trait-rewired DB-free unit tests (ARC-12 InMemory Mock) ──────────
    // These tests exercise PresenceService logic via InMemoryPresenceStore
    // without touching PostgreSQL. Ref: TDD落地执行清单 §8.3 ARC-12a.

    #[tokio::test]
    async fn get_presence_with_meta_returns_none_for_unknown_user() {
        let svc = test_service();
        let result = svc.get_presence_with_meta("@nobody:example.com").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn set_and_get_presence_round_trip() {
        let svc = test_service();
        svc.set_presence("@alice:example.com", "online", Some("at work")).await.unwrap();

        let result = svc.get_presence_with_meta("@alice:example.com").await.unwrap();
        assert_eq!(result.as_ref().unwrap().0, "online");
        assert_eq!(result.as_ref().unwrap().1.as_deref(), Some("at work"));
        assert!(result.unwrap().2.is_some(), "last_active_ts should be populated");
    }

    #[tokio::test]
    async fn set_presence_with_none_status_msg() {
        let svc = test_service();
        svc.set_presence("@bob:example.com", "away", None).await.unwrap();

        let result = svc.get_presence_with_meta("@bob:example.com").await.unwrap().unwrap();
        assert_eq!(result.0, "away");
        assert!(result.1.is_none(), "status_msg should be None");
    }

    #[tokio::test]
    async fn set_presence_overwrites_previous_value() {
        let svc = test_service();
        svc.set_presence("@carol:example.com", "online", Some("initial")).await.unwrap();
        svc.set_presence("@carol:example.com", "offline", Some("final")).await.unwrap();

        let result = svc.get_presence_with_meta("@carol:example.com").await.unwrap().unwrap();
        assert_eq!(result.0, "offline", "presence should reflect the latest set");
        assert_eq!(result.1.as_deref(), Some("final"));
    }

    #[tokio::test]
    async fn add_and_get_subscriptions_round_trip() {
        let svc = test_service();
        svc.add_subscription("@sub:example.com", "@target_a:example.com").await.unwrap();
        svc.add_subscription("@sub:example.com", "@target_b:example.com").await.unwrap();

        let subs = svc.get_subscriptions("@sub:example.com").await.unwrap();
        assert_eq!(subs.len(), 2);
        assert!(subs.contains(&"@target_a:example.com".to_string()));
        assert!(subs.contains(&"@target_b:example.com".to_string()));
    }

    #[tokio::test]
    async fn add_subscription_dedupes_identical_entries() {
        let svc = test_service();
        svc.add_subscription("@sub:example.com", "@target:example.com").await.unwrap();
        svc.add_subscription("@sub:example.com", "@target:example.com").await.unwrap();

        let subs = svc.get_subscriptions("@sub:example.com").await.unwrap();
        assert_eq!(subs.len(), 1, "duplicate add should be deduped");
    }

    #[tokio::test]
    async fn remove_subscription_deletes_matching_entry() {
        let svc = test_service();
        svc.add_subscription("@sub:example.com", "@target_a:example.com").await.unwrap();
        svc.add_subscription("@sub:example.com", "@target_b:example.com").await.unwrap();

        svc.remove_subscription("@sub:example.com", "@target_a:example.com").await.unwrap();

        let subs = svc.get_subscriptions("@sub:example.com").await.unwrap();
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0], "@target_b:example.com");
    }

    #[tokio::test]
    async fn remove_subscription_is_idempotent_for_missing_entry() {
        let svc = test_service();
        // Removing a subscription that never existed should not error.
        svc.remove_subscription("@sub:example.com", "@never_subscribed:example.com").await.unwrap();
    }

    #[tokio::test]
    async fn remove_subscription_only_removes_matching_target() {
        let svc = test_service();
        svc.add_subscription("@sub:example.com", "@target_a:example.com").await.unwrap();
        svc.add_subscription("@sub:example.com", "@target_b:example.com").await.unwrap();

        // Removing target_b should not affect target_a.
        svc.remove_subscription("@sub:example.com", "@target_b:example.com").await.unwrap();

        let subs = svc.get_subscriptions("@sub:example.com").await.unwrap();
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0], "@target_a:example.com");
    }

    #[tokio::test]
    async fn get_subscriptions_returns_empty_for_subscriber_with_no_subs() {
        let svc = test_service();
        let subs = svc.get_subscriptions("@lonely:example.com").await.unwrap();
        assert!(subs.is_empty());
    }

    #[tokio::test]
    async fn get_subscriptions_filters_by_subscriber() {
        let svc = test_service();
        svc.add_subscription("@alice:example.com", "@target_a:example.com").await.unwrap();
        svc.add_subscription("@bob:example.com", "@target_b:example.com").await.unwrap();

        let alice_subs = svc.get_subscriptions("@alice:example.com").await.unwrap();
        assert_eq!(alice_subs.len(), 1);
        assert_eq!(alice_subs[0], "@target_a:example.com");

        let bob_subs = svc.get_subscriptions("@bob:example.com").await.unwrap();
        assert_eq!(bob_subs.len(), 1);
        assert_eq!(bob_subs[0], "@target_b:example.com");
    }

    #[tokio::test]
    async fn get_presence_batch_with_meta_returns_partial_results() {
        let svc = test_service();
        svc.set_presence("@alice:example.com", "online", Some("working")).await.unwrap();
        // @bob is intentionally not seeded.

        let user_ids = vec!["@alice:example.com".to_string(), "@bob:example.com".to_string()];
        let batch = svc.get_presence_batch_with_meta(&user_ids).await.unwrap();

        assert_eq!(batch.len(), 1, "only @alice should be in the result");
        assert_eq!(batch[0].0, "@alice:example.com");
        assert_eq!(batch[0].1, "online");
        assert_eq!(batch[0].2.as_deref(), Some("working"));
    }

    #[tokio::test]
    async fn get_presence_batch_with_meta_empty_input_returns_empty() {
        let svc = test_service();
        let batch = svc.get_presence_batch_with_meta(&[]).await.unwrap();
        assert!(batch.is_empty());
    }

    #[tokio::test]
    async fn get_presence_batch_with_meta_returns_all_seeded_users() {
        let svc = test_service();
        svc.set_presence("@alice:example.com", "online", None).await.unwrap();
        svc.set_presence("@bob:example.com", "away", Some("lunch")).await.unwrap();
        svc.set_presence("@carol:example.com", "offline", None).await.unwrap();

        let user_ids =
            vec!["@alice:example.com".to_string(), "@bob:example.com".to_string(), "@carol:example.com".to_string()];
        let batch = svc.get_presence_batch_with_meta(&user_ids).await.unwrap();
        assert_eq!(batch.len(), 3);
    }
}
