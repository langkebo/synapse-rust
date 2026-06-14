use serde_json::Value;
use sqlx::Row;
use synapse_common::ApiError;

pub struct RoomAccountDataStorage;

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
