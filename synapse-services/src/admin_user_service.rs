use sqlx::PgPool;
use std::sync::Arc;
use synapse_common::crypto::{hash_password, random_string};
use synapse_common::error::ApiError;
use synapse_storage::{DeviceStorage, RoomMemberRepository, RoomStorage, User, UserStore};
use tracing::instrument;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminUserCursor {
    pub created_ts: i64,
    pub user_id: String,
}

pub fn decode_user_cursor(cursor: Option<&str>) -> Option<AdminUserCursor> {
    let cursor = cursor?;
    let (created_ts, user_id) = cursor.split_once('|')?;
    let created_ts = created_ts.parse::<i64>().ok()?;
    if user_id.is_empty() {
        return None;
    }
    Some(AdminUserCursor { created_ts, user_id: user_id.to_owned() })
}

pub fn encode_user_cursor(cursor: &AdminUserCursor) -> String {
    format!("{}|{}", cursor.created_ts, cursor.user_id)
}

#[derive(Debug, Clone)]
pub struct AdminUserListItem {
    pub user_id: String,
    pub created_ts: i64,
    pub is_admin: bool,
    pub is_guest: bool,
    pub user_type: Option<String>,
    pub is_deactivated: bool,
    pub displayname: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AdminUserDeviceInfo {
    pub device_id: String,
    pub display_name: Option<String>,
    pub last_seen_ts: Option<i64>,
    pub last_seen_ip: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AdminUsersPage {
    pub users: Vec<AdminUserListItem>,
    pub total: i64,
    pub next_token: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AdminUserProfile {
    pub user_id: String,
    pub username: String,
    pub is_admin: bool,
    pub is_guest: bool,
    pub is_deactivated: bool,
    pub created_ts: i64,
    pub displayname: Option<String>,
    pub avatar_url: Option<String>,
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

#[derive(Debug, Clone)]
pub struct AdminUserDetails {
    pub user: AdminUserProfile,
    pub devices: Vec<AdminUserDeviceInfo>,
}

#[derive(Debug, Clone)]
pub struct AdminLegacyUsersPage {
    pub users: Vec<User>,
    pub total: i64,
}

#[derive(Debug, Clone)]
pub struct AdminEvictionFailure {
    pub room_id: String,
    pub error: String,
}

#[derive(Debug, Clone)]
pub struct AdminUserEvictionResult {
    pub joined_rooms: Vec<String>,
    pub failures: Vec<AdminEvictionFailure>,
}

#[derive(Debug, Clone)]
pub struct AdminUserStats {
    pub total_users: i64,
    pub active_users: i64,
    pub admin_users: i64,
    pub deactivated_users: i64,
    pub guest_users: i64,
    pub average_rooms_per_user: f64,
}

#[derive(Debug, Clone)]
pub struct AdminSingleUserStats {
    pub user: AdminUserProfile,
    pub rooms_joined: i64,
    pub messages_sent: i64,
    pub last_seen_ts: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct BatchUsersResult {
    pub succeeded: Vec<String>,
    pub failed: Vec<String>,
}

pub struct AdminUserService {
    user_storage: Arc<dyn UserStore>,
    device_storage: DeviceStorage,
    room_storage: RoomStorage,
    member_storage: Arc<dyn RoomMemberRepository>,
    server_name: String,
}

impl AdminUserService {
    pub fn new(
        _pool: Arc<PgPool>,
        user_storage: Arc<dyn UserStore>,
        device_storage: DeviceStorage,
        room_storage: RoomStorage,
        member_storage: Arc<dyn RoomMemberRepository>,
        server_name: String,
    ) -> Self {
        Self { user_storage, device_storage, room_storage, member_storage, server_name }
    }

    #[instrument(skip(self))]
    pub async fn get_user_by_identifier(&self, identifier: &str) -> Result<Option<User>, ApiError> {
        self.user_storage
            .get_user_by_identifier(identifier)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))
    }

    #[instrument(skip(self))]
    pub async fn get_user_or_not_found(&self, identifier: &str) -> Result<User, ApiError> {
        self.get_user_by_identifier(identifier).await?.ok_or_else(|| ApiError::not_found("User not found".to_string()))
    }

    #[instrument(skip(self))]
    pub async fn list_users_legacy(
        &self,
        limit: i64,
        created_ts_cursor: Option<i64>,
        user_id_cursor: Option<&str>,
    ) -> Result<AdminLegacyUsersPage, ApiError> {
        let users = self
            .user_storage
            .get_users_paginated(limit, created_ts_cursor, user_id_cursor)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        let total =
            self.user_storage.get_user_count().await.map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        Ok(AdminLegacyUsersPage { users, total })
    }

    #[instrument(skip(self))]
    pub async fn delete_user(&self, user_id: &str) -> Result<(), ApiError> {
        self.user_storage.delete_user(user_id).await.map_err(|e| ApiError::internal_with_log("Database error", &e))
    }

    #[instrument(skip(self))]
    pub async fn set_admin_status(&self, user_id: &str, is_admin: bool) -> Result<(), ApiError> {
        self.user_storage
            .set_admin_status(user_id, is_admin)
            .await
            .map(|_| ())
            .map_err(|e| ApiError::internal_with_log("Database error", &e))
    }

    #[instrument(skip(self))]
    pub async fn get_user_rooms_paginated(
        &self,
        user_id: &str,
        limit: i64,
        from: Option<&str>,
    ) -> Result<Vec<String>, ApiError> {
        RoomStorage::get_user_rooms_paginated(&self.room_storage, user_id, limit, from)
            .await
            .map_err(|e| ApiError::database(format!("A database error occurred: {e}")))
    }

    #[instrument(skip(self))]
    pub async fn get_user_devices(&self, user_id: &str) -> Result<Vec<synapse_storage::Device>, ApiError> {
        self.device_storage
            .get_user_devices(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))
    }

    #[instrument(skip(self))]
    pub async fn get_user_device_count(&self, user_id: &str) -> Result<i64, ApiError> {
        self.device_storage
            .get_device_count(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))
    }

    #[instrument(skip(self))]
    pub async fn get_joined_room_count(&self, user_id: &str) -> Result<i64, ApiError> {
        self.member_storage
            .get_joined_room_count(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))
    }

    #[instrument(skip(self))]
    pub async fn evict_user_from_joined_rooms(&self, user_id: &str) -> Result<AdminUserEvictionResult, ApiError> {
        let joined_rooms = self
            .member_storage
            .get_joined_rooms(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        let mut failures = Vec::new();
        for room_id in &joined_rooms {
            if let Err(e) = self.member_storage.remove_member(room_id, user_id).await {
                failures.push(AdminEvictionFailure { room_id: room_id.clone(), error: e.to_string() });
            } else {
                let _ = self.room_storage.decrement_member_count(room_id).await;
            }
        }

        Ok(AdminUserEvictionResult { joined_rooms, failures })
    }

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
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        let total =
            self.user_storage.get_user_count().await.map_err(|e| ApiError::internal_with_log("Database error", &e))?;

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

    #[instrument(skip(self))]
    pub async fn get_user_v2(&self, identifier: &str) -> Result<Option<AdminUserDetails>, ApiError> {
        let user = self
            .user_storage
            .get_user_by_identifier(identifier)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        let Some(user) = user else {
            return Ok(None);
        };

        let devices = self
            .device_storage
            .get_user_devices(&user.user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

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
        let existing_user = self
            .user_storage
            .get_user_by_identifier(identifier)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        if let Some(existing_user) = existing_user {
            if let Some(displayname) = displayname {
                self.user_storage
                    .update_displayname(&existing_user.user_id, Some(displayname))
                    .await
                    .map_err(|e| ApiError::internal_with_log("Failed to update user displayname", &e))?;
            }

            if let Some(avatar_url) = avatar_url {
                self.user_storage
                    .update_avatar_url(&existing_user.user_id, Some(avatar_url))
                    .await
                    .map_err(|e| ApiError::internal_with_log("Failed to update user avatar", &e))?;
            }

            if let Some(is_admin) = is_admin {
                self.user_storage
                    .set_admin_status(&existing_user.user_id, is_admin)
                    .await
                    .map_err(|e| ApiError::internal_with_log("Failed to update user admin status", &e))?;
            }

            if let Some(is_deactivated) = is_deactivated {
                self.user_storage
                    .set_deactivation_status(&existing_user.user_id, is_deactivated)
                    .await
                    .map_err(|e| ApiError::internal_with_log("Failed to update user deactivation status", &e))?;
            }

            if let Some(user_type) = user_type {
                self.user_storage
                    .set_user_type(&existing_user.user_id, Some(user_type))
                    .await
                    .map_err(|e| ApiError::internal_with_log("Failed to update user type", &e))?;
            }

            if let Some(password) = password {
                let password_hash =
                    hash_password(password).map_err(|e| ApiError::internal_with_log("Password hashing failed", &e))?;
                self.user_storage
                    .update_password(&existing_user.user_id, &password_hash)
                    .await
                    .map_err(|e| ApiError::internal_with_log("Failed to update password", &e))?;
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
            hash_password(password).map_err(|e| ApiError::internal_with_log("Password hashing failed", &e))?
        } else {
            hash_password(&random_string(16)).map_err(|e| ApiError::internal_with_log("Password hashing failed", &e))?
        };

        let created = self
            .user_storage
            .create_user(&user_id, &username, Some(&password_hash), is_admin.unwrap_or(false))
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to create user", &e))?;

        if let Some(displayname) = displayname {
            self.user_storage
                .update_displayname(&created.user_id, Some(displayname))
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to update user displayname", &e))?;
        }

        if let Some(avatar_url) = avatar_url {
            self.user_storage
                .update_avatar_url(&created.user_id, Some(avatar_url))
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to update user avatar", &e))?;
        }

        if is_deactivated.unwrap_or(false) {
            self.user_storage
                .set_deactivation_status(&created.user_id, true)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to deactivate created user", &e))?;
        }

        if let Some(user_type) = user_type {
            self.user_storage
                .set_user_type(&created.user_id, Some(user_type))
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to set user type", &e))?;
        }

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn get_user_stats(&self) -> Result<AdminUserStats, ApiError> {
        let stats = self
            .user_storage
            .get_user_stats_summary()
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get user stats", &e))?;

        let room_count = self
            .room_storage
            .get_room_count()
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get room count", &e))?;
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

    #[instrument(skip(self))]
    pub async fn get_single_user_stats(&self, identifier: &str) -> Result<AdminSingleUserStats, ApiError> {
        let user = self
            .user_storage
            .get_user_by_identifier(identifier)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?
            .ok_or_else(|| ApiError::not_found("User not found"))?;

        let rooms_joined = self
            .member_storage
            .get_joined_room_count(&user.user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to count rooms", &e))?;
        let messages_sent = self
            .user_storage
            .count_sent_messages(&user.user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to count messages", &e))?;
        let last_seen_ts = self
            .device_storage
            .get_user_devices(&user.user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get last seen", &e))?
            .into_iter()
            .filter_map(|device| device.last_seen_ts)
            .max();

        Ok(AdminSingleUserStats { user: AdminUserProfile::from(&user), rooms_joined, messages_sent, last_seen_ts })
    }

    #[instrument(skip(self))]
    pub async fn batch_create_users(
        &self,
        users: &[(String, String, Option<String>, bool)],
    ) -> Result<BatchUsersResult, ApiError> {
        let mut succeeded = Vec::new();
        let mut failed = Vec::new();

        for (username, password, displayname, is_admin) in users {
            let password_hash =
                hash_password(password).map_err(|e| ApiError::internal_with_log("Failed to hash password", &e))?;
            let full_user_id = format!("@{}:{}", username, self.server_name);

            match self.user_storage.create_user(&full_user_id, username, Some(&password_hash), *is_admin).await {
                Ok(created) => {
                    if let Some(displayname) = displayname.as_deref() {
                        self.user_storage
                            .update_displayname(&created.user_id, Some(displayname))
                            .await
                            .map_err(|e| ApiError::internal_with_log("Failed to update displayname", &e))?;
                    }
                    succeeded.push(username.clone());
                }
                Err(_) => failed.push(username.clone()),
            }
        }

        Ok(BatchUsersResult { succeeded, failed })
    }

    #[instrument(skip(self))]
    pub async fn batch_deactivate_users(&self, user_ids: &[String]) -> Result<BatchUsersResult, ApiError> {
        let mut succeeded = Vec::new();
        let mut failed = Vec::new();

        for user_id in user_ids {
            if !user_id.starts_with('@') || !user_id.contains(':') {
                failed.push(user_id.clone());
                continue;
            }

            match self.user_storage.set_deactivation_status(user_id, true).await {
                Ok(true) => succeeded.push(user_id.clone()),
                _ => failed.push(user_id.clone()),
            }
        }

        Ok(BatchUsersResult { succeeded, failed })
    }

    #[instrument(skip(self))]
    pub async fn update_account(
        &self,
        user_id: &str,
        displayname: Option<&str>,
        avatar_url: Option<&str>,
        is_admin: Option<bool>,
    ) -> Result<(), ApiError> {
        if let Some(displayname) = displayname {
            self.user_storage
                .update_displayname(user_id, Some(displayname))
                .await
                .map_err(|e| ApiError::internal_with_log("Database error", &e))?;
        }

        if let Some(avatar_url) = avatar_url {
            self.user_storage
                .update_avatar_url(user_id, Some(avatar_url))
                .await
                .map_err(|e| ApiError::internal_with_log("Database error", &e))?;
        }

        if let Some(is_admin) = is_admin {
            self.user_storage
                .set_admin_status(user_id, is_admin)
                .await
                .map_err(|e| ApiError::internal_with_log("Database error", &e))?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod cursor_tests {
    use super::{decode_user_cursor, encode_user_cursor, AdminUserCursor};

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
}
