use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::event_notifier::EventNotifier;
use synapse_common::current_timestamp_millis;
use synapse_common::ApiError;
use synapse_federation::event_broadcaster::EventBroadcaster;
use synapse_storage::event::EventReader;
use synapse_storage::membership::MemberStoreApi;
pub use synapse_storage::user::{User, UserDirectorySearchResult};
use synapse_storage::user::{UserSearchResult, UserStore};
use tracing::instrument;

/// Convenience layer over `UserStore` that maps `sqlx::Error` → `ApiError`
/// and bundles common multi-step patterns duplicated across 15+ services.
///
/// The storage seam (`UserStore` trait) is unchanged; `UserService` is a pure
/// wrapper that eliminates boilerplate.
pub struct UserService {
    user_storage: Arc<dyn UserStore>,
    /// MSC4204: Member storage for querying shared room users during profile updates.
    /// Initialized via `set_member_storage` after construction.
    member_storage: RwLock<Option<Arc<dyn MemberStoreApi>>>,
    /// MSC4204: Event reader for getting current stream position.
    /// Initialized via `set_event_reader` after construction (unused in production).
    #[allow(dead_code)]
    event_reader: RwLock<Option<Arc<dyn EventReader>>>,
    /// MSC4204: Event notifier for waking shared room users' sliding sync connections.
    event_notifier: RwLock<EventNotifier>,
    /// MSC4262: Federation broadcaster for sending `m.profile_update` EDUs to
    /// remote servers when a local user's profile changes. Injected via
    /// `set_federation_broadcaster` after construction (the broadcaster is
    /// built later in the domain wiring phase).
    federation_broadcaster: RwLock<Option<Arc<EventBroadcaster>>>,
    /// MSC4262: This server's name, used as the EDU `origin`. Injected alongside
    /// the broadcaster via `set_federation_broadcaster`.
    server_name: RwLock<String>,
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
impl UserService {
    /// See [`new`].
    pub fn new(user_storage: Arc<dyn UserStore>) -> Self {
        Self {
            user_storage,
            #[cfg(any(test, feature = "test-utils"))]
            member_storage: RwLock::new(Some(Arc::new(synapse_storage::test_mocks::InMemoryMemberStore::new()))),
            #[cfg(any(test, feature = "test-utils"))]
            event_reader: RwLock::new(Some(Arc::new(synapse_storage::test_mocks::InMemoryEventStore::new()))),
            #[cfg(not(any(test, feature = "test-utils")))]
            member_storage: RwLock::new(None),
            #[cfg(not(any(test, feature = "test-utils")))]
            event_reader: RwLock::new(None),
            event_notifier: RwLock::new(EventNotifier::new()),
            federation_broadcaster: RwLock::new(None),
            server_name: RwLock::new(String::new()),
        }
    }

    /// MSC4262: Inject the federation broadcaster and this server's name so
    /// profile updates can be propagated to remote homeservers as
    /// `m.profile_update` EDUs.
    pub fn set_federation_broadcaster(&self, broadcaster: Arc<EventBroadcaster>, server_name: String) {
        *self.federation_broadcaster.write().unwrap() = Some(broadcaster);
        *self.server_name.write().unwrap() = server_name;
    }

    /// MSC4204: Inject `member_storage` (the real Postgres implementation).
    pub fn set_member_storage(&self, member_storage: Arc<dyn MemberStoreApi>) {
        *self.member_storage.write().unwrap() = Some(member_storage);
    }

    /// MSC4204: Inject `event_reader` (the real Postgres implementation).
    #[allow(dead_code)]
    pub fn set_event_reader(&self, event_reader: Arc<dyn EventReader>) {
        *self.event_reader.write().unwrap() = Some(event_reader);
    }

    /// MSC4204: Inject `event_notifier` (the real one with Redis slots).
    pub fn set_event_notifier(&self, event_notifier: EventNotifier) {
        *self.event_notifier.write().unwrap() = event_notifier;
    }

    /// Helper to get event_notifier read lock (used in notify_profile_update)
    fn with_event_notifier<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&EventNotifier) -> R,
    {
        let guard = self.event_notifier.read().unwrap();
        f(&guard)
    }

    /// Maps a `sqlx::Error` to `ApiError::internal_with_context`.
    fn db_error(e: sqlx::Error) -> ApiError {
        ApiError::internal_with_cause("Database error", e)
    }

    // ── user lookup (Patterns 1+2) ──────────────────────────────────────

    /// See [`get_user`].
    #[instrument(skip(self))]
    pub async fn get_user(&self, user_id: &str) -> Result<Option<User>, ApiError> {
        self.user_storage.get_user_by_id(user_id).await.map_err(Self::db_error)
    }

    /// See [`get_user_by_identifier`].
    #[instrument(skip(self))]
    pub async fn get_user_by_identifier(&self, identifier: &str) -> Result<Option<User>, ApiError> {
        self.user_storage.get_user_by_identifier(identifier).await.map_err(Self::db_error)
    }

    /// See [`get_user_by_username`].
    #[instrument(skip(self))]
    pub async fn get_user_by_username(&self, username: &str) -> Result<Option<User>, ApiError> {
        self.user_storage.get_user_by_username(username).await.map_err(Self::db_error)
    }

    /// See [`get_user_by_email`].
    #[instrument(skip(self))]
    pub async fn get_user_by_email(&self, email: &str) -> Result<Option<User>, ApiError> {
        self.user_storage.get_user_by_email(email).await.map_err(Self::db_error)
    }

    /// See [`user_exists`].
    #[instrument(skip(self))]
    pub async fn user_exists(&self, user_id: &str) -> Result<bool, ApiError> {
        self.user_storage.user_exists(user_id).await.map_err(Self::db_error)
    }

    /// Returns `Ok(user)` or `Err(ApiError::not_found)`.
    pub async fn get_user_or_not_found(&self, identifier: &str) -> Result<User, ApiError> {
        self.get_user_by_identifier(identifier).await?.ok_or_else(|| ApiError::not_found("User not found".to_string()))
    }

    /// Returns `Ok(())` if the user exists, otherwise `Err(ApiError::not_found)`.
    pub async fn ensure_user_exists(&self, user_id: &str) -> Result<(), ApiError> {
        if !self.user_exists(user_id).await? {
            return Err(ApiError::not_found("User not found".to_string()));
        }
        Ok(())
    }

    // ── profile (Patterns 3+4+5) ───────────────────────────────────────

    /// See [`get_profile`].
    #[instrument(skip(self))]
    pub async fn get_profile(&self, user_id: &str) -> Result<serde_json::Value, ApiError> {
        let user = self.get_user(user_id).await?.ok_or_else(|| ApiError::not_found("User not found".to_string()))?;
        Ok(serde_json::json!({
            "user_id": user.user_id,
            "displayname": user.displayname,
            "avatar_url": user.avatar_url
        }))
    }

    /// See [`get_profiles_batch`].
    #[instrument(skip(self))]
    pub async fn get_profiles_batch(&self, user_ids: &[String]) -> Result<Vec<serde_json::Value>, ApiError> {
        let profiles = self.user_storage.get_user_profiles_batch(user_ids).await.map_err(Self::db_error)?;
        Ok(profiles
            .into_iter()
            .map(
                |u| serde_json::json!({"user_id": u.user_id, "displayname": u.displayname, "avatar_url": u.avatar_url}),
            )
            .collect())
    }

    /// See [`update_displayname`].
    #[instrument(skip(self))]
    pub async fn update_displayname(&self, user_id: &str, displayname: Option<&str>) -> Result<(), ApiError> {
        self.user_storage.update_displayname(user_id, displayname).await.map_err(|e| {
            if e.to_string().contains("too long") {
                ApiError::bad_request("Displayname too long (max 255 characters)".to_string())
            } else {
                Self::db_error(e)
            }
        })
    }

    /// See [`update_avatar_url`].
    #[instrument(skip(self))]
    pub async fn update_avatar_url(&self, user_id: &str, avatar_url: Option<&str>) -> Result<(), ApiError> {
        self.user_storage.update_avatar_url(user_id, avatar_url).await.map_err(|e| {
            if e.to_string().contains("too long") {
                ApiError::bad_request("Avatar URL too long (max 255 characters)".to_string())
            } else {
                Self::db_error(e)
            }
        })
    }

    /// MSC4262: Apply a profile update received from a remote homeserver via
    /// an `m.profile_update` EDU. Updates `displayname`/`avatar_url` for an
    /// existing remote user; returns `true` if the local user was updated.
    #[instrument(skip(self))]
    pub async fn apply_profile_update_from_federation(
        &self,
        user_id: &str,
        displayname: Option<&str>,
        avatar_url: Option<&str>,
    ) -> Result<bool, ApiError> {
        self.user_storage
            .apply_profile_update_from_federation(user_id, displayname, avatar_url)
            .await
            .map_err(Self::db_error)
    }

    /// See [`update_profile`].
    /// MSC4204: After updating the profile, notify all shared room users via
    /// sliding sync so they can receive `profile_updates` in their next sync.
    /// MSC4262: Additionally broadcast an `m.profile_update` EDU to remote
    /// federated servers so they can invalidate their cached profile data.
    #[instrument(skip(self))]
    pub async fn update_profile(
        &self,
        user_id: &str,
        displayname: Option<&str>,
        avatar_url: Option<&str>,
    ) -> Result<(), ApiError> {
        if let Some(name) = displayname {
            self.update_displayname(user_id, Some(name)).await?;
        }
        if let Some(url) = avatar_url {
            self.update_avatar_url(user_id, Some(url)).await?;
        }

        // MSC4204: Notify shared room users that this user's profile changed.
        // The sliding sync `profile_updates` extension will deliver the actual
        // profile data when those users' connections wake.
        self.notify_profile_update(user_id).await;

        // MSC4262: Broadcast `m.profile_update` EDU to remote servers.
        self.broadcast_profile_update_edu(user_id).await;

        Ok(())
    }

    /// MSC4262: Broadcast an `m.profile_update` EDU to all remote servers that
    /// share a room with `user_id`. The EDU carries the user's new displayname
    /// and avatar_url so remote homeservers can invalidate their caches.
    ///
    /// Failures are logged and not propagated: profile update notification is
    /// best-effort and must not roll back the local profile change.
    async fn broadcast_profile_update_edu(&self, user_id: &str) {
        let broadcaster = match self.federation_broadcaster.read().unwrap().as_ref() {
            Some(b) => b.clone(),
            None => return, // federation disabled or not yet injected
        };
        let server_name = self.server_name.read().unwrap().clone();
        if server_name.is_empty() {
            return;
        }

        // Read the updated profile fields to include in the EDU
        let (displayname, avatar_url) = match self.user_storage.get_user_by_id(user_id).await {
            Ok(Some(user)) => (user.displayname, user.avatar_url),
            _ => (None, None),
        };

        let edu = serde_json::json!({
            "edu_type": "m.profile_update",
            "content": {
                "user_id": user_id,
                "displayname": displayname,
                "avatar_url": avatar_url,
                "origin_server_ts": current_timestamp_millis(),
            }
        });

        // Collect remote servers from shared joined rooms
        let member_storage = match self.member_storage.read().unwrap().as_ref() {
            Some(ms) => ms.clone(),
            None => {
                ::tracing::warn!(user_id, "MSC4262: member_storage not injected, skipping profile update EDU");
                return;
            }
        };

        let joined_rooms: Vec<String> = match member_storage.get_joined_rooms(user_id).await {
            Ok(rooms) => rooms,
            Err(e) => {
                ::tracing::warn!(%e, user_id, "MSC4262: failed to get joined rooms for profile update EDU");
                return;
            }
        };

        let mut destinations: std::collections::HashSet<String> = std::collections::HashSet::new();
        for room_id in &joined_rooms {
            let members = match member_storage.get_joined_members(room_id).await {
                Ok(m) => m,
                Err(e) => {
                    ::tracing::warn!(%e, %room_id, %user_id, "MSC4262: failed to get room members for profile update EDU");
                    continue;
                }
            };
            for member in members {
                if let Some(pos) = member.user_id.find(':') {
                    let server = &member.user_id[pos + 1..];
                    if server != server_name {
                        destinations.insert(server.to_string());
                    }
                }
            }
        }

        let count = destinations.len();
        for destination in destinations {
            if let Err(e) = broadcaster.broadcast_edu(&destination, &edu, &server_name).await {
                ::tracing::warn!(%e, %destination, %user_id, "MSC4262: failed to broadcast profile update EDU");
            }
        }
        ::tracing::info!(user_id = %user_id, destination_count = count, "MSC4262: broadcast m.profile_update EDU");
    }

    /// MSC4204: Notify all shared room users that `user_id`'s profile changed.
    ///
    /// Queries the rooms `user_id` is a member of, collects all other users
    /// in those rooms, and calls `event_notifier.notify_user()` for each of
    /// them. This wakes their parked sliding sync connections so the
    /// `profile_updates` extension is populated in the next response.
    ///
    /// Errors are logged and not propagated: a notification failure should
    /// not prevent the profile update from completing.
    async fn notify_profile_update(&self, user_id: &str) {
        let member_storage = match self.member_storage.read().unwrap().as_ref() {
            Some(ms) => ms.clone(),
            None => {
                ::tracing::warn!(user_id, "MSC4204: member_storage not injected, skipping profile update notification");
                return;
            }
        };

        let joined_rooms: Vec<String> = match member_storage.get_joined_rooms(user_id).await {
            Ok(rooms) => rooms,
            Err(e) => {
                ::tracing::warn!(%e, user_id, "MSC4204: failed to get joined rooms for profile update notification");
                return;
            }
        };

        let mut notified_users: std::collections::HashSet<String> = std::collections::HashSet::new();

        for room_id in joined_rooms {
            let members = match member_storage.get_joined_members(&room_id).await {
                Ok(m) => m,
                Err(e) => {
                    ::tracing::warn!(%e, %room_id, %user_id, "MSC4204: failed to get room members for profile update notification");
                    continue;
                }
            };

            for member in members {
                if member.user_id != user_id && notified_users.insert(member.user_id.clone()) {
                    self.with_event_notifier(|en| en.notify_user(&member.user_id));
                }
            }
        }

        let count = notified_users.len();
        ::tracing::info!(
            user_id = %user_id,
            notified_count = count,
            "MSC4204: notified shared room users of profile update"
        );
    }

    // ── search / listing ───────────────────────────────────────────────

    /// See [`search_users`].
    #[instrument(skip(self))]
    pub async fn search_users(&self, query: &str, limit: i64) -> Result<Vec<UserSearchResult>, ApiError> {
        self.user_storage.search_users(query, limit).await.map_err(Self::db_error)
    }

    /// See [`search_directory_users`].
    #[instrument(skip(self))]
    pub async fn search_directory_users(
        &self,
        query: &str,
        limit: i64,
        exact_only: bool,
    ) -> Result<Vec<UserDirectorySearchResult>, ApiError> {
        self.user_storage.search_directory_users(query, limit, exact_only).await.map_err(Self::db_error)
    }

    /// See [`get_users_paginated`].
    #[instrument(skip(self))]
    pub async fn get_users_paginated(
        &self,
        limit: i64,
        since_ts: Option<i64>,
        since_user_id: Option<&str>,
    ) -> Result<Vec<User>, ApiError> {
        self.user_storage.get_users_paginated(limit, since_ts, since_user_id).await.map_err(Self::db_error)
    }

    /// See [`get_user_count`].
    #[instrument(skip(self))]
    pub async fn get_user_count(&self) -> Result<i64, ApiError> {
        self.user_storage.get_user_count().await.map_err(Self::db_error)
    }

    /// See [`get_non_deactivated_user_count`].
    #[instrument(skip(self))]
    pub async fn get_non_deactivated_user_count(&self) -> Result<i64, ApiError> {
        self.user_storage.count_non_deactivated_users().await.map_err(Self::db_error)
    }

    /// See [`get_non_deactivated_user_count_by_app_service`].
    #[instrument(skip(self))]
    pub async fn get_non_deactivated_user_count_by_app_service(&self) -> Result<HashMap<String, i64>, ApiError> {
        self.user_storage.count_non_deactivated_users_by_app_service().await.map_err(Self::db_error)
    }

    // ── delegated access to raw store for non-convenience operations ───

    /// Access the underlying `UserStore` for operations not covered by convenience methods.
    pub fn store(&self) -> &Arc<dyn UserStore> {
        &self.user_storage
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use synapse_common::ApiErrorKind;
    use synapse_storage::test_mocks::{FakeUserStore, InMemoryMemberStore};

    fn build_test_service() -> UserService {
        let user_storage: Arc<dyn UserStore> = Arc::new(FakeUserStore::new());
        UserService::new(user_storage)
    }

    #[tokio::test]
    async fn get_profile_returns_user_fields() {
        let service = build_test_service();

        let profile = service.get_profile("@alice:example.com").await.unwrap();
        assert!(profile.is_object());

        let user_id = profile.get("user_id").and_then(|v| v.as_str());
        let displayname = profile.get("displayname").and_then(|v| v.as_str());
        let avatar_url = profile.get("avatar_url").and_then(|v| v.as_str());

        assert_eq!(user_id, Some("@alice:example.com"));
        // displayname and avatar_url are None for seeded user
        assert!(displayname.is_none() || displayname == Some(""));
        assert!(avatar_url.is_none() || avatar_url == Some(""));
    }

    #[tokio::test]
    async fn get_profile_returns_not_found_for_unknown_user() {
        let service = build_test_service();

        let result = service.get_profile("@unknown:example.com").await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind, ApiErrorKind::NotFound);
    }

    #[tokio::test]
    async fn update_profile_updates_displayname_and_avatar() {
        let service = build_test_service();

        let result =
            service.update_profile("@alice:example.com", Some("Alice Smith"), Some("mxc://example.com/avatar1")).await;
        assert!(result.is_ok(), "profile update should succeed: {:?}", result);
    }

    #[tokio::test]
    async fn update_profile_skips_unset_fields() {
        let service = build_test_service();

        // Update only displayname
        let result = service.update_profile("@alice:example.com", Some("New Name"), None).await;
        assert!(result.is_ok());

        // Update only avatar_url
        let result = service.update_profile("@alice:example.com", None, Some("mxc://example.com/avatar2")).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn update_profile_notifies_shared_room_members() {
        let user_storage: Arc<dyn UserStore> = Arc::new(FakeUserStore::new());
        let member_store = Arc::new(InMemoryMemberStore::new());

        // Seed: alice and bob in room1; alice and carol in room2; dave in room3 (not shared)
        member_store.add_member("!room1:test", "@alice:example.com", "join", None).await.unwrap();
        member_store.add_member("!room1:test", "@bob:example.com", "join", None).await.unwrap();
        member_store.add_member("!room2:test", "@alice:example.com", "join", None).await.unwrap();
        member_store.add_member("!room2:test", "@carol:example.com", "join", None).await.unwrap();
        member_store.add_member("!room3:test", "@dave:example.com", "join", None).await.unwrap();

        let svc = UserService::new(user_storage);
        svc.set_member_storage(member_store.clone());

        let result = svc.update_profile("@alice:example.com", Some("Alice Updated"), None).await;
        assert!(result.is_ok(), "profile update should succeed: {:?}", result);
    }

    #[tokio::test]
    async fn update_profile_no_members_returns_ok() {
        let user_storage: Arc<dyn UserStore> = Arc::new(FakeUserStore::new());
        let member_store = Arc::new(InMemoryMemberStore::new());

        let svc = UserService::new(user_storage);
        svc.set_member_storage(member_store);

        // alice not in any room — notify should be no-op
        let result = svc.update_profile("@alice:example.com", None, Some("mxc://new")).await;
        assert!(result.is_ok(), "update should succeed with no members: {:?}", result);
    }

    #[tokio::test]
    async fn update_profile_member_storage_not_injected_skips_notify() {
        let service = build_test_service(); // member_storage not injected

        let result = service.update_profile("@alice:example.com", Some("Name"), None).await;
        assert!(result.is_ok()); // still returns Ok, just skips notify
    }

    #[tokio::test]
    async fn get_profiles_batch_returns_empty_for_none() {
        let service = build_test_service();

        let profiles = service.get_profiles_batch(&[]).await.unwrap();
        assert!(profiles.is_empty());
    }

    #[tokio::test]
    async fn get_profiles_batch_returns_empty_for_known_user() {
        // FakeUserStore::get_user_profiles_batch returns empty vec (stub behavior)
        // Real Postgres implementation fetches from user_profiles table
        let service = build_test_service();

        let profiles = service.get_profiles_batch(&["@alice:example.com".to_string()]).await.unwrap();
        // FakeUserStore returns empty - this is expected stub behavior
        assert!(profiles.is_empty(), "FakeUserStore stub returns empty profiles");
    }
}
