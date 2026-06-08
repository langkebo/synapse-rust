use crate::cache::CacheManager;
use crate::common::constants::USER_PROFILE_CACHE_TTL;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use std::sync::Arc;
use tracing;

const USER_DIRECTORY_SEARCH_CACHE_TTL_SECS: u64 = 30;
const USER_PROFILE_BATCH_CACHE_TTL: u64 = 300;

fn escape_like_pattern(input: &str) -> String {
    input.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct User {
    pub user_id: String,
    pub username: String,
    #[serde(skip_serializing)]
    pub password_hash: Option<String>,
    pub is_admin: bool,
    pub is_guest: bool,
    pub is_shadow_banned: bool,
    pub is_deactivated: bool,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
    pub displayname: Option<String>,
    pub avatar_url: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub generation: i64,
    pub consent_version: Option<String>,
    pub appservice_id: Option<String>,
    pub user_type: Option<String>,
    pub invalid_update_at: Option<i64>,
    pub migration_state: Option<String>,
    pub password_changed_ts: Option<i64>,
    pub is_password_change_required: bool,
    pub password_expires_at: Option<i64>,
    pub failed_login_attempts: i32,
    pub locked_until: Option<i64>,
    pub must_change_password: bool,
}

impl User {
    pub fn user_id(&self) -> String {
        self.user_id.clone()
    }
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct UserProfile {
    pub user_id: String,
    pub username: String,
    pub displayname: Option<String>,
    pub avatar_url: Option<String>,
    pub created_ts: i64,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct UserSearchResult {
    pub user_id: String,
    pub username: String,
    pub displayname: Option<String>,
    pub avatar_url: Option<String>,
    pub created_ts: i64,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct UserSearchResultWithPresence {
    pub user_id: String,
    pub username: String,
    pub displayname: Option<String>,
    pub avatar_url: Option<String>,
    pub created_ts: i64,
    pub presence: Option<String>,
    pub last_active_ts: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct UserDirectorySearchResult {
    pub user_id: String,
    pub username: String,
    pub displayname: Option<String>,
    pub avatar_url: Option<String>,
    pub created_ts: i64,
    pub presence: Option<String>,
    pub last_active_ts: Option<i64>,
    pub match_score: i32,
    pub match_type: String,
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
        let now = chrono::Utc::now().timestamp_millis();
        let generation = now;
        sqlx::query_as!(User,
            r#"
            INSERT INTO users (user_id, username, password_hash, is_admin, created_ts, generation)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING
                user_id AS "user_id!",
                username AS "username!",
                password_hash AS "password_hash?",
                COALESCE(is_admin, FALSE) AS "is_admin!",
                COALESCE(is_guest, FALSE) AS "is_guest!",
                COALESCE(is_shadow_banned, FALSE) AS "is_shadow_banned!",
                COALESCE(is_deactivated, FALSE) AS "is_deactivated!",
                created_ts AS "created_ts!",
                updated_ts AS "updated_ts?",
                displayname AS "displayname?",
                avatar_url AS "avatar_url?",
                email AS "email?",
                phone AS "phone?",
                COALESCE(generation, 0) AS "generation!",
                consent_version AS "consent_version?",
                appservice_id AS "appservice_id?",
                user_type AS "user_type?",
                invalid_update_at AS "invalid_update_at?",
                migration_state AS "migration_state?",
                password_changed_ts AS "password_changed_ts?",
                COALESCE(is_password_change_required, FALSE) AS "is_password_change_required!",
                password_expires_at AS "password_expires_at?",
                COALESCE(failed_login_attempts, 0) AS "failed_login_attempts!",
                locked_until AS "locked_until?",
                COALESCE(must_change_password, FALSE) AS "must_change_password!"
            "#,
            user_id,
            username,
            password_hash,
            is_admin,
            now,
            generation,
        )
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
        let now = chrono::Utc::now().timestamp_millis();
        let generation = now;
        sqlx::query_as!(User,
            r#"
            INSERT INTO users (user_id, username, password_hash, is_admin, created_ts, generation)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING
                user_id AS "user_id!",
                username AS "username!",
                password_hash AS "password_hash?",
                COALESCE(is_admin, FALSE) AS "is_admin!",
                COALESCE(is_guest, FALSE) AS "is_guest!",
                COALESCE(is_shadow_banned, FALSE) AS "is_shadow_banned!",
                COALESCE(is_deactivated, FALSE) AS "is_deactivated!",
                created_ts AS "created_ts!",
                updated_ts AS "updated_ts?",
                displayname AS "displayname?",
                avatar_url AS "avatar_url?",
                email AS "email?",
                phone AS "phone?",
                COALESCE(generation, 0) AS "generation!",
                consent_version AS "consent_version?",
                appservice_id AS "appservice_id?",
                user_type AS "user_type?",
                invalid_update_at AS "invalid_update_at?",
                migration_state AS "migration_state?",
                password_changed_ts AS "password_changed_ts?",
                COALESCE(is_password_change_required, FALSE) AS "is_password_change_required!",
                password_expires_at AS "password_expires_at?",
                COALESCE(failed_login_attempts, 0) AS "failed_login_attempts!",
                locked_until AS "locked_until?",
                COALESCE(must_change_password, FALSE) AS "must_change_password!"
            "#,
            user_id,
            username,
            password_hash,
            is_admin,
            now,
            generation,
        )
        .fetch_one(&mut **tx)
        .await
    }

    pub async fn get_user_by_id(&self, user_id: &str) -> Result<Option<User>, sqlx::Error> {
        tracing::debug!(user_id = %user_id, "Querying user by id");
        sqlx::query_as!(User,
            r#"
            SELECT
                user_id AS "user_id!",
                username AS "username!",
                password_hash AS "password_hash?",
                COALESCE(is_admin, FALSE) AS "is_admin!",
                COALESCE(is_guest, FALSE) AS "is_guest!",
                COALESCE(is_shadow_banned, FALSE) AS "is_shadow_banned!",
                COALESCE(is_deactivated, FALSE) AS "is_deactivated!",
                created_ts AS "created_ts!",
                updated_ts AS "updated_ts?",
                displayname AS "displayname?",
                avatar_url AS "avatar_url?",
                email AS "email?",
                phone AS "phone?",
                COALESCE(generation, 0) AS "generation!",
                consent_version AS "consent_version?",
                appservice_id AS "appservice_id?",
                user_type AS "user_type?",
                invalid_update_at AS "invalid_update_at?",
                migration_state AS "migration_state?",
                password_changed_ts AS "password_changed_ts?",
                COALESCE(is_password_change_required, FALSE) AS "is_password_change_required!",
                password_expires_at AS "password_expires_at?",
                COALESCE(failed_login_attempts, 0) AS "failed_login_attempts!",
                locked_until AS "locked_until?",
                COALESCE(must_change_password, FALSE) AS "must_change_password!"
            FROM users
            WHERE user_id = $1
            "#,
            user_id,
        )
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn get_user_by_username(&self, username: &str) -> Result<Option<User>, sqlx::Error> {
        sqlx::query_as!(User,
            r#"
            SELECT
                user_id AS "user_id!",
                username AS "username!",
                password_hash AS "password_hash?",
                COALESCE(is_admin, FALSE) AS "is_admin!",
                COALESCE(is_guest, FALSE) AS "is_guest!",
                COALESCE(is_shadow_banned, FALSE) AS "is_shadow_banned!",
                COALESCE(is_deactivated, FALSE) AS "is_deactivated!",
                created_ts AS "created_ts!",
                updated_ts AS "updated_ts?",
                displayname AS "displayname?",
                avatar_url AS "avatar_url?",
                email AS "email?",
                phone AS "phone?",
                COALESCE(generation, 0) AS "generation!",
                consent_version AS "consent_version?",
                appservice_id AS "appservice_id?",
                user_type AS "user_type?",
                invalid_update_at AS "invalid_update_at?",
                migration_state AS "migration_state?",
                password_changed_ts AS "password_changed_ts?",
                COALESCE(is_password_change_required, FALSE) AS "is_password_change_required!",
                password_expires_at AS "password_expires_at?",
                COALESCE(failed_login_attempts, 0) AS "failed_login_attempts!",
                locked_until AS "locked_until?",
                COALESCE(must_change_password, FALSE) AS "must_change_password!"
            FROM users
            WHERE username = $1
            "#,
            username,
        )
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn get_user_by_email(&self, email: &str) -> Result<Option<User>, sqlx::Error> {
        sqlx::query_as!(User,
            r#"
            SELECT
                user_id AS "user_id!",
                username AS "username!",
                password_hash AS "password_hash?",
                COALESCE(is_admin, FALSE) AS "is_admin!",
                COALESCE(is_guest, FALSE) AS "is_guest!",
                COALESCE(is_shadow_banned, FALSE) AS "is_shadow_banned!",
                COALESCE(is_deactivated, FALSE) AS "is_deactivated!",
                created_ts AS "created_ts!",
                updated_ts AS "updated_ts?",
                displayname AS "displayname?",
                avatar_url AS "avatar_url?",
                email AS "email?",
                phone AS "phone?",
                COALESCE(generation, 0) AS "generation!",
                consent_version AS "consent_version?",
                appservice_id AS "appservice_id?",
                user_type AS "user_type?",
                invalid_update_at AS "invalid_update_at?",
                migration_state AS "migration_state?",
                password_changed_ts AS "password_changed_ts?",
                COALESCE(is_password_change_required, FALSE) AS "is_password_change_required!",
                password_expires_at AS "password_expires_at?",
                COALESCE(failed_login_attempts, 0) AS "failed_login_attempts!",
                locked_until AS "locked_until?",
                COALESCE(must_change_password, FALSE) AS "must_change_password!"
            FROM users
            WHERE email = $1 AND COALESCE(is_deactivated, FALSE) = FALSE
            "#,
            email,
        )
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn get_user_by_identifier(&self, identifier: &str) -> Result<Option<User>, sqlx::Error> {
        if identifier.starts_with('@') && identifier.contains(':') {
            self.get_user_by_id(identifier).await
        } else {
            self.get_user_by_username(identifier).await
        }
    }

    pub async fn get_all_users(&self, limit: i64) -> Result<Vec<User>, sqlx::Error> {
        sqlx::query_as!(User,
            r#"
            SELECT
                user_id AS "user_id!",
                username AS "username!",
                password_hash AS "password_hash?",
                COALESCE(is_admin, FALSE) AS "is_admin!",
                COALESCE(is_guest, FALSE) AS "is_guest!",
                COALESCE(is_shadow_banned, FALSE) AS "is_shadow_banned!",
                COALESCE(is_deactivated, FALSE) AS "is_deactivated!",
                created_ts AS "created_ts!",
                updated_ts AS "updated_ts?",
                displayname AS "displayname?",
                avatar_url AS "avatar_url?",
                email AS "email?",
                phone AS "phone?",
                COALESCE(generation, 0) AS "generation!",
                consent_version AS "consent_version?",
                appservice_id AS "appservice_id?",
                user_type AS "user_type?",
                invalid_update_at AS "invalid_update_at?",
                migration_state AS "migration_state?",
                password_changed_ts AS "password_changed_ts?",
                COALESCE(is_password_change_required, FALSE) AS "is_password_change_required!",
                password_expires_at AS "password_expires_at?",
                COALESCE(failed_login_attempts, 0) AS "failed_login_attempts!",
                locked_until AS "locked_until?",
                COALESCE(must_change_password, FALSE) AS "must_change_password!"
            FROM users
            ORDER BY created_ts DESC
            LIMIT $1
            "#,
            limit,
        )
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_users_paginated(
        &self,
        limit: i64,
        since_ts: Option<i64>,
        since_user_id: Option<&str>,
    ) -> Result<Vec<User>, sqlx::Error> {
        if let (Some(ts), Some(user_id)) = (since_ts, since_user_id) {
            sqlx::query_as!(User,
                r#"
                SELECT
                    user_id AS "user_id!",
                    username AS "username!",
                    password_hash AS "password_hash?",
                    COALESCE(is_admin, FALSE) AS "is_admin!",
                    COALESCE(is_guest, FALSE) AS "is_guest!",
                    COALESCE(is_shadow_banned, FALSE) AS "is_shadow_banned!",
                    COALESCE(is_deactivated, FALSE) AS "is_deactivated!",
                    created_ts AS "created_ts!",
                    updated_ts AS "updated_ts?",
                    displayname AS "displayname?",
                    avatar_url AS "avatar_url?",
                    email AS "email?",
                    phone AS "phone?",
                    COALESCE(generation, 0) AS "generation!",
                    consent_version AS "consent_version?",
                    appservice_id AS "appservice_id?",
                    user_type AS "user_type?",
                    invalid_update_at AS "invalid_update_at?",
                    migration_state AS "migration_state?",
                    password_changed_ts AS "password_changed_ts?",
                    COALESCE(is_password_change_required, FALSE) AS "is_password_change_required!",
                    password_expires_at AS "password_expires_at?",
                    COALESCE(failed_login_attempts, 0) AS "failed_login_attempts!",
                    locked_until AS "locked_until?",
                    COALESCE(must_change_password, FALSE) AS "must_change_password!"
                FROM users
                WHERE (created_ts < $2 OR (created_ts = $2 AND user_id < $3))
                ORDER BY created_ts DESC, user_id DESC
                LIMIT $1
                "#,
                limit,
                ts,
                user_id,
            )
            .fetch_all(&*self.pool)
            .await
        } else {
            sqlx::query_as!(User,
                r#"
                SELECT
                    user_id AS "user_id!",
                    username AS "username!",
                    password_hash AS "password_hash?",
                    COALESCE(is_admin, FALSE) AS "is_admin!",
                    COALESCE(is_guest, FALSE) AS "is_guest!",
                    COALESCE(is_shadow_banned, FALSE) AS "is_shadow_banned!",
                    COALESCE(is_deactivated, FALSE) AS "is_deactivated!",
                    created_ts AS "created_ts!",
                    updated_ts AS "updated_ts?",
                    displayname AS "displayname?",
                    avatar_url AS "avatar_url?",
                    email AS "email?",
                    phone AS "phone?",
                    COALESCE(generation, 0) AS "generation!",
                    consent_version AS "consent_version?",
                    appservice_id AS "appservice_id?",
                    user_type AS "user_type?",
                    invalid_update_at AS "invalid_update_at?",
                    migration_state AS "migration_state?",
                    password_changed_ts AS "password_changed_ts?",
                    COALESCE(is_password_change_required, FALSE) AS "is_password_change_required!",
                    password_expires_at AS "password_expires_at?",
                    COALESCE(failed_login_attempts, 0) AS "failed_login_attempts!",
                    locked_until AS "locked_until?",
                    COALESCE(must_change_password, FALSE) AS "must_change_password!"
                FROM users
                ORDER BY created_ts DESC, user_id DESC
                LIMIT $1
                "#,
                limit,
            )
            .fetch_all(&*self.pool)
            .await
        }
    }

    pub async fn get_user_count(&self) -> Result<i64, sqlx::Error> {
        let count = sqlx::query_scalar!(
            r#"SELECT COALESCE(COUNT(*), 0) AS "count!" FROM users"#
        )
        .fetch_one(&*self.pool)
        .await?;
        Ok(count)
    }

    pub async fn user_exists(&self, user_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query_scalar!(
            r#"SELECT 1 AS "exists!" FROM users WHERE user_id = $1 AND COALESCE(is_deactivated, FALSE) = FALSE LIMIT 1"#,
            user_id,
        )
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result.is_some())
    }

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

    pub async fn update_password(&self, user_id: &str, password_hash: &str) -> Result<(), sqlx::Error> {
        tracing::info!(user_id = %user_id, "Updating user password");
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query!(
            r"UPDATE users SET password_hash = $1, password_changed_ts = $2, is_password_change_required = FALSE, must_change_password = FALSE WHERE user_id = $3",
            password_hash,
            now,
            user_id,
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn update_displayname(&self, user_id: &str, displayname: Option<&str>) -> Result<(), sqlx::Error> {
        tracing::info!(user_id = %user_id, "Updating user displayname");
        sqlx::query!(r"UPDATE users SET displayname = $1 WHERE user_id = $2",
            displayname,
            user_id,
        )
        .execute(&*self.pool)
        .await?;

        if let Ok(Some(profile)) = self.get_user_profile(user_id).await {
            let key = format!("user:profile:{user_id}");
            let _ = self.cache.set(&key, &profile, USER_PROFILE_CACHE_TTL).await;
        }

        Ok(())
    }

    pub async fn update_avatar_url(&self, user_id: &str, avatar_url: Option<&str>) -> Result<(), sqlx::Error> {
        sqlx::query!(r"UPDATE users SET avatar_url = $1 WHERE user_id = $2",
            avatar_url,
            user_id,
        )
        .execute(&*self.pool)
        .await?;

        if let Ok(Some(profile)) = self.get_user_profile(user_id).await {
            let key = format!("user:profile:{user_id}");
            let _ = self.cache.set(&key, &profile, USER_PROFILE_CACHE_TTL).await;
        }

        Ok(())
    }

    pub async fn deactivate_user(&self, user_id: &str) -> Result<(), sqlx::Error> {
        tracing::info!(user_id = %user_id, "Deactivating user");
        sqlx::query!(r"UPDATE users SET is_deactivated = TRUE WHERE user_id = $1",
            user_id,
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn set_admin_status(&self, user_id: &str, is_admin: bool) -> Result<(), sqlx::Error> {
        sqlx::query!(r"UPDATE users SET is_admin = $1 WHERE user_id = $2",
            is_admin,
            user_id,
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn set_account_data(
        &self,
        user_id: &str,
        event_type: &str,
        content: &serde_json::Value,
    ) -> Result<(), sqlx::Error> {
        let content_str = serde_json::to_string(content).unwrap_or_default();
        let now: i64 = chrono::Utc::now().timestamp();
        sqlx::query!(
            r"
            INSERT INTO user_account_data (user_id, event_type, content, created_ts)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (user_id, event_type) DO UPDATE SET content = EXCLUDED.content, created_ts = EXCLUDED.created_ts
            ",
            user_id,
            event_type,
            content_str,
            now,
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_account_data_content(
        &self,
        user_id: &str,
        data_type: &str,
    ) -> Result<Option<serde_json::Value>, sqlx::Error> {
        let content = sqlx::query_scalar!(
            r#"SELECT content AS "content!" FROM account_data WHERE user_id = $1 AND data_type = $2"#,
            user_id,
            data_type,
        )
        .fetch_optional(&*self.pool)
        .await?;
        Ok(content)
    }

    pub async fn upsert_account_data_content(
        &self,
        user_id: &str,
        data_type: &str,
        content: &serde_json::Value,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query!(
            r"
            INSERT INTO account_data (user_id, data_type, content, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $4)
            ON CONFLICT (user_id, data_type) DO UPDATE SET content = EXCLUDED.content, updated_ts = EXCLUDED.updated_ts
            ",
            user_id,
            data_type,
            content,
            now,
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn search_users(&self, query: &str, limit: i64) -> Result<Vec<UserSearchResult>, sqlx::Error> {
        let normalized = query.trim();
        if normalized.is_empty() {
            return Ok(Vec::new());
        }

        let escaped = escape_like_pattern(normalized);
        let exact_pattern = escaped.clone();
        let prefix_pattern = format!("{escaped}%");
        let contains_pattern = format!("%{escaped}%");

        sqlx::query_as!(UserSearchResult,
            r#"
            WITH candidate_matches AS (
                SELECT
                    user_id,
                    MIN(match_priority) AS match_priority,
                    MAX(match_similarity) AS match_similarity
                FROM (
                    SELECT
                        user_id,
                        CASE
                            WHEN username ILIKE $1 ESCAPE '\' THEN 0
                            WHEN username ILIKE $2 ESCAPE '\' THEN 1
                            WHEN username ILIKE $3 ESCAPE '\' THEN 2
                            ELSE 3
                        END AS match_priority,
                        similarity(username, $4) AS match_similarity
                    FROM users
                    WHERE COALESCE(is_deactivated, FALSE) = FALSE
                      AND (
                            username ILIKE $1 ESCAPE '\'
                            OR username ILIKE $2 ESCAPE '\'
                            OR username ILIKE $3 ESCAPE '\'
                            OR (char_length($4) >= 3 AND username % $4)
                      )

                    UNION ALL

                    SELECT
                        user_id,
                        CASE
                            WHEN user_id ILIKE $1 ESCAPE '\' THEN 0
                            WHEN user_id ILIKE $2 ESCAPE '\' THEN 1
                            WHEN user_id ILIKE $3 ESCAPE '\' THEN 2
                            ELSE 3
                        END AS match_priority,
                        similarity(user_id, $4) AS match_similarity
                    FROM users
                    WHERE COALESCE(is_deactivated, FALSE) = FALSE
                      AND (
                            user_id ILIKE $1 ESCAPE '\'
                            OR user_id ILIKE $2 ESCAPE '\'
                            OR user_id ILIKE $3 ESCAPE '\'
                            OR (char_length($4) >= 3 AND user_id % $4)
                      )

                    UNION ALL

                    SELECT
                        user_id,
                        CASE
                            WHEN displayname ILIKE $1 ESCAPE '\' THEN 0
                            WHEN displayname ILIKE $2 ESCAPE '\' THEN 1
                            WHEN displayname ILIKE $3 ESCAPE '\' THEN 2
                            ELSE 3
                        END AS match_priority,
                        COALESCE(similarity(displayname, $4), 0.0) AS match_similarity
                    FROM users
                    WHERE COALESCE(is_deactivated, FALSE) = FALSE
                      AND displayname IS NOT NULL
                      AND (
                            displayname ILIKE $1 ESCAPE '\'
                            OR displayname ILIKE $2 ESCAPE '\'
                            OR displayname ILIKE $3 ESCAPE '\'
                            OR (char_length($4) >= 3 AND displayname % $4)
                      )
                ) AS matches
                GROUP BY user_id
            )
            SELECT
                u.user_id AS "user_id!",
                u.username AS "username!",
                COALESCE(u.displayname, u.username) AS "displayname!",
                u.avatar_url AS "avatar_url?",
                u.created_ts AS "created_ts!"
            FROM candidate_matches cm
            JOIN users u ON u.user_id = cm.user_id
            ORDER BY
                cm.match_priority ASC,
                cm.match_similarity DESC,
                u.created_ts DESC
            LIMIT $5
            "#,
            exact_pattern,
            prefix_pattern,
            contains_pattern,
            normalized,
            limit,
        )
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_user_profile(&self, user_id: &str) -> Result<Option<UserProfile>, sqlx::Error> {
        tracing::debug!(user_id = %user_id, "Querying user profile");
        let key = format!("user:profile:{user_id}");

        if let Ok(Some(profile)) = self.cache.get::<UserProfile>(&key).await {
            return Ok(Some(profile));
        }

        let result = sqlx::query_as!(UserProfile,
            r#"
            SELECT
                user_id AS "user_id!",
                username AS "username!",
                COALESCE(displayname, username) AS "displayname!",
                avatar_url AS "avatar_url?",
                created_ts AS "created_ts!"
            FROM users
            WHERE user_id = $1 AND COALESCE(is_deactivated, FALSE) = FALSE
            "#,
            user_id,
        )
        .fetch_optional(&*self.pool)
        .await?;

        if let Some(profile) = &result {
            let _ = self.cache.set(&key, profile, USER_PROFILE_CACHE_TTL).await;
        }

        Ok(result)
    }

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
            SELECT user_id, username, COALESCE(displayname, username) as displayname, avatar_url, created_ts
            FROM users
            WHERE user_id = ANY($1) AND COALESCE(is_deactivated, FALSE) = FALSE
            ",
        )
        .bind(&missing_ids)
        .fetch_all(&*self.pool)
        .await?;

        for profile in &fetched {
            let key = format!("user:profile:{}", profile.user_id);
            let _ = self.cache.set(&key, profile, USER_PROFILE_BATCH_CACHE_TTL).await;
        }

        let mut all_profiles = cached_profiles;
        all_profiles.extend(fetched);
        Ok(all_profiles)
    }

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

    pub async fn update_displayname_batch(&self, updates: &[(String, Option<String>)]) -> Result<u64, sqlx::Error> {
        if updates.is_empty() {
            return Ok(0);
        }

        let mut count = 0u64;
        for (user_id, displayname) in updates {
            sqlx::query!(r"UPDATE users SET displayname = $1 WHERE user_id = $2",
                displayname.as_deref(),
                user_id,
            )
            .execute(&*self.pool)
            .await?;
            count += 1;
        }

        Ok(count)
    }

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

        sqlx::query_as!(UserSearchResultWithPresence,
            r#"
            WITH candidate_matches AS (
                SELECT
                    user_id,
                    MIN(match_priority) AS match_priority,
                    MAX(match_similarity) AS match_similarity
                FROM (
                    SELECT
                        user_id,
                        CASE
                            WHEN username ILIKE $1 ESCAPE '\' THEN 0
                            WHEN username ILIKE $2 ESCAPE '\' THEN 1
                            WHEN username ILIKE $3 ESCAPE '\' THEN 2
                            ELSE 3
                        END AS match_priority,
                        similarity(username, $4) AS match_similarity
                    FROM users
                    WHERE COALESCE(is_deactivated, FALSE) = FALSE
                      AND (
                            username ILIKE $1 ESCAPE '\'
                            OR username ILIKE $2 ESCAPE '\'
                            OR username ILIKE $3 ESCAPE '\'
                            OR (char_length($4) >= 3 AND username % $4)
                      )

                    UNION ALL

                    SELECT
                        user_id,
                        CASE
                            WHEN user_id ILIKE $1 ESCAPE '\' THEN 0
                            WHEN user_id ILIKE $2 ESCAPE '\' THEN 1
                            WHEN user_id ILIKE $3 ESCAPE '\' THEN 2
                            ELSE 3
                        END AS match_priority,
                        similarity(user_id, $4) AS match_similarity
                    FROM users
                    WHERE COALESCE(is_deactivated, FALSE) = FALSE
                      AND (
                            user_id ILIKE $1 ESCAPE '\'
                            OR user_id ILIKE $2 ESCAPE '\'
                            OR user_id ILIKE $3 ESCAPE '\'
                            OR (char_length($4) >= 3 AND user_id % $4)
                      )

                    UNION ALL

                    SELECT
                        user_id,
                        CASE
                            WHEN displayname ILIKE $1 ESCAPE '\' THEN 0
                            WHEN displayname ILIKE $2 ESCAPE '\' THEN 1
                            WHEN displayname ILIKE $3 ESCAPE '\' THEN 2
                            ELSE 3
                        END AS match_priority,
                        COALESCE(similarity(displayname, $4), 0.0) AS match_similarity
                    FROM users
                    WHERE COALESCE(is_deactivated, FALSE) = FALSE
                      AND displayname IS NOT NULL
                      AND (
                            displayname ILIKE $1 ESCAPE '\'
                            OR displayname ILIKE $2 ESCAPE '\'
                            OR displayname ILIKE $3 ESCAPE '\'
                            OR (char_length($4) >= 3 AND displayname % $4)
                      )
                ) AS matches
                GROUP BY user_id
            )
            SELECT
                u.user_id AS "user_id!",
                u.username AS "username!",
                COALESCE(u.displayname, u.username) AS "displayname!",
                u.avatar_url AS "avatar_url?",
                u.created_ts AS "created_ts!",
                p.presence AS "presence?",
                p.last_active_ts AS "last_active_ts?"
            FROM candidate_matches cm
            JOIN users u ON u.user_id = cm.user_id
            LEFT JOIN presence p ON u.user_id = p.user_id
            ORDER BY
                cm.match_priority ASC,
                cm.match_similarity DESC,
                u.created_ts DESC
            LIMIT $5
            "#,
            exact_pattern,
            prefix_pattern,
            contains_pattern,
            normalized,
            limit,
        )
        .fetch_all(&*self.pool)
        .await
    }

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

        let rows = sqlx::query_as!(UserDirectorySearchResult,
            r#"
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
                u.user_id AS "user_id!",
                u.username AS "username!",
                COALESCE(u.displayname, u.username) AS "displayname!",
                u.avatar_url AS "avatar_url?",
                u.created_ts AS "created_ts!",
                p.presence AS "presence?",
                p.last_active_ts AS "last_active_ts?",
                (
                    cm.rank_score
                    + CASE
                        WHEN COALESCE(p.presence, 'offline') = 'online' THEN 50
                        WHEN COALESCE(p.presence, 'offline') = 'unavailable' THEN 20
                        ELSE 0
                    END
                )::INTEGER AS "match_score!",
                CASE cm.match_category
                    WHEN 0 THEN 'exact'
                    WHEN 1 THEN 'prefix'
                    WHEN 2 THEN 'contains'
                    ELSE 'fuzzy'
                END AS "match_type!"
            FROM candidate_matches cm
            JOIN users u ON u.user_id = cm.user_id
            LEFT JOIN presence p ON p.user_id = u.user_id
            ORDER BY
                cm.rank_score DESC,
                COALESCE(p.last_active_ts, 0) DESC,
                u.created_ts DESC,
                u.username ASC
            LIMIT $6
            "#,
            exact_pattern,
            prefix_pattern,
            contains_pattern,
            exact_only,
            normalized,
            safe_limit,
        )
        .fetch_all(&*self.pool)
        .await?;

        let _ = self.cache.set(&cache_key, rows.clone(), USER_DIRECTORY_SEARCH_CACHE_TTL_SECS).await;

        Ok(rows)
    }

    pub async fn delete_user(&self, user_id: &str) -> Result<(), sqlx::Error> {
        tracing::info!(user_id = %user_id, "Deleting user");
        sqlx::query!(r"DELETE FROM users WHERE user_id = $1", user_id)
            .execute(&*self.pool)
            .await?;
        Ok(())
    }
}
