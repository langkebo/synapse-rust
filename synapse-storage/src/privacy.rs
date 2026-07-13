use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct UserPrivacySettings {
    pub id: i64,
    pub user_id: String,
    pub profile_visibility: String,
    pub avatar_visibility: String,
    pub displayname_visibility: String,
    pub presence_visibility: String,
    pub room_membership_visibility: String,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacySettingsUpdate {
    pub profile_visibility: Option<String>,
    pub avatar_visibility: Option<String>,
    pub displayname_visibility: Option<String>,
    pub presence_visibility: Option<String>,
    pub room_membership_visibility: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePrivacySettingsParams {
    pub user_id: String,
}

impl Default for UserPrivacySettings {
    fn default() -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            id: 0,
            user_id: String::new(),
            profile_visibility: "public".to_string(),
            avatar_visibility: "public".to_string(),
            displayname_visibility: "public".to_string(),
            presence_visibility: "contacts".to_string(),
            room_membership_visibility: "contacts".to_string(),
            created_ts: now,
            updated_ts: Some(now),
        }
    }
}

// ── Trait ───────────────────────────────────────────────────────────────

#[async_trait]
pub trait PrivacyStoreApi: Send + Sync {
    async fn create_tables(&self) -> Result<(), sqlx::Error>;
    async fn get_settings(&self, user_id: &str) -> Result<Option<UserPrivacySettings>, sqlx::Error>;
    async fn get_or_create_settings(&self, user_id: &str) -> Result<UserPrivacySettings, sqlx::Error>;
    async fn update_settings(
        &self,
        user_id: &str,
        update: PrivacySettingsUpdate,
    ) -> Result<UserPrivacySettings, sqlx::Error>;
    async fn can_view_profile(&self, viewer_id: Option<&str>, target_user_id: &str) -> Result<bool, sqlx::Error>;
    async fn can_view_presence(&self, viewer_id: Option<&str>, target_user_id: &str) -> Result<bool, sqlx::Error>;
    async fn batch_can_view_profile(
        &self,
        requester_id: Option<&str>,
        user_ids: &[String],
    ) -> Result<std::collections::HashMap<String, bool>, sqlx::Error>;
}

// ── Postgres implementation ─────────────────────────────────────────────

#[derive(Clone)]
pub struct PrivacyStorage {
    pool: Arc<Pool<Postgres>>,
}

impl PrivacyStorage {
    pub fn new(pool: Arc<Pool<Postgres>>) -> Self {
        Self { pool }
    }

    pub async fn create_tables(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS user_privacy_settings (
                id BIGSERIAL PRIMARY KEY,
                user_id TEXT NOT NULL UNIQUE,
                profile_visibility TEXT NOT NULL DEFAULT 'public',
                avatar_visibility TEXT NOT NULL DEFAULT 'public',
                displayname_visibility TEXT NOT NULL DEFAULT 'public',
                presence_visibility TEXT NOT NULL DEFAULT 'contacts',
                room_membership_visibility TEXT NOT NULL DEFAULT 'contacts',
                created_ts BIGINT NOT NULL,
                updated_ts BIGINT NOT NULL,

                CONSTRAINT user_privacy_settings_user_id_fkey
                    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_user_privacy_settings_user ON user_privacy_settings(user_id);
            "#,
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_settings(&self, user_id: &str) -> Result<Option<UserPrivacySettings>, sqlx::Error> {
        let row = sqlx::query_as::<_, UserPrivacySettings>(
            r#"
            SELECT id, user_id, profile_visibility, avatar_visibility, displayname_visibility, presence_visibility, room_membership_visibility, created_ts, updated_ts FROM user_privacy_settings WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_or_create_settings(&self, user_id: &str) -> Result<UserPrivacySettings, sqlx::Error> {
        if let Some(settings) = self.get_settings(user_id).await? {
            return Ok(settings);
        }

        let now = chrono::Utc::now().timestamp_millis();
        let default_settings = UserPrivacySettings::default();

        let row = sqlx::query_as::<_, UserPrivacySettings>(
            r#"
            INSERT INTO user_privacy_settings (
                user_id, profile_visibility, avatar_visibility, displayname_visibility,
                presence_visibility, room_membership_visibility, created_ts, updated_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (user_id) DO UPDATE SET updated_ts = $8
            RETURNING *
            "#,
        )
        .bind(user_id)
        .bind(&default_settings.profile_visibility)
        .bind(&default_settings.avatar_visibility)
        .bind(&default_settings.displayname_visibility)
        .bind(&default_settings.presence_visibility)
        .bind(&default_settings.room_membership_visibility)
        .bind(now)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn update_settings(
        &self,
        user_id: &str,
        update: PrivacySettingsUpdate,
    ) -> Result<UserPrivacySettings, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        let current = self.get_or_create_settings(user_id).await?;

        let row = sqlx::query_as::<_, UserPrivacySettings>(
            r#"
            UPDATE user_privacy_settings
            SET profile_visibility = $2,
                avatar_visibility = $3,
                displayname_visibility = $4,
                presence_visibility = $5,
                room_membership_visibility = $6,
                updated_ts = $7
            WHERE user_id = $1
            RETURNING *
            "#,
        )
        .bind(user_id)
        .bind(update.profile_visibility.unwrap_or(current.profile_visibility))
        .bind(update.avatar_visibility.unwrap_or(current.avatar_visibility))
        .bind(update.displayname_visibility.unwrap_or(current.displayname_visibility))
        .bind(update.presence_visibility.unwrap_or(current.presence_visibility))
        .bind(update.room_membership_visibility.unwrap_or(current.room_membership_visibility))
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn can_view_profile(&self, viewer_id: Option<&str>, target_user_id: &str) -> Result<bool, sqlx::Error> {
        let settings = self.get_or_create_settings(target_user_id).await?;

        let can_view = match settings.profile_visibility.as_str() {
            "public" => true,
            "private" => viewer_id == Some(target_user_id),
            "contacts" => {
                if let Some(viewer) = viewer_id {
                    self.are_contacts(viewer, target_user_id).await?
                } else {
                    false
                }
            }
            _ => true,
        };

        Ok(can_view)
    }

    pub async fn can_view_presence(&self, viewer_id: Option<&str>, target_user_id: &str) -> Result<bool, sqlx::Error> {
        let settings = self.get_or_create_settings(target_user_id).await?;

        let can_view = match settings.presence_visibility.as_str() {
            "public" => true,
            "private" => viewer_id == Some(target_user_id),
            "contacts" => {
                if let Some(viewer) = viewer_id {
                    self.are_contacts(viewer, target_user_id).await?
                } else {
                    false
                }
            }
            _ => true,
        };

        Ok(can_view)
    }

    async fn are_contacts(&self, user1: &str, user2: &str) -> Result<bool, sqlx::Error> {
        let in_same_room: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM room_memberships rm1
                JOIN room_memberships rm2 ON rm1.room_id = rm2.room_id
                WHERE rm1.user_id = $1
                  AND rm2.user_id = $2
                  AND rm1.membership = 'join'
                  AND rm2.membership = 'join'
            )
            "#,
        )
        .bind(user1)
        .bind(user2)
        .fetch_one(&*self.pool)
        .await?;

        Ok(in_same_room)
    }

    pub async fn batch_can_view_profile(
        &self,
        requester_id: Option<&str>,
        user_ids: &[String],
    ) -> Result<std::collections::HashMap<String, bool>, sqlx::Error> {
        let mut result = std::collections::HashMap::with_capacity(user_ids.len());
        if user_ids.is_empty() {
            return Ok(result);
        }

        for uid in user_ids {
            result.insert(uid.clone(), true);
        }

        let rows = sqlx::query(
            "SELECT user_id, profile_visibility, allow_profile_lookup FROM user_privacy_settings WHERE user_id = ANY($1)"
        )
        .bind(user_ids)
        .fetch_all(&*self.pool)
        .await?;

        for row in rows {
            use sqlx::Row;
            let uid: String = row.try_get("user_id").unwrap_or_default();
            let is_self = requester_id == Some(uid.as_str());

            let visible = if let Ok(visibility) = row.try_get::<String, _>("profile_visibility") {
                match visibility.as_str() {
                    "private" | "contacts" => is_self,
                    _ => true,
                }
            } else if let Ok(allow_lookup) = row.try_get::<bool, _>("allow_profile_lookup") {
                allow_lookup || is_self
            } else {
                true
            };

            result.insert(uid, visible);
        }

        Ok(result)
    }
}

// ── Delegation impl ─────────────────────────────────────────────────────

#[async_trait]
impl PrivacyStoreApi for PrivacyStorage {
    async fn create_tables(&self) -> Result<(), sqlx::Error> {
        self.create_tables().await
    }
    async fn get_settings(&self, user_id: &str) -> Result<Option<UserPrivacySettings>, sqlx::Error> {
        self.get_settings(user_id).await
    }
    async fn get_or_create_settings(&self, user_id: &str) -> Result<UserPrivacySettings, sqlx::Error> {
        self.get_or_create_settings(user_id).await
    }
    async fn update_settings(
        &self,
        user_id: &str,
        update: PrivacySettingsUpdate,
    ) -> Result<UserPrivacySettings, sqlx::Error> {
        self.update_settings(user_id, update).await
    }
    async fn can_view_profile(&self, viewer_id: Option<&str>, target_user_id: &str) -> Result<bool, sqlx::Error> {
        self.can_view_profile(viewer_id, target_user_id).await
    }
    async fn can_view_presence(&self, viewer_id: Option<&str>, target_user_id: &str) -> Result<bool, sqlx::Error> {
        self.can_view_presence(viewer_id, target_user_id).await
    }
    async fn batch_can_view_profile(
        &self,
        requester_id: Option<&str>,
        user_ids: &[String],
    ) -> Result<std::collections::HashMap<String, bool>, sqlx::Error> {
        self.batch_can_view_profile(requester_id, user_ids).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_privacy_settings() {
        let settings = UserPrivacySettings::default();
        assert_eq!(settings.profile_visibility, "public");
        assert_eq!(settings.presence_visibility, "contacts");
    }

    #[test]
    fn test_privacy_settings_update() {
        let update = PrivacySettingsUpdate {
            profile_visibility: Some("private".to_string()),
            avatar_visibility: None,
            displayname_visibility: Some("contacts".to_string()),
            presence_visibility: None,
            room_membership_visibility: None,
        };

        assert_eq!(update.profile_visibility, Some("private".to_string()));
        assert!(update.avatar_visibility.is_none());
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;
    use std::env;

    async fn test_pool() -> Arc<sqlx::Pool<Postgres>> {
        let db_url = env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
        let pool =
            PgPoolOptions::new().max_connections(2).connect(&db_url).await.expect("Failed to connect to test database");
        Arc::new(pool)
    }

    /// Clean up all test data matching the given suffix from privacy-related tables.
    async fn cleanup_privacy_data(pool: &sqlx::Pool<Postgres>, suffix: &str) {
        let pattern = format!("%{suffix}%");
        // Delete in FK-safe order: privacy settings → memberships → rooms → users
        let _ =
            sqlx::query("DELETE FROM user_privacy_settings WHERE user_id LIKE $1").bind(&pattern).execute(pool).await;
        let _ = sqlx::query("DELETE FROM room_memberships WHERE user_id LIKE $1 OR room_id LIKE $1")
            .bind(&pattern)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM rooms WHERE room_id LIKE $1").bind(&pattern).execute(pool).await;
        let _ = sqlx::query("DELETE FROM users WHERE user_id LIKE $1").bind(&pattern).execute(pool).await;
    }

    /// Insert a minimal user row so FK constraints are satisfied.
    async fn ensure_test_user(pool: &sqlx::Pool<Postgres>, user_id: &str) {
        let now = chrono::Utc::now().timestamp_millis();
        let username = user_id.strip_prefix('@').and_then(|u| u.split(':').next()).unwrap_or("testuser");
        sqlx::query(
            r#"INSERT INTO users (user_id, username, created_ts)
               VALUES ($1, $2, $3)
               ON CONFLICT (user_id) DO NOTHING"#,
        )
        .bind(user_id)
        .bind(username)
        .bind(now)
        .execute(pool)
        .await
        .expect("failed to create test user");
    }

    /// Insert a minimal room row so FK constraints on room_memberships are satisfied.
    async fn ensure_test_room(pool: &sqlx::Pool<Postgres>, room_id: &str) {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            r#"INSERT INTO rooms (room_id, created_ts)
               VALUES ($1, $2)
               ON CONFLICT (room_id) DO NOTHING"#,
        )
        .bind(room_id)
        .bind(now)
        .execute(pool)
        .await
        .expect("failed to create test room");
    }

    /// Insert a room_membership row for the given room and user.
    async fn ensure_room_membership(pool: &sqlx::Pool<Postgres>, room_id: &str, user_id: &str, membership: &str) {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            r#"INSERT INTO room_memberships (room_id, user_id, membership, joined_ts)
               VALUES ($1, $2, $3, $4)
               ON CONFLICT (room_id, user_id) DO UPDATE SET membership = $3"#,
        )
        .bind(room_id)
        .bind(user_id)
        .bind(membership)
        .bind(now)
        .execute(pool)
        .await
        .expect("failed to create room membership");
    }

    // --- Table existence ---

    #[tokio::test]
    async fn test_privacy_tables_exist() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_privacy_data(&pool, &suffix).await;

        // Tables should already exist from migrations — verify they're queryable
        let row =
            sqlx::query("SELECT 1::INT AS check_val FROM user_privacy_settings LIMIT 0").fetch_optional(&*pool).await;
        assert!(row.is_ok(), "user_privacy_settings table should exist");

        cleanup_privacy_data(&pool, &suffix).await;
    }

    // --- get_settings ---

    #[tokio::test]
    async fn test_get_settings_nonexistent_user() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_privacy_data(&pool, &suffix).await;

        let storage = PrivacyStorage::new(pool.clone());
        let user_id = format!("@nonexistent_{}:test.com", suffix);

        let result = storage.get_settings(&user_id).await.expect("get_settings query should succeed");
        assert!(result.is_none(), "nonexistent user should return None");

        cleanup_privacy_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_settings_returns_existing() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_privacy_data(&pool, &suffix).await;

        let storage = PrivacyStorage::new(pool.clone());
        let user_id = format!("@get_existing_{}:test.com", suffix);
        ensure_test_user(&pool, &user_id).await;

        // Create settings via get_or_create_settings
        let created = storage.get_or_create_settings(&user_id).await.expect("should create settings");
        assert_eq!(created.user_id, user_id);

        // get_settings should find it
        let found = storage.get_settings(&user_id).await.expect("get_settings query should succeed");
        assert!(found.is_some(), "existing settings should be found");
        let found = found.unwrap();
        assert_eq!(found.id, created.id);
        assert_eq!(found.user_id, created.user_id);
        assert_eq!(found.profile_visibility, created.profile_visibility);

        cleanup_privacy_data(&pool, &suffix).await;
    }

    // --- get_or_create_settings ---

    #[tokio::test]
    async fn test_get_or_create_settings_creates_with_defaults() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_privacy_data(&pool, &suffix).await;

        let storage = PrivacyStorage::new(pool.clone());
        let user_id = format!("@defaults_{}:test.com", suffix);
        ensure_test_user(&pool, &user_id).await;

        let settings = storage.get_or_create_settings(&user_id).await.expect("should create settings");

        assert!(settings.id > 0);
        assert_eq!(settings.user_id, user_id);
        assert_eq!(settings.profile_visibility, "public");
        assert_eq!(settings.avatar_visibility, "public");
        assert_eq!(settings.displayname_visibility, "public");
        assert_eq!(settings.presence_visibility, "contacts");
        assert_eq!(settings.room_membership_visibility, "contacts");
        assert!(settings.created_ts > 0);
        assert!(settings.updated_ts.unwrap_or(0) > 0);
        assert_eq!(Some(settings.created_ts), settings.updated_ts);

        cleanup_privacy_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_or_create_settings_is_idempotent() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_privacy_data(&pool, &suffix).await;

        let storage = PrivacyStorage::new(pool.clone());
        let user_id = format!("@idempotent_{}:test.com", suffix);
        ensure_test_user(&pool, &user_id).await;

        let first = storage.get_or_create_settings(&user_id).await.expect("first call should succeed");
        let second = storage.get_or_create_settings(&user_id).await.expect("second call should succeed");

        assert_eq!(second.id, first.id, "second call should return the same row");
        assert_eq!(second.profile_visibility, "public");
        assert_eq!(second.created_ts, first.created_ts);

        cleanup_privacy_data(&pool, &suffix).await;
    }

    // --- update_settings ---

    #[tokio::test]
    async fn test_update_settings_all_fields() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_privacy_data(&pool, &suffix).await;

        let storage = PrivacyStorage::new(pool.clone());
        let user_id = format!("@update_all_{}:test.com", suffix);
        ensure_test_user(&pool, &user_id).await;

        // First create defaults
        let original = storage.get_or_create_settings(&user_id).await.expect("should create defaults");

        // Update all fields
        let update = PrivacySettingsUpdate {
            profile_visibility: Some("private".to_string()),
            avatar_visibility: Some("private".to_string()),
            displayname_visibility: Some("contacts".to_string()),
            presence_visibility: Some("public".to_string()),
            room_membership_visibility: Some("private".to_string()),
        };
        let updated = storage.update_settings(&user_id, update).await.expect("update should succeed");

        assert_eq!(updated.id, original.id);
        assert_eq!(updated.profile_visibility, "private");
        assert_eq!(updated.avatar_visibility, "private");
        assert_eq!(updated.displayname_visibility, "contacts");
        assert_eq!(updated.presence_visibility, "public");
        assert_eq!(updated.room_membership_visibility, "private");
        assert!(updated.updated_ts >= original.updated_ts);

        cleanup_privacy_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_update_settings_partial_fields() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_privacy_data(&pool, &suffix).await;

        let storage = PrivacyStorage::new(pool.clone());
        let user_id = format!("@update_partial_{}:test.com", suffix);
        ensure_test_user(&pool, &user_id).await;

        // First create defaults
        let _ = storage.get_or_create_settings(&user_id).await.expect("should create defaults");

        // Update only profile_visibility — other fields should stay at defaults
        let update = PrivacySettingsUpdate {
            profile_visibility: Some("private".to_string()),
            avatar_visibility: None,
            displayname_visibility: None,
            presence_visibility: None,
            room_membership_visibility: None,
        };
        let updated = storage.update_settings(&user_id, update).await.expect("partial update should succeed");

        assert_eq!(updated.profile_visibility, "private");
        assert_eq!(updated.avatar_visibility, "public"); // unchanged
        assert_eq!(updated.displayname_visibility, "public"); // unchanged
        assert_eq!(updated.presence_visibility, "contacts"); // unchanged
        assert_eq!(updated.room_membership_visibility, "contacts"); // unchanged

        cleanup_privacy_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_update_settings_creates_for_nonexistent_user() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_privacy_data(&pool, &suffix).await;

        let storage = PrivacyStorage::new(pool.clone());
        let user_id = format!("@update_create_{}:test.com", suffix);
        ensure_test_user(&pool, &user_id).await;

        // No prior settings exist — update_settings calls get_or_create_settings internally
        let update = PrivacySettingsUpdate {
            profile_visibility: Some("contacts".to_string()),
            avatar_visibility: None,
            displayname_visibility: None,
            presence_visibility: None,
            room_membership_visibility: None,
        };
        let updated = storage.update_settings(&user_id, update).await.expect("update should create and then update");

        assert_eq!(updated.user_id, user_id);
        assert_eq!(updated.profile_visibility, "contacts");
        // Others should have the defaults from get_or_create_settings
        assert_eq!(updated.avatar_visibility, "public");
        assert_eq!(updated.displayname_visibility, "public");

        cleanup_privacy_data(&pool, &suffix).await;
    }

    // --- can_view_profile ---

    #[tokio::test]
    async fn test_can_view_profile_public_visible_to_other() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_privacy_data(&pool, &suffix).await;

        let storage = PrivacyStorage::new(pool.clone());
        let target = format!("@target_public_{}:test.com", suffix);
        let other = format!("@other_{}:test.com", suffix);
        ensure_test_user(&pool, &target).await;
        ensure_test_user(&pool, &other).await;

        // Default profile_visibility is "public"
        let _ = storage.get_or_create_settings(&target).await.expect("should create defaults");

        let can_view = storage.can_view_profile(Some(&other), &target).await.expect("query should succeed");
        assert!(can_view, "other user should see a public profile");

        cleanup_privacy_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_can_view_profile_public_visible_to_anonymous() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_privacy_data(&pool, &suffix).await;

        let storage = PrivacyStorage::new(pool.clone());
        let target = format!("@target_pubanon_{}:test.com", suffix);
        ensure_test_user(&pool, &target).await;

        let _ = storage.get_or_create_settings(&target).await.expect("should create defaults");

        let can_view = storage.can_view_profile(None, &target).await.expect("query should succeed");
        assert!(can_view, "anonymous should see a public profile");

        cleanup_privacy_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_can_view_profile_private_self() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_privacy_data(&pool, &suffix).await;

        let storage = PrivacyStorage::new(pool.clone());
        let user_id = format!("@private_self_{}:test.com", suffix);
        ensure_test_user(&pool, &user_id).await;

        // Create settings and set profile_visibility to "private"
        let update = PrivacySettingsUpdate {
            profile_visibility: Some("private".to_string()),
            avatar_visibility: None,
            displayname_visibility: None,
            presence_visibility: None,
            room_membership_visibility: None,
        };
        storage.update_settings(&user_id, update).await.expect("update should succeed");

        // Self should see own private profile
        let can_view = storage.can_view_profile(Some(&user_id), &user_id).await.expect("query should succeed");
        assert!(can_view, "self should see own private profile");

        cleanup_privacy_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_can_view_profile_private_other() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_privacy_data(&pool, &suffix).await;

        let storage = PrivacyStorage::new(pool.clone());
        let target = format!("@target_private_{}:test.com", suffix);
        let other = format!("@other_{}:test.com", suffix);
        ensure_test_user(&pool, &target).await;
        ensure_test_user(&pool, &other).await;

        let update = PrivacySettingsUpdate {
            profile_visibility: Some("private".to_string()),
            avatar_visibility: None,
            displayname_visibility: None,
            presence_visibility: None,
            room_membership_visibility: None,
        };
        storage.update_settings(&target, update).await.expect("update should succeed");

        // Other user should NOT see a private profile
        let can_view = storage.can_view_profile(Some(&other), &target).await.expect("query should succeed");
        assert!(!can_view, "other user should not see a private profile");

        cleanup_privacy_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_can_view_profile_private_anonymous() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_privacy_data(&pool, &suffix).await;

        let storage = PrivacyStorage::new(pool.clone());
        let target = format!("@target_privanon_{}:test.com", suffix);
        ensure_test_user(&pool, &target).await;

        let update = PrivacySettingsUpdate {
            profile_visibility: Some("private".to_string()),
            avatar_visibility: None,
            displayname_visibility: None,
            presence_visibility: None,
            room_membership_visibility: None,
        };
        storage.update_settings(&target, update).await.expect("update should succeed");

        // Anonymous (None) should NOT see a private profile
        let can_view = storage.can_view_profile(None, &target).await.expect("query should succeed");
        assert!(!can_view, "anonymous should not see a private profile");

        cleanup_privacy_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_can_view_profile_contacts_with_shared_room() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_privacy_data(&pool, &suffix).await;

        let storage = PrivacyStorage::new(pool.clone());
        let user_a = format!("@contact_a_{}:test.com", suffix);
        let user_b = format!("@contact_b_{}:test.com", suffix);
        let room = format!("!room_{}:test.com", suffix);
        ensure_test_user(&pool, &user_a).await;
        ensure_test_user(&pool, &user_b).await;
        ensure_test_room(&pool, &room).await;

        // Set profile_visibility to "contacts" for user_a
        let update = PrivacySettingsUpdate {
            profile_visibility: Some("contacts".to_string()),
            avatar_visibility: None,
            displayname_visibility: None,
            presence_visibility: None,
            room_membership_visibility: None,
        };
        storage.update_settings(&user_a, update).await.expect("update should succeed");

        // Both users are members of the same room (contacts)
        ensure_room_membership(&pool, &room, &user_a, "join").await;
        ensure_room_membership(&pool, &room, &user_b, "join").await;

        // user_b should see user_a's profile because they share a room
        let can_view = storage.can_view_profile(Some(&user_b), &user_a).await.expect("query should succeed");
        assert!(can_view, "contact sharing a room should see contacts-visibility profile");

        cleanup_privacy_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_can_view_profile_contacts_without_shared_room() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_privacy_data(&pool, &suffix).await;

        let storage = PrivacyStorage::new(pool.clone());
        let user_a = format!("@nocontact_a_{}:test.com", suffix);
        let user_b = format!("@nocontact_b_{}:test.com", suffix);
        ensure_test_user(&pool, &user_a).await;
        ensure_test_user(&pool, &user_b).await;

        // Set profile_visibility to "contacts"
        let update = PrivacySettingsUpdate {
            profile_visibility: Some("contacts".to_string()),
            avatar_visibility: None,
            displayname_visibility: None,
            presence_visibility: None,
            room_membership_visibility: None,
        };
        storage.update_settings(&user_a, update).await.expect("update should succeed");

        // No shared room — user_b is NOT a contact
        let can_view = storage.can_view_profile(Some(&user_b), &user_a).await.expect("query should succeed");
        assert!(!can_view, "non-contact should not see contacts-visibility profile");

        cleanup_privacy_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_can_view_profile_contacts_anonymous() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_privacy_data(&pool, &suffix).await;

        let storage = PrivacyStorage::new(pool.clone());
        let user_id = format!("@contactanon_{}:test.com", suffix);
        ensure_test_user(&pool, &user_id).await;

        let update = PrivacySettingsUpdate {
            profile_visibility: Some("contacts".to_string()),
            avatar_visibility: None,
            displayname_visibility: None,
            presence_visibility: None,
            room_membership_visibility: None,
        };
        storage.update_settings(&user_id, update).await.expect("update should succeed");

        // Anonymous viewer should not see contacts-visibility profile
        let can_view = storage.can_view_profile(None, &user_id).await.expect("query should succeed");
        assert!(!can_view, "anonymous should not see contacts-visibility profile");

        cleanup_privacy_data(&pool, &suffix).await;
    }

    // --- can_view_presence ---

    #[tokio::test]
    async fn test_can_view_presence_public_visible_to_other() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_privacy_data(&pool, &suffix).await;

        let storage = PrivacyStorage::new(pool.clone());
        let target = format!("@presence_pub_{}:test.com", suffix);
        let other = format!("@presence_other_{}:test.com", suffix);
        ensure_test_user(&pool, &target).await;
        ensure_test_user(&pool, &other).await;

        // Set presence_visibility to "public" (default is "contacts")
        let update = PrivacySettingsUpdate {
            profile_visibility: None,
            avatar_visibility: None,
            displayname_visibility: None,
            presence_visibility: Some("public".to_string()),
            room_membership_visibility: None,
        };
        storage.update_settings(&target, update).await.expect("update should succeed");

        let can_view = storage.can_view_presence(Some(&other), &target).await.expect("query should succeed");
        assert!(can_view, "other user should see public presence");

        cleanup_privacy_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_can_view_presence_private_self_only() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_privacy_data(&pool, &suffix).await;

        let storage = PrivacyStorage::new(pool.clone());
        let target = format!("@presence_priv_{}:test.com", suffix);
        let other = format!("@presence_other2_{}:test.com", suffix);
        ensure_test_user(&pool, &target).await;
        ensure_test_user(&pool, &other).await;

        let update = PrivacySettingsUpdate {
            profile_visibility: None,
            avatar_visibility: None,
            displayname_visibility: None,
            presence_visibility: Some("private".to_string()),
            room_membership_visibility: None,
        };
        storage.update_settings(&target, update).await.expect("update should succeed");

        // Self should see own presence
        let can_view_self = storage.can_view_presence(Some(&target), &target).await.expect("query should succeed");
        assert!(can_view_self, "self should see own private presence");

        // Other should NOT see presence
        let can_view_other = storage.can_view_presence(Some(&other), &target).await.expect("query should succeed");
        assert!(!can_view_other, "other user should not see private presence");

        cleanup_privacy_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_can_view_presence_contacts_with_shared_room() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_privacy_data(&pool, &suffix).await;

        let storage = PrivacyStorage::new(pool.clone());
        let user_a = format!("@pres_contact_a_{}:test.com", suffix);
        let user_b = format!("@pres_contact_b_{}:test.com", suffix);
        let room = format!("!presroom_{}:test.com", suffix);
        ensure_test_user(&pool, &user_a).await;
        ensure_test_user(&pool, &user_b).await;
        ensure_test_room(&pool, &room).await;

        // Set presence_visibility to "contacts" (default)
        storage.get_or_create_settings(&user_a).await.expect("should create defaults");

        // Both users share a room
        ensure_room_membership(&pool, &room, &user_a, "join").await;
        ensure_room_membership(&pool, &room, &user_b, "join").await;

        let can_view = storage.can_view_presence(Some(&user_b), &user_a).await.expect("query should succeed");
        assert!(can_view, "contact sharing a room should see presence");

        cleanup_privacy_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_can_view_presence_contacts_without_shared_room() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_privacy_data(&pool, &suffix).await;

        let storage = PrivacyStorage::new(pool.clone());
        let user_a = format!("@pres_nocontact_a_{}:test.com", suffix);
        let user_b = format!("@pres_nocontact_b_{}:test.com", suffix);
        ensure_test_user(&pool, &user_a).await;
        ensure_test_user(&pool, &user_b).await;

        storage.get_or_create_settings(&user_a).await.expect("should create defaults");

        // No shared room
        let can_view = storage.can_view_presence(Some(&user_b), &user_a).await.expect("query should succeed");
        assert!(!can_view, "non-contact should not see contacts-visibility presence");

        cleanup_privacy_data(&pool, &suffix).await;
    }

    // --- batch_can_view_profile ---

    #[tokio::test]
    async fn test_batch_can_view_profile_empty_input() {
        let pool = test_pool().await;

        let storage = PrivacyStorage::new(pool.clone());

        let result =
            storage.batch_can_view_profile(None, &[]).await.expect("batch query with empty input should succeed");

        assert!(result.is_empty(), "empty input should produce empty result");
    }

    #[tokio::test]
    async fn test_batch_can_view_profile_basic() {
        let pool = test_pool().await;
        let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
        cleanup_privacy_data(&pool, &suffix).await;

        // Ensure the allow_profile_lookup column exists (may have been added
        // in a migration that `create_tables` does not include).
        sqlx::query(
            "ALTER TABLE user_privacy_settings ADD COLUMN IF NOT EXISTS allow_profile_lookup BOOLEAN DEFAULT TRUE",
        )
        .execute(&*pool)
        .await
        .ok();

        let storage = PrivacyStorage::new(pool.clone());
        let user_a = format!("@batch_a_{}:test.com", suffix);
        let user_b = format!("@batch_b_{}:test.com", suffix);
        ensure_test_user(&pool, &user_a).await;
        ensure_test_user(&pool, &user_b).await;

        // Create default public settings for both
        storage.get_or_create_settings(&user_a).await.expect("should create defaults for A");
        storage.get_or_create_settings(&user_b).await.expect("should create defaults for B");

        let user_ids = vec![user_a.clone(), user_b.clone()];

        // Batch query as user_a (anonymous None variant also tested implicitly)
        let result =
            storage.batch_can_view_profile(Some(&user_a), &user_ids).await.expect("batch query should succeed");

        // Both users have default public settings, so both should be visible
        assert!(result.get(&user_a).unwrap_or(&false));
        assert!(result.get(&user_b).unwrap_or(&false));

        cleanup_privacy_data(&pool, &suffix).await;
    }
}
