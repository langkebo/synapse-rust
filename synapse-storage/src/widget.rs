// Widget Storage - MSC4261
// Implements embedded application support for Matrix rooms
// Following project field naming standards

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Widget {
    pub id: i64,
    pub widget_id: String,
    pub room_id: Option<String>,
    pub user_id: String,
    pub widget_type: String,
    pub url: String,
    pub name: String,
    pub data: serde_json::Value,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateWidgetParams {
    pub widget_id: String,
    pub room_id: Option<String>,
    pub user_id: String,
    pub widget_type: String,
    pub url: String,
    pub name: String,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct WidgetPermission {
    pub id: i64,
    pub widget_id: String,
    pub user_id: String,
    pub permissions: serde_json::Value,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct WidgetSession {
    pub id: i64,
    pub session_id: String,
    pub widget_id: String,
    pub user_id: String,
    pub device_id: Option<String>,
    pub created_ts: i64,
    pub last_active_ts: Option<i64>,
    pub expires_at: Option<i64>,
    pub is_active: bool,
}

#[async_trait]
pub trait WidgetStoreApi: Send + Sync {
    async fn create_widget(&self, params: CreateWidgetParams) -> Result<Widget, sqlx::Error>;
    async fn get_widget(&self, widget_id: &str) -> Result<Option<Widget>, sqlx::Error>;
    async fn get_room_widgets(&self, room_id: &str) -> Result<Vec<Widget>, sqlx::Error>;
    async fn get_user_widgets(&self, user_id: &str) -> Result<Vec<Widget>, sqlx::Error>;
    async fn update_widget(
        &self,
        widget_id: &str,
        url: Option<&str>,
        name: Option<&str>,
        data: Option<&serde_json::Value>,
    ) -> Result<Option<Widget>, sqlx::Error>;
    async fn delete_widget(&self, widget_id: &str) -> Result<bool, sqlx::Error>;
    async fn set_widget_permission(
        &self,
        widget_id: &str,
        user_id: &str,
        permissions: serde_json::Value,
    ) -> Result<WidgetPermission, sqlx::Error>;
    async fn get_widget_permissions(&self, widget_id: &str) -> Result<Vec<WidgetPermission>, sqlx::Error>;
    async fn get_user_widget_permission(
        &self,
        widget_id: &str,
        user_id: &str,
    ) -> Result<Option<WidgetPermission>, sqlx::Error>;
    async fn delete_widget_permission(&self, widget_id: &str, user_id: &str) -> Result<bool, sqlx::Error>;
    async fn create_session(
        &self,
        session_id: &str,
        widget_id: &str,
        user_id: &str,
        device_id: Option<&str>,
        expires_in_ms: Option<i64>,
    ) -> Result<WidgetSession, sqlx::Error>;
    async fn get_session(&self, session_id: &str) -> Result<Option<WidgetSession>, sqlx::Error>;
    async fn update_session_activity(&self, session_id: &str) -> Result<bool, sqlx::Error>;
    async fn terminate_session(&self, session_id: &str) -> Result<bool, sqlx::Error>;
    async fn get_widget_sessions(&self, widget_id: &str) -> Result<Vec<WidgetSession>, sqlx::Error>;
    async fn cleanup_expired_sessions(&self) -> Result<u64, sqlx::Error>;
}

#[derive(Clone)]
pub struct WidgetStorage {
    pool: Arc<PgPool>,
}

impl WidgetStorage {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    pub async fn create_widget(&self, params: CreateWidgetParams) -> Result<Widget, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        let row = sqlx::query_as::<_, Widget>(
            r#"
            INSERT INTO widgets (widget_id, room_id, user_id, widget_type, url, name, data, created_ts, is_active)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, TRUE)
            RETURNING *
            "#,
        )
        .bind(&params.widget_id)
        .bind(&params.room_id)
        .bind(&params.user_id)
        .bind(&params.widget_type)
        .bind(&params.url)
        .bind(&params.name)
        .bind(&params.data)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_widget(&self, widget_id: &str) -> Result<Option<Widget>, sqlx::Error> {
        let row = sqlx::query_as::<_, Widget>(
            r#"
            SELECT id, widget_id, room_id, user_id, widget_type, url, name, data, created_ts, updated_ts, is_active FROM widgets WHERE widget_id = $1 AND is_active = TRUE
            "#,
        )
        .bind(widget_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_room_widgets(&self, room_id: &str) -> Result<Vec<Widget>, sqlx::Error> {
        let rows = sqlx::query_as::<_, Widget>(
            r#"
            SELECT id, widget_id, room_id, user_id, widget_type, url, name, data, created_ts, updated_ts, is_active FROM widgets WHERE room_id = $1 AND is_active = TRUE ORDER BY created_ts DESC
            "#,
        )
        .bind(room_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn get_user_widgets(&self, user_id: &str) -> Result<Vec<Widget>, sqlx::Error> {
        let rows = sqlx::query_as::<_, Widget>(
            r#"
            SELECT id, widget_id, room_id, user_id, widget_type, url, name, data, created_ts, updated_ts, is_active FROM widgets WHERE user_id = $1 AND is_active = TRUE ORDER BY created_ts DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn update_widget(
        &self,
        widget_id: &str,
        url: Option<&str>,
        name: Option<&str>,
        data: Option<&serde_json::Value>,
    ) -> Result<Option<Widget>, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        let row = sqlx::query_as::<_, Widget>(
            r#"
            UPDATE widgets
            SET url = COALESCE($2, url),
                name = COALESCE($3, name),
                data = COALESCE($4, data),
                updated_ts = $5
            WHERE widget_id = $1 AND is_active = TRUE
            RETURNING *
            "#,
        )
        .bind(widget_id)
        .bind(url)
        .bind(name)
        .bind(data)
        .bind(now)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn delete_widget(&self, widget_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"
            UPDATE widgets SET is_active = FALSE, updated_ts = $2 WHERE widget_id = $1 AND is_active = TRUE
            "#,
        )
        .bind(widget_id)
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn set_widget_permission(
        &self,
        widget_id: &str,
        user_id: &str,
        permissions: serde_json::Value,
    ) -> Result<WidgetPermission, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        let row = sqlx::query_as::<_, WidgetPermission>(
            r#"
            INSERT INTO widget_permissions (widget_id, user_id, permissions, created_ts)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (widget_id, user_id) DO UPDATE SET
                permissions = EXCLUDED.permissions,
                updated_ts = EXCLUDED.created_ts
            RETURNING *
            "#,
        )
        .bind(widget_id)
        .bind(user_id)
        .bind(&permissions)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_widget_permissions(&self, widget_id: &str) -> Result<Vec<WidgetPermission>, sqlx::Error> {
        let rows = sqlx::query_as::<_, WidgetPermission>(
            r#"
            SELECT id, widget_id, user_id, permissions, created_ts, updated_ts FROM widget_permissions WHERE widget_id = $1
            "#,
        )
        .bind(widget_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn get_user_widget_permission(
        &self,
        widget_id: &str,
        user_id: &str,
    ) -> Result<Option<WidgetPermission>, sqlx::Error> {
        let row = sqlx::query_as::<_, WidgetPermission>(
            r#"
            SELECT id, widget_id, user_id, permissions, created_ts, updated_ts FROM widget_permissions WHERE widget_id = $1 AND user_id = $2
            "#,
        )
        .bind(widget_id)
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn delete_widget_permission(&self, widget_id: &str, user_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"
            DELETE FROM widget_permissions WHERE widget_id = $1 AND user_id = $2
            "#,
        )
        .bind(widget_id)
        .bind(user_id)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn create_session(
        &self,
        session_id: &str,
        widget_id: &str,
        user_id: &str,
        device_id: Option<&str>,
        expires_in_ms: Option<i64>,
    ) -> Result<WidgetSession, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let expires_at = expires_in_ms.map(|ms| now + ms);

        let row = sqlx::query_as::<_, WidgetSession>(
            r#"
            INSERT INTO widget_sessions (session_id, widget_id, user_id, device_id, created_ts, last_active_ts, expires_at, is_active)
            VALUES ($1, $2, $3, $4, $5, $5, $6, TRUE)
            RETURNING *
            "#,
        )
        .bind(session_id)
        .bind(widget_id)
        .bind(user_id)
        .bind(device_id)
        .bind(now)
        .bind(expires_at)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_session(&self, session_id: &str) -> Result<Option<WidgetSession>, sqlx::Error> {
        let row = sqlx::query_as::<_, WidgetSession>(
            r#"
            SELECT id, session_id, widget_id, user_id, device_id, created_ts, last_active_ts, expires_at, is_active FROM widget_sessions WHERE session_id = $1 AND is_active = TRUE
            "#,
        )
        .bind(session_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn update_session_activity(&self, session_id: &str) -> Result<bool, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        let result = sqlx::query(
            r#"
            UPDATE widget_sessions SET last_active_ts = $2 WHERE session_id = $1 AND is_active = TRUE
            "#,
        )
        .bind(session_id)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn terminate_session(&self, session_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"
            UPDATE widget_sessions SET is_active = FALSE WHERE session_id = $1 AND is_active = TRUE
            "#,
        )
        .bind(session_id)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn get_widget_sessions(&self, widget_id: &str) -> Result<Vec<WidgetSession>, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        let rows = sqlx::query_as::<_, WidgetSession>(
            r#"
            SELECT id, session_id, widget_id, user_id, device_id, created_ts, last_active_ts, expires_at, is_active FROM widget_sessions
            WHERE widget_id = $1 AND is_active = TRUE AND (expires_at IS NULL OR expires_at > $2)
            ORDER BY last_active_ts DESC
            "#,
        )
        .bind(widget_id)
        .bind(now)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn cleanup_expired_sessions(&self) -> Result<u64, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        let result = sqlx::query(
            r#"
            UPDATE widget_sessions SET is_active = FALSE
            WHERE expires_at IS NOT NULL AND expires_at < $1 AND is_active = TRUE
            "#,
        )
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected())
    }
}

#[async_trait]
impl WidgetStoreApi for WidgetStorage {
    async fn create_widget(&self, params: CreateWidgetParams) -> Result<Widget, sqlx::Error> {
        self.create_widget(params).await
    }
    async fn get_widget(&self, widget_id: &str) -> Result<Option<Widget>, sqlx::Error> {
        self.get_widget(widget_id).await
    }
    async fn get_room_widgets(&self, room_id: &str) -> Result<Vec<Widget>, sqlx::Error> {
        self.get_room_widgets(room_id).await
    }
    async fn get_user_widgets(&self, user_id: &str) -> Result<Vec<Widget>, sqlx::Error> {
        self.get_user_widgets(user_id).await
    }
    async fn update_widget(
        &self,
        widget_id: &str,
        url: Option<&str>,
        name: Option<&str>,
        data: Option<&serde_json::Value>,
    ) -> Result<Option<Widget>, sqlx::Error> {
        self.update_widget(widget_id, url, name, data).await
    }
    async fn delete_widget(&self, widget_id: &str) -> Result<bool, sqlx::Error> {
        self.delete_widget(widget_id).await
    }
    async fn set_widget_permission(
        &self,
        widget_id: &str,
        user_id: &str,
        permissions: serde_json::Value,
    ) -> Result<WidgetPermission, sqlx::Error> {
        self.set_widget_permission(widget_id, user_id, permissions).await
    }
    async fn get_widget_permissions(&self, widget_id: &str) -> Result<Vec<WidgetPermission>, sqlx::Error> {
        self.get_widget_permissions(widget_id).await
    }
    async fn get_user_widget_permission(
        &self,
        widget_id: &str,
        user_id: &str,
    ) -> Result<Option<WidgetPermission>, sqlx::Error> {
        self.get_user_widget_permission(widget_id, user_id).await
    }
    async fn delete_widget_permission(&self, widget_id: &str, user_id: &str) -> Result<bool, sqlx::Error> {
        self.delete_widget_permission(widget_id, user_id).await
    }
    async fn create_session(
        &self,
        session_id: &str,
        widget_id: &str,
        user_id: &str,
        device_id: Option<&str>,
        expires_in_ms: Option<i64>,
    ) -> Result<WidgetSession, sqlx::Error> {
        self.create_session(session_id, widget_id, user_id, device_id, expires_in_ms).await
    }
    async fn get_session(&self, session_id: &str) -> Result<Option<WidgetSession>, sqlx::Error> {
        self.get_session(session_id).await
    }
    async fn update_session_activity(&self, session_id: &str) -> Result<bool, sqlx::Error> {
        self.update_session_activity(session_id).await
    }
    async fn terminate_session(&self, session_id: &str) -> Result<bool, sqlx::Error> {
        self.terminate_session(session_id).await
    }
    async fn get_widget_sessions(&self, widget_id: &str) -> Result<Vec<WidgetSession>, sqlx::Error> {
        self.get_widget_sessions(widget_id).await
    }
    async fn cleanup_expired_sessions(&self) -> Result<u64, sqlx::Error> {
        self.cleanup_expired_sessions().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_widget_params() {
        let params = CreateWidgetParams {
            widget_id: "widget_123".to_string(),
            room_id: Some("!room:example.com".to_string()),
            user_id: "@user:example.com".to_string(),
            widget_type: "customwidget".to_string(),
            url: "https://example.com/widget".to_string(),
            name: "My Widget".to_string(),
            data: serde_json::json!({"key": "value"}),
        };

        assert_eq!(params.widget_id, "widget_123");
        assert_eq!(params.widget_type, "customwidget");
    }

    #[test]
    fn test_widget_struct() {
        let widget = Widget {
            id: 1,
            widget_id: "widget_123".to_string(),
            room_id: Some("!room:example.com".to_string()),
            user_id: "@user:example.com".to_string(),
            widget_type: "customwidget".to_string(),
            url: "https://example.com/widget".to_string(),
            name: "My Widget".to_string(),
            data: serde_json::json!({}),
            created_ts: 1234567890000,
            updated_ts: None,
            is_active: true,
        };

        assert_eq!(widget.widget_id, "widget_123");
        assert!(widget.is_active);
    }

    #[test]
    fn test_widget_permission_struct() {
        let permission = WidgetPermission {
            id: 1,
            widget_id: "widget_123".to_string(),
            user_id: "@user:example.com".to_string(),
            permissions: serde_json::json!(["read", "write"]),
            created_ts: 1234567890000,
            updated_ts: None,
        };

        assert_eq!(permission.widget_id, "widget_123");
        assert!(permission.permissions.is_array());
    }

    #[test]
    fn test_widget_session_struct() {
        let session = WidgetSession {
            id: 1,
            session_id: "session_123".to_string(),
            widget_id: "widget_123".to_string(),
            user_id: "@user:example.com".to_string(),
            device_id: Some("DEVICE123".to_string()),
            created_ts: 1234567890000,
            last_active_ts: Some(1234567890000),
            expires_at: Some(1234571490000),
            is_active: true,
        };

        assert_eq!(session.session_id, "session_123");
        assert!(session.is_active);
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;

    async fn test_pool() -> Arc<PgPool> {
        let db_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
        let pool =
            PgPoolOptions::new().max_connections(2).connect(&db_url).await.expect("Failed to connect to test database");
        Arc::new(pool)
    }

    async fn ensure_test_user(pool: &PgPool, user_id: &str) {
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

    async fn ensure_test_room(pool: &PgPool, room_id: &str) {
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

    /// Hard-delete a widget and everything that cascades from it
    /// (widget_permissions, widget_sessions). Idempotent.
    async fn cleanup_widget(pool: &PgPool, widget_id: &str) {
        sqlx::query("DELETE FROM widgets WHERE widget_id = $1").bind(widget_id).execute(pool).await.ok();
    }

    /// Hard-delete a specific session. Idempotent.
    async fn cleanup_session(pool: &PgPool, session_id: &str) {
        sqlx::query("DELETE FROM widget_sessions WHERE session_id = $1").bind(session_id).execute(pool).await.ok();
    }

    // ——— Widget CRUD ————————————————————————————————————————————————

    #[tokio::test]
    async fn create_and_get_widget() {
        let pool = test_pool().await;
        let storage = WidgetStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let widget_id = format!("widget_{suffix}");
        let user_id = format!("@user_{suffix}:test.com");
        let room_id = format!("!room_{suffix}:test.com");

        ensure_test_user(&pool, &user_id).await;
        ensure_test_room(&pool, &room_id).await;
        cleanup_widget(&pool, &widget_id).await;

        let created = storage
            .create_widget(CreateWidgetParams {
                widget_id: widget_id.clone(),
                room_id: Some(room_id.clone()),
                user_id: user_id.clone(),
                widget_type: "customwidget".to_string(),
                url: "https://example.com/widget".to_string(),
                name: "Test Widget".to_string(),
                data: serde_json::json!({"key": "value"}),
            })
            .await
            .expect("create_widget should succeed");

        assert!(created.id > 0);
        assert_eq!(created.widget_id, widget_id);
        assert_eq!(created.room_id.as_deref(), Some(room_id.as_str()));
        assert_eq!(created.user_id, user_id);
        assert_eq!(created.widget_type, "customwidget");
        assert_eq!(created.url, "https://example.com/widget");
        assert_eq!(created.name, "Test Widget");
        assert_eq!(created.data, serde_json::json!({"key": "value"}));
        assert!(created.created_ts > 0);
        assert!(created.updated_ts.is_none());
        assert!(created.is_active);

        let found =
            storage.get_widget(&widget_id).await.expect("get_widget should succeed").expect("widget should be found");
        assert_eq!(found.id, created.id);
        assert_eq!(found.widget_id, widget_id);

        cleanup_widget(&pool, &widget_id).await;
    }

    #[tokio::test]
    async fn get_widget_not_found() {
        let pool = test_pool().await;
        let storage = WidgetStorage::new(pool.clone());

        let result = storage.get_widget("nonexistent_widget_12345").await.expect("query should succeed");
        assert!(result.is_none(), "non-existent widget should return None");
    }

    #[tokio::test]
    async fn get_room_widgets_filters_by_room() {
        let pool = test_pool().await;
        let storage = WidgetStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let user_id = format!("@roomtest_{suffix}:test.com");
        let room_a = format!("!room_a_{suffix}:test.com");
        let room_b = format!("!room_b_{suffix}:test.com");
        let w1 = format!("w1_{suffix}");
        let w2 = format!("w2_{suffix}");
        let w3 = format!("w3_{suffix}");

        ensure_test_user(&pool, &user_id).await;
        ensure_test_room(&pool, &room_a).await;
        ensure_test_room(&pool, &room_b).await;
        for wid in &[&w1, &w2, &w3] {
            cleanup_widget(&pool, wid).await;
        }

        let base_params = |w_id: &str, r_id: &str| CreateWidgetParams {
            widget_id: w_id.to_string(),
            room_id: Some(r_id.to_string()),
            user_id: user_id.clone(),
            widget_type: "m.custom".to_string(),
            url: "https://example.com".to_string(),
            name: "W".to_string(),
            data: serde_json::json!({}),
        };

        storage.create_widget(base_params(&w1, &room_a)).await.unwrap();
        storage.create_widget(base_params(&w2, &room_a)).await.unwrap();
        storage.create_widget(base_params(&w3, &room_b)).await.unwrap();

        let room_a_widgets = storage.get_room_widgets(&room_a).await.expect("get_room_widgets should succeed");
        assert_eq!(room_a_widgets.len(), 2);
        for w in &room_a_widgets {
            assert_eq!(w.room_id.as_deref(), Some(room_a.as_str()));
        }

        let room_b_widgets = storage.get_room_widgets(&room_b).await.expect("get_room_widgets should succeed");
        assert_eq!(room_b_widgets.len(), 1);

        for wid in &[&w1, &w2, &w3] {
            cleanup_widget(&pool, wid).await;
        }
    }

    #[tokio::test]
    async fn get_user_widgets_filters_by_user() {
        let pool = test_pool().await;
        let storage = WidgetStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let user_a = format!("@usera_{suffix}:test.com");
        let user_b = format!("@userb_{suffix}:test.com");
        let w1 = format!("uw1_{suffix}");
        let w2 = format!("uw2_{suffix}");
        let w3 = format!("uw3_{suffix}");

        ensure_test_user(&pool, &user_a).await;
        ensure_test_user(&pool, &user_b).await;
        for wid in &[&w1, &w2, &w3] {
            cleanup_widget(&pool, wid).await;
        }

        let base_params = |w_id: &str, u_id: &str| CreateWidgetParams {
            widget_id: w_id.to_string(),
            room_id: None,
            user_id: u_id.to_string(),
            widget_type: "m.custom".to_string(),
            url: "https://example.com".to_string(),
            name: "W".to_string(),
            data: serde_json::json!({}),
        };

        storage.create_widget(base_params(&w1, &user_a)).await.unwrap();
        storage.create_widget(base_params(&w2, &user_a)).await.unwrap();
        storage.create_widget(base_params(&w3, &user_b)).await.unwrap();

        let user_a_widgets = storage.get_user_widgets(&user_a).await.expect("get_user_widgets should succeed");
        assert_eq!(user_a_widgets.len(), 2);
        for w in &user_a_widgets {
            assert_eq!(w.user_id, user_a);
        }

        for wid in &[&w1, &w2, &w3] {
            cleanup_widget(&pool, wid).await;
        }
    }

    #[tokio::test]
    async fn update_widget_changes_fields() {
        let pool = test_pool().await;
        let storage = WidgetStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let widget_id = format!("upd_{suffix}");
        let user_id = format!("@updater_{suffix}:test.com");

        ensure_test_user(&pool, &user_id).await;
        cleanup_widget(&pool, &widget_id).await;

        storage
            .create_widget(CreateWidgetParams {
                widget_id: widget_id.clone(),
                room_id: None,
                user_id: user_id.clone(),
                widget_type: "m.custom".to_string(),
                url: "https://old.example.com".to_string(),
                name: "Old Name".to_string(),
                data: serde_json::json!({"old": true}),
            })
            .await
            .unwrap();

        let new_data = serde_json::json!({"new": true, "count": 42});
        let updated = storage
            .update_widget(&widget_id, Some("https://new.example.com"), Some("New Name"), Some(&new_data))
            .await
            .expect("update should succeed")
            .expect("widget should exist");

        assert_eq!(updated.url, "https://new.example.com");
        assert_eq!(updated.name, "New Name");
        assert_eq!(updated.data, new_data);
        assert_eq!(updated.widget_type, "m.custom"); // unchanged
        assert!(updated.updated_ts.is_some());

        // Verify partial update: only URL
        let updated2 = storage
            .update_widget(&widget_id, Some("https://partial.example.com"), None, None)
            .await
            .expect("partial update should succeed")
            .expect("widget should exist");
        assert_eq!(updated2.url, "https://partial.example.com");
        assert_eq!(updated2.name, "New Name"); // unchanged by partial

        cleanup_widget(&pool, &widget_id).await;
    }

    #[tokio::test]
    async fn update_widget_not_found_returns_none() {
        let pool = test_pool().await;
        let storage = WidgetStorage::new(pool.clone());

        let result = storage
            .update_widget("nonexistent_widget", Some("https://x.com"), None, None)
            .await
            .expect("query should succeed");
        assert!(result.is_none(), "update on non-existent widget should return None");
    }

    #[tokio::test]
    async fn delete_widget_soft_deactivates() {
        let pool = test_pool().await;
        let storage = WidgetStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let widget_id = format!("del_{suffix}");
        let user_id = format!("@deleter_{suffix}:test.com");

        ensure_test_user(&pool, &user_id).await;
        cleanup_widget(&pool, &widget_id).await;

        storage
            .create_widget(CreateWidgetParams {
                widget_id: widget_id.clone(),
                room_id: None,
                user_id: user_id.clone(),
                widget_type: "m.custom".to_string(),
                url: "https://example.com".to_string(),
                name: "To Delete".to_string(),
                data: serde_json::json!({}),
            })
            .await
            .unwrap();

        let deleted = storage.delete_widget(&widget_id).await.expect("delete should succeed");
        assert!(deleted, "delete should return true for existing widget");

        let found = storage.get_widget(&widget_id).await.expect("get should succeed");
        assert!(found.is_none(), "soft-deleted widget should not appear in get");

        // Deleting again should return false
        let deleted_again = storage.delete_widget(&widget_id).await.unwrap();
        assert!(!deleted_again, "second delete should return false");

        cleanup_widget(&pool, &widget_id).await;
    }

    #[tokio::test]
    async fn delete_widget_not_found_returns_false() {
        let pool = test_pool().await;
        let storage = WidgetStorage::new(pool.clone());

        let result = storage.delete_widget("nonexistent_widget").await.expect("query should succeed");
        assert!(!result, "delete non-existent should return false");
    }

    // ——— Permission CRUD —————————————————————————————————————————————

    #[tokio::test]
    async fn set_and_get_widget_permission() {
        let pool = test_pool().await;
        let storage = WidgetStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let widget_id = format!("perm_{suffix}");
        let user_id = format!("@permuser_{suffix}:test.com");
        let other_user = format!("@other_{suffix}:test.com");

        ensure_test_user(&pool, &user_id).await;
        ensure_test_user(&pool, &other_user).await;
        cleanup_widget(&pool, &widget_id).await;

        storage
            .create_widget(CreateWidgetParams {
                widget_id: widget_id.clone(),
                room_id: None,
                user_id: user_id.clone(),
                widget_type: "m.custom".to_string(),
                url: "https://example.com".to_string(),
                name: "Perm Widget".to_string(),
                data: serde_json::json!({}),
            })
            .await
            .unwrap();

        // Set permission
        let perm = storage
            .set_widget_permission(&widget_id, &user_id, serde_json::json!(["read", "send"]))
            .await
            .expect("set_widget_permission should succeed");
        assert_eq!(perm.widget_id, widget_id);
        assert_eq!(perm.user_id, user_id);
        assert_eq!(perm.permissions, serde_json::json!(["read", "send"]));

        // Upsert — update same user's permissions
        let perm2 = storage
            .set_widget_permission(&widget_id, &user_id, serde_json::json!(["admin"]))
            .await
            .expect("upsert should succeed");
        assert_eq!(perm2.permissions, serde_json::json!(["admin"]));
        assert_eq!(perm2.id, perm.id); // same row, updated in place

        // Get all permissions for widget
        let all_perms =
            storage.get_widget_permissions(&widget_id).await.expect("get_widget_permissions should succeed");
        assert_eq!(all_perms.len(), 1);

        // Get specific user permission
        let user_perm = storage
            .get_user_widget_permission(&widget_id, &user_id)
            .await
            .expect("get_user_widget_permission should succeed")
            .expect("user permission should exist");
        assert_eq!(user_perm.permissions, serde_json::json!(["admin"]));

        // Get non-existent user permission
        let none_perm =
            storage.get_user_widget_permission(&widget_id, &other_user).await.expect("query should succeed");
        assert!(none_perm.is_none(), "no permission for other user");

        cleanup_widget(&pool, &widget_id).await;
    }

    #[tokio::test]
    async fn delete_widget_permission_hard_deletes() {
        let pool = test_pool().await;
        let storage = WidgetStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let widget_id = format!("delperm_{suffix}");
        let user_id = format!("@delpermuser_{suffix}:test.com");

        ensure_test_user(&pool, &user_id).await;
        cleanup_widget(&pool, &widget_id).await;

        storage
            .create_widget(CreateWidgetParams {
                widget_id: widget_id.clone(),
                room_id: None,
                user_id: user_id.clone(),
                widget_type: "m.custom".to_string(),
                url: "https://example.com".to_string(),
                name: "DP".to_string(),
                data: serde_json::json!({}),
            })
            .await
            .unwrap();

        storage.set_widget_permission(&widget_id, &user_id, serde_json::json!(["read"])).await.unwrap();

        let deleted = storage.delete_widget_permission(&widget_id, &user_id).await.expect("delete should succeed");
        assert!(deleted, "first delete should return true");

        let deleted_again =
            storage.delete_widget_permission(&widget_id, &user_id).await.expect("second delete should succeed");
        assert!(!deleted_again, "second delete should return false");

        let perm = storage.get_user_widget_permission(&widget_id, &user_id).await.unwrap();
        assert!(perm.is_none());

        cleanup_widget(&pool, &widget_id).await;
    }

    // ——— Session CRUD ————————————————————————————————————————————————

    #[tokio::test]
    async fn create_and_get_session() {
        let pool = test_pool().await;
        let storage = WidgetStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let widget_id = format!("sessw_{suffix}");
        let session_id = format!("session_{suffix}");
        let user_id = format!("@sessuser_{suffix}:test.com");

        ensure_test_user(&pool, &user_id).await;
        cleanup_widget(&pool, &widget_id).await;
        cleanup_session(&pool, &session_id).await;

        storage
            .create_widget(CreateWidgetParams {
                widget_id: widget_id.clone(),
                room_id: None,
                user_id: user_id.clone(),
                widget_type: "m.custom".to_string(),
                url: "https://example.com".to_string(),
                name: "Session Widget".to_string(),
                data: serde_json::json!({}),
            })
            .await
            .unwrap();

        let expires_in = Some(3_600_000i64); // 1 hour
        let session = storage
            .create_session(&session_id, &widget_id, &user_id, Some("DEVICE1"), expires_in)
            .await
            .expect("create_session should succeed");

        assert!(session.id > 0);
        assert_eq!(session.session_id, session_id);
        assert_eq!(session.widget_id, widget_id);
        assert_eq!(session.user_id, user_id);
        assert_eq!(session.device_id.as_deref(), Some("DEVICE1"));
        assert!(session.is_active);
        assert!(session.created_ts > 0);
        assert!(session.expires_at.is_some());

        let found = storage
            .get_session(&session_id)
            .await
            .expect("get_session should succeed")
            .expect("session should be found");
        assert_eq!(found.id, session.id);

        cleanup_widget(&pool, &widget_id).await;
    }

    #[tokio::test]
    async fn update_session_activity_and_terminate() {
        let pool = test_pool().await;
        let storage = WidgetStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let widget_id = format!("actsessw_{suffix}");
        let session_id = format!("actsession_{suffix}");
        let user_id = format!("@actuser_{suffix}:test.com");

        ensure_test_user(&pool, &user_id).await;
        cleanup_widget(&pool, &widget_id).await;
        cleanup_session(&pool, &session_id).await;

        storage
            .create_widget(CreateWidgetParams {
                widget_id: widget_id.clone(),
                room_id: None,
                user_id: user_id.clone(),
                widget_type: "m.custom".to_string(),
                url: "https://example.com".to_string(),
                name: "Act Widget".to_string(),
                data: serde_json::json!({}),
            })
            .await
            .unwrap();

        storage.create_session(&session_id, &widget_id, &user_id, None, None).await.unwrap();

        let original = storage.get_session(&session_id).await.unwrap().unwrap();
        let original_activity = original.last_active_ts;

        // Update activity
        let updated =
            storage.update_session_activity(&session_id).await.expect("update_session_activity should succeed");
        assert!(updated);

        let after_update = storage.get_session(&session_id).await.unwrap().unwrap();
        assert!(after_update.last_active_ts >= original_activity, "last_active_ts should be updated");

        // Terminate session
        let terminated = storage.terminate_session(&session_id).await.expect("terminate_session should succeed");
        assert!(terminated);

        let after_term = storage.get_session(&session_id).await.unwrap();
        assert!(after_term.is_none(), "terminated session should not appear in get");

        // Terminating again returns false
        let term_again = storage.terminate_session(&session_id).await.unwrap();
        assert!(!term_again);

        cleanup_widget(&pool, &widget_id).await;
    }

    #[tokio::test]
    async fn get_widget_sessions_lists_active() {
        let pool = test_pool().await;
        let storage = WidgetStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let widget_id = format!("listw_{suffix}");
        let user_id = format!("@listuser_{suffix}:test.com");
        let s1 = format!("lists1_{suffix}");
        let s2 = format!("lists2_{suffix}");

        ensure_test_user(&pool, &user_id).await;
        cleanup_widget(&pool, &widget_id).await;
        cleanup_session(&pool, &s1).await;
        cleanup_session(&pool, &s2).await;

        storage
            .create_widget(CreateWidgetParams {
                widget_id: widget_id.clone(),
                room_id: None,
                user_id: user_id.clone(),
                widget_type: "m.custom".to_string(),
                url: "https://example.com".to_string(),
                name: "List Widget".to_string(),
                data: serde_json::json!({}),
            })
            .await
            .unwrap();

        // s1: active, future expiry; s2: active, no expiry
        storage.create_session(&s1, &widget_id, &user_id, None, Some(3_600_000)).await.unwrap();
        storage.create_session(&s2, &widget_id, &user_id, None, None).await.unwrap();

        let sessions = storage.get_widget_sessions(&widget_id).await.expect("get_widget_sessions should succeed");
        assert_eq!(sessions.len(), 2);
        assert!(sessions.iter().all(|s| s.is_active));
        assert!(sessions.iter().all(|s| s.widget_id == widget_id));

        cleanup_widget(&pool, &widget_id).await;
    }

    #[tokio::test]
    async fn cleanup_expired_sessions_deactivates_old() {
        let pool = test_pool().await;
        let storage = WidgetStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let widget_id = format!("expw_{suffix}");
        let user_id = format!("@expuser_{suffix}:test.com");
        let expired_sid = format!("expired_{suffix}");
        let valid_sid = format!("valid_{suffix}");

        ensure_test_user(&pool, &user_id).await;
        cleanup_widget(&pool, &widget_id).await;
        cleanup_session(&pool, &expired_sid).await;
        cleanup_session(&pool, &valid_sid).await;

        storage
            .create_widget(CreateWidgetParams {
                widget_id: widget_id.clone(),
                room_id: None,
                user_id: user_id.clone(),
                widget_type: "m.custom".to_string(),
                url: "https://example.com".to_string(),
                name: "Expiry Widget".to_string(),
                data: serde_json::json!({}),
            })
            .await
            .unwrap();

        // Create sessions then manually set one to expired in the past
        storage.create_session(&expired_sid, &widget_id, &user_id, None, Some(1)).await.unwrap();
        storage.create_session(&valid_sid, &widget_id, &user_id, None, Some(86_400_000)).await.unwrap();

        // Manually set the expired session's expires_at to the past
        let past = chrono::Utc::now().timestamp_millis() - 10_000;
        sqlx::query("UPDATE widget_sessions SET expires_at = $1 WHERE session_id = $2")
            .bind(past)
            .bind(&expired_sid)
            .execute(&*pool)
            .await
            .unwrap();

        let count = storage.cleanup_expired_sessions().await.expect("cleanup_expired_sessions should succeed");
        assert!(count >= 1, "should have cleaned up at least one expired session");

        let expired = storage.get_session(&expired_sid).await.unwrap();
        assert!(expired.is_none(), "expired session should be deactivated");

        let valid = storage.get_session(&valid_sid).await.unwrap();
        assert!(valid.is_some(), "valid session should still be active");

        cleanup_widget(&pool, &widget_id).await;
    }

    #[tokio::test]
    async fn widget_round_trip_full_lifecycle() {
        let pool = test_pool().await;
        let storage = WidgetStorage::new(pool.clone());
        let suffix = uuid::Uuid::new_v4();
        let widget_id = format!("rt_{suffix}");
        let user_id = format!("@rttrip_{suffix}:test.com");
        let room_id = format!("!rtroom_{suffix}:test.com");

        ensure_test_user(&pool, &user_id).await;
        ensure_test_room(&pool, &room_id).await;
        cleanup_widget(&pool, &widget_id).await;

        // Create
        let created = storage
            .create_widget(CreateWidgetParams {
                widget_id: widget_id.clone(),
                room_id: Some(room_id.clone()),
                user_id: user_id.clone(),
                widget_type: "m.custom".to_string(),
                url: "https://example.com/start".to_string(),
                name: "Round Trip".to_string(),
                data: serde_json::json!({"step": 1}),
            })
            .await
            .unwrap();
        assert!(created.is_active);

        // Get
        let fetched = storage.get_widget(&widget_id).await.unwrap().unwrap();
        assert_eq!(fetched.name, "Round Trip");

        // Update
        let updated = storage
            .update_widget(
                &widget_id,
                Some("https://example.com/end"),
                Some("Round Trip Done"),
                Some(&serde_json::json!({"step": 2})),
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated.name, "Round Trip Done");
        assert_eq!(updated.url, "https://example.com/end");

        // Verify update persisted
        let refetched = storage.get_widget(&widget_id).await.unwrap().unwrap();
        assert_eq!(refetched.name, "Round Trip Done");

        // Delete
        let deleted = storage.delete_widget(&widget_id).await.unwrap();
        assert!(deleted);

        // Verify deleted
        let none = storage.get_widget(&widget_id).await.unwrap();
        assert!(none.is_none());

        cleanup_widget(&pool, &widget_id).await;
    }
}
