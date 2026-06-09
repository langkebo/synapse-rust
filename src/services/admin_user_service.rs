use crate::common::ApiError;
use crate::common::crypto::hash_password;
use crate::storage::{DeviceStorage, RoomMemberStorage, RoomStorage, User, UserStorage};
use sqlx::{PgPool, QueryBuilder};
use std::sync::Arc;

pub use crate::storage::User as AdminUserRecord;

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

#[derive(Debug, Clone, sqlx::FromRow)]
struct AdminUserListRow {
    user_id: String,
    created_ts: i64,
    is_admin: bool,
    is_guest: bool,
    user_type: Option<String>,
    is_deactivated: bool,
    displayname: Option<String>,
    avatar_url: Option<String>,
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
pub struct AdminUserDetails {
    pub user: User,
    pub devices: Vec<AdminUserDeviceInfo>,
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
    pub user: User,
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
    pool: Arc<PgPool>,
    user_storage: UserStorage,
    device_storage: DeviceStorage,
    room_storage: RoomStorage,
    member_storage: RoomMemberStorage,
    server_name: String,
}

impl AdminUserService {
    pub fn new(
        pool: Arc<PgPool>,
        user_storage: UserStorage,
        device_storage: DeviceStorage,
        room_storage: RoomStorage,
        member_storage: RoomMemberStorage,
        server_name: String,
    ) -> Self {
        Self { pool, user_storage, device_storage, room_storage, member_storage, server_name }
    }

    pub async fn list_users_v2(
        &self,
        limit: i64,
        cursor: Option<AdminUserCursor>,
        name_filter: Option<&str>,
    ) -> Result<AdminUsersPage, ApiError> {
        let mut query = QueryBuilder::<sqlx::Postgres>::new(
            "SELECT user_id, created_ts, COALESCE(is_admin, FALSE) AS is_admin, \
             COALESCE(is_guest, FALSE) AS is_guest, user_type, \
             COALESCE(is_deactivated, FALSE) AS is_deactivated, displayname, avatar_url \
             FROM users WHERE 1=1",
        );

        if let Some(name) = name_filter {
            query.push(" AND username LIKE ");
            query.push_bind(format!("%{name}%"));
        }

        if let Some(cursor) = cursor.as_ref() {
            query.push(" AND (created_ts < ");
            query.push_bind(cursor.created_ts);
            query.push(" OR (created_ts = ");
            query.push_bind(cursor.created_ts);
            query.push(" AND user_id < ");
            query.push_bind(&cursor.user_id);
            query.push("))");
        }

        query.push(" ORDER BY created_ts DESC, user_id DESC LIMIT ");
        query.push_bind(limit);

        let rows = query
            .build_query_as::<AdminUserListRow>()
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        let total = self
            .user_storage
            .get_user_count()
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        let users: Vec<AdminUserListItem> = rows
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
                encode_user_cursor(&AdminUserCursor {
                    created_ts: row.created_ts,
                    user_id: row.user_id.clone(),
                })
            })
        } else {
            None
        };

        Ok(AdminUsersPage { users, total, next_token })
    }

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
            user,
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
        let now = chrono::Utc::now().timestamp_millis();
        let existing_user = self
            .user_storage
            .get_user_by_identifier(identifier)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        if existing_user.is_some() {
            sqlx::query!(
                r#"
                UPDATE users SET
                    displayname = COALESCE($2, displayname),
                    avatar_url = COALESCE($3, avatar_url),
                    is_admin = COALESCE($4, is_admin),
                    is_deactivated = COALESCE($5, is_deactivated),
                    user_type = COALESCE($6, user_type),
                    updated_ts = $7
                WHERE username = $1 OR user_id = $1
                "#,
                identifier,
                displayname,
                avatar_url,
                is_admin,
                is_deactivated,
                user_type,
                now,
            )
            .execute(&*self.pool)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to update user", &e))?;

            return Ok(());
        }

        let user_id = if identifier.starts_with('@') {
            identifier.to_owned()
        } else {
            format!("@{}:{}", identifier, self.server_name)
        };
        let username = user_id
            .strip_prefix('@')
            .and_then(|value| value.split(':').next())
            .unwrap_or(identifier)
            .to_owned();
        let password_hash = if let Some(password) = password {
            hash_password(password).map_err(|e| ApiError::internal_with_log("Password hashing failed", &e))?
        } else {
            hash_password(&crate::common::random_string(16))
                .map_err(|e| ApiError::internal_with_log("Password hashing failed", &e))?
        };

        sqlx::query!(
            r#"
            INSERT INTO users (
                user_id, username, password_hash, displayname, avatar_url,
                is_admin, is_deactivated, user_type, created_ts, updated_ts, generation
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 0)
            "#,
            &user_id,
            &username,
            &password_hash,
            displayname,
            avatar_url,
            is_admin.unwrap_or(false),
            is_deactivated.unwrap_or(false),
            user_type,
            now,
            now,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to create user", &e))?;

        Ok(())
    }

    pub async fn get_user_stats(&self) -> Result<AdminUserStats, ApiError> {
        let stats = sqlx::query!(
            r#"
            SELECT
                COUNT(*) AS "total_users!",
                COUNT(*) FILTER (WHERE COALESCE(is_deactivated, FALSE) = FALSE) AS "active_users!",
                COUNT(*) FILTER (WHERE COALESCE(is_admin, FALSE) = TRUE) AS "admin_users!",
                COUNT(*) FILTER (WHERE COALESCE(is_deactivated, FALSE) = TRUE) AS "deactivated_users!",
                COUNT(*) FILTER (WHERE COALESCE(is_guest, FALSE) = TRUE) AS "guest_users!"
            FROM users
            "#
        )
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to get user stats", &e))?;

        let room_count = self
            .room_storage
            .get_room_count()
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get room count", &e))?;
        let average_rooms_per_user = if stats.total_users > 0 {
            (room_count as f64 / stats.total_users as f64).round()
        } else {
            0.0
        };

        Ok(AdminUserStats {
            total_users: stats.total_users,
            active_users: stats.active_users,
            admin_users: stats.admin_users,
            deactivated_users: stats.deactivated_users,
            guest_users: stats.guest_users,
            average_rooms_per_user,
        })
    }

    pub async fn get_single_user_stats(&self, user: &User) -> Result<AdminSingleUserStats, ApiError> {
        let rooms_joined = self
            .member_storage
            .get_joined_room_count(&user.user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to count rooms", &e))?;
        let messages_sent = sqlx::query_scalar!(
            r#"SELECT COUNT(*) AS "count!" FROM events WHERE sender = $1 AND event_type = 'm.room.message' AND is_redacted = false"#,
            &user.user_id,
        )
        .fetch_one(&*self.pool)
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

        Ok(AdminSingleUserStats {
            user: user.clone(),
            rooms_joined,
            messages_sent,
            last_seen_ts,
        })
    }

    pub async fn batch_create_users(
        &self,
        users: &[(String, String, Option<String>, bool)],
    ) -> Result<BatchUsersResult, ApiError> {
        let mut succeeded = Vec::new();
        let mut failed = Vec::new();
        let now = chrono::Utc::now().timestamp_millis();

        for (username, password, displayname, is_admin) in users {
            let password_hash =
                hash_password(password).map_err(|e| ApiError::internal_with_log("Failed to hash password", &e))?;
            let full_user_id = format!("@{}:{}", username, self.server_name);
            let effective_displayname = displayname.as_deref().unwrap_or(username.as_str());

            let result = sqlx::query!(
                r#"
                INSERT INTO users (user_id, username, password_hash, displayname, is_admin, created_ts, updated_ts)
                VALUES ($1, $2, $3, $4, $5, $6, $7)
                ON CONFLICT (username) DO NOTHING
                "#,
                &full_user_id,
                username,
                &password_hash,
                effective_displayname,
                *is_admin,
                now,
                now,
            )
            .execute(&*self.pool)
            .await;

            match result {
                Ok(result) if result.rows_affected() > 0 => succeeded.push(username.clone()),
                Ok(_) => failed.push(username.clone()),
                Err(_) => failed.push(username.clone()),
            }
        }

        Ok(BatchUsersResult { succeeded, failed })
    }

    pub async fn batch_deactivate_users(&self, user_ids: &[String]) -> Result<BatchUsersResult, ApiError> {
        let mut succeeded = Vec::new();
        let mut failed = Vec::new();

        for user_id in user_ids {
            if !user_id.starts_with('@') || !user_id.contains(':') {
                failed.push(user_id.clone());
                continue;
            }

            let result = sqlx::query!(
                "UPDATE users SET is_deactivated = TRUE WHERE user_id = $1",
                user_id,
            )
            .execute(&*self.pool)
            .await;

            match result {
                Ok(result) if result.rows_affected() > 0 => succeeded.push(user_id.clone()),
                _ => failed.push(user_id.clone()),
            }
        }

        Ok(BatchUsersResult { succeeded, failed })
    }

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
            Some(AdminUserCursor {
                created_ts: 1_700_000_000_000,
                user_id: "@alice:example.com".to_string(),
            })
        );
    }

    #[test]
    fn test_user_cursor_rejects_invalid_value() {
        assert_eq!(decode_user_cursor(Some("bad-cursor")), None);
        assert_eq!(decode_user_cursor(Some("123|")), None);
    }
}
