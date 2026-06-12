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
    pub async fn get_all_tags(pool: &sqlx::PgPool, user_id: &str) -> Result<Vec<RoomTag>, sqlx::Error> {
        sqlx::query_as!(
            RoomTag,
            r#"SELECT id, user_id, room_id, tag, order_value AS "order", created_ts FROM room_tags WHERE user_id = $1 ORDER BY room_id, tag"#,
            user_id,
        )
        .fetch_all(pool)
        .await
    }

    pub async fn get_tags(pool: &sqlx::PgPool, user_id: &str, room_id: &str) -> Result<Vec<RoomTag>, sqlx::Error> {
        sqlx::query_as!(
            RoomTag,
            r#"SELECT id, user_id, room_id, tag, order_value AS "order", created_ts FROM room_tags WHERE user_id = $1 AND room_id = $2 ORDER BY tag"#,
            user_id,
            room_id,
        )
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
        sqlx::query!(
            r#"INSERT INTO room_tags (user_id, room_id, tag, order_value, created_ts) VALUES ($1, $2, $3, $4, $5) ON CONFLICT (user_id, room_id, tag) DO UPDATE SET order_value = EXCLUDED.order_value"#,
            user_id,
            room_id,
            tag,
            order,
            created_ts,
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn remove_tag(pool: &sqlx::PgPool, user_id: &str, room_id: &str, tag: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"DELETE FROM room_tags WHERE user_id = $1 AND room_id = $2 AND tag = $3"#,
            user_id,
            room_id,
            tag,
        )
        .execute(pool)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_room_tag_fields() {
        let tag = RoomTag {
            id: 1,
            user_id: "@alice:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            tag: "m.favourite".to_string(),
            order: Some(0.5),
            created_ts: 1700000000000,
        };
        assert_eq!(tag.id, 1);
        assert_eq!(tag.user_id, "@alice:example.com");
        assert_eq!(tag.room_id, "!room:example.com");
        assert_eq!(tag.tag, "m.favourite");
        assert_eq!(tag.order, Some(0.5));
        assert_eq!(tag.created_ts, 1700000000000);
    }

    #[test]
    fn test_room_tag_no_order() {
        let tag = RoomTag {
            id: 2,
            user_id: "@bob:example.com".to_string(),
            room_id: "!room2:example.com".to_string(),
            tag: "m.lowpriority".to_string(),
            order: None,
            created_ts: 1700000000000,
        };
        assert_eq!(tag.tag, "m.lowpriority");
        assert!(tag.order.is_none());
    }

    #[test]
    fn test_room_tag_negative_order() {
        let tag = RoomTag {
            id: 3,
            user_id: "@carol:example.com".to_string(),
            room_id: "!room3:example.com".to_string(),
            tag: "u.custom".to_string(),
            order: Some(-1.0),
            created_ts: 1700000000000,
        };
        assert_eq!(tag.order, Some(-1.0));
    }

    #[test]
    fn test_room_tag_system_tags() {
        let system_tags = vec!["m.favourite", "m.lowpriority", "m.server_notice"];
        for tag_name in system_tags {
            let tag = RoomTag {
                id: 1,
                user_id: "@alice:example.com".to_string(),
                room_id: "!room:example.com".to_string(),
                tag: tag_name.to_string(),
                order: None,
                created_ts: 1700000000000,
            };
            assert!(tag.tag.starts_with("m."));
        }
    }

    #[test]
    fn test_room_tag_user_tags() {
        let user_tag = RoomTag {
            id: 1,
            user_id: "@alice:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            tag: "u.work".to_string(),
            order: Some(0.8),
            created_ts: 1700000000000,
        };
        assert!(user_tag.tag.starts_with("u."));
        assert_eq!(user_tag.order, Some(0.8));
    }

    #[test]
    fn test_room_tag_serialization() {
        let tag = RoomTag {
            id: 1,
            user_id: "@alice:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            tag: "m.favourite".to_string(),
            order: Some(0.5),
            created_ts: 1700000000000,
        };
        let json = serde_json::to_string(&tag).expect("Failed to serialize");
        let deserialized: RoomTag = serde_json::from_str(&json).expect("Failed to deserialize");
        assert_eq!(tag.id, deserialized.id);
        assert_eq!(tag.tag, deserialized.tag);
        assert_eq!(tag.order, deserialized.order);
    }
}
