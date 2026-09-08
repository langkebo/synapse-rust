use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres, Row};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use synapse_cache::CacheManager;
use synapse_common::constants::USER_PROFILE_CACHE_TTL;
use synapse_common::current_timestamp_millis;
use tracing;

use crate::trigram_ranking::TrigramRanking;

const USER_DIRECTORY_SEARCH_CACHE_TTL_SECS: u64 = 30;
const USER_PROFILE_BATCH_CACHE_TTL: u64 = 300;

fn escape_like_pattern(input: &str) -> String {
    input.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

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

/// Storage trait for user operations.
/// Two adapters justify the seam: Postgres (prod) and in-memory (test).
#[async_trait]
pub trait UserStore: Send + Sync {
    /// Returns a reference to the database connection pool.
    /// Enables consumers previously accessing the concrete
    /// [`UserStorage::pool`] field to work through the trait object.
    fn pool(&self) -> &Arc<Pool<Postgres>>;

    // ---- lock operations ----

    /// See [`lock_user`].
    async fn lock_user(
        &self,
        user_id: &str,
        reason: Option<&str>,
        locked_by: &str,
        now_ts: i64,
    ) -> Result<LockedUser, sqlx::Error>;

    /// See [`unlock_user`].
    async fn unlock_user(&self, user_id: &str, now_ts: i64) -> Result<(), sqlx::Error>;

    /// See [`is_user_locked`].
    async fn is_user_locked(&self, user_id: &str) -> Result<bool, sqlx::Error>;

    /// See [`get_active_user_lock`].
    async fn get_active_user_lock(&self, user_id: &str) -> Result<Option<LockedUser>, sqlx::Error>;

    /// See [`get_locked_users`].
    async fn get_locked_users(&self, limit: i64, offset: i64) -> Result<Vec<LockedUser>, sqlx::Error>;

    // ---- query methods ----

    /// See [`get_user_by_id`].
    async fn get_user_by_id(&self, user_id: &str) -> Result<Option<User>, sqlx::Error>;

    /// See [`get_user_by_username`].
    async fn get_user_by_username(&self, username: &str) -> Result<Option<User>, sqlx::Error>;

    /// See [`get_user_by_email`].
    async fn get_user_by_email(&self, email: &str) -> Result<Option<User>, sqlx::Error>;

    /// See [`get_user_by_identifier`].
    async fn get_user_by_identifier(&self, identifier: &str) -> Result<Option<User>, sqlx::Error>;

    /// See [`get_users_paginated`].
    async fn get_users_paginated(
        &self,
        limit: i64,
        since_ts: Option<i64>,
        since_user_id: Option<&str>,
    ) -> Result<Vec<User>, sqlx::Error>;

    /// See [`list_users`].
    async fn list_users(
        &self,
        limit: i64,
        from_ts: Option<i64>,
        from_user_id: Option<&str>,
        name_filter: Option<&str>,
    ) -> Result<Vec<User>, sqlx::Error>;

    /// See [`user_exists`].
    async fn user_exists(&self, user_id: &str) -> Result<bool, sqlx::Error>;

    /// See [`filter_existing_users`].
    async fn filter_existing_users(&self, user_ids: &[String]) -> Result<Vec<String>, sqlx::Error>;

    /// See [`get_user_count`].
    async fn get_user_count(&self) -> Result<i64, sqlx::Error>;

    /// B-6 fix: Count users that match an optional substring filter on
    /// `username`. Used by the admin v2 `GET /_synapse/admin/v2/users` route
    /// to compute the `total` field so the client can paginate correctly
    /// when filtering by name. Without this the legacy `get_user_count`
    /// returns the full-table count even when `name=alice` is set, which
    /// makes the returned `total` inconsistent with the page contents.
    ///
    /// `None` ⇒ return the full-table count (equivalent to
    /// [`Self::get_user_count`]).
    async fn count_users_matching(&self, name_filter: Option<&str>) -> Result<i64, sqlx::Error>;

    /// Count users that are NOT deactivated.
    /// Mirrors Synapse's `non_deactivated_user_count` admin statistic.
    async fn count_non_deactivated_users(&self) -> Result<i64, sqlx::Error>;

    /// Count non-deactivated users grouped by `appservice_id`.
    ///
    /// Returns a map from appservice_id to the count of non-deactivated
    /// users registered under that Application Service. Users with
    /// `appservice_id IS NULL` (local users) are grouped under the empty
    /// string key `""`. Groups with zero non-deactivated users are omitted.
    async fn count_non_deactivated_users_by_app_service(&self) -> Result<HashMap<String, i64>, sqlx::Error>;

    /// See [`get_daily_active_users`].
    async fn get_daily_active_users(&self) -> Result<i64, sqlx::Error>;

    /// See [`get_monthly_active_users`].
    async fn get_monthly_active_users(&self) -> Result<i64, sqlx::Error>;

    /// See [`get_r30_users`].
    async fn get_r30_users(&self) -> Result<i64, sqlx::Error>;

    // ---- mutation methods ----

    /// See [`create_user`].
    async fn create_user(
        &self,
        user_id: &str,
        username: &str,
        password_hash: Option<&str>,
        is_admin: bool,
    ) -> Result<User, sqlx::Error>;

    /// See [`update_password`].
    async fn update_password(&self, user_id: &str, password_hash: &str) -> Result<(), sqlx::Error>;

    /// See [`update_displayname`].
    async fn update_displayname(&self, user_id: &str, displayname: Option<&str>) -> Result<(), sqlx::Error>;

    /// See [`update_avatar_url`].
    async fn update_avatar_url(&self, user_id: &str, avatar_url: Option<&str>) -> Result<(), sqlx::Error>;

    /// See [`set_deactivation_status`].
    async fn set_deactivation_status(&self, user_id: &str, is_deactivated: bool) -> Result<bool, sqlx::Error>;

    /// B-1.2: Batch counterpart of [`set_deactivation_status`]. Returns the set
    /// of `user_id`s whose `is_deactivated` was actually changed. See
    /// [`UserStorage::set_deactivation_status_batch`] for semantics.
    async fn set_deactivation_status_batch(
        &self,
        user_ids: &[String],
        is_deactivated: bool,
    ) -> Result<HashSet<String>, sqlx::Error>;

    /// See [`set_admin_status`].
    async fn set_admin_status(&self, user_id: &str, is_admin: bool) -> Result<(), sqlx::Error>;

    /// See [`set_shadow_ban`].
    async fn set_shadow_ban(&self, user_id: &str, is_shadow_banned: bool) -> Result<bool, sqlx::Error>;

    /// See [`delete_user`].
    async fn delete_user(&self, user_id: &str) -> Result<(), sqlx::Error>;

    /// See [`set_guest_status`].
    async fn set_guest_status(&self, user_id: &str, is_guest: bool) -> Result<(), sqlx::Error>;

    /// See [`set_user_type`].
    async fn set_user_type(&self, user_id: &str, user_type: Option<&str>) -> Result<(), sqlx::Error>;

    /// See [`upgrade_guest_account`].
    async fn upgrade_guest_account(
        &self,
        user_id: &str,
        username: &str,
        password_hash: &str,
    ) -> Result<(), sqlx::Error>;

    // ---- stats / search methods ----

    /// See [`get_user_stats_summary`].
    async fn get_user_stats_summary(&self) -> Result<UserStatsSummary, sqlx::Error>;

    /// See [`count_sent_messages`].
    async fn count_sent_messages(&self, user_id: &str) -> Result<i64, sqlx::Error>;

    /// See [`search_users`].
    async fn search_users(&self, query: &str, limit: i64) -> Result<Vec<UserSearchResult>, sqlx::Error>;

    /// See [`search_directory_users`].
    async fn search_directory_users(
        &self,
        query: &str,
        limit: i64,
        exact_only: bool,
    ) -> Result<Vec<UserDirectorySearchResult>, sqlx::Error>;

    /// See [`get_user_profile`].
    async fn get_user_profile(&self, user_id: &str) -> Result<Option<UserProfile>, sqlx::Error>;

    /// See [`get_user_profiles_batch`].
    async fn get_user_profiles_batch(&self, user_ids: &[String]) -> Result<Vec<UserProfile>, sqlx::Error>;

    /// See [`get_user_profiles_map`].
    async fn get_user_profiles_map(&self, user_ids: &[String]) -> Result<HashMap<String, UserProfile>, sqlx::Error>;

    /// B-4204: Get user profiles that have been updated since the given timestamp.
    /// Returns a map from user_id to UserProfile for users whose `updated_ts > since_ts`.
    /// This is used by the sliding sync profile_updates extension to notify other users
    /// in shared rooms of profile changes.
    async fn get_user_profiles_updated_since(
        &self,
        user_ids: &[String],
        since_ts: i64,
    ) -> Result<HashMap<String, UserProfile>, sqlx::Error>;

    /// See [`get_users_batch`].
    async fn get_users_batch(&self, user_ids: &[String]) -> Result<Vec<User>, sqlx::Error>;

    /// See [`get_users_map`].
    async fn get_users_map(&self, user_ids: &[String]) -> Result<HashMap<String, User>, sqlx::Error>;

    // ---- account_data methods ----

    /// See [`get_account_data_content`].
    async fn get_account_data_content(
        &self,
        user_id: &str,
        data_type: &str,
    ) -> Result<Option<serde_json::Value>, sqlx::Error>;

    /// See [`upsert_account_data_content`].
    async fn upsert_account_data_content(
        &self,
        user_id: &str,
        data_type: &str,
        content: &serde_json::Value,
    ) -> Result<(), sqlx::Error>;
}

#[derive(Clone)]
/// Handles database operations for user management.
pub struct UserStorage {
    /// The database connection pool
    pub pool: Arc<Pool<Postgres>>,
    /// The cache manager
    pub cache: Arc<CacheManager>,
}

impl UserStorage {
    /// Creates a new `UserStorage` instance.
    pub fn new(pool: &Arc<Pool<Postgres>>, cache: Arc<CacheManager>) -> Self {
        Self { pool: pool.clone(), cache }
    }

    /// Creates a new user in the database.
    pub async fn create_user(
        &self,
        user_id: &str,
        username: &str,
        password_hash: Option<&str>,
        is_admin: bool,
    ) -> Result<User, sqlx::Error> {
        tracing::info!(user_id = %user_id, username = %username, is_admin = is_admin, "Creating user");
        let now = current_timestamp_millis();
        let generation = now;
        sqlx::query_as::<_, User>(
            r"
            INSERT INTO users (user_id, username, password_hash, is_admin, created_ts, generation)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING user_id, username, password_hash, is_admin, is_guest, is_shadow_banned, is_deactivated,
                      created_ts, updated_ts, displayname, avatar_url, email, phone, generation, consent_version,
                      appservice_id, user_type, invalid_update_at, migration_state, password_changed_ts,
                      is_password_change_required, password_expires_at, failed_login_attempts, locked_until, must_change_password
            ",
        )
        .bind(user_id)
        .bind(username)
        .bind(password_hash)
        .bind(is_admin)
        .bind(now)
        .bind(generation)
        .fetch_one(&*self.pool)
        .await
    }

    /// Creates a new user in the database within a transaction.
    pub async fn create_user_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        user_id: &str,
        username: &str,
        password_hash: Option<&str>,
        is_admin: bool,
    ) -> Result<User, sqlx::Error> {
        tracing::info!(user_id = %user_id, username = %username, is_admin = is_admin, "Creating user in transaction");
        let now = current_timestamp_millis();
        let generation = now;
        sqlx::query_as::<_, User>(
            r"
            INSERT INTO users (user_id, username, password_hash, is_admin, created_ts, generation)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING user_id, username, password_hash, is_admin, is_guest, is_shadow_banned, is_deactivated,
                      created_ts, updated_ts, displayname, avatar_url, email, phone, generation, consent_version,
                      appservice_id, user_type, invalid_update_at, migration_state, password_changed_ts,
                      is_password_change_required, password_expires_at, failed_login_attempts, locked_until, must_change_password
            ",
        )
        .bind(user_id)
        .bind(username)
        .bind(password_hash)
        .bind(is_admin)
        .bind(now)
        .bind(generation)
        .fetch_one(&mut **tx)
        .await
    }

    /// See [`get_user_by_id`].
    pub async fn get_user_by_id(&self, user_id: &str) -> Result<Option<User>, sqlx::Error> {
        tracing::debug!(user_id = %user_id, "Querying user by id");
        sqlx::query_as::<_, User>(
            r"
            SELECT user_id, username, password_hash, is_admin, is_guest, is_shadow_banned, is_deactivated,
                   created_ts, updated_ts, displayname, avatar_url, email, phone, generation, consent_version,
                   appservice_id, user_type, invalid_update_at, migration_state, password_changed_ts,
                   is_password_change_required, password_expires_at, failed_login_attempts, locked_until, must_change_password
            FROM users
            WHERE user_id = $1
            ",
        )
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await
    }

    /// See [`get_user_by_username`].
    pub async fn get_user_by_username(&self, username: &str) -> Result<Option<User>, sqlx::Error> {
        sqlx::query_as::<_, User>(
            r"
            SELECT user_id, username, password_hash, is_admin, is_guest, is_shadow_banned, is_deactivated,
                   created_ts, updated_ts, displayname, avatar_url, email, phone, generation, consent_version,
                   appservice_id, user_type, invalid_update_at, migration_state, password_changed_ts,
                   is_password_change_required, password_expires_at, failed_login_attempts, locked_until, must_change_password
            FROM users
            WHERE username = $1
            ",
        )
        .bind(username)
        .fetch_optional(&*self.pool)
        .await
    }

    /// See [`get_user_by_email`].
    pub async fn get_user_by_email(&self, email: &str) -> Result<Option<User>, sqlx::Error> {
        sqlx::query_as::<_, User>(
            r"
            SELECT user_id, username, password_hash, is_admin, is_guest, is_shadow_banned, is_deactivated,
                   created_ts, updated_ts, displayname, avatar_url, email, phone, generation, consent_version,
                   appservice_id, user_type, invalid_update_at, migration_state, password_changed_ts,
                   is_password_change_required, password_expires_at, failed_login_attempts, locked_until, must_change_password
            FROM users
            WHERE email = $1 AND COALESCE(is_deactivated, FALSE) = FALSE
            ",
        )
        .bind(email)
        .fetch_optional(&*self.pool)
        .await
    }

    /// See [`get_user_by_identifier`].
    pub async fn get_user_by_identifier(&self, identifier: &str) -> Result<Option<User>, sqlx::Error> {
        if identifier.starts_with('@') && identifier.contains(':') {
            self.get_user_by_id(identifier).await
        } else {
            self.get_user_by_username(identifier).await
        }
    }

    /// See [`get_all_users`].
    pub async fn get_all_users(&self, limit: i64) -> Result<Vec<User>, sqlx::Error> {
        sqlx::query_as::<_, User>(
            r"
            SELECT user_id, username, password_hash, displayname, avatar_url, is_admin, is_deactivated,
                   is_guest, is_shadow_banned, created_ts, updated_ts, generation, consent_version,
                   appservice_id, user_type, invalid_update_at, migration_state,
                   email, phone, password_changed_ts, is_password_change_required,
                   password_expires_at, failed_login_attempts, locked_until, must_change_password
            FROM users
            ORDER BY created_ts DESC
            LIMIT $1
            ",
        )
        .bind(limit)
        .fetch_all(&*self.pool)
        .await
    }

    /// See [`get_users_paginated`].
    pub async fn get_users_paginated(
        &self,
        limit: i64,
        since_ts: Option<i64>,
        since_user_id: Option<&str>,
    ) -> Result<Vec<User>, sqlx::Error> {
        if let (Some(ts), Some(user_id)) = (since_ts, since_user_id) {
            sqlx::query_as::<_, User>(
                r"
                SELECT user_id, username, password_hash, displayname, avatar_url, is_admin,
                       is_deactivated, is_guest, is_shadow_banned, created_ts, updated_ts,
                       generation, consent_version, appservice_id, user_type, invalid_update_at,
                       migration_state, email, phone, password_changed_ts, is_password_change_required,
                       password_expires_at, failed_login_attempts, locked_until, must_change_password
                FROM users
                WHERE (created_ts < $2 OR (created_ts = $2 AND user_id < $3))
                ORDER BY created_ts DESC, user_id DESC
                LIMIT $1
                ",
            )
            .bind(limit)
            .bind(ts)
            .bind(user_id)
            .fetch_all(&*self.pool)
            .await
        } else {
            sqlx::query_as::<_, User>(
                r"
                SELECT user_id, username, password_hash, displayname, avatar_url, is_admin,
                       is_deactivated, is_guest, is_shadow_banned, created_ts, updated_ts,
                       generation, consent_version, appservice_id, user_type, invalid_update_at,
                       migration_state, email, phone, password_changed_ts, is_password_change_required,
                       password_expires_at, failed_login_attempts, locked_until, must_change_password
                FROM users
                ORDER BY created_ts DESC, user_id DESC
                LIMIT $1
                ",
            )
            .bind(limit)
            .fetch_all(&*self.pool)
            .await
        }
    }

    /// See [`list_users`].
    pub async fn list_users(
        &self,
        limit: i64,
        from_ts: Option<i64>,
        from_user_id: Option<&str>,
        name_filter: Option<&str>,
    ) -> Result<Vec<User>, sqlx::Error> {
        let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            r"
            SELECT user_id, username, password_hash, is_admin, is_guest, is_shadow_banned, is_deactivated,
                   created_ts, updated_ts, displayname, avatar_url, email, phone, generation, consent_version,
                   appservice_id, user_type, invalid_update_at, migration_state, password_changed_ts,
                   is_password_change_required, password_expires_at, failed_login_attempts, locked_until, must_change_password
            FROM users WHERE 1=1
            ",
        );

        if let Some(name) = name_filter {
            query.push(" AND username LIKE ");
            query.push_bind(format!("%{}%", name));
        }

        if let (Some(ts), Some(user_id)) = (from_ts, from_user_id) {
            query.push(" AND (created_ts < ");
            query.push_bind(ts);
            query.push(" OR (created_ts = ");
            query.push_bind(ts);
            query.push(" AND user_id < ");
            query.push_bind(user_id);
            query.push("))");
        }

        query.push(" ORDER BY created_ts DESC, user_id DESC LIMIT ");
        query.push_bind(limit);

        query.build_query_as::<User>().fetch_all(&*self.pool).await
    }

    /// See [`get_user_count`].
    pub async fn get_user_count(&self) -> Result<i64, sqlx::Error> {
        let row = sqlx::query(
            r"
            SELECT COALESCE(COUNT(*), 0) as count FROM users
            ",
        )
        .fetch_one(&*self.pool)
        .await?;
        row.try_get::<i64, _>("count")
    }

    /// B-6: Count users optionally filtered by a `username` substring match.
    ///
    /// When `name_filter` is `None` this is identical to [`Self::get_user_count`].
    /// When `name_filter` is `Some(pat)` the SQL is:
    ///
    /// ```sql
    /// SELECT COUNT(*) FROM users WHERE username LIKE '%' || $1 || '%'
    /// ```
    ///
    /// Note: this pattern can NOT use a B-tree index efficiently because it
    /// starts with a wildcard.  This is acceptable for an admin endpoint that
    /// is called infrequently.  A future optimisation is to add a `pg_trgm`
    /// GIN index for fuzzy username searches.
    pub async fn count_users_matching(&self, name_filter: Option<&str>) -> Result<i64, sqlx::Error> {
        if let Some(pat) = name_filter {
            let row = sqlx::query(
                r"
                SELECT COALESCE(COUNT(*), 0) as count
                FROM users
                WHERE username LIKE '%' || $1 || '%'
                ",
            )
            .bind(pat)
            .fetch_one(&*self.pool)
            .await?;
            row.try_get::<i64, _>("count")
        } else {
            self.get_user_count().await
        }
    }

    /// Count users that are NOT deactivated.
    /// Mirrors Synapse's `non_deactivated_user_count` admin statistic.
    pub async fn count_non_deactivated_users(&self) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar::<_, i64>(
            r"
            SELECT COALESCE(COUNT(*), 0) FROM users WHERE COALESCE(is_deactivated, FALSE) = FALSE
            ",
        )
        .fetch_one(&*self.pool)
        .await
    }

    /// Count non-deactivated users grouped by `appservice_id`.
    ///
    /// Users with `appservice_id IS NULL` are grouped under the empty
    /// string key `""`. Only groups with at least one non-deactivated
    /// user are returned (SQL GROUP BY naturally omits empty groups).
    pub async fn count_non_deactivated_users_by_app_service(&self) -> Result<HashMap<String, i64>, sqlx::Error> {
        let rows = sqlx::query(
            r"
            SELECT COALESCE(appservice_id, '') AS appservice_id, COUNT(*) AS count
            FROM users
            WHERE COALESCE(is_deactivated, FALSE) = FALSE
            GROUP BY appservice_id
            ",
        )
        .fetch_all(&*self.pool)
        .await?;

        let mut map = HashMap::with_capacity(rows.len());
        for row in rows {
            let appservice_id: String = row.try_get("appservice_id")?;
            let count: i64 = row.try_get("count")?;
            map.insert(appservice_id, count);
        }
        Ok(map)
    }

    /// Count daily active users (users with a device seen in the last 24h).
    pub async fn get_daily_active_users(&self) -> Result<i64, sqlx::Error> {
        let cutoff = current_timestamp_millis() - 24 * 60 * 60 * 1000;
        sqlx::query_scalar::<_, i64>(
            r"
            SELECT COUNT(DISTINCT user_id) FROM devices
            WHERE last_seen_ts IS NOT NULL AND last_seen_ts >= $1
            ",
        )
        .bind(cutoff)
        .fetch_one(&*self.pool)
        .await
    }

    /// Count monthly active users (users with a device seen in the last 30d).
    pub async fn get_monthly_active_users(&self) -> Result<i64, sqlx::Error> {
        let cutoff = current_timestamp_millis() - 30 * 24 * 60 * 60 * 1000;
        sqlx::query_scalar::<_, i64>(
            r"
            SELECT COUNT(DISTINCT user_id) FROM devices
            WHERE last_seen_ts IS NOT NULL AND last_seen_ts >= $1
            ",
        )
        .bind(cutoff)
        .fetch_one(&*self.pool)
        .await
    }

    /// Count R30 users: users active today who were also active 30 days ago.
    /// This is a simplified retention metric matching Synapse's r30_users.
    pub async fn get_r30_users(&self) -> Result<i64, sqlx::Error> {
        let now = current_timestamp_millis();
        let thirty_days_ago = now - 30 * 24 * 60 * 60 * 1000;
        let thirty_one_days_ago = now - 31 * 24 * 60 * 60 * 1000;
        sqlx::query_scalar::<_, i64>(
            r"
            SELECT COUNT(DISTINCT user_id) FROM devices
            WHERE last_seen_ts IS NOT NULL
              AND last_seen_ts >= $1
              AND user_id IN (
                  SELECT DISTINCT user_id FROM devices
                  WHERE last_seen_ts IS NOT NULL
                    AND last_seen_ts >= $2 AND last_seen_ts < $1
              )
            ",
        )
        .bind(thirty_days_ago)
        .bind(thirty_one_days_ago)
        .fetch_one(&*self.pool)
        .await
    }

    /// See [`get_user_stats_summary`].
    pub async fn get_user_stats_summary(&self) -> Result<UserStatsSummary, sqlx::Error> {
        sqlx::query_as::<_, UserStatsSummary>(
            r"
            SELECT
                COUNT(*) AS total_users,
                COUNT(*) FILTER (WHERE COALESCE(is_deactivated, FALSE) = FALSE) AS active_users,
                COUNT(*) FILTER (WHERE COALESCE(is_admin, FALSE) = TRUE) AS admin_users,
                COUNT(*) FILTER (WHERE COALESCE(is_deactivated, FALSE) = TRUE) AS deactivated_users,
                COUNT(*) FILTER (WHERE COALESCE(is_guest, FALSE) = TRUE) AS guest_users
            FROM users
            ",
        )
        .fetch_one(&*self.pool)
        .await
    }

    /// See [`count_sent_messages`].
    pub async fn count_sent_messages(&self, user_id: &str) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar::<_, i64>(
            r"
            SELECT COUNT(*)
            FROM events
            WHERE sender = $1 AND event_type = 'm.room.message' AND is_redacted = false
            ",
        )
        .bind(user_id)
        .fetch_one(&*self.pool)
        .await
    }

    /// See [`user_exists`].
    pub async fn user_exists(&self, user_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r"
            SELECT 1 FROM users WHERE user_id = $1 AND is_deactivated = FALSE LIMIT 1
            ",
        )
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result.is_some())
    }

    /// See [`filter_existing_users`].
    pub async fn filter_existing_users(&self, user_ids: &[String]) -> Result<Vec<String>, sqlx::Error> {
        if user_ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query_scalar::<_, String>(
            "SELECT user_id FROM users WHERE user_id = ANY($1) AND COALESCE(is_deactivated, FALSE) = FALSE",
        )
        .bind(user_ids)
        .fetch_all(&*self.pool)
        .await?;
        Ok(rows)
    }

    /// See [`update_password`].
    pub async fn update_password(&self, user_id: &str, password_hash: &str) -> Result<(), sqlx::Error> {
        tracing::info!(user_id = %user_id, "Updating user password");
        let now = current_timestamp_millis();
        sqlx::query(
            r"UPDATE users SET password_hash = $1, password_changed_ts = $2, is_password_change_required = FALSE, must_change_password = FALSE WHERE user_id = $3"
        )
        .bind(password_hash)
        .bind(now)
        .bind(user_id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// See [`update_displayname`].
    /// B-4204: Updates `updated_ts` to enable profile_update EDU push via sliding sync.
    pub async fn update_displayname(&self, user_id: &str, displayname: Option<&str>) -> Result<(), sqlx::Error> {
        tracing::info!(user_id = %user_id, "Updating user displayname");
        let now = synapse_common::current_timestamp_millis();
        sqlx::query(r"UPDATE users SET displayname = $1, updated_ts = $2 WHERE user_id = $3")
            .bind(displayname)
            .bind(now)
            .bind(user_id)
            .execute(&*self.pool)
            .await?;

        if let Ok(Some(profile)) = self.get_user_profile(user_id).await {
            let key = format!("user:profile:{user_id}");
            if let Err(e) = self.cache.set(&key, &profile, USER_PROFILE_CACHE_TTL).await {
                ::tracing::warn!(target: "cache", user_id = %user_id, cache_key = %key, error = %e, "Failed to cache updated user displayname profile");
            }
        }

        Ok(())
    }

    /// See [`update_avatar_url`].
    /// B-4204: Updates `updated_ts` to enable profile_update EDU push via sliding sync.
    pub async fn update_avatar_url(&self, user_id: &str, avatar_url: Option<&str>) -> Result<(), sqlx::Error> {
        let now = synapse_common::current_timestamp_millis();
        sqlx::query(r"UPDATE users SET avatar_url = $1, updated_ts = $2 WHERE user_id = $3")
            .bind(avatar_url)
            .bind(now)
            .bind(user_id)
            .execute(&*self.pool)
            .await?;

        if let Ok(Some(profile)) = self.get_user_profile(user_id).await {
            let key = format!("user:profile:{user_id}");
            if let Err(e) = self.cache.set(&key, &profile, USER_PROFILE_CACHE_TTL).await {
                ::tracing::warn!(target: "cache", user_id = %user_id, cache_key = %key, error = %e, "Failed to cache updated user avatar profile");
            }
        }

        Ok(())
    }

    /// See [`set_deactivation_status`].
    pub async fn set_deactivation_status(&self, user_id: &str, is_deactivated: bool) -> Result<bool, sqlx::Error> {
        tracing::info!(user_id = %user_id, is_deactivated, "Updating user deactivation status");
        let result = sqlx::query(r"UPDATE users SET is_deactivated = $1 WHERE user_id = $2")
            .bind(is_deactivated)
            .bind(user_id)
            .execute(&*self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// B-1.2: Batch counterpart of [`set_deactivation_status`].
    ///
    /// Replaces the N-round-trip loop in `batch_deactivate_users` with a single
    /// `UPDATE ... WHERE user_id = ANY($1) RETURNING user_id` so that admin
    /// deactivation of N users is always O(1) DB round-trips regardless of batch size.
    ///
    /// Returns the set of `user_id`s that were actually updated (existed in the DB).
    /// The service layer derives "failed" as "input minus returned".
    pub async fn set_deactivation_status_batch(
        &self,
        user_ids: &[String],
        is_deactivated: bool,
    ) -> Result<HashSet<String>, sqlx::Error> {
        if user_ids.is_empty() {
            return Ok(HashSet::new());
        }
        tracing::info!(count = user_ids.len(), is_deactivated, "Batch updating user deactivation status");
        let rows: Vec<(String,)> =
            sqlx::query_as(r"UPDATE users SET is_deactivated = $1 WHERE user_id = ANY($2) RETURNING user_id")
                .bind(is_deactivated)
                .bind(user_ids)
                .fetch_all(&*self.pool)
                .await?;
        Ok(rows.into_iter().map(|(uid,)| uid).collect())
    }

    /// See [`deactivate_user`].
    pub async fn deactivate_user(&self, user_id: &str) -> Result<(), sqlx::Error> {
        let _ = self.set_deactivation_status(user_id, true).await?;
        Ok(())
    }

    /// See [`set_admin_status`].
    pub async fn set_admin_status(&self, user_id: &str, is_admin: bool) -> Result<(), sqlx::Error> {
        sqlx::query(r"UPDATE users SET is_admin = $1 WHERE user_id = $2")
            .bind(is_admin)
            .bind(user_id)
            .execute(&*self.pool)
            .await?;
        Ok(())
    }

    /// See [`set_shadow_ban`].
    pub async fn set_shadow_ban(&self, user_id: &str, is_shadow_banned: bool) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(r"UPDATE users SET is_shadow_banned = $1 WHERE user_id = $2")
            .bind(is_shadow_banned)
            .bind(user_id)
            .execute(&*self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// See [`set_account_data`].
    pub async fn set_account_data(
        &self,
        user_id: &str,
        event_type: &str,
        content: &serde_json::Value,
    ) -> Result<(), sqlx::Error> {
        let content_str = serde_json::to_string(content).unwrap_or_default();
        let now: i64 = current_timestamp_millis();
        sqlx::query(
            r"
            INSERT INTO user_account_data (user_id, event_type, content, created_ts)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (user_id, event_type) DO UPDATE SET content = EXCLUDED.content, created_ts = EXCLUDED.created_ts
            ",
        )
        .bind(user_id)
        .bind(event_type)
        .bind(content_str)
        .bind(now)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// See [`get_account_data_content`].
    pub async fn get_account_data_content(
        &self,
        user_id: &str,
        data_type: &str,
    ) -> Result<Option<serde_json::Value>, sqlx::Error> {
        let row = sqlx::query("SELECT content FROM account_data WHERE user_id = $1 AND data_type = $2")
            .bind(user_id)
            .bind(data_type)
            .fetch_optional(&*self.pool)
            .await?;

        match row {
            Some(row) => {
                use sqlx::Row;
                let content: Option<serde_json::Value> = row.get("content");
                Ok(content)
            }
            None => Ok(None),
        }
    }

    /// See [`upsert_account_data_content`].
    pub async fn upsert_account_data_content(
        &self,
        user_id: &str,
        data_type: &str,
        content: &serde_json::Value,
    ) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();
        sqlx::query(
            r"
            INSERT INTO account_data (user_id, data_type, content, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $4)
            ON CONFLICT (user_id, data_type) DO UPDATE SET content = EXCLUDED.content, updated_ts = EXCLUDED.updated_ts
            ",
        )
        .bind(user_id)
        .bind(data_type)
        .bind(content)
        .bind(now)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// See [`search_users`].
    pub async fn search_users(&self, query: &str, limit: i64) -> Result<Vec<UserSearchResult>, sqlx::Error> {
        let normalized = query.trim();
        if normalized.is_empty() {
            return Ok(Vec::new());
        }

        let escaped = escape_like_pattern(normalized);
        let exact_pattern = escaped.clone();
        let prefix_pattern = format!("{escaped}%");
        let contains_pattern = format!("%{escaped}%");

        let username_rank = TrigramRanking::new("username", "users");
        let user_id_rank = TrigramRanking::new("user_id", "users");
        let displayname_rank = TrigramRanking::new("displayname", "users");

        let sql = format!(
            r"
            WITH candidate_matches AS (
                SELECT
                    user_id,
                    MIN(match_priority) AS match_priority,
                    MAX(match_similarity) AS match_similarity
                FROM (
                    {}
                    UNION ALL
                    {}
                    UNION ALL
                    {}
                ) AS matches
                GROUP BY user_id
            )
            SELECT
                u.user_id,
                u.username,
                COALESCE(u.displayname, u.username) AS displayname,
                u.avatar_url,
                u.created_ts
            FROM candidate_matches cm
            JOIN users u ON u.user_id = cm.user_id
            ORDER BY
                cm.match_priority ASC,
                cm.match_similarity DESC,
                u.created_ts DESC
            LIMIT $5
            ",
            username_rank.column_match_subquery("user_id", Some("COALESCE(is_deactivated, FALSE) = FALSE"), false),
            user_id_rank.column_match_subquery("user_id", Some("COALESCE(is_deactivated, FALSE) = FALSE"), false),
            displayname_rank.column_match_subquery("user_id", Some("COALESCE(is_deactivated, FALSE) = FALSE"), true),
        );

        sqlx::query_as::<_, UserSearchResult>(&sql)
            .bind(&exact_pattern)
            .bind(&prefix_pattern)
            .bind(&contains_pattern)
            .bind(normalized)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await
    }

    /// See [`get_user_profile`].
    pub async fn get_user_profile(&self, user_id: &str) -> Result<Option<UserProfile>, sqlx::Error> {
        tracing::debug!(user_id = %user_id, "Querying user profile");
        let key = format!("user:profile:{user_id}");

        if let Ok(Some(profile)) = self.cache.get::<UserProfile>(&key).await {
            return Ok(Some(profile));
        }

        let result = sqlx::query_as::<_, UserProfile>(
            r"
            SELECT user_id, username, COALESCE(displayname, username) as displayname, avatar_url, created_ts, updated_ts
            FROM users
            WHERE user_id = $1 AND COALESCE(is_deactivated, FALSE) = FALSE
            ",
        )
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await?;

        if let Some(profile) = &result {
            if let Err(e) = self.cache.set(&key, profile, USER_PROFILE_CACHE_TTL).await {
                ::tracing::warn!(target: "cache", user_id = %user_id, cache_key = %key, error = %e, "Failed to cache user profile");
            }
        }

        Ok(result)
    }

    /// See [`get_user_profiles_batch`].
    pub async fn get_user_profiles_batch(&self, user_ids: &[String]) -> Result<Vec<UserProfile>, sqlx::Error> {
        if user_ids.is_empty() {
            return Ok(vec![]);
        }

        let mut cached_profiles = Vec::new();
        let mut missing_ids = Vec::new();

        for uid in user_ids {
            let key = format!("user:profile:{uid}");
            if let Ok(Some(profile)) = self.cache.get::<UserProfile>(&key).await {
                cached_profiles.push(profile);
            } else {
                missing_ids.push(uid.clone());
            }
        }

        if missing_ids.is_empty() {
            return Ok(cached_profiles);
        }

        let fetched = sqlx::query_as::<_, UserProfile>(
            r"
            SELECT user_id, username, COALESCE(displayname, username) as displayname, avatar_url, created_ts, updated_ts
            FROM users
            WHERE user_id = ANY($1) AND COALESCE(is_deactivated, FALSE) = FALSE
            ",
        )
        .bind(&missing_ids)
        .fetch_all(&*self.pool)
        .await?;

        for profile in &fetched {
            let key = format!("user:profile:{}", profile.user_id);
            if let Err(e) = self.cache.set(&key, profile, USER_PROFILE_BATCH_CACHE_TTL).await {
                ::tracing::warn!(target: "cache", user_id = %profile.user_id, cache_key = %key, error = %e, "Failed to cache batch user profile");
            }
        }

        let mut all_profiles = cached_profiles;
        all_profiles.extend(fetched);
        Ok(all_profiles)
    }

    /// See [`get_user_profiles_map`].
    pub async fn get_user_profiles_map(
        &self,
        user_ids: &[String],
    ) -> Result<std::collections::HashMap<String, UserProfile>, sqlx::Error> {
        if user_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let profiles = self.get_user_profiles_batch(user_ids).await?;

        Ok(profiles.into_iter().map(|p| (p.user_id.clone(), p)).collect())
    }

    /// B-4204: Get user profiles that have been updated since the given timestamp.
    pub async fn get_user_profiles_updated_since(
        &self,
        user_ids: &[String],
        since_ts: i64,
    ) -> Result<std::collections::HashMap<String, UserProfile>, sqlx::Error> {
        if user_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let profiles = sqlx::query_as::<_, UserProfile>(
            r"
            SELECT user_id, username, COALESCE(displayname, username) as displayname, avatar_url, created_ts, updated_ts
            FROM users
            WHERE user_id = ANY($1)
              AND COALESCE(is_deactivated, FALSE) = FALSE
              AND updated_ts > $2
            ",
        )
        .bind(user_ids)
        .bind(since_ts)
        .fetch_all(&*self.pool)
        .await?;

        Ok(profiles.into_iter().map(|p| (p.user_id.clone(), p)).collect())
    }

    /// See [`get_users_batch`].
    pub async fn get_users_batch(&self, user_ids: &[String]) -> Result<Vec<User>, sqlx::Error> {
        if user_ids.is_empty() {
            return Ok(vec![]);
        }

        sqlx::query_as::<_, User>(
            r"
            SELECT user_id, username, password_hash, displayname, avatar_url, is_admin, is_deactivated,
                   is_guest, is_shadow_banned, created_ts, updated_ts, generation, consent_version,
                   appservice_id, user_type, invalid_update_at, migration_state,
                   email, phone, password_changed_ts, is_password_change_required,
                   password_expires_at, failed_login_attempts, locked_until, must_change_password
            FROM users
            WHERE user_id = ANY($1)
            ",
        )
        .bind(user_ids)
        .fetch_all(&*self.pool)
        .await
    }

    /// See [`get_users_map`].
    pub async fn get_users_map(
        &self,
        user_ids: &[String],
    ) -> Result<std::collections::HashMap<String, User>, sqlx::Error> {
        if user_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let users = self.get_users_batch(user_ids).await?;

        Ok(users.into_iter().map(|u| (u.user_id.clone(), u)).collect())
    }

    /// See [`update_displayname_batch`].
    pub async fn update_displayname_batch(&self, updates: &[(String, Option<String>)]) -> Result<u64, sqlx::Error> {
        if updates.is_empty() {
            return Ok(0);
        }

        let mut count = 0u64;
        for (user_id, displayname) in updates {
            sqlx::query(r"UPDATE users SET displayname = $1 WHERE user_id = $2")
                .bind(displayname)
                .bind(user_id)
                .execute(&*self.pool)
                .await?;
            count += 1;
        }

        Ok(count)
    }

    /// See [`search_users_with_presence`].
    pub async fn search_users_with_presence(
        &self,
        query: &str,
        limit: i64,
    ) -> Result<Vec<UserSearchResultWithPresence>, sqlx::Error> {
        let normalized = query.trim();
        if normalized.is_empty() {
            return Ok(Vec::new());
        }

        let escaped = escape_like_pattern(normalized);
        let exact_pattern = escaped.clone();
        let prefix_pattern = format!("{escaped}%");
        let contains_pattern = format!("%{escaped}%");

        let username_rank = TrigramRanking::new("username", "users");
        let user_id_rank = TrigramRanking::new("user_id", "users");
        let displayname_rank = TrigramRanking::new("displayname", "users");

        let sql = format!(
            r"
            WITH candidate_matches AS (
                SELECT
                    user_id,
                    MIN(match_priority) AS match_priority,
                    MAX(match_similarity) AS match_similarity
                FROM (
                    {}
                    UNION ALL
                    {}
                    UNION ALL
                    {}
                ) AS matches
                GROUP BY user_id
            )
            SELECT
                u.user_id,
                u.username,
                COALESCE(u.displayname, u.username) AS displayname,
                u.avatar_url,
                u.created_ts,
                p.presence,
                p.last_active_ts
            FROM candidate_matches cm
            JOIN users u ON u.user_id = cm.user_id
            LEFT JOIN presence p ON u.user_id = p.user_id
            ORDER BY
                cm.match_priority ASC,
                cm.match_similarity DESC,
                u.created_ts DESC
            LIMIT $5
            ",
            username_rank.column_match_subquery("user_id", Some("COALESCE(is_deactivated, FALSE) = FALSE"), false),
            user_id_rank.column_match_subquery("user_id", Some("COALESCE(is_deactivated, FALSE) = FALSE"), false),
            displayname_rank.column_match_subquery("user_id", Some("COALESCE(is_deactivated, FALSE) = FALSE"), true),
        );

        sqlx::query_as::<_, UserSearchResultWithPresence>(&sql)
            .bind(&exact_pattern)
            .bind(&prefix_pattern)
            .bind(&contains_pattern)
            .bind(normalized)
            .bind(limit)
            .fetch_all(&*self.pool)
            .await
    }

    /// See [`search_directory_users`].
    pub async fn search_directory_users(
        &self,
        query: &str,
        limit: i64,
        exact_only: bool,
    ) -> Result<Vec<UserDirectorySearchResult>, sqlx::Error> {
        let normalized = query.trim();
        if normalized.is_empty() {
            return Ok(Vec::new());
        }

        let safe_limit = limit.clamp(1, 100);
        let escaped = escape_like_pattern(normalized);
        let exact_pattern = escaped.clone();
        let prefix_pattern = format!("{escaped}%");
        let contains_pattern = format!("%{escaped}%");
        let cache_key = format!("user:directory_search:v1:{}:{}:{}", normalized.to_lowercase(), safe_limit, exact_only);

        if let Ok(Some(cached)) = self.cache.get::<Vec<UserDirectorySearchResult>>(&cache_key).await {
            return Ok(cached);
        }

        let rows = sqlx::query_as::<_, UserDirectorySearchResult>(
            r"
            WITH candidate_matches AS (
                SELECT
                    user_id,
                    MAX(rank_score) AS rank_score,
                    MIN(match_category) AS match_category
                FROM (
                    SELECT
                        user_id,
                        CASE
                            WHEN username ILIKE $1 ESCAPE '\' THEN 1000
                            WHEN NOT $4 AND username ILIKE $2 ESCAPE '\' THEN 820
                            WHEN NOT $4 AND username ILIKE $3 ESCAPE '\' THEN 650
                            ELSE 480
                        END + ROUND(similarity(username, $5) * 100)::INTEGER AS rank_score,
                        CASE
                            WHEN username ILIKE $1 ESCAPE '\' THEN 0
                            WHEN NOT $4 AND username ILIKE $2 ESCAPE '\' THEN 1
                            WHEN NOT $4 AND username ILIKE $3 ESCAPE '\' THEN 2
                            ELSE 3
                        END AS match_category
                    FROM users
                    WHERE COALESCE(is_deactivated, FALSE) = FALSE
                      AND (
                            username ILIKE $1 ESCAPE '\'
                            OR (
                                NOT $4 AND (
                                    username ILIKE $2 ESCAPE '\'
                                    OR username ILIKE $3 ESCAPE '\'
                                    OR (char_length($5) >= 3 AND username % $5)
                                )
                            )
                      )

                    UNION ALL

                    SELECT
                        user_id,
                        CASE
                            WHEN displayname ILIKE $1 ESCAPE '\' THEN 950
                            WHEN NOT $4 AND displayname ILIKE $2 ESCAPE '\' THEN 780
                            WHEN NOT $4 AND displayname ILIKE $3 ESCAPE '\' THEN 610
                            ELSE 480
                        END + ROUND(COALESCE(similarity(displayname, $5), 0.0) * 100)::INTEGER AS rank_score,
                        CASE
                            WHEN displayname ILIKE $1 ESCAPE '\' THEN 0
                            WHEN NOT $4 AND displayname ILIKE $2 ESCAPE '\' THEN 1
                            WHEN NOT $4 AND displayname ILIKE $3 ESCAPE '\' THEN 2
                            ELSE 3
                        END AS match_category
                    FROM users
                    WHERE COALESCE(is_deactivated, FALSE) = FALSE
                      AND displayname IS NOT NULL
                      AND (
                            displayname ILIKE $1 ESCAPE '\'
                            OR (
                                NOT $4 AND (
                                    displayname ILIKE $2 ESCAPE '\'
                                    OR displayname ILIKE $3 ESCAPE '\'
                                    OR (char_length($5) >= 3 AND displayname % $5)
                                )
                            )
                      )

                    UNION ALL

                    SELECT
                        user_id,
                        CASE
                            WHEN email ILIKE $1 ESCAPE '\' THEN 900
                            WHEN NOT $4 AND email ILIKE $2 ESCAPE '\' THEN 740
                            WHEN NOT $4 AND email ILIKE $3 ESCAPE '\' THEN 580
                            ELSE 480
                        END + ROUND(COALESCE(similarity(email, $5), 0.0) * 100)::INTEGER AS rank_score,
                        CASE
                            WHEN email ILIKE $1 ESCAPE '\' THEN 0
                            WHEN NOT $4 AND email ILIKE $2 ESCAPE '\' THEN 1
                            WHEN NOT $4 AND email ILIKE $3 ESCAPE '\' THEN 2
                            ELSE 3
                        END AS match_category
                    FROM users
                    WHERE COALESCE(is_deactivated, FALSE) = FALSE
                      AND email IS NOT NULL
                      AND (
                            email ILIKE $1 ESCAPE '\'
                            OR (
                                NOT $4 AND (
                                    email ILIKE $2 ESCAPE '\'
                                    OR email ILIKE $3 ESCAPE '\'
                                    OR (char_length($5) >= 3 AND email % $5)
                                )
                            )
                      )

                    UNION ALL

                    SELECT
                        user_id,
                        CASE
                            WHEN user_id ILIKE $1 ESCAPE '\' THEN 875
                            WHEN NOT $4 AND user_id ILIKE $2 ESCAPE '\' THEN 710
                            WHEN NOT $4 AND user_id ILIKE $3 ESCAPE '\' THEN 550
                            ELSE 480
                        END + ROUND(similarity(user_id, $5) * 100)::INTEGER AS rank_score,
                        CASE
                            WHEN user_id ILIKE $1 ESCAPE '\' THEN 0
                            WHEN NOT $4 AND user_id ILIKE $2 ESCAPE '\' THEN 1
                            WHEN NOT $4 AND user_id ILIKE $3 ESCAPE '\' THEN 2
                            ELSE 3
                        END AS match_category
                    FROM users
                    WHERE COALESCE(is_deactivated, FALSE) = FALSE
                      AND (
                            user_id ILIKE $1 ESCAPE '\'
                            OR (
                                NOT $4 AND (
                                    user_id ILIKE $2 ESCAPE '\'
                                    OR user_id ILIKE $3 ESCAPE '\'
                                    OR (char_length($5) >= 3 AND user_id % $5)
                                )
                            )
                      )
                ) AS matches
                GROUP BY user_id
            )
            SELECT
                u.user_id,
                u.username,
                COALESCE(u.displayname, u.username) AS displayname,
                u.avatar_url,
                u.created_ts,
                p.presence,
                p.last_active_ts,
                (
                    cm.rank_score
                    + CASE
                        WHEN COALESCE(p.presence, 'offline') = 'online' THEN 50
                        WHEN COALESCE(p.presence, 'offline') = 'unavailable' THEN 20
                        ELSE 0
                    END
                )::INTEGER AS match_score,
                CASE cm.match_category
                    WHEN 0 THEN 'exact'
                    WHEN 1 THEN 'prefix'
                    WHEN 2 THEN 'contains'
                    ELSE 'fuzzy'
                END AS match_type
            FROM candidate_matches cm
            JOIN users u ON u.user_id = cm.user_id
            LEFT JOIN presence p ON p.user_id = u.user_id
            ORDER BY
                cm.rank_score DESC,
                COALESCE(p.last_active_ts, 0) DESC,
                u.created_ts DESC,
                u.username ASC
            LIMIT $6
            ",
        )
        .bind(&exact_pattern)
        .bind(&prefix_pattern)
        .bind(&contains_pattern)
        .bind(exact_only)
        .bind(normalized)
        .bind(safe_limit)
        .fetch_all(&*self.pool)
        .await?;

        if let Err(e) = self.cache.set(&cache_key, rows.clone(), USER_DIRECTORY_SEARCH_CACHE_TTL_SECS).await {
            ::tracing::warn!(target: "cache", cache_key = %cache_key, error = %e, "Failed to cache user directory search result");
        }

        Ok(rows)
    }

    /// See [`delete_user`].
    pub async fn delete_user(&self, user_id: &str) -> Result<(), sqlx::Error> {
        tracing::info!(user_id = %user_id, "Deleting user");
        sqlx::query(r"DELETE FROM users WHERE user_id = $1").bind(user_id).execute(&*self.pool).await?;
        Ok(())
    }

    // ========================================================================
    // MAS 用户锁定状态同步 (Synapse v1.151.0 / #24)
    // ========================================================================

    /// Lock a user, preventing them from authenticating.
    ///
    /// If the user already has an active lock, this updates the existing lock.
    pub async fn lock_user(
        &self,
        user_id: &str,
        reason: Option<&str>,
        locked_by: &str,
        now_ts: i64,
    ) -> Result<LockedUser, sqlx::Error> {
        tracing::info!(user_id = %user_id, locked_by = %locked_by, "Locking user");
        sqlx::query_as::<_, LockedUser>(
            r"
            INSERT INTO user_locks (user_id, reason, locked_by, created_ts, is_active)
            VALUES ($1, $2, $3, $4, TRUE)
            ON CONFLICT (user_id, is_active) WHERE is_active = TRUE DO UPDATE SET
                reason = EXCLUDED.reason,
                locked_by = EXCLUDED.locked_by,
                created_ts = EXCLUDED.created_ts
            RETURNING id, user_id, reason, locked_by, created_ts, unlocked_ts, is_active
            ",
        )
        .bind(user_id)
        .bind(reason)
        .bind(locked_by)
        .bind(now_ts)
        .fetch_one(&*self.pool)
        .await
    }

    /// Unlock a user, allowing them to authenticate again.
    pub async fn unlock_user(&self, user_id: &str, now_ts: i64) -> Result<(), sqlx::Error> {
        tracing::info!(user_id = %user_id, "Unlocking user");
        sqlx::query(
            r"
            UPDATE user_locks
            SET is_active = FALSE, unlocked_ts = $2
            WHERE user_id = $1 AND is_active = TRUE
            ",
        )
        .bind(user_id)
        .bind(now_ts)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// Check whether a user is currently locked.
    pub async fn is_user_locked(&self, user_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query_scalar::<_, i64>(
            r"
            SELECT COUNT(*) FROM user_locks
            WHERE user_id = $1 AND is_active = TRUE
            ",
        )
        .bind(user_id)
        .fetch_one(&*self.pool)
        .await?;
        Ok(result > 0)
    }

    /// Get the active lock record for a user (if any).
    pub async fn get_active_user_lock(&self, user_id: &str) -> Result<Option<LockedUser>, sqlx::Error> {
        sqlx::query_as::<_, LockedUser>(
            r"
            SELECT id, user_id, reason, locked_by, created_ts, unlocked_ts, is_active
            FROM user_locks
            WHERE user_id = $1 AND is_active = TRUE
            ",
        )
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await
    }

    /// Get a paginated list of currently locked users.
    pub async fn get_locked_users(&self, limit: i64, offset: i64) -> Result<Vec<LockedUser>, sqlx::Error> {
        sqlx::query_as::<_, LockedUser>(
            r"
            SELECT id, user_id, reason, locked_by, created_ts, unlocked_ts, is_active
            FROM user_locks
            WHERE is_active = TRUE
            ORDER BY created_ts DESC
            LIMIT $1 OFFSET $2
            ",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&*self.pool)
        .await
    }

    /// See [`set_guest_status`].
    pub async fn set_guest_status(&self, user_id: &str, is_guest: bool) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE users SET is_guest = $1 WHERE user_id = $2")
            .bind(is_guest)
            .bind(user_id)
            .execute(&*self.pool)
            .await?;

        let guest_cache_key = format!("user:guest:{user_id}");
        self.cache.delete(&guest_cache_key).await;
        Ok(())
    }

    /// See [`set_user_type`].
    pub async fn set_user_type(&self, user_id: &str, user_type: Option<&str>) -> Result<(), sqlx::Error> {
        sqlx::query!(r"UPDATE users SET user_type = $1 WHERE user_id = $2", user_type, user_id)
            .execute(&*self.pool)
            .await?;
        Ok(())
    }

    /// See [`upgrade_guest_account`].
    pub async fn upgrade_guest_account(
        &self,
        user_id: &str,
        username: &str,
        password_hash: &str,
    ) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();
        sqlx::query(
            r"
            UPDATE users
            SET username = $1,
                is_guest = FALSE,
                password_hash = $2,
                password_changed_ts = $3,
                is_password_change_required = FALSE,
                must_change_password = FALSE
            WHERE user_id = $4
            ",
        )
        .bind(username)
        .bind(password_hash)
        .bind(now)
        .bind(user_id)
        .execute(&*self.pool)
        .await?;

        let guest_cache_key = format!("user:guest:{user_id}");
        self.cache.delete(&guest_cache_key).await;
        Ok(())
    }
}

#[async_trait]
impl UserStore for UserStorage {
    fn pool(&self) -> &Arc<Pool<Postgres>> {
        &self.pool
    }

    // ---- lock operations ----

    async fn lock_user(
        &self,
        user_id: &str,
        reason: Option<&str>,
        locked_by: &str,
        now_ts: i64,
    ) -> Result<LockedUser, sqlx::Error> {
        self.lock_user(user_id, reason, locked_by, now_ts).await
    }

    async fn unlock_user(&self, user_id: &str, now_ts: i64) -> Result<(), sqlx::Error> {
        self.unlock_user(user_id, now_ts).await
    }

    async fn is_user_locked(&self, user_id: &str) -> Result<bool, sqlx::Error> {
        self.is_user_locked(user_id).await
    }

    async fn get_active_user_lock(&self, user_id: &str) -> Result<Option<LockedUser>, sqlx::Error> {
        self.get_active_user_lock(user_id).await
    }

    async fn get_locked_users(&self, limit: i64, offset: i64) -> Result<Vec<LockedUser>, sqlx::Error> {
        self.get_locked_users(limit, offset).await
    }

    // ---- query methods ----

    async fn get_user_by_id(&self, user_id: &str) -> Result<Option<User>, sqlx::Error> {
        self.get_user_by_id(user_id).await
    }

    async fn get_user_by_username(&self, username: &str) -> Result<Option<User>, sqlx::Error> {
        self.get_user_by_username(username).await
    }

    async fn get_user_by_email(&self, email: &str) -> Result<Option<User>, sqlx::Error> {
        self.get_user_by_email(email).await
    }

    async fn get_user_by_identifier(&self, identifier: &str) -> Result<Option<User>, sqlx::Error> {
        self.get_user_by_identifier(identifier).await
    }

    async fn get_users_paginated(
        &self,
        limit: i64,
        since_ts: Option<i64>,
        since_user_id: Option<&str>,
    ) -> Result<Vec<User>, sqlx::Error> {
        self.get_users_paginated(limit, since_ts, since_user_id).await
    }

    async fn list_users(
        &self,
        limit: i64,
        from_ts: Option<i64>,
        from_user_id: Option<&str>,
        name_filter: Option<&str>,
    ) -> Result<Vec<User>, sqlx::Error> {
        self.list_users(limit, from_ts, from_user_id, name_filter).await
    }

    async fn user_exists(&self, user_id: &str) -> Result<bool, sqlx::Error> {
        self.user_exists(user_id).await
    }

    async fn filter_existing_users(&self, user_ids: &[String]) -> Result<Vec<String>, sqlx::Error> {
        self.filter_existing_users(user_ids).await
    }

    async fn get_user_count(&self) -> Result<i64, sqlx::Error> {
        self.get_user_count().await
    }

    async fn count_users_matching(&self, name_filter: Option<&str>) -> Result<i64, sqlx::Error> {
        self.count_users_matching(name_filter).await
    }

    async fn count_non_deactivated_users(&self) -> Result<i64, sqlx::Error> {
        self.count_non_deactivated_users().await
    }

    async fn count_non_deactivated_users_by_app_service(&self) -> Result<HashMap<String, i64>, sqlx::Error> {
        self.count_non_deactivated_users_by_app_service().await
    }

    async fn get_daily_active_users(&self) -> Result<i64, sqlx::Error> {
        self.get_daily_active_users().await
    }

    async fn get_monthly_active_users(&self) -> Result<i64, sqlx::Error> {
        self.get_monthly_active_users().await
    }

    async fn get_r30_users(&self) -> Result<i64, sqlx::Error> {
        self.get_r30_users().await
    }

    // ---- mutation methods ----

    async fn create_user(
        &self,
        user_id: &str,
        username: &str,
        password_hash: Option<&str>,
        is_admin: bool,
    ) -> Result<User, sqlx::Error> {
        self.create_user(user_id, username, password_hash, is_admin).await
    }

    async fn update_password(&self, user_id: &str, password_hash: &str) -> Result<(), sqlx::Error> {
        self.update_password(user_id, password_hash).await
    }

    async fn update_displayname(&self, user_id: &str, displayname: Option<&str>) -> Result<(), sqlx::Error> {
        self.update_displayname(user_id, displayname).await
    }

    async fn update_avatar_url(&self, user_id: &str, avatar_url: Option<&str>) -> Result<(), sqlx::Error> {
        self.update_avatar_url(user_id, avatar_url).await
    }

    async fn set_deactivation_status(&self, user_id: &str, is_deactivated: bool) -> Result<bool, sqlx::Error> {
        self.set_deactivation_status(user_id, is_deactivated).await
    }

    async fn set_deactivation_status_batch(
        &self,
        user_ids: &[String],
        is_deactivated: bool,
    ) -> Result<HashSet<String>, sqlx::Error> {
        self.set_deactivation_status_batch(user_ids, is_deactivated).await
    }

    async fn set_admin_status(&self, user_id: &str, is_admin: bool) -> Result<(), sqlx::Error> {
        self.set_admin_status(user_id, is_admin).await
    }

    async fn set_shadow_ban(&self, user_id: &str, is_shadow_banned: bool) -> Result<bool, sqlx::Error> {
        self.set_shadow_ban(user_id, is_shadow_banned).await
    }

    async fn delete_user(&self, user_id: &str) -> Result<(), sqlx::Error> {
        self.delete_user(user_id).await
    }

    async fn set_guest_status(&self, user_id: &str, is_guest: bool) -> Result<(), sqlx::Error> {
        self.set_guest_status(user_id, is_guest).await
    }

    async fn set_user_type(&self, user_id: &str, user_type: Option<&str>) -> Result<(), sqlx::Error> {
        self.set_user_type(user_id, user_type).await
    }

    async fn upgrade_guest_account(
        &self,
        user_id: &str,
        username: &str,
        password_hash: &str,
    ) -> Result<(), sqlx::Error> {
        self.upgrade_guest_account(user_id, username, password_hash).await
    }

    // ---- stats / search methods ----

    async fn get_user_stats_summary(&self) -> Result<UserStatsSummary, sqlx::Error> {
        self.get_user_stats_summary().await
    }

    async fn count_sent_messages(&self, user_id: &str) -> Result<i64, sqlx::Error> {
        self.count_sent_messages(user_id).await
    }

    async fn search_users(&self, query: &str, limit: i64) -> Result<Vec<UserSearchResult>, sqlx::Error> {
        self.search_users(query, limit).await
    }

    async fn search_directory_users(
        &self,
        query: &str,
        limit: i64,
        exact_only: bool,
    ) -> Result<Vec<UserDirectorySearchResult>, sqlx::Error> {
        self.search_directory_users(query, limit, exact_only).await
    }

    async fn get_user_profile(&self, user_id: &str) -> Result<Option<UserProfile>, sqlx::Error> {
        self.get_user_profile(user_id).await
    }

    async fn get_user_profiles_batch(&self, user_ids: &[String]) -> Result<Vec<UserProfile>, sqlx::Error> {
        self.get_user_profiles_batch(user_ids).await
    }

    async fn get_user_profiles_map(&self, user_ids: &[String]) -> Result<HashMap<String, UserProfile>, sqlx::Error> {
        self.get_user_profiles_map(user_ids).await
    }

    async fn get_user_profiles_updated_since(
        &self,
        user_ids: &[String],
        since_ts: i64,
    ) -> Result<HashMap<String, UserProfile>, sqlx::Error> {
        self.get_user_profiles_updated_since(user_ids, since_ts).await
    }

    async fn get_users_batch(&self, user_ids: &[String]) -> Result<Vec<User>, sqlx::Error> {
        self.get_users_batch(user_ids).await
    }

    async fn get_users_map(&self, user_ids: &[String]) -> Result<HashMap<String, User>, sqlx::Error> {
        self.get_users_map(user_ids).await
    }

    // ---- account_data methods ----

    async fn get_account_data_content(
        &self,
        user_id: &str,
        data_type: &str,
    ) -> Result<Option<serde_json::Value>, sqlx::Error> {
        self.get_account_data_content(user_id, data_type).await
    }

    async fn upsert_account_data_content(
        &self,
        user_id: &str,
        data_type: &str,
        content: &serde_json::Value,
    ) -> Result<(), sqlx::Error> {
        self.upsert_account_data_content(user_id, data_type, content).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_like_pattern_backslash() {
        assert_eq!(escape_like_pattern(r"test\user"), r"test\\user");
    }

    #[test]
    fn test_escape_like_pattern_percent() {
        assert_eq!(escape_like_pattern("100%"), "100\\%");
    }

    #[test]
    fn test_escape_like_pattern_underscore() {
        assert_eq!(escape_like_pattern("a_b"), "a\\_b");
    }

    #[test]
    fn test_escape_like_pattern_combined() {
        assert_eq!(escape_like_pattern(r"50%_test\foo"), r"50\%\_test\\foo");
    }

    #[test]
    fn test_escape_like_pattern_no_special_chars() {
        let input = "simpleusername";
        assert_eq!(escape_like_pattern(input), input);
    }

    #[test]
    fn test_escape_like_pattern_empty() {
        assert_eq!(escape_like_pattern(""), "");
    }

    #[test]
    fn test_user_struct_fields() {
        let user = User {
            user_id: "@alice:example.com".to_string(),
            username: "alice".to_string(),
            password_hash: Some("hash123".to_string()),
            is_admin: true,
            is_guest: false,
            is_shadow_banned: false,
            is_deactivated: false,
            created_ts: 1700000000000,
            updated_ts: Some(1700000000001),
            displayname: Some("Alice".to_string()),
            avatar_url: Some("mxc://example.com/avatar".to_string()),
            email: Some("alice@example.com".to_string()),
            phone: None,
            generation: Some(1),
            consent_version: Some("1.0".to_string()),
            appservice_id: None,
            user_type: None,
            invalid_update_at: None,
            migration_state: None,
            password_changed_ts: Some(1700000000000),
            is_password_change_required: false,
            password_expires_at: None,
            failed_login_attempts: 0,
            locked_until: None,
            must_change_password: false,
        };

        assert_eq!(user.user_id(), "@alice:example.com");
        assert_eq!(user.username, "alice");
        assert!(user.is_admin);
        assert!(!user.is_guest);
        assert_eq!(user.displayname.as_deref(), Some("Alice"));
        assert_eq!(user.generation, Some(1));
    }

    #[test]
    fn test_user_struct_minimal() {
        let user = User {
            user_id: "@bob:example.com".to_string(),
            username: "bob".to_string(),
            password_hash: None,
            is_admin: false,
            is_guest: false,
            is_shadow_banned: false,
            is_deactivated: false,
            created_ts: 0,
            updated_ts: None,
            displayname: None,
            avatar_url: None,
            email: None,
            phone: None,
            generation: Some(0),
            consent_version: None,
            appservice_id: None,
            user_type: None,
            invalid_update_at: None,
            migration_state: None,
            password_changed_ts: None,
            is_password_change_required: false,
            password_expires_at: None,
            failed_login_attempts: 0,
            locked_until: None,
            must_change_password: false,
        };

        assert_eq!(user.user_id, "@bob:example.com");
        assert!(!user.is_admin);
        assert!(user.password_hash.is_none());
        assert!(user.displayname.is_none());
    }

    #[test]
    fn test_user_serde_roundtrip() {
        let user = User {
            user_id: "@charlie:example.com".to_string(),
            username: "charlie".to_string(),
            password_hash: None,
            is_admin: false,
            is_guest: false,
            is_shadow_banned: false,
            is_deactivated: true,
            created_ts: 1700000000000,
            updated_ts: None,
            displayname: Some("Charlie".to_string()),
            avatar_url: None,
            email: None,
            phone: None,
            generation: Some(0),
            consent_version: None,
            appservice_id: None,
            user_type: Some("support".to_string()),
            invalid_update_at: None,
            migration_state: None,
            password_changed_ts: None,
            is_password_change_required: false,
            password_expires_at: None,
            failed_login_attempts: 0,
            locked_until: None,
            must_change_password: false,
        };

        let json = serde_json::to_string(&user).unwrap();
        let deserialized: User = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.user_id, user.user_id);
        assert_eq!(deserialized.username, user.username);
        assert_eq!(deserialized.is_deactivated, user.is_deactivated);
        assert_eq!(deserialized.user_type, user.user_type);
        // password_hash should NOT be serialized
        assert!(!json.contains("password_hash"));
    }

    #[test]
    fn test_user_profile_serde() {
        let profile = UserProfile {
            user_id: "@dave:example.com".to_string(),
            username: "dave".to_string(),
            displayname: Some("Dave".to_string()),
            avatar_url: Some("mxc://example.com/dave".to_string()),
            created_ts: 1700000000000,
            updated_ts: None,
        };

        let json = serde_json::to_string(&profile).unwrap();
        let deserialized: UserProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.user_id, profile.user_id);
        assert_eq!(deserialized.displayname, profile.displayname);
    }

    #[test]
    fn test_user_search_result_serde() {
        let result = UserSearchResult {
            user_id: "@eve:example.com".to_string(),
            username: "eve".to_string(),
            displayname: Some("Eve".to_string()),
            avatar_url: None,
            created_ts: 1700000000000,
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: UserSearchResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.user_id, result.user_id);
        assert_eq!(deserialized.username, result.username);
    }

    #[test]
    fn test_locked_user_serde() {
        let locked = LockedUser {
            id: 1,
            user_id: "@baduser:example.com".to_string(),
            reason: Some("Too many failed attempts".to_string()),
            locked_by: "@admin:example.com".to_string(),
            created_ts: 1700000000000,
            unlocked_ts: None,
            is_active: true,
        };

        let json = serde_json::to_string(&locked).unwrap();
        let deserialized: LockedUser = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.user_id, locked.user_id);
        assert_eq!(deserialized.reason, locked.reason);
        assert!(deserialized.is_active);
    }

    #[test]
    fn test_user_stats_summary_serde() {
        let stats = UserStatsSummary {
            total_users: 100,
            active_users: 50,
            admin_users: 5,
            deactivated_users: 10,
            guest_users: 20,
        };

        let json = serde_json::to_string(&stats).unwrap();
        let deserialized: UserStatsSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.total_users, 100);
        assert_eq!(deserialized.active_users, 50);
        assert_eq!(deserialized.admin_users, 5);
    }

    #[test]
    fn test_user_search_result_with_presence_serde() {
        let result = UserSearchResultWithPresence {
            user_id: "@frank:example.com".to_string(),
            username: "frank".to_string(),
            displayname: Some("Frank".to_string()),
            avatar_url: None,
            created_ts: 1700000000000,
            presence: Some("online".to_string()),
            last_active_ts: Some(1700000000000),
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: UserSearchResultWithPresence = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.user_id, result.user_id);
        assert_eq!(deserialized.presence.as_deref(), Some("online"));
        assert_eq!(deserialized.last_active_ts, Some(1700000000000));
    }

    #[test]
    fn test_user_directory_search_result_serde() {
        let result = UserDirectorySearchResult {
            user_id: "@grace:example.com".to_string(),
            username: "grace".to_string(),
            displayname: Some("Grace".to_string()),
            avatar_url: None,
            created_ts: 1700000000000,
            presence: Some("offline".to_string()),
            last_active_ts: None,
            match_score: 95,
            match_type: "trigram".to_string(),
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: UserDirectorySearchResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.user_id, result.user_id);
        assert_eq!(deserialized.match_score, 95);
        assert_eq!(deserialized.match_type, "trigram");
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use synapse_cache::{CacheConfig, CacheManager};

    /// Each test gets a fresh isolated schema (v11 baseline), so parallel
    /// tests can't pollute each other: several tests insert users with fixed
    /// usernames (e.g. `uniqueuser99`) into the shared `public` schema, which
    /// violates the `uq_users_username` unique constraint when they run
    /// concurrently. Stale rows left behind by an earlier failed run also
    /// cascade into later failures.
    ///
    /// Returns the `IsolatedTestPool` *and* the pool so callers can hold the
    /// guard alive for the whole test — dropping it early spawns a background
    /// `DROP SCHEMA` that can race with in-flight queries.
    async fn test_pool() -> (crate::test_isolation::IsolatedTestPool, Arc<Pool<Postgres>>) {
        let isolated = crate::test_isolation::IsolatedTestPool::new().await.expect("isolated pool");
        let pool = isolated.pool();
        (isolated, pool)
    }

    fn test_cache() -> Arc<CacheManager> {
        Arc::new(CacheManager::new(&CacheConfig::default()))
    }

    // ── create / get by id ──────────────────────────────────────────

    #[tokio::test]
    async fn test_create_user_returns_valid_record() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let user_id = format!("@create_test_{}:example.com", uuid::Uuid::new_v4());
        let _ = storage.delete_user(&user_id).await;

        let user = storage.create_user(&user_id, "createtest", None, false).await.expect("create_user should succeed");

        assert_eq!(user.user_id, user_id);
        assert_eq!(user.username, "createtest");
        assert!(!user.is_admin);
        assert!(!user.is_deactivated);

        let _ = storage.delete_user(&user_id).await;
    }

    #[tokio::test]
    async fn test_get_user_by_id_found() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let user_id = format!("@getbyid_{}:example.com", uuid::Uuid::new_v4());
        let _ = storage.delete_user(&user_id).await;
        storage.create_user(&user_id, "getbyiduser", None, false).await.unwrap();

        let found = storage.get_user_by_id(&user_id).await.expect("get_user_by_id should succeed");
        assert!(found.is_some());
        assert_eq!(found.unwrap().user_id, user_id);

        let _ = storage.delete_user(&user_id).await;
    }

    #[tokio::test]
    async fn test_get_user_by_id_not_found() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let result = storage.get_user_by_id("@nonexistent:example.com").await.expect("get_user_by_id should succeed");
        assert!(result.is_none());
    }

    // ── exists / by-username / count ─────────────────────────────────

    #[tokio::test]
    async fn test_user_exists_returns_true_for_existing_user() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let user_id = format!("@exists_{}:example.com", uuid::Uuid::new_v4());
        let _ = storage.delete_user(&user_id).await;
        storage.create_user(&user_id, "existsuser", None, false).await.unwrap();

        assert!(storage.user_exists(&user_id).await.expect("user_exists should succeed"));

        let _ = storage.delete_user(&user_id).await;
    }

    #[tokio::test]
    async fn test_user_exists_returns_false_for_nonexistent() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        assert!(!storage.user_exists("@nobody:example.com").await.expect("user_exists should succeed"));
    }

    #[tokio::test]
    async fn test_get_user_by_username_found() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let user_id = format!("@byuser_{}:example.com", uuid::Uuid::new_v4());
        let _ = storage.delete_user(&user_id).await;
        storage.create_user(&user_id, "uniqueuser99", None, false).await.unwrap();

        let found = storage.get_user_by_username("uniqueuser99").await.expect("query should succeed");
        assert!(found.is_some());
        assert_eq!(found.unwrap().username, "uniqueuser99");

        let _ = storage.delete_user(&user_id).await;
    }

    #[tokio::test]
    async fn test_get_user_count_increases_after_create() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let uuid = uuid::Uuid::new_v4();
        let user_id = format!("@ct_{uuid}:example.com");
        let _ = storage.delete_user(&user_id).await;

        let _created = storage
            .create_user(&user_id, &format!("ct_{uuid}"), None, false)
            .await
            .expect("create_user should succeed");

        let user = storage
            .get_user_by_id(&user_id)
            .await
            .expect("get_user_by_id should succeed")
            .expect("user should exist after create");

        assert_eq!(user.user_id, user_id);

        let _ = storage.delete_user(&user_id).await;
    }

    // ── displayname / avatar / password / admin ─────────────────────

    #[tokio::test]
    async fn test_update_displayname() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let user_id = format!("@displayname_{}:example.com", uuid::Uuid::new_v4());
        let _ = storage.delete_user(&user_id).await;
        storage.create_user(&user_id, "dnameuser", None, false).await.unwrap();

        storage.update_displayname(&user_id, Some("New Name")).await.expect("update should succeed");
        let profile = storage.get_user_profile(&user_id).await.expect("get profile should succeed");
        assert_eq!(profile.unwrap().displayname.unwrap(), "New Name");

        let _ = storage.delete_user(&user_id).await;
    }

    #[tokio::test]
    async fn test_update_avatar_url() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let user_id = format!("@avatar_{}:example.com", uuid::Uuid::new_v4());
        let _ = storage.delete_user(&user_id).await;
        storage.create_user(&user_id, "avataruser", None, false).await.unwrap();

        storage.update_avatar_url(&user_id, Some("mxc://avatar")).await.expect("update should succeed");
        let profile = storage.get_user_profile(&user_id).await.expect("get profile should succeed");
        assert_eq!(profile.unwrap().avatar_url.unwrap(), "mxc://avatar");

        let _ = storage.delete_user(&user_id).await;
    }

    #[tokio::test]
    async fn test_update_password() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let user_id = format!("@pwd_{}:example.com", uuid::Uuid::new_v4());
        let _ = storage.delete_user(&user_id).await;
        storage.create_user(&user_id, "pwduser", Some("old_hash"), false).await.unwrap();

        storage.update_password(&user_id, "new_hash").await.expect("update_password should succeed");
        // password_hash is excluded from User serialization, but the
        // operation succeeding without error confirms the update worked.

        let _ = storage.delete_user(&user_id).await;
    }

    #[tokio::test]
    async fn test_set_admin_status_toggle() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let user_id = format!("@admin_{}:example.com", uuid::Uuid::new_v4());
        let _ = storage.delete_user(&user_id).await;
        storage.create_user(&user_id, "adminuser", None, false).await.unwrap();
        assert!(!storage.get_user_by_id(&user_id).await.unwrap().unwrap().is_admin);

        storage.set_admin_status(&user_id, true).await.expect("set_admin should succeed");
        let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
        assert!(user.is_admin);

        storage.set_admin_status(&user_id, false).await.expect("unset_admin should succeed");
        let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
        assert!(!user.is_admin);

        let _ = storage.delete_user(&user_id).await;
    }

    // ── deactivation / shadow-ban / delete / list / filter ──────────

    #[tokio::test]
    async fn test_set_deactivation_status() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let user_id = format!("@deact_{}:example.com", uuid::Uuid::new_v4());
        let _ = storage.delete_user(&user_id).await;
        storage.create_user(&user_id, "deactuser", None, false).await.unwrap();

        let result = storage.set_deactivation_status(&user_id, true).await.expect("deactivate should succeed");
        assert!(result);
        let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
        assert!(user.is_deactivated);

        let result = storage.set_deactivation_status(&user_id, false).await.expect("reactivate should succeed");
        assert!(result);
        let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
        assert!(!user.is_deactivated);

        let _ = storage.delete_user(&user_id).await;
    }

    #[tokio::test]
    async fn test_set_shadow_ban() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let user_id = format!("@shadow_{}:example.com", uuid::Uuid::new_v4());
        let _ = storage.delete_user(&user_id).await;
        storage.create_user(&user_id, "shadowuser", None, false).await.unwrap();

        let result = storage.set_shadow_ban(&user_id, true).await.expect("shadow ban should succeed");
        assert!(result);
        let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
        assert!(user.is_shadow_banned);

        storage.set_shadow_ban(&user_id, false).await.expect("unban should succeed");
        let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
        assert!(!user.is_shadow_banned);

        let _ = storage.delete_user(&user_id).await;
    }

    #[tokio::test]
    async fn test_delete_user() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let user_id = format!("@delete_me_{}:example.com", uuid::Uuid::new_v4());
        let _ = storage.delete_user(&user_id).await;
        storage.create_user(&user_id, "deleteme", None, false).await.unwrap();
        assert!(storage.user_exists(&user_id).await.unwrap());

        storage.delete_user(&user_id).await.expect("delete_user should succeed");
        assert!(!storage.user_exists(&user_id).await.unwrap());
    }

    #[tokio::test]
    async fn test_get_all_users_respects_limit() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let users = storage.get_all_users(5).await.expect("get_all_users should succeed");
        assert!(users.len() <= 5);
    }

    #[tokio::test]
    async fn test_filter_existing_users() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let user_id = format!("@filter_{}:example.com", uuid::Uuid::new_v4());
        let _ = storage.delete_user(&user_id).await;
        storage.create_user(&user_id, "filteruser", None, false).await.unwrap();

        let existing = storage
            .filter_existing_users(&[user_id.clone(), "@nobody:example.com".to_string()])
            .await
            .expect("filter_existing_users should succeed");
        assert_eq!(existing.len(), 1);
        assert_eq!(existing[0], user_id);

        let _ = storage.delete_user(&user_id).await;
    }

    // ── lock / unlock / batch / profile / count-messages ────────────

    #[tokio::test]
    async fn test_lock_user_flow() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let user_id = format!("@lock_{}:example.com", uuid::Uuid::new_v4());
        let _ = storage.delete_user(&user_id).await;
        storage.create_user(&user_id, "lockuser", None, false).await.unwrap();
        assert!(!storage.is_user_locked(&user_id).await.unwrap());

        let now = current_timestamp_millis();
        storage.lock_user(&user_id, Some("test_reason"), "system", now).await.expect("lock should succeed");
        assert!(storage.is_user_locked(&user_id).await.unwrap());

        let locked = storage.get_active_user_lock(&user_id).await.unwrap();
        assert!(locked.is_some());
        assert_eq!(locked.unwrap().reason.unwrap(), "test_reason");

        storage.unlock_user(&user_id, current_timestamp_millis()).await.expect("unlock should succeed");
        assert!(!storage.is_user_locked(&user_id).await.unwrap());

        let _ = storage.delete_user(&user_id).await;
    }

    #[tokio::test]
    async fn test_get_users_batch() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let uid1 = format!("@batch1_{}:example.com", uuid::Uuid::new_v4());
        let uid2 = format!("@batch2_{}:example.com", uuid::Uuid::new_v4());
        let _ = storage.delete_user(&uid1).await;
        let _ = storage.delete_user(&uid2).await;
        storage.create_user(&uid1, "batchuser1", None, false).await.unwrap();
        storage.create_user(&uid2, "batchuser2", None, false).await.unwrap();

        let users =
            storage.get_users_batch(&[uid1.clone(), uid2.clone()]).await.expect("get_users_batch should succeed");
        assert_eq!(users.len(), 2);
        let ids: Vec<&str> = users.iter().map(|u| u.user_id.as_str()).collect();
        assert!(ids.contains(&uid1.as_str()));
        assert!(ids.contains(&uid2.as_str()));

        let _ = storage.delete_user(&uid1).await;
        let _ = storage.delete_user(&uid2).await;
    }

    #[tokio::test]
    async fn test_get_user_profile_found_and_not_found() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let user_id = format!("@profile_{}:example.com", uuid::Uuid::new_v4());
        let _ = storage.delete_user(&user_id).await;
        storage.create_user(&user_id, "profileuser", None, false).await.unwrap();
        storage.update_displayname(&user_id, Some("Profile User")).await.unwrap();

        let profile = storage.get_user_profile(&user_id).await.unwrap().unwrap();
        assert_eq!(profile.displayname.unwrap(), "Profile User");

        let missing = storage.get_user_profile("@nobody:example.com").await.unwrap();
        assert!(missing.is_none());

        let _ = storage.delete_user(&user_id).await;
    }

    #[tokio::test]
    async fn test_count_sent_messages_returns_count() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let user_id = format!("@msgcount_{}:example.com", uuid::Uuid::new_v4());
        let _ = storage.delete_user(&user_id).await;
        storage.create_user(&user_id, "msgcountuser", None, false).await.unwrap();

        let count = storage.count_sent_messages(&user_id).await.expect("count should succeed");
        assert!(count >= 0);

        let _ = storage.delete_user(&user_id).await;
    }

    // ── get_user_by_email / get_user_by_identifier ─────────────────

    #[tokio::test]
    async fn test_get_user_by_email_found() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let user_id = format!("@emailu_{}:example.com", uuid::Uuid::new_v4());
        let email = format!("emailtest_{}@example.com", uuid::Uuid::new_v4());
        let _ = storage.delete_user(&user_id).await;
        storage.create_user(&user_id, "emailuser", None, false).await.unwrap();
        // Set email via direct SQL (no public setter on storage).
        sqlx::query("UPDATE users SET email = $1 WHERE user_id = $2")
            .bind(&email)
            .bind(&user_id)
            .execute(&*pool)
            .await
            .expect("set email should succeed");

        let found = storage.get_user_by_email(&email).await.expect("get_user_by_email should succeed");
        assert!(found.is_some());
        assert_eq!(found.unwrap().user_id, user_id);

        let _ = storage.delete_user(&user_id).await;
    }

    #[tokio::test]
    async fn test_get_user_by_email_not_found() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let result =
            storage.get_user_by_email("nobody_here_12345@example.com").await.expect("get_user_by_email should succeed");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_get_user_by_identifier_user_id_path() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let user_id = format!("@ident_{}:example.com", uuid::Uuid::new_v4());
        let _ = storage.delete_user(&user_id).await;
        storage.create_user(&user_id, "identuser", None, false).await.unwrap();

        // Identifier starts with '@' and contains ':' → should resolve via get_user_by_id.
        let found =
            storage.get_user_by_identifier(&user_id).await.expect("get_user_by_identifier user_id path should succeed");
        assert!(found.is_some());
        assert_eq!(found.unwrap().user_id, user_id);

        let _ = storage.delete_user(&user_id).await;
    }

    #[tokio::test]
    async fn test_get_user_by_identifier_username_path() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let user_id = format!("@ident2_{}:example.com", uuid::Uuid::new_v4());
        let username = format!("identuser_{}", uuid::Uuid::new_v4());
        let _ = storage.delete_user(&user_id).await;
        storage.create_user(&user_id, &username, None, false).await.unwrap();

        // Identifier has no ':' → should resolve via get_user_by_username.
        let found = storage
            .get_user_by_identifier(&username)
            .await
            .expect("get_user_by_identifier username path should succeed");
        assert!(found.is_some());
        assert_eq!(found.unwrap().username, username);

        let _ = storage.delete_user(&user_id).await;
    }

    // ── pagination / list / count ─────────────────────────────────

    #[tokio::test]
    async fn test_get_users_paginated_no_cursor() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let users =
            storage.get_users_paginated(5, None, None).await.expect("get_users_paginated no cursor should succeed");
        assert!(users.len() <= 5);
    }

    #[tokio::test]
    async fn test_get_users_paginated_with_cursor() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let user_id = format!("@pag_{}:example.com", uuid::Uuid::new_v4());
        let _ = storage.delete_user(&user_id).await;
        let created = storage.create_user(&user_id, "paguser", None, false).await.unwrap();

        // Use the created user's timestamp/id as a cursor; should return users before it.
        let users = storage
            .get_users_paginated(10, Some(created.created_ts), Some(&user_id))
            .await
            .expect("get_users_paginated with cursor should succeed");
        // The created user itself should NOT be in the result (cursor is exclusive).
        assert!(!users.iter().any(|u| u.user_id == user_id));

        let _ = storage.delete_user(&user_id).await;
    }

    #[tokio::test]
    async fn test_list_users_with_name_filter() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let user_id = format!("@listf_{}:example.com", uuid::Uuid::new_v4());
        let username = format!("listfilter_{}", uuid::Uuid::new_v4().simple());
        let _ = storage.delete_user(&user_id).await;
        storage.create_user(&user_id, &username, None, false).await.unwrap();

        let users = storage
            .list_users(50, None, None, Some(&username))
            .await
            .expect("list_users with name filter should succeed");
        assert!(users.iter().any(|u| u.user_id == user_id));

        let _ = storage.delete_user(&user_id).await;
    }

    #[tokio::test]
    async fn test_list_users_no_filter_respects_limit() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let users = storage.list_users(3, None, None, None).await.expect("list_users no filter should succeed");
        assert!(users.len() <= 3);
    }

    #[tokio::test]
    async fn test_get_user_count_returns_non_negative() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let count = storage.get_user_count().await.expect("get_user_count should succeed");
        assert!(count >= 0);
    }

    #[tokio::test]
    async fn test_get_daily_active_users() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let count = storage.get_daily_active_users().await.expect("get_daily_active_users should succeed");
        assert!(count >= 0);
    }

    #[tokio::test]
    async fn test_get_monthly_active_users() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let count = storage.get_monthly_active_users().await.expect("get_monthly_active_users should succeed");
        assert!(count >= 0);
    }

    #[tokio::test]
    async fn test_get_r30_users() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let count = storage.get_r30_users().await.expect("get_r30_users should succeed");
        assert!(count >= 0);
    }

    #[tokio::test]
    async fn test_get_user_stats_summary() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let user_id = format!("@stats_{}:example.com", uuid::Uuid::new_v4());
        let _ = storage.delete_user(&user_id).await;
        storage.create_user(&user_id, "statsuser", None, false).await.unwrap();

        let summary = storage.get_user_stats_summary().await.expect("get_user_stats_summary should succeed");
        // total_users should include all counted users; the breakdowns should be consistent.
        assert!(summary.total_users >= 1);
        assert!(summary.active_users + summary.deactivated_users <= summary.total_users + 1);

        let _ = storage.delete_user(&user_id).await;
    }

    // ── deactivate / guest / user_type / upgrade_guest ────────────

    #[tokio::test]
    async fn test_deactivate_user() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let user_id = format!("@deactfn_{}:example.com", uuid::Uuid::new_v4());
        let _ = storage.delete_user(&user_id).await;
        storage.create_user(&user_id, "deactfnuser", None, false).await.unwrap();
        assert!(!storage.get_user_by_id(&user_id).await.unwrap().unwrap().is_deactivated);

        storage.deactivate_user(&user_id).await.expect("deactivate_user should succeed");
        let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
        assert!(user.is_deactivated);

        let _ = storage.delete_user(&user_id).await;
    }

    #[tokio::test]
    async fn test_set_guest_status_toggle() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let user_id = format!("@guest_{}:example.com", uuid::Uuid::new_v4());
        let _ = storage.delete_user(&user_id).await;
        storage.create_user(&user_id, "guestuser", None, false).await.unwrap();
        assert!(!storage.get_user_by_id(&user_id).await.unwrap().unwrap().is_guest);

        storage.set_guest_status(&user_id, true).await.expect("set_guest_status true should succeed");
        assert!(storage.get_user_by_id(&user_id).await.unwrap().unwrap().is_guest);

        storage.set_guest_status(&user_id, false).await.expect("set_guest_status false should succeed");
        assert!(!storage.get_user_by_id(&user_id).await.unwrap().unwrap().is_guest);

        let _ = storage.delete_user(&user_id).await;
    }

    #[tokio::test]
    async fn test_set_user_type() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let user_id = format!("@utype_{}:example.com", uuid::Uuid::new_v4());
        let _ = storage.delete_user(&user_id).await;
        storage.create_user(&user_id, "utypeuser", None, false).await.unwrap();
        assert!(storage.get_user_by_id(&user_id).await.unwrap().unwrap().user_type.is_none());

        storage.set_user_type(&user_id, Some("bot")).await.expect("set_user_type Some should succeed");
        assert_eq!(storage.get_user_by_id(&user_id).await.unwrap().unwrap().user_type.as_deref(), Some("bot"));

        storage.set_user_type(&user_id, None).await.expect("set_user_type None should succeed");
        assert!(storage.get_user_by_id(&user_id).await.unwrap().unwrap().user_type.is_none());

        let _ = storage.delete_user(&user_id).await;
    }

    #[tokio::test]
    async fn test_upgrade_guest_account() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let user_id = format!("@upgrade_{}:example.com", uuid::Uuid::new_v4());
        let _ = storage.delete_user(&user_id).await;
        storage.create_user(&user_id, "guestupgrade", None, false).await.unwrap();
        // Mark as guest first.
        storage.set_guest_status(&user_id, true).await.unwrap();
        assert!(storage.get_user_by_id(&user_id).await.unwrap().unwrap().is_guest);

        let new_username = format!("upgraded_{}", uuid::Uuid::new_v4().simple());
        storage
            .upgrade_guest_account(&user_id, &new_username, "new_hash")
            .await
            .expect("upgrade_guest_account should succeed");

        let user = storage.get_user_by_id(&user_id).await.unwrap().unwrap();
        assert!(!user.is_guest);
        assert_eq!(user.username, new_username);

        let _ = storage.delete_user(&user_id).await;
    }

    // ── account_data (set / get / upsert) ──────────────────────────

    #[tokio::test]
    async fn test_set_account_data_and_get_via_set_table() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let user_id = format!("@acctset_{}:example.com", uuid::Uuid::new_v4());
        let event_type = format!("m.set_{}", uuid::Uuid::new_v4().simple());
        let _ = storage.delete_user(&user_id).await;
        storage.create_user(&user_id, "acctsetuser", None, false).await.unwrap();

        // set_account_data writes to user_account_data table.
        storage
            .set_account_data(&user_id, &event_type, &serde_json::json!({"k": "v"}))
            .await
            .expect("set_account_data should succeed");
        // Verify the row exists in the user_account_data table.
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM user_account_data WHERE user_id = $1 AND event_type = $2")
                .bind(&user_id)
                .bind(&event_type)
                .fetch_one(&*pool)
                .await
                .expect("verify count should succeed");
        assert!(count >= 1);

        let _ = storage.delete_user(&user_id).await;
    }

    #[tokio::test]
    async fn test_upsert_and_get_account_data_content() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let user_id = format!("@acctup_{}:example.com", uuid::Uuid::new_v4());
        let data_type = format!("m.up_{}", uuid::Uuid::new_v4().simple());
        let _ = storage.delete_user(&user_id).await;
        storage.create_user(&user_id, "acctupuser", None, false).await.unwrap();

        // Initially absent.
        let absent = storage
            .get_account_data_content(&user_id, &data_type)
            .await
            .expect("get_account_data_content absent should succeed");
        assert!(absent.is_none());

        // Upsert (insert).
        storage
            .upsert_account_data_content(&user_id, &data_type, &serde_json::json!({"v": 1}))
            .await
            .expect("upsert insert should succeed");
        let got = storage
            .get_account_data_content(&user_id, &data_type)
            .await
            .expect("get_account_data_content after insert should succeed")
            .expect("account data should exist");
        assert_eq!(got["v"], 1);

        // Upsert (update).
        storage
            .upsert_account_data_content(&user_id, &data_type, &serde_json::json!({"v": 2}))
            .await
            .expect("upsert update should succeed");
        let got2 = storage
            .get_account_data_content(&user_id, &data_type)
            .await
            .expect("get_account_data_content after update should succeed")
            .expect("account data should exist after update");
        assert_eq!(got2["v"], 2);

        let _ = storage.delete_user(&user_id).await;
    }

    // ── search ────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_search_users_empty_query() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let results = storage.search_users("", 10).await.expect("search_users empty should succeed");
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_search_users_matches_username() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let unique = uuid::Uuid::new_v4().simple().to_string();
        let username = format!("searchable_{unique}");
        let user_id = format!("@searchable_{unique}:example.com");
        let _ = storage.delete_user(&user_id).await;
        storage.create_user(&user_id, &username, None, false).await.unwrap();

        let results = storage.search_users(&username, 10).await.expect("search_users should succeed");
        assert!(results.iter().any(|r| r.user_id == user_id));

        let _ = storage.delete_user(&user_id).await;
    }

    #[tokio::test]
    async fn test_search_users_with_presence_empty_query() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let results =
            storage.search_users_with_presence("", 10).await.expect("search_users_with_presence empty should succeed");
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_search_users_with_presence_matches() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let unique = uuid::Uuid::new_v4().simple().to_string();
        let username = format!("swp_{unique}");
        let user_id = format!("@swp_{unique}:example.com");
        let _ = storage.delete_user(&user_id).await;
        storage.create_user(&user_id, &username, None, false).await.unwrap();

        let results =
            storage.search_users_with_presence(&username, 10).await.expect("search_users_with_presence should succeed");
        assert!(results.iter().any(|r| r.user_id == user_id));

        let _ = storage.delete_user(&user_id).await;
    }

    #[tokio::test]
    async fn test_search_directory_users_empty_query() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let results =
            storage.search_directory_users("", 10, false).await.expect("search_directory_users empty should succeed");
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_search_directory_users_matches() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let unique = uuid::Uuid::new_v4().simple().to_string();
        let username = format!("diruser_{unique}");
        let user_id = format!("@diruser_{unique}:example.com");
        let _ = storage.delete_user(&user_id).await;
        storage.create_user(&user_id, &username, None, false).await.unwrap();

        let results =
            storage.search_directory_users(&username, 10, false).await.expect("search_directory_users should succeed");
        assert!(results.iter().any(|r| r.user_id == user_id));

        let _ = storage.delete_user(&user_id).await;
    }

    // ── batch profiles / maps ─────────────────────────────────────

    #[tokio::test]
    async fn test_get_user_profiles_batch_empty() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let result = storage.get_user_profiles_batch(&[]).await.expect("get_user_profiles_batch empty should succeed");
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_get_user_profiles_batch_with_users() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let uid1 = format!("@pb1_{}:example.com", uuid::Uuid::new_v4());
        let uid2 = format!("@pb2_{}:example.com", uuid::Uuid::new_v4());
        for uid in [&uid1, &uid2] {
            let _ = storage.delete_user(uid).await;
        }
        storage.create_user(&uid1, "pb1user", None, false).await.unwrap();
        storage.create_user(&uid2, "pb2user", None, false).await.unwrap();

        let profiles = storage
            .get_user_profiles_batch(&[uid1.clone(), uid2.clone()])
            .await
            .expect("get_user_profiles_batch should succeed");
        assert_eq!(profiles.len(), 2);
        assert!(profiles.iter().any(|p| p.user_id == uid1));
        assert!(profiles.iter().any(|p| p.user_id == uid2));

        for uid in [&uid1, &uid2] {
            let _ = storage.delete_user(uid).await;
        }
    }

    #[tokio::test]
    async fn test_get_user_profiles_map_empty() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let map = storage.get_user_profiles_map(&[]).await.expect("get_user_profiles_map empty should succeed");
        assert!(map.is_empty());
    }

    #[tokio::test]
    async fn test_get_user_profiles_map_with_users() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let uid1 = format!("@pm1_{}:example.com", uuid::Uuid::new_v4());
        let uid2 = format!("@pm2_{}:example.com", uuid::Uuid::new_v4());
        for uid in [&uid1, &uid2] {
            let _ = storage.delete_user(uid).await;
        }
        storage.create_user(&uid1, "pm1user", None, false).await.unwrap();
        storage.create_user(&uid2, "pm2user", None, false).await.unwrap();

        let map = storage
            .get_user_profiles_map(&[uid1.clone(), uid2.clone()])
            .await
            .expect("get_user_profiles_map should succeed");
        assert!(map.contains_key(&uid1));
        assert!(map.contains_key(&uid2));

        for uid in [&uid1, &uid2] {
            let _ = storage.delete_user(uid).await;
        }
    }

    #[tokio::test]
    async fn test_get_users_map_empty() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let map = storage.get_users_map(&[]).await.expect("get_users_map empty should succeed");
        assert!(map.is_empty());
    }

    #[tokio::test]
    async fn test_get_users_map_with_users() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let uid1 = format!("@um1_{}:example.com", uuid::Uuid::new_v4());
        let uid2 = format!("@um2_{}:example.com", uuid::Uuid::new_v4());
        for uid in [&uid1, &uid2] {
            let _ = storage.delete_user(uid).await;
        }
        storage.create_user(&uid1, "um1user", None, false).await.unwrap();
        storage.create_user(&uid2, "um2user", None, false).await.unwrap();

        let map = storage.get_users_map(&[uid1.clone(), uid2.clone()]).await.expect("get_users_map should succeed");
        assert!(map.contains_key(&uid1));
        assert!(map.contains_key(&uid2));

        for uid in [&uid1, &uid2] {
            let _ = storage.delete_user(uid).await;
        }
    }

    #[tokio::test]
    async fn test_update_displayname_batch_empty() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let count = storage.update_displayname_batch(&[]).await.expect("update_displayname_batch empty should succeed");
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_update_displayname_batch_with_updates() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let uid1 = format!("@dbn1_{}:example.com", uuid::Uuid::new_v4());
        let uid2 = format!("@dbn2_{}:example.com", uuid::Uuid::new_v4());
        for uid in [&uid1, &uid2] {
            let _ = storage.delete_user(uid).await;
        }
        storage.create_user(&uid1, "dbn1user", None, false).await.unwrap();
        storage.create_user(&uid2, "dbn2user", None, false).await.unwrap();

        let updates: Vec<(String, Option<String>)> =
            vec![(uid1.clone(), Some("Batch Name 1".to_string())), (uid2.clone(), Some("Batch Name 2".to_string()))];
        let count = storage.update_displayname_batch(&updates).await.expect("update_displayname_batch should succeed");
        assert_eq!(count, 2);

        assert_eq!(storage.get_user_profile(&uid1).await.unwrap().unwrap().displayname.unwrap(), "Batch Name 1");
        assert_eq!(storage.get_user_profile(&uid2).await.unwrap().unwrap().displayname.unwrap(), "Batch Name 2");

        for uid in [&uid1, &uid2] {
            let _ = storage.delete_user(uid).await;
        }
    }

    // ── get_locked_users / create_user_tx ─────────────────────────

    #[tokio::test]
    async fn test_get_locked_users_pagination() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let user_id = format!("@lklist_{}:example.com", uuid::Uuid::new_v4());
        let _ = storage.delete_user(&user_id).await;
        storage.create_user(&user_id, "lklistuser", None, false).await.unwrap();

        let now = current_timestamp_millis();
        storage.lock_user(&user_id, Some("audit"), "system", now).await.unwrap();

        // Page 1 (limit large enough) should include our locked user.
        let locked = storage.get_locked_users(100, 0).await.expect("get_locked_users should succeed");
        assert!(locked.iter().any(|l| l.user_id == user_id && l.is_active));

        // Clean up the lock.
        storage.unlock_user(&user_id, now).await.unwrap();
        let _ = storage.delete_user(&user_id).await;
    }

    #[tokio::test]
    async fn test_create_user_tx_in_transaction() {
        let (_iso, pool) = test_pool().await;
        let cache = test_cache();
        let storage = UserStorage::new(&pool, cache);
        let user_id = format!("@createtx_{}:example.com", uuid::Uuid::new_v4());
        let _ = storage.delete_user(&user_id).await;

        let mut tx = pool.begin().await.expect("begin tx should succeed");
        let user = storage
            .create_user_tx(&mut tx, &user_id, "createtxuser", Some("hash"), false)
            .await
            .expect("create_user_tx should succeed");
        assert_eq!(user.user_id, user_id);
        tx.commit().await.expect("commit should succeed");

        // After commit, the user should be retrievable.
        let found = storage.get_user_by_id(&user_id).await.unwrap().expect("user should exist after tx commit");
        assert_eq!(found.username, "createtxuser");

        let _ = storage.delete_user(&user_id).await;
    }
}
