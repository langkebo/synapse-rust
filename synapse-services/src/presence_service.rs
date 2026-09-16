use crate::common::error::{ApiError, ApiResult};
use serde_json::json;
use std::collections::HashSet;
use std::sync::Arc;

use synapse_federation::event_broadcaster::EventBroadcaster;
use synapse_storage::presence::PresenceStoreApi;

/// Presence status tuple: (presence_state, status_msg, last_active_ts)
pub type PresenceRecord = (String, Option<String>, Option<i64>);
/// Batch presence tuple: (user_id, presence_state, status_msg, last_active_ts)
pub type PresenceBatchRecord = (String, String, Option<String>, Option<i64>);

/// Presence tuning parameters sourced from `ServerConfig`.
/// Kept as a plain struct so `PresenceService` can be constructed without
/// pulling the entire config object into the service layer.
#[derive(Debug, Clone)]
pub struct PresenceTuning {
    /// Rooms whose events/membership must NOT trigger presence updates.
    pub excluded_rooms: Vec<String>,
    /// Granularity of `last_active_ts` updates, in milliseconds.
    pub last_active_granularity: u64,
    /// Long-poll timeout for online presence sync, in milliseconds.
    pub sync_online_timeout: u64,
    /// Idle timeout before online → unavailable transition, in milliseconds.
    pub idle_timeout: u64,
}

impl Default for PresenceTuning {
    fn default() -> Self {
        Self {
            excluded_rooms: Vec::new(),
            last_active_granularity: 120_000,
            sync_online_timeout: 30_000,
            idle_timeout: 300_000,
        }
    }
}

use std::sync::RwLock;

/// Extract unique remote server names from presence subscriber IDs,
/// filtering out the local server.
fn extract_remote_servers(subscribers: &[String], local_server: &str) -> HashSet<String> {
    let mut remote: HashSet<String> = HashSet::new();
    for subscriber_id in subscribers {
        if let Some((_, server)) = subscriber_id.rsplit_once(':') {
            if !server.is_empty() && server != local_server {
                remote.insert(server.to_string());
            }
        }
    }
    remote
}

/// The `PresenceService` struct.
pub struct PresenceService {
    storage: Arc<dyn PresenceStoreApi>,
    tuning: PresenceTuning,
    /// Federation event broadcaster for outbound presence EDU federation.
    /// Wrapped in RwLock for late binding after federation services are built.
    federation: RwLock<FederationOutbound>,
}

/// Internal struct for federation outbound configuration.
#[derive(Default)]
struct FederationOutbound {
    event_broadcaster: Option<Arc<EventBroadcaster>>,
    server_name: String,
}

impl PresenceService {
    /// See [`new`].
    pub fn new(storage: Arc<dyn PresenceStoreApi>) -> Self {
        Self { storage, tuning: PresenceTuning::default(), federation: RwLock::new(FederationOutbound::default()) }
    }

    /// Construct with explicit presence tuning parameters (sourced from config).
    pub fn with_tuning(storage: Arc<dyn PresenceStoreApi>, tuning: PresenceTuning) -> Self {
        Self { storage, tuning, federation: RwLock::new(FederationOutbound::default()) }
    }

    /// Attach the event broadcaster for federation outbound presence broadcast.
    /// This must be called after EventBroadcaster is constructed in DomainPhase.
    pub fn set_event_broadcaster(&self, event_broadcaster: Arc<EventBroadcaster>, server_name: String) {
        let mut federation = match self.federation.write() {
            Ok(f) => f,
            Err(poisoned) => poisoned.into_inner(),
        };
        federation.event_broadcaster = Some(event_broadcaster);
        federation.server_name = server_name;
    }

    /// Returns true if `room_id` is in `exclude_rooms_from_presence` and must
    /// NOT participate in presence calculations. Mirrors Synapse's
    /// `exclude_rooms_from_presence` config behavior.
    pub fn is_room_excluded_from_presence(&self, room_id: &str) -> bool {
        self.tuning.excluded_rooms.iter().any(|r| r == room_id)
    }

    /// Granularity (ms) of `last_active_ts` updates.
    pub fn last_active_granularity(&self) -> u64 {
        self.tuning.last_active_granularity
    }

    /// Long-poll timeout (ms) for online presence sync.
    pub fn sync_online_timeout(&self) -> u64 {
        self.tuning.sync_online_timeout
    }

    /// Idle timeout (ms) before online → unavailable transition.
    pub fn idle_timeout(&self) -> u64 {
        self.tuning.idle_timeout
    }

    /// See [`get_presence_with_meta`].
    #[tracing::instrument(skip(self))]
    pub async fn get_presence_with_meta(&self, user_id: &str) -> ApiResult<Option<PresenceRecord>> {
        self.storage
            .get_presence_with_meta(user_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get presence", e))
    }

    /// See [`set_presence`].
    #[tracing::instrument(skip(self))]
    pub async fn set_presence(&self, user_id: &str, presence: &str, status_msg: Option<&str>) -> ApiResult<()> {
        self.storage
            .set_presence(user_id, presence, status_msg)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to set presence", e))?;

        // T04: Broadcast presence update to remote servers via federation
        self.broadcast_presence_to_subscribers(user_id).await;

        Ok(())
    }

    /// Apply a federated typing flag for `user_id` in `room_id`.
    ///
    /// Unlike the client-facing typing API this carries no local timeout: the
    /// remote server owns the lifetime and clears it with its own EDU.
    pub async fn set_typing_flag(&self, room_id: &str, user_id: &str, typing: bool) -> ApiResult<()> {
        self.storage
            .set_typing(room_id, user_id, typing)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to persist typing EDU", e))
    }

    /// C-3: Batch set presence for multiple users in a single SQL statement.
    /// Each entry is `(user_id, presence, status_msg)`.
    #[tracing::instrument(skip(self, entries))]
    pub async fn set_presence_batch(&self, entries: &[(String, String, Option<String>)]) -> ApiResult<()> {
        if entries.is_empty() {
            return Ok(());
        }

        self.storage
            .set_presence_batch(entries)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to batch set presence", e))?;

        // T04: Broadcast presence updates to remote servers via federation
        for (user_id, _, _) in entries {
            self.broadcast_presence_to_subscribers(user_id).await;
        }

        Ok(())
    }

    /// Broadcast presence update for a user to all remote servers that have
    /// subscribers on this server tracking that user.
    /// T04: Uses `broadcast_edu` to send `m.presence` EDU to each remote server.
    #[tracing::instrument(skip(self, user_id))]
    async fn broadcast_presence_to_subscribers(&self, user_id: &str) {
        // Gather federation config under lock
        let (broadcaster, server_name) = {
            let federation = match self.federation.read() {
                Ok(f) => f,
                Err(poisoned) => poisoned.into_inner(),
            };
            match federation.event_broadcaster.as_ref() {
                Some(b) => (b.clone(), federation.server_name.clone()),
                None => return, // Federation not configured
            }
        };

        // Get all subscribers for this user from the storage layer
        let subscribers = match self.storage.get_subscribers(user_id).await {
            Ok(subs) => subs,
            Err(e) => {
                tracing::warn!(user_id = %user_id, error = %e, "Failed to get presence subscribers for federation broadcast");
                return;
            }
        };

        if subscribers.is_empty() {
            return;
        }

        // Extract unique remote server names from subscriber IDs
        let remote_servers = extract_remote_servers(&subscribers, &server_name);

        if remote_servers.is_empty() {
            return;
        }

        // Get current presence for the user
        let current_presence = self.storage.get_presence_with_meta(user_id).await.ok().flatten();

        let (presence_state, status_msg, last_active_ts) =
            current_presence.unwrap_or_else(|| ("offline".to_string(), None, None));

        // Build presence EDU following Matrix spec:
        // { "type": "m.presence", "content": { "push": [ ... ] } }
        let push_entry = json!({
            "user_id": user_id,
            "presence": presence_state,
            "status_msg": status_msg,
            "last_active_ts": last_active_ts,
        });

        let presence_edu = json!({
            "type": "m.presence",
            "content": {
                "push": [push_entry]
            }
        });

        // Broadcast EDU to each remote server via federation
        for destination in remote_servers {
            if let Err(e) = broadcaster.broadcast_edu(&destination, &presence_edu, &server_name).await {
                tracing::warn!(
                    destination = %destination,
                    user_id = %user_id,
                    error = %e,
                    "Failed to broadcast presence EDU to federation peer"
                );
            }
        }
    }

    /// See [`add_subscription`].
    #[tracing::instrument(skip(self))]
    pub async fn add_subscription(&self, subscriber_id: &str, target_id: &str) -> ApiResult<()> {
        self.storage
            .add_subscription(subscriber_id, target_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to add presence subscription", e))
    }

    /// See [`remove_subscription`].
    #[tracing::instrument(skip(self))]
    pub async fn remove_subscription(&self, subscriber_id: &str, target_id: &str) -> ApiResult<()> {
        self.storage
            .remove_subscription(subscriber_id, target_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to remove presence subscription", e))
    }

    /// See [`get_subscriptions`].
    #[tracing::instrument(skip(self))]
    pub async fn get_subscriptions(&self, subscriber_id: &str) -> ApiResult<Vec<String>> {
        self.storage
            .get_subscriptions(subscriber_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get subscriptions", e))
    }

    /// See [`get_presence_batch_with_meta`].
    #[tracing::instrument(skip(self))]
    pub async fn get_presence_batch_with_meta(&self, user_ids: &[String]) -> ApiResult<Vec<PresenceBatchRecord>> {
        self.storage
            .get_presence_batch_with_meta(user_ids)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get presence batch", e))
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

    // ── exclude_rooms_from_presence (P2.3) ──────────────────────────

    #[test]
    fn is_room_excluded_returns_false_for_empty_list() {
        let svc = test_service();
        assert!(!svc.is_room_excluded_from_presence("!any:example.com"));
    }

    #[test]
    fn is_room_excluded_returns_true_for_listed_room() {
        let tuning = PresenceTuning {
            excluded_rooms: vec!["!internal:example.com".to_string(), "!lobby:example.com".to_string()],
            ..PresenceTuning::default()
        };
        let svc = PresenceService::with_tuning(Arc::new(InMemoryPresenceStore::new()), tuning);
        assert!(svc.is_room_excluded_from_presence("!internal:example.com"));
        assert!(svc.is_room_excluded_from_presence("!lobby:example.com"));
    }

    #[test]
    fn is_room_excluded_returns_false_for_unlisted_room() {
        let tuning =
            PresenceTuning { excluded_rooms: vec!["!internal:example.com".to_string()], ..PresenceTuning::default() };
        let svc = PresenceService::with_tuning(Arc::new(InMemoryPresenceStore::new()), tuning);
        assert!(!svc.is_room_excluded_from_presence("!other:example.com"));
    }

    // ── Presence tuning accessors (P2.4) ────────────────────────────

    #[test]
    fn tuning_defaults_match_synapse_defaults() {
        let svc = test_service();
        assert_eq!(svc.last_active_granularity(), 120_000);
        assert_eq!(svc.sync_online_timeout(), 30_000);
        assert_eq!(svc.idle_timeout(), 300_000);
    }

    #[test]
    fn tuning_custom_values_are_exposed_via_accessors() {
        let tuning = PresenceTuning {
            excluded_rooms: vec![],
            last_active_granularity: 60_000,
            sync_online_timeout: 15_000,
            idle_timeout: 180_000,
        };
        let svc = PresenceService::with_tuning(Arc::new(InMemoryPresenceStore::new()), tuning);
        assert_eq!(svc.last_active_granularity(), 60_000);
        assert_eq!(svc.sync_online_timeout(), 15_000);
        assert_eq!(svc.idle_timeout(), 180_000);
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
    async fn set_presence_batch_inserts_all_entries() {
        let svc = test_service();
        let entries = vec![
            ("@alice:example.com".to_string(), "online".to_string(), Some("working".to_string())),
            ("@bob:example.com".to_string(), "away".to_string(), None),
            ("@carol:example.com".to_string(), "offline".to_string(), Some("done".to_string())),
        ];
        svc.set_presence_batch(&entries).await.unwrap();

        let alice = svc.get_presence_with_meta("@alice:example.com").await.unwrap().unwrap();
        assert_eq!(alice.0, "online");
        assert_eq!(alice.1.as_deref(), Some("working"));

        let bob = svc.get_presence_with_meta("@bob:example.com").await.unwrap().unwrap();
        assert_eq!(bob.0, "away");
        assert!(bob.1.is_none());

        let carol = svc.get_presence_with_meta("@carol:example.com").await.unwrap().unwrap();
        assert_eq!(carol.0, "offline");
        assert_eq!(carol.1.as_deref(), Some("done"));
    }

    #[tokio::test]
    async fn set_presence_batch_empty_is_noop() {
        let svc = test_service();
        let entries: Vec<(String, String, Option<String>)> = vec![];
        svc.set_presence_batch(&entries).await.unwrap();
        // No error, no panic
    }

    #[tokio::test]
    async fn set_presence_batch_upserts_existing_entries() {
        let svc = test_service();
        // Seed via single set
        svc.set_presence("@alice:example.com", "online", Some("initial")).await.unwrap();

        // Batch upsert
        let entries = vec![
            ("@alice:example.com".to_string(), "offline".to_string(), Some("updated".to_string())),
            ("@bob:example.com".to_string(), "online".to_string(), None),
        ];
        svc.set_presence_batch(&entries).await.unwrap();

        let alice = svc.get_presence_with_meta("@alice:example.com").await.unwrap().unwrap();
        assert_eq!(alice.0, "offline", "should be updated by batch");
        assert_eq!(alice.1.as_deref(), Some("updated"));

        let bob = svc.get_presence_with_meta("@bob:example.com").await.unwrap().unwrap();
        assert_eq!(bob.0, "online");
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

    #[tokio::test]
    async fn extract_remote_servers_filters_local_and_dedups() {
        // 混合订阅者：本地、远程两个、远程一个重复
        let subscribers = vec![
            "@local1:example.com".to_string(),
            "@local2:example.com".to_string(),
            "@remote1:matrix.org".to_string(),
            "@remote2:matrix.org".to_string(),
            "@remote3:matrix.org".to_string(), // 同一 server 重复出现
            "@other:envs.net".to_string(),
        ];

        let servers = super::extract_remote_servers(&subscribers, "example.com");

        // 应排除 example.com，dedup 后剩 matrix.org 和 envs.net
        assert_eq!(servers.len(), 2);
        assert!(servers.contains("matrix.org"));
        assert!(servers.contains("envs.net"));
    }

    #[tokio::test]
    async fn extract_remote_servers_returns_empty_when_all_local() {
        let subscribers = vec!["@local1:example.com".to_string(), "@local2:example.com".to_string()];

        let servers = super::extract_remote_servers(&subscribers, "example.com");
        assert!(servers.is_empty());
    }

    #[tokio::test]
    async fn extract_remote_servers_skips_malformed_user_ids_without_panic() {
        // 不含冒号的异常条目必须被静默跳过（不 panic、不污染结果）
        let subscribers = vec![
            "no-colon-here".to_string(),
            "@valid:remote.server".to_string(),
            "".to_string(),
            "@:empty-local".to_string(), // local_part 为空但仍能 split
        ];

        let servers = super::extract_remote_servers(&subscribers, "example.com");
        // "no-colon-here" 与 "" 没有 server，":" 后是空 local 但 split 仍能切出 "",
        // 我们只关心本地 example.com 被排除 + 远程 remote.server 出现
        assert!(servers.contains("remote.server"));
        assert!(!servers.contains("example.com"));
    }

    #[tokio::test]
    async fn extract_remote_servers_handles_empty_input() {
        let servers = super::extract_remote_servers(&[], "example.com");
        assert!(servers.is_empty());
    }
}
