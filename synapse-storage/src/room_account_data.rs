use serde_json::Value;

pub struct RoomAccountDataStorage;

impl RoomAccountDataStorage {
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
}