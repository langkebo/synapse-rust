use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AiConnection {
    pub id: String,
    pub user_id: String,
    pub provider: String,
    pub config: Option<serde_json::Value>,
    pub is_active: bool,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
}

pub struct AiConnectionStorage {
    db: Arc<PgPool>,
}

impl AiConnectionStorage {
    pub fn new(db: Arc<PgPool>) -> Self {
        Self { db }
    }

    pub async fn create_connection(&self, conn: &AiConnection) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO ai_connections (id, user_id, provider, config, is_active, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (id) DO UPDATE SET
                config = EXCLUDED.config,
                is_active = EXCLUDED.is_active,
                updated_ts = EXCLUDED.updated_ts
            "#,
        )
        .bind(&conn.id)
        .bind(&conn.user_id)
        .bind(&conn.provider)
        .bind(&conn.config)
        .bind(conn.is_active)
        .bind(conn.created_ts)
        .bind(conn.updated_ts)
        .execute(&*self.db)
        .await?;

        Ok(())
    }

    pub async fn get_connection(&self, id: &str) -> Result<Option<AiConnection>, sqlx::Error> {
        sqlx::query_as::<_, AiConnection>(
            r#"
            SELECT id, user_id, provider, config, is_active, created_ts, updated_ts
            FROM ai_connections
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&*self.db)
        .await
    }

    pub async fn get_user_connections(
        &self,
        user_id: &str,
    ) -> Result<Vec<AiConnection>, sqlx::Error> {
        sqlx::query_as::<_, AiConnection>(
            r#"
            SELECT id, user_id, provider, config, is_active, created_ts, updated_ts
            FROM ai_connections
            WHERE user_id = $1
            ORDER BY created_ts DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&*self.db)
        .await
    }

    pub async fn get_user_provider_connection(
        &self,
        user_id: &str,
        provider: &str,
    ) -> Result<Option<AiConnection>, sqlx::Error> {
        sqlx::query_as::<_, AiConnection>(
            r#"
            SELECT id, user_id, provider, config, is_active, created_ts, updated_ts
            FROM ai_connections
            WHERE user_id = $1 AND provider = $2 AND is_active = true
            ORDER BY created_ts DESC
            LIMIT 1
            "#,
        )
        .bind(user_id)
        .bind(provider)
        .fetch_optional(&*self.db)
        .await
    }

    pub async fn update_connection_status(
        &self,
        id: &str,
        is_active: bool,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            r#"
            UPDATE ai_connections
            SET is_active = $1, updated_ts = $2
            WHERE id = $3
            "#,
        )
        .bind(is_active)
        .bind(now)
        .bind(id)
        .execute(&*self.db)
        .await?;

        Ok(())
    }

    pub async fn delete_connection(&self, id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            DELETE FROM ai_connections
            WHERE id = $1
            "#,
        )
        .bind(id)
        .execute(&*self.db)
        .await?;

        Ok(())
    }
}
