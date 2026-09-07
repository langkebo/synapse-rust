use futures::stream::{self, StreamExt};
use sqlx::PgPool;
use std::sync::Arc;
use synapse_common::crypto::{hash_password, random_string};
use synapse_common::error::ApiError;
use synapse_storage::device::DeviceListStoreApi;
use synapse_storage::{RoomStoreApi, User, UserStore};
use tracing::instrument;

/// The `AdminUserCursor` struct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminUserCursor {
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `user_id` field.
    pub user_id: String,
}

/// See [`decode_user_cursor`].
pub fn decode_user_cursor(cursor: Option<&str>) -> Option<AdminUserCursor> {
    let cursor = cursor?;
    let (created_ts, user_id) = cursor.split_once('|')?;
    let created_ts = created_ts.parse::<i64>().ok()?;
    if user_id.is_empty() {
        return None;
    }
    Some(AdminUserCursor { created_ts, user_id: user_id.to_owned() })
}

/// See [`encode_user_cursor`].
pub fn encode_user_cursor(cursor: &AdminUserCursor) -> String {
    format!("{}|{}", cursor.created_ts, cursor.user_id)
}

/// The `AdminUserListItem` struct.
#[derive(Debug, Clone)]
pub struct AdminUserListItem {
    /// The `user_id` field.
    pub user_id: String,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `is_admin` field.
    pub is_admin: bool,
    /// The `is_guest` field.
    pub is_guest: bool,
    /// The `user_type` field.
    pub user_type: Option<String>,
    /// The `is_deactivated` field.
    pub is_deactivated: bool,
    /// The `displayname` field.
    pub displayname: Option<String>,
    /// The `avatar_url` field.
    pub avatar_url: Option<String>,
}

/// The `AdminUserDeviceInfo` struct.
#[derive(Debug, Clone)]
pub struct AdminUserDeviceInfo {
    /// The `device_id` field.
    pub device_id: String,
    /// The `display_name` field.
    pub display_name: Option<String>,
    /// The `last_seen_ts` field.
    pub last_seen_ts: Option<i64>,
    /// The `last_seen_ip` field.
    pub last_seen_ip: Option<String>,
}

/// The `AdminUsersPage` struct.
#[derive(Debug, Clone)]
pub struct AdminUsersPage {
    /// The `users` field.
    pub users: Vec<AdminUserListItem>,
    /// The `total` field.
    pub total: i64,
    /// The `next_token` field.
    pub next_token: Option<String>,
}

/// The `AdminUserProfile` struct.
#[derive(Debug, Clone)]
pub struct AdminUserProfile {
    /// The `user_id` field.
    pub user_id: String,
    /// The `username` field.
    pub username: String,
    /// The `is_admin` field.
    pub is_admin: bool,
    /// The `is_guest` field.
    pub is_guest: bool,
    /// The `is_deactivated` field.
    pub is_deactivated: bool,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `displayname` field.
    pub displayname: Option<String>,
    /// The `avatar_url` field.
    pub avatar_url: Option<String>,
    /// The `user_type` field.
    pub user_type: Option<String>,
}

impl From<&User> for AdminUserProfile {
    fn from(user: &User) -> Self {
        Self {
            user_id: user.user_id.clone(),
            username: user.username.clone(),
            is_admin: user.is_admin,
            is_guest: user.is_guest,
            is_deactivated: user.is_deactivated,
            created_ts: user.created_ts,
            displayname: user.displayname.clone(),
            avatar_url: user.avatar_url.clone(),
            user_type: user.user_type.clone(),
        }
    }
}

/// The `AdminUserDetails` struct.
#[derive(Debug, Clone)]
pub struct AdminUserDetails {
    /// The `user` field.
    pub user: AdminUserProfile,
    /// The `devices` field.
    pub devices: Vec<AdminUserDeviceInfo>,
}

/// The `AdminLegacyUsersPage` struct.
#[derive(Debug, Clone)]
pub struct AdminLegacyUsersPage {
    /// The `users` field.
    pub users: Vec<User>,
    /// The `total` field.
    pub total: i64,
}

/// The `AdminEvictionFailure` struct.
#[derive(Debug, Clone)]
pub struct AdminEvictionFailure {
    /// The `room_id` field.
    pub room_id: String,
    /// The `error` field.
    pub error: String,
}

/// The `AdminUserEvictionResult` struct.
#[derive(Debug, Clone)]
pub struct AdminUserEvictionResult {
    /// The `joined_rooms` field.
    pub joined_rooms: Vec<String>,
    /// The `failures` field.
    pub failures: Vec<AdminEvictionFailure>,
}

/// The `AdminUserStats` struct.
#[derive(Debug, Clone)]
pub struct AdminUserStats {
    /// The `total_users` field.
    pub total_users: i64,
    /// The `active_users` field.
    pub active_users: i64,
    /// The `admin_users` field.
    pub admin_users: i64,
    /// The `deactivated_users` field.
    pub deactivated_users: i64,
    /// The `guest_users` field.
    pub guest_users: i64,
    /// The `average_rooms_per_user` field.
    pub average_rooms_per_user: f64,
}

/// The `AdminSingleUserStats` struct.
#[derive(Debug, Clone)]
pub struct AdminSingleUserStats {
    /// The `user` field.
    pub user: AdminUserProfile,
    /// The `rooms_joined` field.
    pub rooms_joined: i64,
    /// The `messages_sent` field.
    pub messages_sent: i64,
    /// The `last_seen_ts` field.
    pub last_seen_ts: Option<i64>,
}

/// The `BatchUsersResult` struct.
#[derive(Debug, Clone)]
pub struct BatchUsersResult {
    /// The `succeeded` field.
    pub succeeded: Vec<String>,
    /// The `failed` field.
    pub failed: Vec<String>,
}

/// The `AdminUserService` struct.
pub struct AdminUserService {
    user_service: Arc<crate::UserService>,
    user_storage: Arc<dyn UserStore>,
    device_storage: Arc<dyn DeviceListStoreApi>,
    room_storage: Arc<dyn RoomStoreApi>,
    member_storage: Arc<dyn synapse_storage::membership::MemberStoreApi>,
    server_name: String,
    /// Maximum concurrent room removals per evict batch. 0 is coerced to 1.
    evict_max_concurrency: usize,
    /// Page size for paginated room list traversal.
    evict_page_size: i64,
}

impl AdminUserService {
    /// See [`new`].
    pub fn new(
        _pool: Arc<PgPool>,
        user_service: Arc<crate::UserService>,
        user_storage: Arc<dyn UserStore>,
        device_storage: Arc<dyn DeviceListStoreApi>,
        room_storage: Arc<dyn RoomStoreApi>,
        member_storage: Arc<dyn synapse_storage::membership::MemberStoreApi>,
        server_name: String,
    ) -> Self {
        // Backwards-compatible defaults: keep prior behavior (sequential, full
        // page) when callers don't pass the tuning knobs. Production wiring
        // should call `with_evict_concurrency` / `with_evict_page_size`.
        Self {
            user_service,
            user_storage,
            device_storage,
            room_storage,
            member_storage,
            server_name,
            evict_max_concurrency: 1,
            evict_page_size: i64::MAX / 2,
        }
    }

    /// Tune the concurrent room-removal concurrency. `0` is coerced to 1
    /// (sequential) to keep `buffer_unordered` well-defined.
    pub fn with_evict_concurrency(mut self, concurrency: usize) -> Self {
        self.evict_max_concurrency = concurrency.max(1);
        self
    }

    /// Tune the paginated room-list page size. Values `< 1` are coerced to 1.
    pub fn with_evict_page_size(mut self, page_size: i64) -> Self {
        self.evict_page_size = page_size.max(1);
        self
    }

    /// See [`list_users_legacy`].
    #[instrument(skip(self))]
    pub async fn list_users_legacy(
        &self,
        limit: i64,
        created_ts_cursor: Option<i64>,
        user_id_cursor: Option<&str>,
    ) -> Result<AdminLegacyUsersPage, ApiError> {
        let users = self.user_service.get_users_paginated(limit, created_ts_cursor, user_id_cursor).await?;

        let total = self.user_service.get_user_count().await?;

        Ok(AdminLegacyUsersPage { users, total })
    }

    /// See [`delete_user`].
    /// See [`delete_user`].
    #[instrument(skip(self))]
    pub async fn delete_user(&self, user_id: &str) -> Result<(), ApiError> {
        self.user_storage.delete_user(user_id).await.map_err(|e| ApiError::internal_with_context("Database error", &e))
    }

    /// See [`set_admin_status`].
    /// See [`set_admin_status`].
    #[instrument(skip(self))]
    pub async fn set_admin_status(&self, user_id: &str, is_admin: bool) -> Result<(), ApiError> {
        self.user_storage
            .set_admin_status(user_id, is_admin)
            .await
            .map(|_| ())
            .map_err(|e| ApiError::internal_with_context("Database error", &e))
    }

    /// See [`get_user_rooms_paginated`].
    #[instrument(skip(self))]
    pub async fn get_user_rooms_paginated(
        &self,
        user_id: &str,
        limit: i64,
        from: Option<&str>,
    ) -> Result<Vec<String>, ApiError> {
        self.room_storage
            .get_user_rooms_paginated(user_id, limit, from)
            .await
            .map_err(|e| ApiError::database(format!("A database error occurred: {e}")))
    }

    /// See [`get_user_devices`].
    /// See [`get_user_devices`].
    #[instrument(skip(self))]
    pub async fn get_user_devices(&self, user_id: &str) -> Result<Vec<synapse_storage::Device>, ApiError> {
        self.device_storage
            .get_user_devices(user_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Database error", &e))
    }

    /// See [`get_user_device_count`].
    /// See [`get_user_device_count`].
    #[instrument(skip(self))]
    pub async fn get_user_device_count(&self, user_id: &str) -> Result<i64, ApiError> {
        self.device_storage
            .get_device_count(user_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Database error", &e))
    }

    /// See [`get_joined_room_count`].
    /// See [`get_joined_room_count`].
    #[instrument(skip(self))]
    pub async fn get_joined_room_count(&self, user_id: &str) -> Result<i64, ApiError> {
        self.member_storage
            .get_joined_room_count(user_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Database error", &e))
    }

    /// See [`evict_user_from_joined_rooms`].
    /// See [`evict_user_from_joined_rooms`].
    #[instrument(skip(self))]
    pub async fn evict_user_from_joined_rooms(&self, user_id: &str) -> Result<AdminUserEvictionResult, ApiError> {
        // B-1.1 fix (Phase 2 — pagination + concurrent removal + visible failures):
        // 1. Walk joined rooms via keyset pagination (LIMIT N, room_id > cursor)
        //    to bound memory on users with very large joined-room sets.
        // 2. Drive `remove_member` with a `buffer_unordered` pool capped at
        //    `evict_max_concurrency` to avoid saturating the connection pool.
        // 3. If the post-batch `decrement_member_counts_batch` fails, push the
        //    error into `failures` and log a `warn!` (was silently swallowed
        //    with `let _ =` before — caused stale `room_summaries.updated_ts`).
        let mut joined_rooms: Vec<String> = Vec::new();
        let mut failures: Vec<AdminEvictionFailure> = Vec::new();
        let mut removed: Vec<String> = Vec::new();
        let page_size = self.evict_page_size;
        let mut after_room_id = String::new();

        loop {
            let page = self
                .member_storage
                .get_joined_rooms_page(user_id, &after_room_id, page_size)
                .await
                .map_err(|e| ApiError::internal_with_context("Database error", &e))?;

            if page.is_empty() {
                break;
            }
            let page_len = page.len() as i64;
            let last = page.last().cloned().unwrap_or_default();
            joined_rooms.extend(page.iter().cloned());

            // Concurrent room removals, capped at `evict_max_concurrency` (>= 1).
            let member_storage = self.member_storage.clone();
            let user_id_owned = user_id.to_string();
            let mut stream = stream::iter(page.into_iter())
                .map(|room_id| {
                    let member_storage = member_storage.clone();
                    let user_id_owned = user_id_owned.clone();
                    async move {
                        let result = member_storage.remove_member(&room_id, &user_id_owned, None).await;
                        (room_id, result)
                    }
                })
                .buffer_unordered(self.evict_max_concurrency);

            while let Some((room_id, result)) = stream.next().await {
                match result {
                    Ok(()) => removed.push(room_id),
                    Err(e) => failures.push(AdminEvictionFailure {
                        room_id: room_id.clone(),
                        error: e.to_string(),
                    }),
                }
            }

            // Termination: a short page means we drained the set; advance
            // the cursor and continue for full pages.
            if page_len < page_size || last.is_empty() {
                break;
            }
            after_room_id = last;
        }

        if !removed.is_empty() {
            // B-1.1: surface failures instead of silently dropping them.
            if let Err(e) = self.room_storage.decrement_member_counts_batch(&removed).await {
                tracing::warn!(
                    error = %e,
                    removed_count = removed.len(),
                    "decrement_member_counts_batch failed; room_summaries.updated_ts may be stale"
                );
                failures.push(AdminEvictionFailure {
                    room_id: "<batch>".to_string(),
                    error: format!("decrement_member_counts_batch: {e}"),
                });
            }
        }

        Ok(AdminUserEvictionResult { joined_rooms, failures })
    }

    /// See [`list_users_v2`].
    #[instrument(skip(self))]
    pub async fn list_users_v2(
        &self,
        limit: i64,
        cursor: Option<AdminUserCursor>,
        name_filter: Option<&str>,
    ) -> Result<AdminUsersPage, ApiError> {
        let rows = self
            .user_storage
            .list_users(
                limit,
                cursor.as_ref().map(|cursor| cursor.created_ts),
                cursor.as_ref().map(|cursor| cursor.user_id.as_str()),
                name_filter,
            )
            .await
            .map_err(|e| ApiError::internal_with_context("Database error", &e))?;

        // B-6 fix: compute `total` against the same name_filter used for the
        // page query so the admin client can paginate correctly.  Previously
        // this called `get_user_count()` which always returned the full-table
        // count, making `total` inconsistent with the page when `name=foo`
        // was supplied.
        let total = self
            .user_storage
            .count_users_matching(name_filter)
            .await
            .map_err(|e| ApiError::internal_with_context("Database error", &e))?;

        let users = rows
            .iter()
            .map(|row| AdminUserListItem {
                user_id: row.user_id.clone(),
                created_ts: row.created_ts,
                is_admin: row.is_admin,
                is_guest: row.is_guest,
                user_type: row.user_type.clone(),
                is_deactivated: row.is_deactivated,
                displayname: row.displayname.clone(),
                avatar_url: row.avatar_url.clone(),
            })
            .collect();

        let next_token = if rows.len() as i64 == limit {
            rows.last().map(|row| {
                encode_user_cursor(&AdminUserCursor { created_ts: row.created_ts, user_id: row.user_id.clone() })
            })
        } else {
            None
        };

        Ok(AdminUsersPage { users, total, next_token })
    }

    /// See [`get_user_v2`].
    /// See [`get_user_v2`].
    #[instrument(skip(self))]
    pub async fn get_user_v2(&self, identifier: &str) -> Result<Option<AdminUserDetails>, ApiError> {
        let user = self.user_service.get_user_by_identifier(identifier).await?;

        let Some(user) = user else {
            return Ok(None);
        };

        let devices = self
            .device_storage
            .get_user_devices(&user.user_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Database error", &e))?;

        Ok(Some(AdminUserDetails {
            user: AdminUserProfile::from(&user),
            devices: devices
                .into_iter()
                .map(|device| AdminUserDeviceInfo {
                    device_id: device.device_id,
                    display_name: device.display_name,
                    last_seen_ts: device.last_seen_ts,
                    last_seen_ip: device.last_seen_ip,
                })
                .collect(),
        }))
    }

    /// See [`create_or_update_user_v2`].
    #[allow(clippy::too_many_arguments)]
    #[instrument(skip(self))]
    pub async fn create_or_update_user_v2(
        &self,
        identifier: &str,
        displayname: Option<&str>,
        avatar_url: Option<&str>,
        is_admin: Option<bool>,
        is_deactivated: Option<bool>,
        user_type: Option<&str>,
        password: Option<&str>,
    ) -> Result<(), ApiError> {
        let existing_user = self.user_service.get_user_by_identifier(identifier).await?;

        if let Some(existing_user) = existing_user {
            if let Some(displayname) = displayname {
                self.user_service.update_displayname(&existing_user.user_id, Some(displayname)).await?;
            }

            if let Some(avatar_url) = avatar_url {
                self.user_service.update_avatar_url(&existing_user.user_id, Some(avatar_url)).await?;
            }

            if let Some(is_admin) = is_admin {
                self.user_storage
                    .set_admin_status(&existing_user.user_id, is_admin)
                    .await
                    .map_err(|e| ApiError::internal_with_context("Failed to update user admin status", &e))?;
            }

            if let Some(is_deactivated) = is_deactivated {
                self.user_storage
                    .set_deactivation_status(&existing_user.user_id, is_deactivated)
                    .await
                    .map_err(|e| ApiError::internal_with_context("Failed to update user deactivation status", &e))?;
            }

            if let Some(user_type) = user_type {
                self.user_storage
                    .set_user_type(&existing_user.user_id, Some(user_type))
                    .await
                    .map_err(|e| ApiError::internal_with_context("Failed to update user type", &e))?;
            }

            if let Some(password) = password {
                let password_hash = hash_password(password)
                    .map_err(|e| ApiError::internal_with_context("Password hashing failed", &e))?;
                self.user_storage
                    .update_password(&existing_user.user_id, &password_hash)
                    .await
                    .map_err(|e| ApiError::internal_with_context("Failed to update password", &e))?;
            }

            return Ok(());
        }

        let user_id = if identifier.starts_with('@') {
            identifier.to_owned()
        } else {
            format!("@{}:{}", identifier, self.server_name)
        };
        let username =
            user_id.strip_prefix('@').and_then(|value| value.split(':').next()).unwrap_or(identifier).to_owned();
        let password_hash = if let Some(password) = password {
            hash_password(password).map_err(|e| ApiError::internal_with_context("Password hashing failed", &e))?
        } else {
            hash_password(&random_string(16))
                .map_err(|e| ApiError::internal_with_context("Password hashing failed", &e))?
        };

        let created = self
            .user_storage
            .create_user(&user_id, &username, Some(&password_hash), is_admin.unwrap_or(false))
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to create user", &e))?;

        if let Some(displayname) = displayname {
            self.user_service.update_displayname(&created.user_id, Some(displayname)).await?;
        }

        if let Some(avatar_url) = avatar_url {
            self.user_service.update_avatar_url(&created.user_id, Some(avatar_url)).await?;
        }

        if is_deactivated.unwrap_or(false) {
            self.user_storage
                .set_deactivation_status(&created.user_id, true)
                .await
                .map_err(|e| ApiError::internal_with_context("Failed to deactivate created user", &e))?;
        }

        if let Some(user_type) = user_type {
            self.user_storage
                .set_user_type(&created.user_id, Some(user_type))
                .await
                .map_err(|e| ApiError::internal_with_context("Failed to set user type", &e))?;
        }

        Ok(())
    }

    /// See [`get_user_stats`].
    /// See [`get_user_stats`].
    #[instrument(skip(self))]
    pub async fn get_user_stats(&self) -> Result<AdminUserStats, ApiError> {
        let stats = self
            .user_storage
            .get_user_stats_summary()
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to get user stats", &e))?;

        let room_count = self
            .room_storage
            .get_room_count()
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to get room count", &e))?;
        let average_rooms_per_user =
            if stats.total_users > 0 { (room_count as f64 / stats.total_users as f64).round() } else { 0.0 };

        Ok(AdminUserStats {
            total_users: stats.total_users,
            active_users: stats.active_users,
            admin_users: stats.admin_users,
            deactivated_users: stats.deactivated_users,
            guest_users: stats.guest_users,
            average_rooms_per_user,
        })
    }

    /// See [`get_single_user_stats`].
    /// See [`get_single_user_stats`].
    #[instrument(skip(self))]
    pub async fn get_single_user_stats(&self, identifier: &str) -> Result<AdminSingleUserStats, ApiError> {
        let user = self.user_service.get_user_or_not_found(identifier).await?;

        let rooms_joined = self
            .member_storage
            .get_joined_room_count(&user.user_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to count rooms", &e))?;
        let messages_sent = self
            .user_storage
            .count_sent_messages(&user.user_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to count messages", &e))?;
        let last_seen_ts = self
            .device_storage
            .get_user_devices(&user.user_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to get last seen", &e))?
            .into_iter()
            .filter_map(|device| device.last_seen_ts)
            .max();

        Ok(AdminSingleUserStats { user: AdminUserProfile::from(&user), rooms_joined, messages_sent, last_seen_ts })
    }

    /// See [`batch_create_users`].
    #[instrument(skip(self))]
    pub async fn batch_create_users(
        &self,
        users: &[(String, String, Option<String>, bool)],
    ) -> Result<BatchUsersResult, ApiError> {
        let mut succeeded = Vec::new();
        let mut failed = Vec::new();

        for (username, password, displayname, is_admin) in users {
            let password_hash =
                hash_password(password).map_err(|e| ApiError::internal_with_context("Failed to hash password", &e))?;
            let full_user_id = format!("@{}:{}", username, self.server_name);

            match self.user_storage.create_user(&full_user_id, username, Some(&password_hash), *is_admin).await {
                Ok(created) => {
                    if let Some(displayname) = displayname.as_deref() {
                        self.user_service.update_displayname(&created.user_id, Some(displayname)).await?;
                    }
                    succeeded.push(username.clone());
                }
                Err(_) => failed.push(username.clone()),
            }
        }

        Ok(BatchUsersResult { succeeded, failed })
    }

    /// See [`batch_deactivate_users`].
    /// See [`batch_deactivate_users`].
    #[instrument(skip(self))]
    pub async fn batch_deactivate_users(&self, user_ids: &[String]) -> Result<BatchUsersResult, ApiError> {
        // B-1.2: Previously each user_id triggered an independent
        // `set_deactivation_status` round-trip. Now we partition the input into
        // valid (`@local:server`) and syntactically invalid ids, then issue a
        // single `UPDATE ... WHERE user_id = ANY($1) RETURNING user_id` to mark
        // every valid id as deactivated in one shot. "Failed" = invalid OR
        // valid-but-missing-from-DB.
        let mut valid: Vec<String> = Vec::with_capacity(user_ids.len());
        let mut failed: Vec<String> = Vec::new();
        for user_id in user_ids {
            if user_id.starts_with('@') && user_id.contains(':') {
                valid.push(user_id.clone());
            } else {
                failed.push(user_id.clone());
            }
        }

        let succeeded: Vec<String> = if valid.is_empty() {
            Vec::new()
        } else {
            self.user_storage
                .set_deactivation_status_batch(&valid, true)
                .await
                .map_err(|e| ApiError::internal_with_context("Failed to batch-deactivate users", &e))?
                .into_iter()
                .collect()
        };

        // Anything valid that didn't come back from the batch UPDATE is a
        // missing user; bucket into failed.
        let succeeded_set: std::collections::HashSet<&String> = succeeded.iter().collect();
        for user_id in &valid {
            if !succeeded_set.contains(user_id) {
                failed.push(user_id.clone());
            }
        }

        Ok(BatchUsersResult { succeeded, failed })
    }

    /// See [`update_account`].
    #[instrument(skip(self))]
    pub async fn update_account(
        &self,
        user_id: &str,
        displayname: Option<&str>,
        avatar_url: Option<&str>,
        is_admin: Option<bool>,
    ) -> Result<(), ApiError> {
        if let Some(displayname) = displayname {
            self.user_service.update_displayname(user_id, Some(displayname)).await?;
        }

        if let Some(avatar_url) = avatar_url {
            self.user_service.update_avatar_url(user_id, Some(avatar_url)).await?;
        }

        if let Some(is_admin) = is_admin {
            self.user_storage
                .set_admin_status(user_id, is_admin)
                .await
                .map_err(|e| ApiError::internal_with_context("Database error", &e))?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod cursor_tests {
    use std::sync::Arc;

    use super::{
        decode_user_cursor, encode_user_cursor, AdminEvictionFailure, AdminUserCursor, AdminUserDeviceInfo,
        AdminUserEvictionResult, AdminUserListItem, AdminUserProfile, AdminUserStats, BatchUsersResult,
    };

    #[test]
    fn test_user_cursor_round_trip() {
        let cursor = encode_user_cursor(&AdminUserCursor {
            created_ts: 1_700_000_000_000,
            user_id: "@alice:example.com".to_string(),
        });
        assert_eq!(
            decode_user_cursor(Some(&cursor)),
            Some(AdminUserCursor { created_ts: 1_700_000_000_000, user_id: "@alice:example.com".to_string() }),
        );
    }

    #[test]
    fn test_user_cursor_rejects_invalid_value() {
        assert_eq!(decode_user_cursor(Some("bad-cursor")), None);
        assert_eq!(decode_user_cursor(Some("123|")), None);
    }

    #[test]
    fn test_user_cursor_empty_user_id() {
        assert_eq!(decode_user_cursor(Some("123|")), None);
    }

    #[test]
    fn test_user_cursor_none() {
        assert_eq!(decode_user_cursor(None), None);
    }

    #[test]
    fn test_user_cursor_invalid_timestamp() {
        assert_eq!(decode_user_cursor(Some("abc|user")), None);
    }

    #[test]
    fn test_admin_user_profile_from_user() {
        let user = synapse_storage::User {
            user_id: "@alice:example.com".to_string(),
            username: "alice".to_string(),
            password_hash: None,
            is_admin: true,
            is_guest: false,
            is_shadow_banned: false,
            is_deactivated: false,
            created_ts: 1_700_000_000_000,
            updated_ts: None,
            displayname: Some("Alice".to_string()),
            avatar_url: Some("mxc://example.com/avatar".to_string()),
            email: None,
            phone: None,
            generation: None,
            consent_version: None,
            appservice_id: None,
            user_type: Some("staff".to_string()),
            invalid_update_at: None,
            migration_state: None,
            password_changed_ts: None,
            is_password_change_required: false,
            password_expires_at: None,
            failed_login_attempts: 0,
            locked_until: None,
            must_change_password: false,
        };

        let profile = AdminUserProfile::from(&user);
        assert_eq!(profile.user_id, "@alice:example.com");
        assert_eq!(profile.username, "alice");
        assert!(profile.is_admin);
        assert!(!profile.is_guest);
        assert!(!profile.is_deactivated);
        assert_eq!(profile.created_ts, 1_700_000_000_000);
        assert_eq!(profile.displayname, Some("Alice".to_string()));
        assert_eq!(profile.avatar_url, Some("mxc://example.com/avatar".to_string()));
        assert_eq!(profile.user_type, Some("staff".to_string()));
    }

    #[test]
    fn test_admin_user_stats_creation() {
        let stats = AdminUserStats {
            total_users: 100,
            active_users: 80,
            admin_users: 5,
            deactivated_users: 2,
            guest_users: 10,
            average_rooms_per_user: 3.5,
        };
        assert_eq!(stats.total_users, 100);
        assert_eq!(stats.active_users, 80);
        assert_eq!(stats.admin_users, 5);
        assert_eq!(stats.deactivated_users, 2);
        assert_eq!(stats.guest_users, 10);
        assert!((stats.average_rooms_per_user - 3.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_batch_users_result_creation() {
        let result = BatchUsersResult {
            succeeded: vec!["user1".to_string(), "user2".to_string()],
            failed: vec!["user3".to_string()],
        };
        assert_eq!(result.succeeded.len(), 2);
        assert_eq!(result.failed.len(), 1);
        assert!(result.succeeded.contains(&"user1".to_string()));
        assert!(result.failed.contains(&"user3".to_string()));
    }

    #[test]
    fn test_admin_user_eviction_result_creation() {
        let result = AdminUserEvictionResult {
            joined_rooms: vec!["!room1:example.com".to_string(), "!room2:example.com".to_string()],
            failures: vec![AdminEvictionFailure {
                room_id: "!room3:example.com".to_string(),
                error: "Permission denied".to_string(),
            }],
        };
        assert_eq!(result.joined_rooms.len(), 2);
        assert_eq!(result.failures.len(), 1);
        assert_eq!(result.failures[0].room_id, "!room3:example.com");
        assert_eq!(result.failures[0].error, "Permission denied");
    }

    #[test]
    fn test_admin_user_list_item_creation() {
        let item = AdminUserListItem {
            user_id: "@alice:example.com".to_string(),
            created_ts: 1_700_000_000_000,
            is_admin: true,
            is_guest: false,
            user_type: Some("staff".to_string()),
            is_deactivated: false,
            displayname: Some("Alice".to_string()),
            avatar_url: Some("mxc://example.com/avatar".to_string()),
        };
        assert_eq!(item.user_id, "@alice:example.com");
        assert!(item.is_admin);
        assert!(!item.is_guest);
        assert!(!item.is_deactivated);
    }

    #[test]
    fn test_admin_user_device_info_creation() {
        let device = AdminUserDeviceInfo {
            device_id: "DEVICE123".to_string(),
            display_name: Some("My Phone".to_string()),
            last_seen_ts: Some(1_700_000_000_000),
            last_seen_ip: Some("192.168.1.1".to_string()),
        };
        assert_eq!(device.device_id, "DEVICE123");
        assert_eq!(device.display_name, Some("My Phone".to_string()));
        assert_eq!(device.last_seen_ts, Some(1_700_000_000_000));
        assert_eq!(device.last_seen_ip, Some("192.168.1.1".to_string()));
    }

    // ── B-1.1: evict builder + concurrency/page-size tests ──

    #[tokio::test]
    async fn test_admin_user_service_builder_tunes_concurrency() {
        // Construct a minimal service — we only test the builder plumbing.
        use synapse_storage::test_mocks::{InMemoryMemberStore, InMemoryRoomStore, FakeUserStore};
        use synapse_storage::device::DeviceListStoreApi;

        let user_store = Arc::new(FakeUserStore::default());
        let member_store: Arc<dyn synapse_storage::membership::MemberStoreApi> =
            Arc::new(InMemoryMemberStore::new());
        let room_store: Arc<dyn synapse_storage::RoomStoreApi> = Arc::new(InMemoryRoomStore::new());
        let device_store: Arc<dyn DeviceListStoreApi> =
            Arc::new(synapse_storage::test_mocks::InMemoryDeviceListStore::new());

        let svc = super::AdminUserService::new(
            Arc::new(sqlx::PgPool::connect_lazy("postgres://localhost/test").unwrap()),
            Arc::new(crate::UserService::new(user_store.clone())),
            user_store,
            device_store,
            room_store,
            member_store,
            "example.com".to_string(),
        )
        .with_evict_concurrency(8)
        .with_evict_page_size(100);

        assert_eq!(svc.evict_max_concurrency, 8);
        assert_eq!(svc.evict_page_size, 100);
    }

    #[tokio::test]
    async fn test_admin_user_service_builder_coerces_zero_concurrency_to_one() {
        use synapse_storage::test_mocks::{InMemoryMemberStore, InMemoryRoomStore, FakeUserStore};
        use synapse_storage::device::DeviceListStoreApi;

        let user_store = Arc::new(FakeUserStore::default());
        let device_store: Arc<dyn DeviceListStoreApi> =
            Arc::new(synapse_storage::test_mocks::InMemoryDeviceListStore::new());

        let svc = super::AdminUserService::new(
            Arc::new(sqlx::PgPool::connect_lazy("postgres://localhost/test").unwrap()),
            Arc::new(crate::UserService::new(user_store.clone())),
            user_store,
            device_store,
            Arc::new(InMemoryRoomStore::new()),
            Arc::new(InMemoryMemberStore::new()),
            "example.com".to_string(),
        )
        .with_evict_concurrency(0)
        .with_evict_page_size(0);

        assert_eq!(svc.evict_max_concurrency, 1, "zero concurrency coerced to 1");
        assert_eq!(svc.evict_page_size, 1, "zero page_size coerced to 1");
    }

    #[test]
    fn test_admin_user_eviction_result_failure_special_marker() {
        // The `<batch>` room_id is used when the *batch*-level
        // `decrement_member_counts_batch` fails — it is distinct from per-room
        // failures. Ensure the field is present and not ambiguous.
        let result = super::AdminUserEvictionResult {
            joined_rooms: vec!["!room1:example.com".to_string()],
            failures: vec![super::AdminEvictionFailure {
                room_id: "<batch>".to_string(),
                error: "decrement_member_counts_batch: db error".to_string(),
            }],
        };
        assert_eq!(result.failures.len(), 1);
        assert_eq!(result.failures[0].room_id, "<batch>");
        assert!(result.failures[0].error.starts_with("decrement_member_counts_batch:"));
    }
}
