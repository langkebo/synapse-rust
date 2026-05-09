use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RoomTag {
    pub id: i32,
    pub user_id: String,
    pub room_id: String,
    pub tag: String,
    #[sqlx(rename = "order_value")]
    pub order: Option<f64>,
    pub created_ts: i64,
}

pub struct RoomTagStorage;

impl RoomTagStorage {
    pub async fn get_tags(
        pool: &sqlx::PgPool,
        user_id: &str,
        room_id: &str,
    ) -> Result<Vec<RoomTag>, sqlx::Error> {
        sqlx::query_as::<_, RoomTag>(
            "SELECT id, user_id, room_id, tag, order_value, created_ts FROM room_tags WHERE user_id = $1 AND room_id = $2 ORDER BY tag"
        )
        .bind(user_id)
        .bind(room_id)
        .fetch_all(pool)
        .await
    }

    pub async fn add_tag(
        pool: &sqlx::PgPool,
        user_id: &str,
        room_id: &str,
        tag: &str,
        order: Option<f64>,
    ) -> Result<(), sqlx::Error> {
        let created_ts = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "INSERT INTO room_tags (user_id, room_id, tag, order_value, created_ts) VALUES ($1, $2, $3, $4, $5) ON CONFLICT (user_id, room_id, tag) DO UPDATE SET order_value = EXCLUDED.order_value"
        )
        .bind(user_id)
        .bind(room_id)
        .bind(tag)
        .bind(order)
        .bind(created_ts)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn remove_tag(
        pool: &sqlx::PgPool,
        user_id: &str,
        room_id: &str,
        tag: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM room_tags WHERE user_id = $1 AND room_id = $2 AND tag = $3")
            .bind(user_id)
            .bind(room_id)
            .bind(tag)
            .execute(pool)
            .await?;
        Ok(())
    }
}
