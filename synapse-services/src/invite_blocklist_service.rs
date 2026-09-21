//! Per-room and global invite blocklist / allowlist policy.
//!
//! Wraps `InviteBlocklistStorage` with `ApiError` mapping so the invite and
//! admin-server routes do not hold a storage handle (B4-5c), and exposes the
//! single `check_invite_allowed` gate that the membership layer enforces.

use std::sync::Arc;
use synapse_common::error::{ApiError, ApiResult};
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
    /// 2. the invitee's own MSC4155 `m.invite_permission_config`.
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
}

impl InviteBlocklistService {
    /// See [`new`].
    pub fn new(storage: Arc<InviteBlocklistStorage>, account_data_store: Arc<dyn AccountDataStoreApi>) -> Self {
        Self { storage, account_data_store }
    }

    /// `true` when the invitee's own MSC4155 policy refuses this invite.
    ///
    /// A missing, malformed or `allow`-defaulted payload never denies.
    async fn account_policy_denies(&self, invitee_id: &str) -> ApiResult<bool> {
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
            .is_some_and(|exceptions| exceptions.iter().any(|v| v.as_str() == Some(invitee_id)))
        {
            return Ok(false);
        }

        let invitee_server = invitee_id.rsplit_once(':').map(|(_, server)| server);
        if invitee_server.is_some_and(|server| {
            content
                .get("server_exceptions")
                .and_then(|v| v.as_array())
                .is_some_and(|exceptions| exceptions.iter().any(|v| v.as_str() == Some(server)))
        }) {
            return Ok(false);
        }

        Ok(true)
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

    /// Read the server-wide invite blocklist rows.
    pub async fn get_global_invite_blocklist(&self) -> Result<Vec<serde_json::Value>, ApiError> {
        self.storage
            .get_global_invite_blocklist()
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get global blocklist", e))
    }

    /// Read the server-wide invite allowlist rows.
    pub async fn get_global_invite_allowlist(&self) -> Result<Vec<serde_json::Value>, ApiError> {
        self.storage
            .get_global_invite_allowlist()
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get global allowlist", e))
    }
}

#[async_trait::async_trait]
impl InvitePolicyGate for InviteBlocklistService {
    async fn check_invite_allowed(&self, room_id: &str, inviter_id: &str, invitee_id: &str) -> ApiResult<()> {
        let restriction = self
            .storage
            .evaluate(room_id, invitee_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to evaluate invite restrictions", e))?;

        if restriction.is_denied() {
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

        if self.account_policy_denies(invitee_id).await? {
            ::tracing::warn!(
                room_id = %room_id,
                inviter_id = %inviter_id,
                invitee_id = %invitee_id,
                "Invite rejected by the invitee's m.invite_permission_config"
            );
            return Err(ApiError::forbidden("This user is not accepting invites".to_string()));
        }

        Ok(())
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
    async fn account_policy_absent_allows() {
        let (svc, _store) = service_with_account_data();
        assert!(!svc.account_policy_denies("@nobody:test.localhost").await.expect("policy read"));
    }

    #[tokio::test]
    async fn account_policy_default_allow_allows() {
        let (svc, store) = service_with_account_data();
        let user = "@open:test.localhost";
        set_policy(&store, user, json!({"default_action": "allow"})).await;
        assert!(!svc.account_policy_denies(user).await.expect("policy read"));
    }

    #[tokio::test]
    async fn account_policy_default_block_denies_unlisted() {
        let (svc, store) = service_with_account_data();
        let user = "@closed:test.localhost";
        set_policy(&store, user, json!({"default_action": "block"})).await;
        assert!(svc.account_policy_denies(user).await.expect("policy read"));
    }

    #[tokio::test]
    async fn account_policy_user_exception_allows() {
        let (svc, store) = service_with_account_data();
        let user = "@closed:test.localhost";
        set_policy(&store, user, json!({"default_action": "block", "user_exceptions": [user]})).await;
        assert!(!svc.account_policy_denies(user).await.expect("policy read"));
    }

    #[tokio::test]
    async fn account_policy_server_exception_allows() {
        let (svc, store) = service_with_account_data();
        let user = "@closed:trusted.example";
        set_policy(&store, user, json!({"default_action": "block", "server_exceptions": ["trusted.example"]})).await;
        assert!(!svc.account_policy_denies(user).await.expect("policy read"));
    }

    #[tokio::test]
    async fn account_policy_server_exception_does_not_match_other_server() {
        let (svc, store) = service_with_account_data();
        let user = "@closed:other.example";
        set_policy(&store, user, json!({"default_action": "block", "server_exceptions": ["trusted.example"]})).await;
        assert!(svc.account_policy_denies(user).await.expect("policy read"));
    }

    #[tokio::test]
    async fn account_policy_malformed_is_not_a_policy() {
        let (svc, store) = service_with_account_data();
        let user = "@broken:test.localhost";
        set_policy(&store, user, json!({"default_action": 42, "user_exceptions": "nope"})).await;
        assert!(
            !svc.account_policy_denies(user).await.expect("policy read"),
            "unparseable payload must not lock the account out of all invites"
        );
    }
}
