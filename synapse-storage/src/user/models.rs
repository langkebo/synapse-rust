//! User data models.
//!
//! All struct definitions for user-related storage entities.

use serde::{Deserialize, Serialize};

/// The `User` struct.
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct User {
    /// The `user_id` field.
    pub user_id: String,
    /// The `username` field.
    pub username: String,
    #[serde(skip_serializing)]
    /// The `password_hash` field.
    pub password_hash: Option<String>,
    /// The `is_admin` field.
    pub is_admin: bool,
    /// The `is_guest` field.
    pub is_guest: bool,
    /// The `is_shadow_banned` field.
    pub is_shadow_banned: bool,
    /// The `is_deactivated` field.
    pub is_deactivated: bool,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `updated_ts` field.
    pub updated_ts: Option<i64>,
    /// The `displayname` field.
    pub displayname: Option<String>,
    /// The `avatar_url` field.
    pub avatar_url: Option<String>,
    /// The `email` field.
    pub email: Option<String>,
    /// The `phone` field.
    pub phone: Option<String>,
    /// The `generation` field.
    pub generation: Option<i64>,
    /// The `consent_version` field.
    pub consent_version: Option<String>,
    /// The `appservice_id` field.
    pub appservice_id: Option<String>,
    /// The `user_type` field.
    pub user_type: Option<String>,
    /// The `invalid_update_at` field.
    pub invalid_update_at: Option<i64>,
    /// The `migration_state` field.
    pub migration_state: Option<String>,
    /// The `password_changed_ts` field.
    pub password_changed_ts: Option<i64>,
    /// The `is_password_change_required` field.
    pub is_password_change_required: bool,
    /// The `password_expires_at` field.
    pub password_expires_at: Option<i64>,
    /// The `failed_login_attempts` field.
    pub failed_login_attempts: i32,
    /// The `locked_until` field.
    pub locked_until: Option<i64>,
    /// The `must_change_password` field.
    pub must_change_password: bool,
}

impl User {
    /// See [`user_id`].
    pub fn user_id(&self) -> String {
        self.user_id.clone()
    }
}

/// The `UserProfile` struct.
/// B-4204: Added `updated_ts` field to support sliding sync profile_updates extension.
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct UserProfile {
    /// The `user_id` field.
    pub user_id: String,
    /// The `username` field.
    pub username: String,
    /// The `displayname` field.
    pub displayname: Option<String>,
    /// The `avatar_url` field.
    pub avatar_url: Option<String>,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `updated_ts` field - tracks last profile change for profile_updates EDU.
    pub updated_ts: Option<i64>,
}

/// The `UserSearchResult` struct.
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct UserSearchResult {
    /// The `user_id` field.
    pub user_id: String,
    /// The `username` field.
    pub username: String,
    /// The `displayname` field.
    pub displayname: Option<String>,
    /// The `avatar_url` field.
    pub avatar_url: Option<String>,
    /// The `created_ts` field.
    pub created_ts: i64,
}

/// The `UserSearchResultWithPresence` struct.
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct UserSearchResultWithPresence {
    /// The `user_id` field.
    pub user_id: String,
    /// The `username` field.
    pub username: String,
    /// The `displayname` field.
    pub displayname: Option<String>,
    /// The `avatar_url` field.
    pub avatar_url: Option<String>,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `presence` field.
    pub presence: Option<String>,
    /// The `last_active_ts` field.
    pub last_active_ts: Option<i64>,
}

/// The `UserStatsSummary` struct.
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct UserStatsSummary {
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
}

/// The `UserDirectorySearchResult` struct.
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct UserDirectorySearchResult {
    /// The `user_id` field.
    pub user_id: String,
    /// The `username` field.
    pub username: String,
    /// The `displayname` field.
    pub displayname: Option<String>,
    /// The `avatar_url` field.
    pub avatar_url: Option<String>,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `presence` field.
    pub presence: Option<String>,
    /// The `last_active_ts` field.
    pub last_active_ts: Option<i64>,
    /// The `match_score` field.
    pub match_score: i32,
    /// The `match_type` field.
    pub match_type: String,
}

/// The `LockedUser` struct.
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct LockedUser {
    /// The `id` field.
    pub id: i64,
    /// The `user_id` field.
    pub user_id: String,
    /// The `reason` field.
    pub reason: Option<String>,
    /// The `locked_by` field.
    pub locked_by: String,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `unlocked_ts` field.
    pub unlocked_ts: Option<i64>,
    /// The `is_active` field.
    pub is_active: bool,
}
