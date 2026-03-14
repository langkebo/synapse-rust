// Widget Storage - MSC4261
// Implements embedded application support for Matrix rooms
// Following project field naming standards

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
            SELECT * FROM widgets WHERE widget_id = $1 AND is_active = TRUE
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
            SELECT * FROM widgets WHERE room_id = $1 AND is_active = TRUE ORDER BY created_ts DESC
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
            SELECT * FROM widgets WHERE user_id = $1 AND is_active = TRUE ORDER BY created_ts DESC
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
            UPDATE widgets SET is_active = FALSE, updated_ts = $2 WHERE widget_id = $1
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

    pub async fn get_widget_permissions(
        &self,
        widget_id: &str,
    ) -> Result<Vec<WidgetPermission>, sqlx::Error> {
        let rows = sqlx::query_as::<_, WidgetPermission>(
            r#"
            SELECT * FROM widget_permissions WHERE widget_id = $1
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
            SELECT * FROM widget_permissions WHERE widget_id = $1 AND user_id = $2
            "#,
        )
        .bind(widget_id)
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn delete_widget_permission(
        &self,
        widget_id: &str,
        user_id: &str,
    ) -> Result<bool, sqlx::Error> {
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
            SELECT * FROM widget_sessions WHERE session_id = $1 AND is_active = TRUE
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
            UPDATE widget_sessions SET is_active = FALSE WHERE session_id = $1
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
            SELECT * FROM widget_sessions 
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
