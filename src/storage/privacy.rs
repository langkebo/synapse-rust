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
    pub updated_ts: i64,
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
            updated_ts: now,
        }
    }
}

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
            SELECT * FROM user_privacy_settings WHERE user_id = $1
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

    pub async fn can_view_profile(
        &self,
        viewer_id: Option<&str>,
        target_user_id: &str,
    ) -> Result<bool, sqlx::Error> {
        let settings = self.get_or_create_settings(target_user_id).await?;

        let can_view = match settings.profile_visibility.as_str() {
            "public" => true,
            "private" => viewer_id.map(|v| v == target_user_id).unwrap_or(false),
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

    pub async fn can_view_presence(
        &self,
        viewer_id: Option<&str>,
        target_user_id: &str,
    ) -> Result<bool, sqlx::Error> {
        let settings = self.get_or_create_settings(target_user_id).await?;

        let can_view = match settings.presence_visibility.as_str() {
            "public" => true,
            "private" => viewer_id.map(|v| v == target_user_id).unwrap_or(false),
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
