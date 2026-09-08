use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::event_notifier::EventNotifier;
use synapse_common::ApiError;
use synapse_storage::event::EventReader;
use synapse_storage::membership::MemberStoreApi;
use synapse_storage::user::{User, UserDirectorySearchResult, UserSearchResult, UserStore};
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
}

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
        }
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

    /// Helper to get member_storage read lock (used in notify_profile_update)
    async fn with_member_storage<F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&Arc<dyn MemberStoreApi>) -> R + Send,
    {
        let guard = self.member_storage.read().unwrap();
        guard.as_ref().map(|ms| f(ms))
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
        ApiError::internal_with_context("Database error", &e)
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

    /// See [`update_profile`].
    /// MSC4204: After updating the profile, notify all shared room users via
    /// sliding sync so they can receive `profile_updates` in their next sync.
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

        Ok(())
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
