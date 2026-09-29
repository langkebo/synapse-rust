//! Per-room and global invite blocklist / allowlist policy.
//!
//! Wraps `InviteBlocklistStorage` with `ApiError` mapping so the invite and
//! admin-server routes do not hold a storage handle (B4-5c), and exposes the
//! single `check_invite_allowed` gate that the membership layer enforces.

use std::sync::Arc;
use synapse_common::error::{ApiError, ApiResult};
use synapse_common::metrics::MetricsCollector;
use synapse_storage::account_data::AccountDataStoreApi;
use synapse_storage::invite_blocklist::InviteBlocklistStorage;

/// MSC4155 account-data type carrying a user's own invite policy.
///
/// Content shape:
/// ```json
/// {
///   "default_action": "allow" | "block",
///   "user_exceptions": ["@friend:example.org"],
///   "server_exceptions": ["example.org"]
/// }
/// ```
/// `default_action` decides the outcome for invitees that appear in neither
/// exception list; the exceptions are an override in the allow direction only,
/// which is how MSC4155 describes them.
pub const INVITE_PERMISSION_CONFIG_TYPE: &str = "m.invite_permission_config";

/// Account data type for the user ignore list (MSC3873).
pub const IGNORED_USER_LIST_TYPE: &str = "m.ignored_user_list";

/// Counter for invites rejected by room blocklist / allowlist.
pub const METRIC_INVITE_REJECTED_ROOM: &str = "invite_rejected_room";
/// Counter for invites rejected by the global server-wide blocklist.
pub const METRIC_INVITE_REJECTED_GLOBAL_BLOCK: &str = "invite_rejected_global_block";
/// Counter for invites allowed by the global allowlist.
pub const METRIC_INVITE_ALLOWED_GLOBAL_ALLOW: &str = "invite_allowed_global_allow";
/// Counter for invites rejected by the invitee's MSC4155 account policy.
pub const METRIC_INVITE_REJECTED_ACCOUNT_POLICY: &str = "invite_rejected_account_policy";
/// Counter for invites rejected by the invitee's ignore list (MSC3873).
pub const METRIC_INVITE_REJECTED_IGNORE: &str = "invite_rejected_ignore";
/// Counter for invites that could not be evaluated due to a storage error.
pub const METRIC_INVITE_EVAL_ERROR: &str = "invite_eval_error";

/// The invite-policy gate the membership layer enforces.
///
/// Deliberately narrow: the membership layer must be able to *enforce* policy
/// without holding the admin-facing read/write surface, and unit tests need a
/// double that does not require Postgres. A trait object rather than an
/// `Option<...>` because a gate that can be absent is a gate that can be
/// skipped — this is the one seam every invite entry point goes through.
#[async_trait::async_trait]
pub trait InvitePolicyGate: Send + Sync {
    /// Decide whether `invitee_id` may be invited to `room_id` by `inviter_id`.
    ///
    /// Order:
    /// 1. the room's blocklist / allowlist (one round-trip, see
    ///    [`InviteBlocklistStorage::evaluate`]);
    /// 2. the global server-wide blocklist / allowlist;
    /// 3. the invitee's own MSC4155 `m.invite_permission_config`.
    ///
    /// Fails closed on storage errors. A malformed account-data payload is
    /// treated as "no policy" — the content is user-supplied, and rejecting
    /// invites wholesale because of an unparseable preference would make the
    /// account unreachable rather than merely unfiltered.
    async fn check_invite_allowed(&self, room_id: &str, inviter_id: &str, invitee_id: &str) -> ApiResult<()>;
}

/// Service over the invite blocklist / allowlist tables.
pub struct InviteBlocklistService {
    storage: Arc<InviteBlocklistStorage>,
    /// Backing store for the MSC4155 account-data policy. A remote invitee has
    /// no row here, which is exactly why the account-level policy is
    /// implicitly local-only: we can only know the preferences of users we host.
    account_data_store: Arc<dyn AccountDataStoreApi>,
    /// Optional metrics collector. Presence lets us record rejection counters
    /// without making this service harder to construct in tests.
    metrics: Option<Arc<MetricsCollector>>,
    /// When `true`, DM rooms (rooms the invitee has DM'd the inviter)
    /// skip the global server-wide blocklist/allowlist check.
    /// This implements the opt-out model for DM rooms.
    dm_rooms_bypass_global_policy: bool,
}

impl InviteBlocklistService {
    /// See [`new`].
    pub fn new(storage: Arc<InviteBlocklistStorage>, account_data_store: Arc<dyn AccountDataStoreApi>) -> Self {
        Self { storage, account_data_store, metrics: None, dm_rooms_bypass_global_policy: false }
    }

    /// Attach a metrics collector for recording invite rejection counters.
    pub fn with_metrics(mut self, metrics: Arc<MetricsCollector>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Enable the DM-room bypass for the global server-wide blocklist/allowlist.
    /// When enabled, `check_invite_allowed` will skip the global policy if the
    /// room is a DM room (the invitee has DM'd the inviter).
    pub fn with_dm_rooms_bypass_global_policy(mut self, enabled: bool) -> Self {
        self.dm_rooms_bypass_global_policy = enabled;
        self
    }

    /// Increment the counter for the given metric name.
    fn inc_counter(&self, name: &str) {
        if let Some(metrics) = &self.metrics {
            if let Some(counter) = metrics.get_counter(name) {
                counter.inc();
            } else {
                let counter = metrics.register_counter(name.to_string());
                counter.inc();
            }
        }
    }

    /// `true` when the invitee's own MSC4155 policy refuses this invite.
    ///
    /// Account data is private and never federated, so this server holds a
    /// policy row only for the users it hosts. A missing row therefore means
    /// "no policy set", which is the default allow — for a remote invitee we
    /// cannot know their preferences and must not invent them. Their policy is
    /// enforced by *their* homeserver, which runs this same gate when it
    /// receives the invite. A malformed or `allow`-defaulted payload never
    /// denies either.
    async fn account_policy_denies(&self, inviter_id: &str, invitee_id: &str) -> ApiResult<bool> {
        let content =
            self.account_data_store.get_account_data_content(invitee_id, INVITE_PERMISSION_CONFIG_TYPE).await?;

        let Some(content) = content else {
            return Ok(false);
        };

        if content.get("default_action").and_then(|v| v.as_str()) != Some("block") {
            return Ok(false);
        }

        if content
            .get("user_exceptions")
            .and_then(|v| v.as_array())
            .is_some_and(|exceptions| exceptions.iter().any(|v| v.as_str() == Some(inviter_id)))
        {
            return Ok(false);
        }

        let inviter_server = inviter_id.rsplit_once(':').map(|(_, server)| server);
        if inviter_server.is_some_and(|server| {
            content
                .get("server_exceptions")
                .and_then(|v| v.as_array())
                .is_some_and(|exceptions| exceptions.iter().any(|v| v.as_str() == Some(server)))
        }) {
            return Ok(false);
        }

        Ok(true)
    }

    /// `true` when the invitee has ignored the inviter (MSC3873).
    ///
    /// Checks the invitee's `m.ignored_user_list` account data. Missing or malformed data is treated as no ignore.
    async fn invite_blocked_by_ignore(&self, inviter_id: &str, invitee_id: &str) -> ApiResult<bool> {
        let content = self.account_data_store.get_account_data_content(invitee_id, IGNORED_USER_LIST_TYPE).await?;
        let Some(content) = content else {
            return Ok(false);
        };

        let ignore_list = content.get("ignored_users");
        let Some(arr) = ignore_list.and_then(|v| v.as_array()) else {
            return Ok(false);
        };

        Ok(arr.iter().any(|v| v.as_str() == Some(inviter_id)))
    }

    /// Replace the room's invite blocklist.
    pub async fn set_invite_blocklist(&self, room_id: &str, user_ids: Vec<String>) -> Result<(), ApiError> {
        self.storage
            .set_invite_blocklist(room_id, user_ids)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to set blocklist", e))
    }

    /// Read the room's invite blocklist.
    pub async fn get_invite_blocklist(&self, room_id: &str) -> Result<Vec<String>, ApiError> {
        self.storage
            .get_invite_blocklist(room_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get blocklist", e))
    }

    /// Replace the room's invite allowlist.
    pub async fn set_invite_allowlist(&self, room_id: &str, user_ids: Vec<String>) -> Result<(), ApiError> {
        self.storage
            .set_invite_allowlist(room_id, user_ids)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to set allowlist", e))
    }

    /// Read the room's invite allowlist.
    pub async fn get_invite_allowlist(&self, room_id: &str) -> Result<Vec<String>, ApiError> {
        self.storage
            .get_invite_allowlist(room_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get allowlist", e))
    }

    /// Read the server-wide invite blocklist rows (paginated).
    pub async fn get_global_invite_blocklist(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<serde_json::Value>, ApiError> {
        self.storage
            .get_global_invite_blocklist_paginated(limit, offset)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get global blocklist", e))
    }

    /// Read the total count of server-wide invite blocklist rows.
    pub async fn get_global_invite_blocklist_count(&self) -> Result<i64, ApiError> {
        self.storage
            .global_invite_blocklist_count()
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to count global blocklist", e))
    }

    /// Read the server-wide invite allowlist rows (paginated).
    pub async fn get_global_invite_allowlist(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<serde_json::Value>, ApiError> {
        self.storage
            .get_global_invite_allowlist_paginated(limit, offset)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get global allowlist", e))
    }

    /// Read the total count of server-wide invite allowlist rows.
    pub async fn get_global_invite_allowlist_count(&self) -> Result<i64, ApiError> {
        self.storage
            .global_invite_allowlist_count()
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to count global allowlist", e))
    }

    /// Replace the global invite blocklist with the given users.
    pub async fn set_global_invite_blocklist(&self, user_ids: Vec<String>) -> Result<(), ApiError> {
        self.storage
            .set_global_invite_blocklist(user_ids)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to set global blocklist", e))
    }

    /// Replace the global invite allowlist with the given users.
    pub async fn set_global_invite_allowlist(&self, user_ids: Vec<String>) -> Result<(), ApiError> {
        self.storage
            .set_global_invite_allowlist(user_ids)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to set global allowlist", e))
    }
}

#[async_trait::async_trait]
impl InvitePolicyGate for InviteBlocklistService {
    async fn check_invite_allowed(&self, room_id: &str, inviter_id: &str, invitee_id: &str) -> ApiResult<()> {
        let restriction = match self.storage.evaluate(room_id, invitee_id).await {
            Ok(r) => r,
            Err(e) => {
                self.inc_counter(METRIC_INVITE_EVAL_ERROR);
                return Err(ApiError::internal_with_cause("Failed to evaluate invite restrictions", e));
            }
        };

        if restriction.is_denied() {
            self.inc_counter(METRIC_INVITE_REJECTED_ROOM);
            ::tracing::warn!(
                room_id = %room_id,
                inviter_id = %inviter_id,
                invitee_id = %invitee_id,
                blocked = restriction.blocked,
                allowlist_set = restriction.allowlist_set,
                "Invite rejected by the room invite lists"
            );
            return Err(ApiError::forbidden("This user cannot be invited to this room".to_string()));
        }

        // Global server-wide policy check. A row in the global blocklist means
        // "this user may never be invited anywhere"; a row in the global allowlist
        // means "this user is explicitly permitted everywhere". The allowlist
        // overrides the blocklist on a per-user basis — whichever is present wins.
        if !self.dm_rooms_bypass_global_policy || !self.is_dm_room(inviter_id, invitee_id).await? {
            if self
                .storage
                .is_user_in_global_allowlist(invitee_id)
                .await
                .map_err(|e| ApiError::internal_with_cause("Failed to check global allowlist", e))?
            {
                self.inc_counter(METRIC_INVITE_ALLOWED_GLOBAL_ALLOW);
            } else if self
                .storage
                .is_user_in_global_blocklist(invitee_id)
                .await
                .map_err(|e| ApiError::internal_with_cause("Failed to check global blocklist", e))?
            {
                self.inc_counter(METRIC_INVITE_REJECTED_GLOBAL_BLOCK);
                ::tracing::warn!(
                    room_id = %room_id,
                    inviter_id = %inviter_id,
                    invitee_id = %invitee_id,
                    "Invite rejected by the global server-wide blocklist"
                );
                return Err(ApiError::forbidden("This user is globally blocked from being invited".to_string()));
            }
        }

        if self.account_policy_denies(inviter_id, invitee_id).await? {
            self.inc_counter(METRIC_INVITE_REJECTED_ACCOUNT_POLICY);
            ::tracing::warn!(
                room_id = %room_id,
                inviter_id = %inviter_id,
                invitee_id = %invitee_id,
                "Invite rejected by the invitee's m.invite_permission_config"
            );
            return Err(ApiError::forbidden("This user is not accepting invites".to_string()));
        }

        if self.invite_blocked_by_ignore(inviter_id, invitee_id).await? {
            self.inc_counter(METRIC_INVITE_REJECTED_IGNORE);
            ::tracing::warn!(
                room_id = %room_id,
                inviter_id = %inviter_id,
                invitee_id = %invitee_id,
                "Invite rejected by the invitee's ignore list (MSC3873)"
            );
            return Err(ApiError::forbidden("This user is ignoring you".to_string()));
        }

        Ok(())
    }
}

impl InviteBlocklistService {
    /// `true` when the given pair is already in a DM relationship: the invitee
    /// has an `m.direct` account-data entry mapping the inviter to this room.
    ///
    /// Missing or malformed account data is treated as "no DM relationship",
    /// so this is a best-effort check used only for the DM-bypass gate.
    async fn is_dm_room(&self, inviter_id: &str, invitee_id: &str) -> ApiResult<bool> {
        let content = self.account_data_store.get_account_data_content(invitee_id, "m.direct").await?;
        let Some(content) = content else {
            return Ok(false);
        };
        Ok(content.get(inviter_id).is_some())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use serde_json::json;
    use synapse_storage::test_mocks::InMemoryAccountDataStore;

    /// A service whose room lists are empty, so only the account policy can
    /// produce a verdict. Exercises the MSC4155 parsing without Postgres.
    fn service_with_account_data() -> (InviteBlocklistService, Arc<InMemoryAccountDataStore>) {
        let account_data_store = Arc::new(InMemoryAccountDataStore::new());
        // The room-list storage is unreachable in these tests; the fake pool
        // never connects because `evaluate` is not exercised here.
        let storage = Arc::new(InviteBlocklistStorage::new(Arc::new(
            sqlx::PgPool::connect_lazy("postgresql://unused:unused@127.0.0.1:1/unused").unwrap(),
        )));
        (InviteBlocklistService::new(storage, account_data_store.clone()), account_data_store)
    }

    async fn set_policy(store: &Arc<InMemoryAccountDataStore>, user_id: &str, policy: serde_json::Value) {
        store.upsert_account_data(user_id, INVITE_PERMISSION_CONFIG_TYPE, policy).await.expect("policy write");
    }

    #[tokio::test]
    async fn account_policy_absent_allows_unhosted_users() {
        // We only hold account data for users we host; a remote invitee has no
        // local row, so we defer to their homeserver instead of denying.
        let (svc, _store) = service_with_account_data();
        assert!(!svc
            .account_policy_denies("@inviter:test.localhost", "@nobody:remote.example")
            .await
            .expect("policy read"));
    }

    #[tokio::test]
    async fn account_policy_absent_allows_local_users() {
        // Local users with no m.invite_permission_config should default to allow.
        let (svc, _store) = service_with_account_data();
        assert!(!svc.account_policy_denies("@inviter:localhost", "@nobody:localhost").await.expect("policy read"));
    }

    #[tokio::test]
    async fn account_policy_default_allow_allows() {
        let (svc, store) = service_with_account_data();
        let invitee = "@open:test.localhost";
        set_policy(&store, invitee, json!({"default_action": "allow"})).await;
        assert!(!svc.account_policy_denies("@inviter:test.localhost", invitee).await.expect("policy read"));
    }

    #[tokio::test]
    async fn account_policy_default_block_denies_unlisted() {
        let (svc, store) = service_with_account_data();
        let invitee = "@closed:test.localhost";
        set_policy(&store, invitee, json!({"default_action": "block"})).await;
        assert!(svc.account_policy_denies("@inviter:test.localhost", invitee).await.expect("policy read"));
    }

    #[tokio::test]
    async fn account_policy_user_exception_allows() {
        let (svc, store) = service_with_account_data();
        let invitee = "@closed:test.localhost";
        let inviter = "@friend:test.localhost";
        set_policy(&store, invitee, json!({"default_action": "block", "user_exceptions": [inviter]})).await;
        assert!(!svc.account_policy_denies(inviter, invitee).await.expect("policy read"));
    }

    #[tokio::test]
    async fn account_policy_server_exception_allows() {
        let (svc, store) = service_with_account_data();
        let invitee = "@closed:trusted.example";
        let inviter = "@inviter:trusted.example";
        set_policy(&store, invitee, json!({"default_action": "block", "server_exceptions": ["trusted.example"]})).await;
        assert!(!svc.account_policy_denies(inviter, invitee).await.expect("policy read"));
    }

    #[tokio::test]
    async fn account_policy_server_exception_does_not_match_other_server() {
        let (svc, store) = service_with_account_data();
        let invitee = "@closed:other.example";
        let inviter = "@inviter:other.example";
        set_policy(&store, invitee, json!({"default_action": "block", "server_exceptions": ["trusted.example"]})).await;
        assert!(svc.account_policy_denies(inviter, invitee).await.expect("policy read"));
    }

    #[tokio::test]
    async fn account_policy_malformed_is_not_a_policy() {
        let (svc, store) = service_with_account_data();
        let invitee = "@broken:test.localhost";
        set_policy(&store, invitee, json!({"default_action": 42, "user_exceptions": "nope"})).await;
        assert!(
            !svc.account_policy_denies("@inviter:test.localhost", invitee).await.expect("policy read"),
            "unparseable payload must not lock the account out of all invites"
        );
    }

    async fn set_ignore(store: &Arc<InMemoryAccountDataStore>, user_id: &str, ignored: Vec<&str>) {
        let list = json!({"ignored_users": ignored});
        store.upsert_account_data(user_id, IGNORED_USER_LIST_TYPE, list).await.expect("ignore write");
    }

    #[tokio::test]
    async fn ignore_blocks_invite() {
        let (svc, store) = service_with_account_data();
        let invitee = "@victim:test.localhost";
        let inviter = "@spammer:test.localhost";
        set_ignore(&store, invitee, vec![inviter]).await;
        assert!(svc.invite_blocked_by_ignore(inviter, invitee).await.expect("ignore check"));
    }

    #[tokio::test]
    async fn ignore_allows_when_not_ignored() {
        let (svc, store) = service_with_account_data();
        let invitee = "@victim:test.localhost";
        let inviter = "@friend:test.localhost";
        set_ignore(&store, invitee, vec![]).await;
        assert!(!svc.invite_blocked_by_ignore(inviter, invitee).await.expect("ignore check"));
    }

    /// Metrics test: verify rejection counters are registered and incremented.
    #[tokio::test]
    async fn rejection_counters_are_increased() {
        let metrics = Arc::new(MetricsCollector::new());
        let service = service_with_account_data().0.with_metrics(metrics.clone());

        // Manually increment each counter to validate the plumbing.
        service.inc_counter(METRIC_INVITE_REJECTED_ROOM);
        service.inc_counter(METRIC_INVITE_REJECTED_ACCOUNT_POLICY);
        service.inc_counter(METRIC_INVITE_REJECTED_IGNORE);
        service.inc_counter(METRIC_INVITE_EVAL_ERROR);

        assert_eq!(metrics.get_counter(METRIC_INVITE_REJECTED_ROOM).unwrap().get(), 1);
        assert_eq!(metrics.get_counter(METRIC_INVITE_REJECTED_ACCOUNT_POLICY).unwrap().get(), 1);
        assert_eq!(metrics.get_counter(METRIC_INVITE_REJECTED_IGNORE).unwrap().get(), 1);
        assert_eq!(metrics.get_counter(METRIC_INVITE_EVAL_ERROR).unwrap().get(), 1);
    }

    /// When metrics are not attached (tests / mocks), incrementing counters
    /// is a safe no-op — the service must not panic.
    #[tokio::test]
    async fn counters_are_noop_without_metrics() {
        let (svc, _store) = service_with_account_data();
        svc.inc_counter(METRIC_INVITE_REJECTED_ROOM);
        svc.inc_counter(METRIC_INVITE_EVAL_ERROR);
        // No assertions needed: we just want to ensure no panic occurs.
    }

    /// DM bypass: when enabled, the global blocklist check is skipped if the
    /// invitee has an `m.direct` entry for the inviter.
    #[tokio::test]
    async fn dm_rooms_bypass_global_blocklist_when_enabled() {
        use synapse_storage::test_mocks::InMemoryAccountDataStore;

        let account_data_store = Arc::new(InMemoryAccountDataStore::new());
        let storage = Arc::new(InviteBlocklistStorage::new(Arc::new(
            sqlx::PgPool::connect_lazy("postgresql://unused:unused@127.0.0.1:1/unused").unwrap(),
        )));
        let svc =
            InviteBlocklistService::new(storage, account_data_store.clone()).with_dm_rooms_bypass_global_policy(true);

        // The room-list storage points at an unreachable fake pool, so the
        // gate fails closed on the storage error regardless of DM bypass.
        let result =
            svc.check_invite_allowed("!room:test.localhost", "@alice:test.localhost", "@bob:test.localhost").await;
        assert!(result.is_err());
    }

    /// Test that the DM bypass is correctly detected via m.direct account data.
    #[tokio::test]
    async fn dm_rooms_bypass_detected_via_m_direct() {
        let account_data_store = Arc::new(InMemoryAccountDataStore::new());
        let storage = Arc::new(InviteBlocklistStorage::new(Arc::new(
            sqlx::PgPool::connect_lazy("postgresql://unused:unused@127.0.0.1:1/unused").unwrap(),
        )));
        let svc =
            InviteBlocklistService::new(storage, account_data_store.clone()).with_dm_rooms_bypass_global_policy(true);

        // Set up m.direct mapping: bob has DM'd alice
        account_data_store
            .upsert_account_data(
                "@bob:test.localhost",
                "m.direct",
                json!({"@alice:test.localhost": ["!dm-room:test.localhost"]}),
            )
            .await
            .expect("write m.direct");

        // is_dm_room should return true
        assert!(svc.is_dm_room("@alice:test.localhost", "@bob:test.localhost").await.expect("check dm"));
        // Reverse should be false (alice hasn't DM'd bob)
        assert!(!svc.is_dm_room("@bob:test.localhost", "@alice:test.localhost").await.expect("check dm"));
        // Missing m.direct should be false
        assert!(!svc.is_dm_room("@eve:test.localhost", "@bob:test.localhost").await.expect("check dm"));
    }
}
