//! User storage: [`UserStore`] trait and [`UserStorage`] implementation.

use async_trait::async_trait;
use sqlx::{Pool, Postgres, Row};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use synapse_cache::CacheManager;
use synapse_common::constants::USER_PROFILE_CACHE_TTL;
use synapse_common::current_timestamp_millis;

use crate::trigram_ranking::TrigramRanking;

const USER_DIRECTORY_SEARCH_CACHE_TTL_SECS: u64 = 30;
const USER_PROFILE_BATCH_CACHE_TTL: u64 = 300;

pub(crate) fn escape_like_pattern(input: &str) -> String {
    input.replace('\\', "\\\\").replace('%', r"\%").replace('_', r"\_")
}

use super::models::*;

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

    /// MSC4262: Apply a profile update received from a remote homeserver.
    ///
    /// `UPDATE`-only: refreshes `displayname`/`avatar_url` for users that
    /// already exist locally. Returns `true` if a row was updated, `false` if
    /// the user is unknown locally (no `INSERT` — see the concrete impl for why).
    /// Does **not** create a placeholder row for never-seen remote users.
    async fn apply_profile_update_from_federation(
        &self,
        user_id: &str,
        displayname: Option<&str>,
        avatar_url: Option<&str>,
    ) -> Result<bool, sqlx::Error>;

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

    /// MSC4262: Persist a profile update received from a remote homeserver.
    ///
    /// This is an `UPDATE`-only operation: it only refreshes `displayname` and
    /// `avatar_url` for users that already exist in the local `users` table.
    ///
    /// We deliberately do **not** `INSERT` a placeholder row for remote users
    /// that are unknown locally, because:
    /// 1. The `users.username` column has a global `UNIQUE` constraint — two
    ///    remote users from different homeservers could collide on the same
    ///    localpart (e.g. `@alice:a.example` vs `@alice:b.example`).
    /// 2. MSC4262 semantics are "invalidate/refresh cached profile data", not
    ///    "materialize unknown remote accounts". If we have never seen the
    ///    user there is nothing to cache or refresh.
    ///
    /// Returns `true` if a local row was updated, `false` if the user does not
    /// exist locally (in which case the cache is simply invalidated).
    pub async fn apply_profile_update_from_federation(
        &self,
        user_id: &str,
        displayname: Option<&str>,
        avatar_url: Option<&str>,
    ) -> Result<bool, sqlx::Error> {
        let now = synapse_common::current_timestamp_millis();
        let result = sqlx::query(
            r"
            UPDATE users
               SET displayname = COALESCE($1, displayname),
                   avatar_url = COALESCE($2, avatar_url),
                   updated_ts = $3
             WHERE user_id = $4
            ",
        )
        .bind(displayname)
        .bind(avatar_url)
        .bind(now)
        .bind(user_id)
        .execute(&*self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Ok(false);
        }

        // Refresh the profile cache so subsequent lookups return the new value.
        if let Ok(Some(profile)) = self.get_user_profile(user_id).await {
            let key = format!("user:profile:{user_id}");
            if let Err(e) = self.cache.set(&key, &profile, USER_PROFILE_CACHE_TTL).await {
                ::tracing::warn!(target: "cache", user_id = %user_id, cache_key = %key, error = %e, "Failed to cache upserted profile");
            }
        }

        tracing::info!(user_id = %user_id, "Applied profile update from federation");
        Ok(true)
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

    async fn apply_profile_update_from_federation(
        &self,
        user_id: &str,
        displayname: Option<&str>,
        avatar_url: Option<&str>,
    ) -> Result<bool, sqlx::Error> {
        self.apply_profile_update_from_federation(user_id, displayname, avatar_url).await
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
