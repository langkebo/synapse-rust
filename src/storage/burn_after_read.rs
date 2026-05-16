use chrono::Utc;
use sqlx::{FromRow, PgPool};
use std::sync::Arc;

#[derive(Debug, Clone, FromRow)]
pub struct BurnSettingsRow {
    pub user_id: String,
    pub room_id: String,
    pub is_enabled: bool,
    pub burn_after_ms: i64,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
}

#[derive(Debug, Clone, FromRow)]
pub struct BurnPendingRow {
    pub id: i64,
    pub user_id: String,
    pub room_id: String,
    pub event_id: String,
    pub created_ts: i64,
    pub delete_at: i64,
    pub is_processed: bool,
}

#[derive(Debug, Clone, FromRow)]
pub struct BurnLogRow {
    pub id: i64,
    pub user_id: String,
    pub room_id: String,
    pub event_id: String,
    pub burned_ts: i64,
}

#[derive(Debug, Clone, FromRow)]
pub struct BurnUserDefaultsRow {
    pub user_id: String,
    pub default_burn_ms: i64,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
}

#[derive(Debug, Clone, FromRow)]
pub struct BurnStatsRow {
    pub total_burned: i64,
    pub total_pending: i64,
    pub rooms_enabled: i64,
}

#[derive(Clone)]
pub struct BurnAfterReadStorage {
    pool: Arc<PgPool>,
}

impl BurnAfterReadStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn get_settings(
        &self,
        user_id: &str,
        room_id: &str,
    ) -> Result<Option<BurnSettingsRow>, sqlx::Error> {
        sqlx::query_as::<_, BurnSettingsRow>(
            r#"
            SELECT user_id, room_id, is_enabled, burn_after_ms, created_ts, updated_ts
            FROM burn_after_read_settings
            WHERE user_id = $1 AND room_id = $2
            "#,
        )
        .bind(user_id)
        .bind(room_id)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn set_settings(
        &self,
        user_id: &str,
        room_id: &str,
        is_enabled: bool,
        burn_after_ms: i64,
    ) -> Result<BurnSettingsRow, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let row = sqlx::query_as::<_, BurnSettingsRow>(
            r#"
            INSERT INTO burn_after_read_settings (user_id, room_id, is_enabled, burn_after_ms, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $5)
            ON CONFLICT (user_id, room_id) DO UPDATE SET
                is_enabled = EXCLUDED.is_enabled,
                burn_after_ms = EXCLUDED.burn_after_ms,
                updated_ts = EXCLUDED.updated_ts
            RETURNING user_id, room_id, is_enabled, burn_after_ms, created_ts, updated_ts
            "#,
        )
        .bind(user_id)
        .bind(room_id)
        .bind(is_enabled)
        .bind(burn_after_ms)
        .bind(now)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn schedule_burn(
        &self,
        user_id: &str,
        room_id: &str,
        event_id: &str,
        delete_at: i64,
    ) -> Result<BurnPendingRow, sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        let row = sqlx::query_as::<_, BurnPendingRow>(
            r#"
            INSERT INTO burn_after_read_pending (user_id, room_id, event_id, created_ts, delete_at)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (user_id, room_id, event_id) WHERE is_processed = FALSE DO UPDATE SET
                delete_at = EXCLUDED.delete_at,
                created_ts = EXCLUDED.created_ts
            RETURNING id, user_id, room_id, event_id, created_ts, delete_at, is_processed
            "#,
        )
        .bind(user_id)
        .bind(room_id)
        .bind(event_id)
        .bind(now)
        .bind(delete_at)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn cancel_burn(
        &self,
        user_id: &str,
        room_id: &str,
        event_id: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE burn_after_read_pending
            SET is_processed = TRUE
            WHERE user_id = $1 AND room_id = $2 AND event_id = $3 AND is_processed = FALSE
            "#,
        )
        .bind(user_id)
        .bind(room_id)
        .bind(event_id)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_pending_burns(
        &self,
        user_id: &str,
        room_id: &str,
    ) -> Result<Vec<BurnPendingRow>, sqlx::Error> {
        let rows = sqlx::query_as::<_, BurnPendingRow>(
            r#"
            SELECT id, user_id, room_id, event_id, created_ts, delete_at, is_processed
            FROM burn_after_read_pending
            WHERE user_id = $1 AND room_id = $2 AND is_processed = FALSE
            ORDER BY delete_at ASC
            "#,
        )
        .bind(user_id)
        .bind(room_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn get_expired_burns(&self, now_ms: i64) -> Result<Vec<BurnPendingRow>, sqlx::Error> {
        let rows = sqlx::query_as::<_, BurnPendingRow>(
            r#"
            SELECT id, user_id, room_id, event_id, created_ts, delete_at, is_processed
            FROM burn_after_read_pending
            WHERE delete_at <= $1 AND is_processed = FALSE
            ORDER BY delete_at ASC
            "#,
        )
        .bind(now_ms)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn mark_burn_processed(&self, id: i64) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE burn_after_read_pending SET is_processed = TRUE WHERE id = $1",
        )
        .bind(id)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn log_burned_event(
        &self,
        user_id: &str,
        room_id: &str,
        event_id: &str,
        burned_ts: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO burn_after_read_log (user_id, room_id, event_id, burned_ts)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(user_id)
        .bind(room_id)
        .bind(event_id)
        .bind(burned_ts)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_user_stats(&self, user_id: &str) -> Result<BurnStatsRow, sqlx::Error> {
        let row = sqlx::query_as::<_, BurnStatsRow>(
            r#"
            SELECT
                COALESCE((SELECT COUNT(*) FROM burn_after_read_log WHERE user_id = $1), 0) AS total_burned,
                COALESCE((SELECT COUNT(*) FROM burn_after_read_pending WHERE user_id = $1 AND is_processed = FALSE), 0) AS total_pending,
                COALESCE((SELECT COUNT(*) FROM burn_after_read_settings WHERE user_id = $1 AND is_enabled = TRUE), 0) AS rooms_enabled
            "#,
        )
        .bind(user_id)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_user_default(
        &self,
        user_id: &str,
    ) -> Result<Option<BurnUserDefaultsRow>, sqlx::Error> {
        let row = sqlx::query_as::<_, BurnUserDefaultsRow>(
            r#"
            SELECT user_id, default_burn_ms, created_ts, updated_ts
            FROM burn_after_read_user_defaults
            WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row)
    }

    pub async fn set_user_default(
        &self,
        user_id: &str,
        default_burn_ms: i64,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            r#"
            INSERT INTO burn_after_read_user_defaults (user_id, default_burn_ms, created_ts, updated_ts)
            VALUES ($1, $2, $3, $3)
            ON CONFLICT (user_id) DO UPDATE SET
                default_burn_ms = EXCLUDED.default_burn_ms,
                updated_ts = EXCLUDED.updated_ts
            "#,
        )
        .bind(user_id)
        .bind(default_burn_ms)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_burn_settings_row_fields() {
        let row = BurnSettingsRow {
            user_id: "@alice:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            is_enabled: true,
            burn_after_ms: 60000,
            created_ts: 1234567890,
            updated_ts: None,
        };
        assert_eq!(row.user_id, "@alice:example.com");
        assert!(row.is_enabled);
        assert_eq!(row.burn_after_ms, 60000);
    }

    #[test]
    fn test_burn_pending_row_fields() {
        let row = BurnPendingRow {
            id: 1,
            user_id: "@alice:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            event_id: "$event1".to_string(),
            created_ts: 1234567890,
            delete_at: 1234567950,
            is_processed: false,
        };
        assert_eq!(row.id, 1);
        assert_eq!(row.event_id, "$event1");
        assert!(!row.is_processed);
    }

    #[test]
    fn test_burn_log_row_fields() {
        let row = BurnLogRow {
            id: 1,
            user_id: "@alice:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            event_id: "$event1".to_string(),
            burned_ts: 1234567890,
        };
        assert_eq!(row.id, 1);
        assert_eq!(row.event_id, "$event1");
    }

    #[test]
    fn test_burn_user_defaults_row_fields() {
        let row = BurnUserDefaultsRow {
            user_id: "@alice:example.com".to_string(),
            default_burn_ms: 30000,
            created_ts: 1234567890,
            updated_ts: None,
        };
        assert_eq!(row.user_id, "@alice:example.com");
        assert_eq!(row.default_burn_ms, 30000);
    }

    #[test]
    fn test_burn_stats_row_fields() {
        let row = BurnStatsRow {
            total_burned: 5,
            total_pending: 2,
            rooms_enabled: 3,
        };
        assert_eq!(row.total_burned, 5);
        assert_eq!(row.total_pending, 2);
        assert_eq!(row.rooms_enabled, 3);
    }
}
