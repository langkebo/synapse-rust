use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;
use synapse_common::ApiError;

pub struct RoomAccountDataStorage;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RoomAccountDataRecord {
    pub room_id: String,
    pub data_type: String,
    pub content: Value,
}

impl RoomAccountDataStorage {
    pub async fn get_room_account_data_content(
        pool: &sqlx::PgPool,
        user_id: &str,
        room_id: &str,
        data_type: &str,
    ) -> Result<Option<Value>, ApiError> {
        let row = Self::get_room_account_data(pool, user_id, room_id, data_type)
            .await
            .map_err(|e| ApiError::internal_with_log("Database error", &e))?;
        Ok(row.map(|row| row.get::<Value, _>("data")))
    }

    pub async fn get_room_account_data_with_ts(
        pool: &sqlx::PgPool,
        user_id: &str,
        room_id: &str,
        data_type: &str,
    ) -> Result<Option<(Value, Option<i64>)>, ApiError> {
        let row = sqlx::query(
            "SELECT data, updated_ts FROM room_account_data WHERE user_id = $1 AND room_id = $2 AND data_type = $3",
        )
        .bind(user_id)
        .bind(room_id)
        .bind(data_type)
        .fetch_optional(pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))?;

        Ok(row.map(|row| {
            let data = row.get::<Value, _>("data");
            let updated_ts = row.try_get::<Option<i64>, _>("updated_ts").ok().flatten();
            (data, updated_ts)
        }))
    }

    pub async fn get_room_account_data(
        pool: &sqlx::PgPool,
        user_id: &str,
        room_id: &str,
        data_type: &str,
    ) -> Result<Option<sqlx::postgres::PgRow>, sqlx::Error> {
        sqlx::query("SELECT data FROM room_account_data WHERE user_id = $1 AND room_id = $2 AND data_type = $3")
            .bind(user_id)
            .bind(room_id)
            .bind(data_type)
            .fetch_optional(pool)
            .await
    }

    pub async fn list_room_account_data(
        pool: &sqlx::PgPool,
        user_id: &str,
        room_id: &str,
    ) -> Result<Vec<RoomAccountDataRecord>, ApiError> {
        sqlx::query_as::<_, RoomAccountDataRecord>(
            "SELECT room_id, data_type, data AS content \
             FROM room_account_data \
             WHERE user_id = $1 AND room_id = $2 \
             ORDER BY data_type ASC",
        )
        .bind(user_id)
        .bind(room_id)
        .fetch_all(pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))
    }

    pub async fn list_room_account_data_batch(
        pool: &sqlx::PgPool,
        user_id: &str,
        room_ids: &[String],
    ) -> Result<Vec<RoomAccountDataRecord>, ApiError> {
        if room_ids.is_empty() {
            return Ok(Vec::new());
        }

        sqlx::query_as::<_, RoomAccountDataRecord>(
            "SELECT room_id, data_type, data AS content \
             FROM room_account_data \
             WHERE user_id = $1 AND room_id = ANY($2) \
             ORDER BY room_id ASC, data_type ASC",
        )
        .bind(user_id)
        .bind(room_ids)
        .fetch_all(pool)
        .await
        .map_err(|e| ApiError::internal_with_log("Database error", &e))
    }

    pub async fn get_room_vault_data(
        pool: &sqlx::PgPool,
        user_id: &str,
        room_id: &str,
    ) -> Result<Option<sqlx::postgres::PgRow>, sqlx::Error> {
        sqlx::query(
            "SELECT data, updated_ts FROM room_account_data WHERE user_id = $1 AND room_id = $2 AND data_type = $3",
        )
        .bind(user_id)
        .bind(room_id)
        .bind("m.room.vault_data")
        .fetch_optional(pool)
        .await
    }

    pub async fn upsert_room_account_data(
        pool: &sqlx::PgPool,
        user_id: &str,
        room_id: &str,
        data_type: &str,
        data: &Value,
        now: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO room_account_data (user_id, room_id, data_type, data, created_ts, updated_ts) \
             VALUES ($1, $2, $3, $4, $5, $5) \
             ON CONFLICT (user_id, room_id, data_type) \
             DO UPDATE SET data = EXCLUDED.data, updated_ts = EXCLUDED.updated_ts",
        )
        .bind(user_id)
        .bind(room_id)
        .bind(data_type)
        .bind(data)
        .bind(now)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn delete_room_account_data(
        pool: &sqlx::PgPool,
        user_id: &str,
        room_id: &str,
        data_type: &str,
    ) -> Result<bool, ApiError> {
        let result =
            sqlx::query("DELETE FROM room_account_data WHERE user_id = $1 AND room_id = $2 AND data_type = $3")
                .bind(user_id)
                .bind(room_id)
                .bind(data_type)
                .execute(pool)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to delete room account data", &e))?;
        Ok(result.rows_affected() > 0)
    }
}
