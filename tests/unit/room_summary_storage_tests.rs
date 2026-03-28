#![cfg(test)]

mod room_summary_storage_tests {
    use sqlx::postgres::PgPoolOptions;
    use std::sync::Arc;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    use synapse_rust::storage::room_summary::{
        CreateRoomSummaryRequest, CreateSummaryMemberRequest, RoomSummaryStorage,
        UpdateRoomSummaryRequest,
    };

    fn unique_suffix() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    }

    async fn connect_pool() -> Arc<sqlx::PgPool> {
        let database_url = std::env::var("TEST_DATABASE_URL")
            .or_else(|_| std::env::var("DATABASE_URL"))
            .unwrap_or_else(|_| "postgresql://synapse:secret@localhost:5432/synapse_test".to_string());

        Arc::new(
            PgPoolOptions::new()
                .max_connections(5)
                .acquire_timeout(Duration::from_secs(10))
                .connect(&database_url)
                .await
                .expect("Failed to connect to test database"),
        )
    }

    async fn seed_users_and_room(pool: &sqlx::PgPool, suffix: u128) -> (String, String, String) {
        let creator = format!("@summarycreator{suffix}:localhost");
        let hero = format!("@summaryhero{suffix}:localhost");
        let room_id = format!("!summaryroom{suffix}:localhost");

        for (user_id, username) in [
            (&creator, format!("summarycreator{suffix}")),
            (&hero, format!("summaryhero{suffix}")),
        ] {
            sqlx::query("INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3) ON CONFLICT (user_id) DO NOTHING")
                .bind(user_id)
                .bind(username)
                .bind(0_i64)
                .execute(pool)
                .await
                .expect("Failed to seed user");
        }

        sqlx::query("INSERT INTO rooms (room_id, creator, created_ts) VALUES ($1, $2, $3) ON CONFLICT (room_id) DO NOTHING")
            .bind(&room_id)
            .bind(&creator)
            .bind(0_i64)
            .execute(pool)
            .await
            .expect("Failed to seed room");

        (creator, hero, room_id)
    }

    async fn cleanup(pool: &sqlx::PgPool, room_id: &str, users: &[String]) {
        sqlx::query("DELETE FROM room_summary_members WHERE room_id = $1")
            .bind(room_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup room_summary_members");

        sqlx::query("DELETE FROM room_summaries WHERE room_id = $1")
            .bind(room_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup room_summaries");

        sqlx::query("DELETE FROM rooms WHERE room_id = $1")
            .bind(room_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup room");

        for user_id in users {
            sqlx::query("DELETE FROM users WHERE user_id = $1")
                .bind(user_id)
                .execute(pool)
                .await
                .expect("Failed to cleanup user");
        }
    }

    #[tokio::test]
    async fn test_room_summary_storage_roundtrip() {
        let pool = connect_pool().await;
        let storage = RoomSummaryStorage::new(&pool);
        let suffix = unique_suffix();
        let (creator, hero, room_id) = seed_users_and_room(&pool, suffix).await;

        let summary = storage
            .create_summary(CreateRoomSummaryRequest {
                room_id: room_id.clone(),
                room_type: Some("m.room".to_string()),
                name: Some("Summary Room".to_string()),
                topic: Some("Storage roundtrip".to_string()),
                avatar_url: Some("mxc://localhost/avatar".to_string()),
                canonical_alias: Some("#summary:localhost".to_string()),
                join_rule: Some("invite".to_string()),
                history_visibility: Some("shared".to_string()),
                guest_access: Some("forbidden".to_string()),
                is_direct: Some(false),
                is_space: Some(false),
            })
            .await
            .expect("Failed to create room summary");

        assert_eq!(summary.room_id, room_id);
        assert_eq!(summary.name.as_deref(), Some("Summary Room"));
        assert_eq!(summary.join_rule, "invite");
        assert_eq!(summary.created_ts, summary.updated_ts);

        let updated_summary = storage
            .update_summary(
                &room_id,
                UpdateRoomSummaryRequest {
                    name: Some("Updated Summary Room".to_string()),
                    topic: None,
                    avatar_url: None,
                    canonical_alias: None,
                    join_rule: Some("public".to_string()),
                    history_visibility: None,
                    guest_access: None,
                    is_direct: Some(true),
                    is_space: Some(false),
                    is_encrypted: Some(true),
                    last_event_id: Some("$summary-event".to_string()),
                    last_event_ts: Some(1234),
                    last_message_ts: Some(2345),
                    hero_users: Some(serde_json::json!([creator.clone(), hero.clone()])),
                },
            )
            .await
            .expect("Failed to update room summary");

        assert_eq!(updated_summary.name.as_deref(), Some("Updated Summary Room"));
        assert_eq!(updated_summary.join_rule, "public");
        assert!(updated_summary.is_direct);
        assert!(updated_summary.is_encrypted);

        storage
            .add_member(CreateSummaryMemberRequest {
                room_id: room_id.clone(),
                user_id: creator.clone(),
                display_name: Some("Creator".to_string()),
                avatar_url: None,
                membership: "join".to_string(),
                is_hero: Some(false),
                last_active_ts: Some(100),
            })
            .await
            .expect("Failed to add creator member");

        storage
            .add_member(CreateSummaryMemberRequest {
                room_id: room_id.clone(),
                user_id: hero.clone(),
                display_name: Some("Hero".to_string()),
                avatar_url: None,
                membership: "join".to_string(),
                is_hero: Some(true),
                last_active_ts: Some(200),
            })
            .await
            .expect("Failed to add hero member");

        let loaded_summary = storage
            .get_summary(&room_id)
            .await
            .expect("Failed to get summary")
            .expect("Summary should exist");

        assert_eq!(loaded_summary.last_event_id.as_deref(), Some("$summary-event"));

        let summaries_for_user = storage
            .get_summaries_for_user(&hero)
            .await
            .expect("Failed to get summaries for user");

        assert_eq!(summaries_for_user.len(), 1);
        assert_eq!(summaries_for_user[0].room_id, room_id);

        let members = storage
            .get_members(&room_id)
            .await
            .expect("Failed to get members");

        assert_eq!(members.len(), 2);

        let heroes = storage
            .get_heroes(&room_id, 5)
            .await
            .expect("Failed to get heroes");

        assert_eq!(heroes.len(), 2);
        assert_eq!(heroes[0].user_id, hero);

        cleanup(&pool, &room_id, &[creator, hero]).await;
    }
}
