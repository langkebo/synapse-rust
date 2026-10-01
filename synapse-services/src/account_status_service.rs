//! MSC3720 — account status.
//!
//! Implements the domain half of
//! <https://github.com/matrix-org/matrix-spec-proposals/blob/main/proposals/3720-account-status.md>:
//! given a list of user IDs, report `{ exists, deactivated }` for local
//! accounts and fetch remote accounts over federation, collecting the user IDs
//! whose status could not be retrieved.
//!
//! Both wire endpoints (client and federation) are thin adapters over
//! [`AccountStatusService::get_account_statuses`]:
//!
//! * client (`POST /_matrix/client/unstable/org.matrix.msc3720/account_status`)
//!   passes `allow_remote = true`;
//! * federation (`POST /_matrix/federation/unstable/org.matrix.msc3720/account_status`)
//!   passes `allow_remote = false`, so a non-local user is rejected with
//!   `M_INVALID_PARAM` as the MSC requires.
//!
//! Security rule from the MSC: when a remote server answers, every status it
//! returns must belong to **that** remote server. Statuses for other servers
//! are ignored, so a mischievous homeserver cannot overwrite another server's
//! account status.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use synapse_common::config::Config;
use synapse_common::validation::is_well_formed_user_id;
use synapse_federation::client_api::FederationClientApi;
use synapse_storage::user::UserStore;

/// The MSC3720 path for the client-server endpoint (unstable: the MSC has not
/// been stabilised, matching upstream Synapse).
pub const MSC3720_CLIENT_PATH: &str = "/_matrix/client/unstable/org.matrix.msc3720/account_status";

/// The MSC3720 path for the server-server endpoint (unstable).
pub const MSC3720_FEDERATION_PATH: &str = "/_matrix/federation/unstable/org.matrix.msc3720/account_status";

/// One account's status on the wire.
///
/// `deactivated` is omitted when `exists` is `false`, per the MSC.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountStatus {
    /// Whether an account with this user ID exists.
    pub exists: bool,
    /// Whether the account is deactivated. Omitted when `exists` is false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deactivated: Option<bool>,
}

/// Result of an account-status lookup: the statuses that could be retrieved
/// and the user IDs that could not.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AccountStatuses {
    /// Retrieved statuses, keyed by user ID.
    pub statuses: BTreeMap<String, AccountStatus>,
    /// User IDs whose status could not be retrieved.
    pub failures: Vec<String>,
}

/// Errors the account-status service can raise.
#[derive(Debug, thiserror::Error)]
pub enum AccountStatusError {
    /// A supplied user ID is not a syntactically valid Matrix user ID.
    #[error("Not a valid Matrix user ID: {0}")]
    InvalidUserId(String),
    /// A supplied user ID is not local, and remote lookup is not allowed.
    #[error("Not a local user: {0}")]
    NotLocalUser(String),
    /// The local user lookup failed.
    #[error("storage error: {0}")]
    Storage(#[from] sqlx::Error),
}

/// Domain service for MSC3720 account-status lookups.
pub struct AccountStatusService {
    user_store: Arc<dyn UserStore>,
    federation_client: Option<Arc<dyn FederationClientApi>>,
    local_server_names: Vec<String>,
}

/// Shape of a remote homeserver's response to the federation endpoint.
#[derive(Debug, Default, Deserialize)]
struct RemoteAccountStatusResponse {
    #[serde(default)]
    account_statuses: HashMap<String, AccountStatus>,
    #[serde(default)]
    failures: Vec<String>,
}

impl AccountStatusService {
    /// Build a service that can answer for local users and (when a federation
    /// client is supplied) look remote users up over federation.
    pub fn new(
        user_store: Arc<dyn UserStore>,
        federation_client: Arc<dyn FederationClientApi>,
        config: &Config,
    ) -> Self {
        Self { user_store, federation_client: Some(federation_client), local_server_names: local_server_names(config) }
    }

    /// Build a local-only service. Used by the inbound federation endpoint,
    /// which must reject non-local users instead of forwarding them.
    pub fn local_only(user_store: Arc<dyn UserStore>, config: &Config) -> Self {
        Self { user_store, federation_client: None, local_server_names: local_server_names(config) }
    }

    /// Look up the status of every user ID in `user_ids`.
    ///
    /// * Invalid user IDs are rejected with [`AccountStatusError::InvalidUserId`]
    ///   (the MSC's `M_INVALID_PARAM`).
    /// * When `allow_remote` is false, a non-local user ID is rejected with
    ///   [`AccountStatusError::NotLocalUser`] (also `M_INVALID_PARAM`).
    /// * When `allow_remote` is true, remote users are grouped by destination
    ///   and fetched over federation; any failure is reported in `failures`
    ///   rather than failing the whole request.
    pub async fn get_account_statuses(
        &self,
        user_ids: &[String],
        allow_remote: bool,
    ) -> Result<AccountStatuses, AccountStatusError> {
        let mut statuses: BTreeMap<String, AccountStatus> = BTreeMap::new();
        let mut failures: Vec<String> = Vec::new();
        let mut remote_by_destination: BTreeMap<String, Vec<String>> = BTreeMap::new();

        for raw_user_id in user_ids {
            if !is_well_formed_user_id(raw_user_id) {
                return Err(AccountStatusError::InvalidUserId(raw_user_id.clone()));
            }

            if self.is_local(raw_user_id) {
                statuses.insert(raw_user_id.clone(), self.local_status(raw_user_id).await?);
            } else if allow_remote {
                let destination = user_id_domain(raw_user_id).unwrap_or_default();
                remote_by_destination.entry(destination.to_string()).or_default().push(raw_user_id.clone());
            } else {
                return Err(AccountStatusError::NotLocalUser(raw_user_id.clone()));
            }
        }

        for (destination, users) in remote_by_destination {
            let (remote_statuses, remote_failures) = self.remote_statuses(&destination, &users).await;
            statuses.extend(remote_statuses);
            failures.extend(remote_failures);
        }

        Ok(AccountStatuses { statuses, failures })
    }

    /// Whether `user_id` belongs to one of this server's accepted names.
    fn is_local(&self, user_id: &str) -> bool {
        user_id_domain(user_id).is_some_and(|domain| self.local_server_names.iter().any(|name| name == domain))
    }

    /// Status of a local account: `exists`, plus `deactivated` when it exists.
    async fn local_status(&self, user_id: &str) -> Result<AccountStatus, AccountStatusError> {
        match self.user_store.get_user_by_id(user_id).await? {
            Some(user) => Ok(AccountStatus { exists: true, deactivated: Some(user.is_deactivated) }),
            None => Ok(AccountStatus { exists: false, deactivated: None }),
        }
    }

    /// Fetch the statuses of `users` (all on `destination`) over federation.
    ///
    /// Returns `(statuses, failures)`. Every requested user ends up in exactly
    /// one of the two, so callers can satisfy the MSC's "statuses ∪ failures =
    /// requested" invariant.
    async fn remote_statuses(
        &self,
        destination: &str,
        users: &[String],
    ) -> (BTreeMap<String, AccountStatus>, Vec<String>) {
        let mut statuses = BTreeMap::new();
        let mut failures: Vec<String> = Vec::new();

        let Some(client) = self.federation_client.as_ref() else {
            failures.extend(users.iter().cloned());
            return (statuses, failures);
        };

        let response = match client.get_account_status(destination, users).await {
            Ok(response) => response,
            Err(error) => {
                tracing::debug!(
                    destination,
                    %error,
                    "MSC3720: remote account status lookup failed; reporting users as failures"
                );
                failures.extend(users.iter().cloned());
                return (statuses, failures);
            }
        };

        let parsed: RemoteAccountStatusResponse = match serde_json::from_value(response) {
            Ok(parsed) => parsed,
            Err(error) => {
                tracing::debug!(
                    destination,
                    %error,
                    "MSC3720: remote account status response did not match the schema; reporting users as failures"
                );
                failures.extend(users.iter().cloned());
                return (statuses, failures);
            }
        };

        let requested: std::collections::HashSet<&str> = users.iter().map(String::as_str).collect();
        for (user_id, status) in parsed.account_statuses {
            // Security: only accept statuses for users that belong to the
            // remote server we asked, and that we actually requested.
            if user_id_domain(&user_id) != Some(destination) || !requested.contains(user_id.as_str()) {
                tracing::warn!(
                    destination,
                    user_id,
                    "MSC3720: ignoring remote account status for a user that does not belong to the queried server"
                );
                continue;
            }
            statuses.insert(user_id, status);
        }

        for user_id in parsed.failures {
            if requested.contains(user_id.as_str()) && !statuses.contains_key(&user_id) {
                failures.push(user_id);
            }
        }

        // MSC invariant: every requested user is either reported or failed.
        for user_id in users {
            if !statuses.contains_key(user_id) && !failures.iter().any(|failed| failed == user_id) {
                failures.push(user_id.clone());
            }
        }

        (statuses, failures)
    }
}

/// Extract the server-name half of a (well-formed) Matrix user ID.
fn user_id_domain(user_id: &str) -> Option<&str> {
    user_id.rsplit_once(':').map(|(_, domain)| domain).filter(|domain| !domain.is_empty())
}

/// Every server name this deployment accepts as local.
///
/// Mirrors the profile-update federation path: `server.server_name` is the
/// public identity, while `federation.server_name` may carry an additional
/// accepted name in delegated deployments.
fn local_server_names(config: &Config) -> Vec<String> {
    let mut names = vec![config.server.get_server_name().to_string()];
    let federation_name = config.federation.server_name.trim();
    if !federation_name.is_empty() && !names.iter().any(|name| name == federation_name) {
        names.push(federation_name.to_string());
    }
    names.retain(|name| !name.is_empty());
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use synapse_federation::test_mocks::MockFederationClient;
    use synapse_storage::test_mocks::FakeUserStore;

    fn config_with_server_name(name: &str) -> Config {
        let mut config = Config::default();
        config.server.server_name = Some(name.to_string());
        config
    }

    /// Build a service backed by the shared federation mock. An unseeded
    /// destination fails the outbound lookup, which is the "unreachable
    /// remote" path the MSC turns into `failures`.
    fn service_with_mock(mock: Arc<MockFederationClient>, server_name: &str) -> AccountStatusService {
        let store: Arc<dyn UserStore> = Arc::new(FakeUserStore::new());
        let client: Arc<dyn FederationClientApi> = mock;
        AccountStatusService::new(store, client, &config_with_server_name(server_name))
    }

    #[tokio::test]
    async fn local_user_reports_exists_and_deactivated() {
        let store: Arc<dyn UserStore> = Arc::new(FakeUserStore::new());
        // FakeUserStore seeds @alice:example.com as an active user.
        let service = AccountStatusService::local_only(store, &config_with_server_name("example.com"));

        let result = service
            .get_account_statuses(&["@alice:example.com".to_string()], false)
            .await
            .expect("local lookup should succeed");

        assert_eq!(
            result.statuses.get("@alice:example.com"),
            Some(&AccountStatus { exists: true, deactivated: Some(false) })
        );
        assert!(result.failures.is_empty());
    }

    #[tokio::test]
    async fn unknown_local_user_reports_not_exists_without_deactivated() {
        let store: Arc<dyn UserStore> = Arc::new(FakeUserStore::new());
        let service = AccountStatusService::local_only(store, &config_with_server_name("example.com"));

        let result = service
            .get_account_statuses(&["@ghost:example.com".to_string()], false)
            .await
            .expect("local lookup should succeed");

        assert_eq!(
            result.statuses.get("@ghost:example.com"),
            Some(&AccountStatus { exists: false, deactivated: None })
        );
    }

    #[tokio::test]
    async fn malformed_user_id_is_rejected() {
        let service =
            AccountStatusService::local_only(Arc::new(FakeUserStore::new()), &config_with_server_name("example.com"));

        let error = service
            .get_account_statuses(&["not-a-user".to_string()], false)
            .await
            .expect_err("malformed user id must be rejected");

        assert!(matches!(error, AccountStatusError::InvalidUserId(id) if id == "not-a-user"));
    }

    #[tokio::test]
    async fn remote_user_is_rejected_when_remote_lookup_is_disallowed() {
        let service =
            AccountStatusService::local_only(Arc::new(FakeUserStore::new()), &config_with_server_name("example.com"));

        let error = service
            .get_account_statuses(&["@bob:remote.example".to_string()], false)
            .await
            .expect_err("remote user must be rejected when allow_remote is false");

        assert!(matches!(error, AccountStatusError::NotLocalUser(id) if id == "@bob:remote.example"));
    }

    #[tokio::test]
    async fn remote_users_are_grouped_by_destination_and_reported() {
        let mock = Arc::new(MockFederationClient::new("example.com"));
        mock.seed_account_status(
            "remote.example",
            serde_json::json!({
                "account_statuses": {
                    "@bob:remote.example": { "exists": true, "deactivated": true },
                },
                "failures": [],
            }),
        )
        .await;
        let service = service_with_mock(mock.clone(), "example.com");

        let result = service
            .get_account_statuses(&["@bob:remote.example".to_string(), "@carol:remote.example".to_string()], true)
            .await
            .expect("remote lookup should succeed");

        assert_eq!(
            result.statuses.get("@bob:remote.example"),
            Some(&AccountStatus { exists: true, deactivated: Some(true) })
        );
        // The remote server did not report carol -> she must be a failure.
        assert_eq!(result.failures, vec!["@carol:remote.example".to_string()]);
        let calls = mock.account_status_calls().await;
        assert_eq!(calls.len(), 1, "one request per destination");
        assert_eq!(calls[0].0, "remote.example");
        assert_eq!(calls[0].1.len(), 2);
    }

    #[tokio::test]
    async fn statuses_for_another_server_are_ignored() {
        let mock = Arc::new(MockFederationClient::new("example.com"));
        mock.seed_account_status(
            "remote.example",
            serde_json::json!({
                "account_statuses": {
                    "@bob:remote.example": { "exists": true, "deactivated": false },
                    "@victim:example.com": { "exists": false },
                },
                "failures": [],
            }),
        )
        .await;
        let service = service_with_mock(mock, "example.com");

        let result = service
            .get_account_statuses(&["@bob:remote.example".to_string()], true)
            .await
            .expect("remote lookup should succeed");

        assert!(result.statuses.contains_key("@bob:remote.example"));
        assert!(
            !result.statuses.contains_key("@victim:example.com"),
            "a remote server must not be able to report statuses for other servers"
        );
    }

    #[tokio::test]
    async fn federation_failure_reports_every_user_as_failure() {
        // No seeded response for the destination -> the mock returns an error.
        let mock = Arc::new(MockFederationClient::new("example.com"));
        let service = service_with_mock(mock, "example.com");

        let result = service
            .get_account_statuses(&["@bob:remote.example".to_string()], true)
            .await
            .expect("a federation failure must not fail the whole request");

        assert!(result.statuses.is_empty());
        assert_eq!(result.failures, vec!["@bob:remote.example".to_string()]);
    }

    #[tokio::test]
    async fn empty_request_returns_empty_result() {
        let service =
            AccountStatusService::local_only(Arc::new(FakeUserStore::new()), &config_with_server_name("example.com"));

        let result = service.get_account_statuses(&[], true).await.expect("empty request should succeed");

        assert!(result.statuses.is_empty());
        assert!(result.failures.is_empty());
    }
}
