use crate::cache::CacheManager;
use crate::common::constants::USER_PROFILE_CACHE_TTL;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres, Row};
use std::sync::Arc;

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct User {
    pub user_id: String,
    pub username: String,
    pub password_hash: Option<String>,
    pub displayname: Option<String>,
    pub avatar_url: Option<String>,
    pub is_admin: Option<bool>,
    pub deactivated: Option<bool>,
    pub is_guest: Option<bool>,
    pub consent_version: Option<String>,
    pub appservice_id: Option<String>,
    pub user_type: Option<String>,
    pub shadow_banned: Option<bool>,
    pub generation: i64,
    pub invalid_update_ts: Option<i64>,
    pub migration_state: Option<String>,
    pub creation_ts: i64,
    pub updated_ts: Option<i64>,
}

impl User {
    pub fn user_id(&self) -> String {
        self.user_id.clone()
    }
}

#[derive(Clone)]
pub struct UserStorage {
    pub pool: Arc<Pool<Postgres>>,
    pub cache: Arc<CacheManager>,
}

impl UserStorage {
    pub fn new(pool: &Arc<Pool<Postgres>>, cache: Arc<CacheManager>) -> Self {
        Self {
            pool: pool.clone(),
            cache,
        }
    }

    pub async fn create_user(
        &self,
        user_id: &str,
        username: &str,
        password_hash: Option<&str>,
        is_admin: bool,
    ) -> Result<User, sqlx::Error> {
        let now = chrono::Utc::now().timestamp();
        let generation = now * 1000;
        sqlx::query_as::<_, User>(
            r#"
            INSERT INTO users (user_id, username, password_hash, is_admin, creation_ts, generation)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING user_id, username, password_hash, displayname, avatar_url, is_admin, deactivated,
                      is_guest, consent_version, appservice_id, user_type, shadow_banned, generation,
                      invalid_update_ts, migration_state, creation_ts, updated_ts
            "#,
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

    pub async fn get_user_by_id(&self, user_id: &str) -> Result<Option<User>, sqlx::Error> {
        sqlx::query_as::<_, User>(
            r#"
            SELECT user_id, username, password_hash, displayname, avatar_url, is_admin, deactivated,
                   is_guest, consent_version, appservice_id, user_type, shadow_banned, generation,
                   invalid_update_ts, migration_state, creation_ts, updated_ts
            FROM users
            WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn get_user_by_username(&self, username: &str) -> Result<Option<User>, sqlx::Error> {
        sqlx::query_as::<_, User>(
            r#"
            SELECT user_id, username, password_hash, displayname, avatar_url, is_admin, deactivated,
                   is_guest, consent_version, appservice_id, user_type, shadow_banned, generation,
                   invalid_update_ts, migration_state, creation_ts, updated_ts
            FROM users
            WHERE username = $1
            "#,
        )
        .bind(username)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn get_user_by_identifier(
        &self,
        identifier: &str,
    ) -> Result<Option<User>, sqlx::Error> {
        if identifier.starts_with('@') && identifier.contains(':') {
            self.get_user_by_id(identifier).await
        } else {
            self.get_user_by_username(identifier).await
        }
    }

    pub async fn get_all_users(&self, limit: i64) -> Result<Vec<User>, sqlx::Error> {
        sqlx::query_as::<_, User>(
            r#"
            SELECT user_id, username, password_hash, displayname, avatar_url, is_admin, deactivated,
                   is_guest, consent_version, appservice_id, user_type, shadow_banned, generation,
                   invalid_update_ts, migration_state, creation_ts, updated_ts
            FROM users
            ORDER BY creation_ts DESC
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_users_paginated(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<User>, sqlx::Error> {
        sqlx::query_as::<_, User>(
            r#"
            SELECT user_id, username, password_hash, displayname, avatar_url, is_admin, deactivated,
                   is_guest, consent_version, appservice_id, user_type, shadow_banned, generation,
                   invalid_update_ts, migration_state, creation_ts, updated_ts
            FROM users
            ORDER BY creation_ts DESC
            LIMIT $1 OFFSET $2
            "#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_user_count(&self) -> Result<i64, sqlx::Error> {
        let row = sqlx::query(
            r#"
            SELECT COALESCE(COUNT(*), 0) as count FROM users
            "#,
        )
        .fetch_one(&*self.pool)
        .await?;
        row.try_get::<i64, _>("count")
    }

    pub async fn user_exists(&self, user_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"
            SELECT 1 FROM users WHERE user_id = $1 AND deactivated = FALSE LIMIT 1
            "#,
        )
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result.is_some())
    }

    pub async fn filter_existing_users(&self, user_ids: &[String]) -> Result<Vec<String>, sqlx::Error> {
        if user_ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query_scalar::<_, String>(
            "SELECT user_id FROM users WHERE user_id = ANY($1) AND COALESCE(deactivated, FALSE) = FALSE"
        )
        .bind(user_ids)
        .fetch_all(&*self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn update_password(
        &self,
        user_id: &str,
        password_hash: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(r#"UPDATE users SET password_hash = $1 WHERE user_id = $2"#)
            .bind(password_hash)
            .bind(user_id)
            .execute(&*self.pool)
            .await?;
        Ok(())
    }

    pub async fn update_displayname(
        &self,
        user_id: &str,
        displayname: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        // CRITICAL FIX: Update database first, then refresh cache to prevent cache stampede
        sqlx::query(r#"UPDATE users SET displayname = $1 WHERE user_id = $2"#)
            .bind(displayname)
            .bind(user_id)
            .execute(&*self.pool)
            .await?;

        // Refresh cache with new data instead of just deleting
        if let Ok(Some(profile)) = self.get_user_profile(user_id).await {
            let key = format!("user:profile:{}", user_id);
            // Cache for 1 hour
            let _ = self.cache.set(&key, &profile, USER_PROFILE_CACHE_TTL).await;
        }

        Ok(())
    }

    pub async fn update_avatar_url(
        &self,
        user_id: &str,
        avatar_url: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        // CRITICAL FIX: Update database first, then refresh cache to prevent cache stampede
        sqlx::query(r#"UPDATE users SET avatar_url = $1 WHERE user_id = $2"#)
            .bind(avatar_url)
            .bind(user_id)
            .execute(&*self.pool)
            .await?;

        // Refresh cache with new data instead of just deleting
        if let Ok(Some(profile)) = self.get_user_profile(user_id).await {
            let key = format!("user:profile:{}", user_id);
            // Cache for 1 hour
            let _ = self.cache.set(&key, &profile, USER_PROFILE_CACHE_TTL).await;
        }

        Ok(())
    }

    pub async fn deactivate_user(&self, user_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(r#"UPDATE users SET deactivated = TRUE WHERE user_id = $1"#)
            .bind(user_id)
            .execute(&*self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_admin_status(&self, user_id: &str, is_admin: bool) -> Result<(), sqlx::Error> {
        sqlx::query(r#"UPDATE users SET is_admin = $1 WHERE user_id = $2"#)
            .bind(is_admin)
            .bind(user_id)
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
        sqlx::query(
            r#"
            INSERT INTO user_account_data (user_id, event_type, content, created_ts)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (user_id, event_type) DO UPDATE SET content = EXCLUDED.content, created_ts = EXCLUDED.created_ts
            "#,
        )
        .bind(user_id)
        .bind(event_type)
        .bind(content_str)
        .bind(now)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn search_users(
        &self,
        query: &str,
        limit: i64,
    ) -> Result<Vec<UserSearchResult>, sqlx::Error> {
        let search_pattern = format!("%{}%", query);
        let rows = sqlx::query_as::<_, UserSearchResult>(
            r#"
            SELECT user_id, username, COALESCE(displayname, username) as displayname, avatar_url, creation_ts
            FROM users
            WHERE (username ILIKE $1 OR user_id ILIKE $1 OR displayname ILIKE $1)
            AND COALESCE(deactivated, FALSE) = FALSE
            ORDER BY
                CASE
                    WHEN username = $2 THEN 0
                    WHEN username ILIKE $2 THEN 1
                    ELSE 2
                END,
                creation_ts DESC
            LIMIT $3
            "#,
        )
        .bind(search_pattern)
        .bind(query)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn get_user_profile(
        &self,
        user_id: &str,
    ) -> Result<Option<UserProfile>, sqlx::Error> {
        let key = format!("user:profile:{}", user_id);
        
        // Try to get from cache
        if let Ok(Some(profile)) = self.cache.get::<UserProfile>(&key).await {
            return Ok(Some(profile));
        }

        let result = sqlx::query_as::<_, UserProfile>(
            r#"
            SELECT user_id, username, COALESCE(displayname, username) as displayname, avatar_url, creation_ts
            FROM users
            WHERE user_id = $1 AND COALESCE(deactivated, FALSE) = FALSE
            "#,
        )
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await?;
        
        // Set cache if found
        if let Some(profile) = &result {
            let _ = self.cache.set(&key, profile, USER_PROFILE_CACHE_TTL).await;
        }

        Ok(result)
    }

    pub async fn get_user_profiles_batch(
        &self,
        user_ids: &[String],
    ) -> Result<Vec<UserProfile>, sqlx::Error> {
        if user_ids.is_empty() {
            return Ok(vec![]);
        }

        sqlx::query_as::<_, UserProfile>(
            r#"
            SELECT user_id, username, COALESCE(displayname, username) as displayname, avatar_url, creation_ts
            FROM users
            WHERE user_id = ANY($1) AND COALESCE(deactivated, FALSE) = FALSE
            "#,
        )
        .bind(user_ids)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn delete_user(&self, user_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(r#"DELETE FROM users WHERE user_id = $1"#)
            .bind(user_id)
            .execute(&*self.pool)
            .await?;
        Ok(())
    }
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct UserSearchResult {
    pub user_id: String,
    pub username: String,
    pub displayname: Option<String>,
    pub avatar_url: Option<String>,
    pub creation_ts: i64,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct UserProfile {
    pub user_id: String,
    pub username: String,
    pub displayname: Option<String>,
    pub avatar_url: Option<String>,
    pub creation_ts: i64,
}
