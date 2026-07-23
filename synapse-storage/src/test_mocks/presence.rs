use super::*;
use crate::presence::PresenceStoreApi;
use synapse_common::current_timestamp_millis;

/// In-memory presence snapshot: `(presence, status_msg, last_active_ts)`.
type PresenceSnapshot = (String, Option<String>, Option<i64>);

/// In-memory presence store for testing [`PresenceService`] and
/// [`FriendRoomService`] without a real PostgreSQL pool.
///
/// Stores presence snapshots in a `HashMap<user_id, PresenceSnapshot>`
/// and subscriptions in a `Vec<(subscriber, target)>`.
///
/// Ref: TDD落地执行清单 §8.3 ARC-12a (Problem #6 Trait 采纳补齐)
#[derive(Clone, Default)]
pub struct InMemoryPresenceStore {
    presences: Arc<tokio::sync::RwLock<HashMap<String, PresenceSnapshot>>>,
    subscriptions: Arc<tokio::sync::RwLock<Vec<(String, String)>>>,
}

impl InMemoryPresenceStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait::async_trait]
impl PresenceStoreApi for InMemoryPresenceStore {
    fn pool(&self) -> &Arc<sqlx::PgPool> {
        unimplemented!("InMemoryPresenceStore has no database pool")
    }

    async fn set_presence(&self, user_id: &str, presence: &str, status_msg: Option<&str>) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();
        self.presences
            .write()
            .await
            .insert(user_id.to_string(), (presence.to_string(), status_msg.map(|s| s.to_string()), Some(now)));
        Ok(())
    }

    async fn get_presences(
        &self,
        user_ids: &[String],
    ) -> Result<HashMap<String, (String, Option<String>)>, sqlx::Error> {
        let map = self.presences.read().await;
        let mut result = HashMap::new();
        for user_id in user_ids {
            if let Some((presence, status_msg, _)) = map.get(user_id) {
                result.insert(user_id.clone(), (presence.clone(), status_msg.clone()));
            }
        }
        Ok(result)
    }

    async fn get_presence_with_meta(
        &self,
        user_id: &str,
    ) -> Result<Option<(String, Option<String>, Option<i64>)>, sqlx::Error> {
        Ok(self.presences.read().await.get(user_id).cloned())
    }

    async fn remove_subscription(&self, subscriber_id: &str, target_id: &str) -> Result<(), sqlx::Error> {
        self.subscriptions.write().await.retain(|(s, t)| !(*s == subscriber_id && *t == target_id));
        Ok(())
    }

    async fn add_subscription(&self, subscriber_id: &str, target_id: &str) -> Result<(), sqlx::Error> {
        let mut subs = self.subscriptions.write().await;
        let entry = (subscriber_id.to_string(), target_id.to_string());
        if !subs.contains(&entry) {
            subs.push(entry);
        }
        Ok(())
    }

    async fn get_subscriptions(&self, subscriber_id: &str) -> Result<Vec<String>, sqlx::Error> {
        Ok(self.subscriptions.read().await.iter().filter(|(s, _)| s == subscriber_id).map(|(_, t)| t.clone()).collect())
    }

    async fn get_presence_batch_with_meta(
        &self,
        user_ids: &[String],
    ) -> Result<Vec<(String, String, Option<String>, Option<i64>)>, sqlx::Error> {
        let map = self.presences.read().await;
        let mut results = Vec::new();
        for user_id in user_ids {
            if let Some((presence, status_msg, last_active_ts)) = map.get(user_id) {
                results.push((user_id.clone(), presence.clone(), status_msg.clone(), *last_active_ts));
            }
        }
        Ok(results)
    }

    async fn get_presence_snapshots(
        &self,
        user_ids: &[String],
    ) -> Result<HashMap<String, crate::presence::PresenceSnapshot>, sqlx::Error> {
        let map = self.presences.read().await;
        let mut result = HashMap::new();
        for user_id in user_ids {
            if let Some((presence, status_msg, last_active_ts)) = map.get(user_id) {
                result.insert(
                    user_id.clone(),
                    crate::presence::PresenceSnapshot {
                        user_id: user_id.clone(),
                        presence: presence.clone(),
                        status_msg: status_msg.clone(),
                        last_active_ts: *last_active_ts,
                    },
                );
            }
        }
        Ok(result)
    }

    async fn set_typing(&self, _room_id: &str, _user_id: &str, _typing: bool) -> Result<(), sqlx::Error> {
        Ok(())
    }
}
