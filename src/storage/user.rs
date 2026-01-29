use sqlx::{Pool, Postgres};
use std::sync::Arc;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct User {
    pub user_id: String,
    pub username: String,
    pub password_hash: Option<String>,
    pub displayname: Option<String>,
    pub avatar_url: Option<String>,
    pub admin: Option<bool>,
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
}

impl User {
    pub fn user_id(&self) -> String {
        self.user_id.clone()
    }
}

#[derive(Clone)]
pub struct UserStorage {
    pub pool: Arc<Pool<Postgres>>,
}

impl UserStorage {
    pub fn new(pool: &Arc<Pool<Postgres>>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_user(
        &self,
        user_id: &str,
        username: &str,
        password_hash: Option<&str>,
        is_admin: bool,
    ) -> Result<User, sqlx::Error> {
        let now = chrono::Utc::now().timestamp();
        sqlx::query_as!(
            User,
            r#"
            INSERT INTO users (user_id, username, password_hash, admin, creation_ts)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING *
            "#,
            user_id,
            username,
            password_hash,
            is_admin,
            now
        )
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn get_user_by_id(&self, user_id: &str) -> Result<Option<User>, sqlx::Error> {
        sqlx::query_as!(
            User,
            r#"
            SELECT * FROM users WHERE user_id = $1
            "#,
            user_id
        )
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn get_user_by_username(&self, username: &str) -> Result<Option<User>, sqlx::Error> {
        sqlx::query_as!(
            User,
            r#"
            SELECT * FROM users WHERE username = $1
            "#,
            username
        )
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn get_all_users(&self, limit: i64) -> Result<Vec<User>, sqlx::Error> {
        sqlx::query_as!(
            User,
            r#"
            SELECT * FROM users ORDER BY creation_ts DESC LIMIT $1
            "#,
            limit
        )
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn user_exists(&self, user_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            SELECT 1 AS "exists" FROM users WHERE user_id = $1 LIMIT 1
            "#,
            user_id
        )
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result.is_some())
    }

    pub async fn update_password(
        &self,
        user_id: &str,
        password_hash: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE users SET password_hash = $1 WHERE user_id = $2
            "#,
            password_hash,
            user_id
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn update_displayname(
        &self,
        user_id: &str,
        displayname: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE users SET displayname = $1 WHERE user_id = $2
            "#,
            displayname,
            user_id
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn update_avatar_url(
        &self,
        user_id: &str,
        avatar_url: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE users SET avatar_url = $1 WHERE user_id = $2
            "#,
            avatar_url,
            user_id
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn deactivate_user(&self, user_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE users SET deactivated = TRUE WHERE user_id = $1
            "#,
            user_id
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_user_count(&self) -> Result<i64, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            SELECT COUNT(*) as count FROM users
            "#
        )
        .fetch_one(&*self.pool)
        .await?;
        Ok(result.count.unwrap_or(0))
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
}
