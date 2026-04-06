#![cfg(test)]
mod schema_contract_room_summary_queue_driver_suite {
    use crate::common::get_test_pool_async;
    use sqlx::Row;
    use std::sync::Arc;
    use synapse_rust::services::room_summary_service::RoomSummaryService;
    use synapse_rust::storage::event::{CreateEventParams, EventStorage};
    use synapse_rust::storage::room_summary::{CreateRoomSummaryRequest, RoomSummaryStorage};
    use uuid::Uuid;

    async fn connect_pool() -> Option<Arc<sqlx::PgPool>> {
        match get_test_pool_async().await {
            Ok(pool) => Some(pool),
            Err(error) => {
                eprintln!(
                    "Skipping room summary queue driver tests because test database is unavailable: {}",
                    error
                );
                None
            }
        }
    }

    async fn seed_users_and_room(pool: &sqlx::PgPool, suffix: &str) -> (String, String, String) {
        let creator = format!("@schema-summary-driver-creator-{suffix}:localhost");
        let hero = format!("@schema-summary-driver-hero-{suffix}:localhost");
        let room_id = format!("!schema-summary-driver-room-{suffix}:localhost");

        for (user_id, username) in [
            (&creator, format!("schema_summary_driver_creator_{suffix}")),
            (&hero, format!("schema_summary_driver_hero_{suffix}")),
        ] {
            sqlx::query(
                "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3) ON CONFLICT (user_id) DO NOTHING",
            )
            .bind(user_id)
            .bind(username)
            .bind(0_i64)
            .execute(pool)
            .await
            .expect("Failed to seed user fixture");
        }

        sqlx::query(
            "INSERT INTO rooms (room_id, creator, created_ts) VALUES ($1, $2, $3) ON CONFLICT (room_id) DO NOTHING",
        )
        .bind(&room_id)
        .bind(&creator)
        .bind(0_i64)
        .execute(pool)
        .await
        .expect("Failed to seed room fixture");

        (creator, hero, room_id)
    }

    async fn create_summary_fixture(pool: &Arc<sqlx::PgPool>, room_id: &str) {
        let storage = RoomSummaryStorage::new(pool);
        storage
            .create_summary(CreateRoomSummaryRequest {
                room_id: room_id.to_string(),
                room_type: None,
                name: Some("Driver Before".to_string()),
                topic: None,
                avatar_url: None,
                canonical_alias: None,
                join_rule: Some("invite".to_string()),
                history_visibility: Some("shared".to_string()),
                guest_access: Some("forbidden".to_string()),
                is_direct: Some(false),
                is_space: Some(false),
            })
            .await
            .expect("Failed to create room summary fixture");
    }

    async fn cleanup_room_summary_fixtures(
        pool: &sqlx::PgPool,
        room_id: &str,
        user_ids: &[String],
    ) {
        sqlx::query("DELETE FROM room_children WHERE parent_room_id = $1 OR child_room_id = $1")
            .bind(room_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup room_children");

        sqlx::query("DELETE FROM room_summary_update_queue WHERE room_id = $1")
            .bind(room_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup room_summary_update_queue");

        sqlx::query("DELETE FROM room_summary_stats WHERE room_id = $1")
            .bind(room_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup room_summary_stats");

        sqlx::query("DELETE FROM room_summary_state WHERE room_id = $1")
            .bind(room_id)
            .execute(pool)
            .await
            .expect("Failed to cleanup room_summary_state");

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
            .expect("Failed to cleanup room fixture");

        for user_id in user_ids {
            sqlx::query("DELETE FROM users WHERE user_id = $1")
                .bind(user_id)
                .execute(pool)
                .await
                .expect("Failed to cleanup user fixture");
        }
    }

    async fn insert_queue_item(
        pool: &sqlx::PgPool,
        room_id: &str,
        event_id: &str,
        event_type: &str,
        state_key: Option<&str>,
        priority: i32,
        created_ts: i64,
    ) {
        sqlx::query(
            r#"
            INSERT INTO room_summary_update_queue (room_id, event_id, event_type, state_key, priority, created_ts)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(room_id)
        .bind(event_id)
        .bind(event_type)
        .bind(state_key)
        .bind(priority)
        .bind(created_ts)
        .execute(pool)
        .await
        .expect("Failed to insert queue item fixture");
    }

    #[tokio::test]
    async fn test_schema_contract_room_summary_queue_driver_batches_and_message_ts() {
        let pool = match connect_pool().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = Arc::new(RoomSummaryStorage::new(&pool));
        let event_storage = Arc::new(EventStorage::new(&pool, "localhost".to_string()));
        let service = RoomSummaryService::new(storage.clone(), event_storage.clone(), None);

        let suffix = Uuid::new_v4().to_string();
        let (creator, hero, room_id) = seed_users_and_room(&pool, &suffix).await;
        create_summary_fixture(&pool, &room_id).await;

        let state_name_event_id = format!("$summary-driver-state-name-{suffix}");
        let state_topic_event_id = format!("$summary-driver-state-topic-{suffix}");
        let message_event_id = format!("$summary-driver-message-{suffix}");
        let other_event_id = format!("$summary-driver-other-{suffix}");

        event_storage
            .create_event(
                CreateEventParams {
                    event_id: state_name_event_id.clone(),
                    room_id: room_id.clone(),
                    user_id: creator.clone(),
                    event_type: "m.room.name".to_string(),
                    content: serde_json::json!({ "name": "Driver Updated" }),
                    state_key: Some(String::new()),
                    origin_server_ts: 100_i64,
                },
                None,
            )
            .await
            .expect("Failed to create name state event fixture");

        event_storage
            .create_event(
                CreateEventParams {
                    event_id: state_topic_event_id.clone(),
                    room_id: room_id.clone(),
                    user_id: creator.clone(),
                    event_type: "m.room.topic".to_string(),
                    content: serde_json::json!({ "topic": "Driver Topic" }),
                    state_key: Some(String::new()),
                    origin_server_ts: 110_i64,
                },
                None,
            )
            .await
            .expect("Failed to create topic state event fixture");

        event_storage
            .create_event(
                CreateEventParams {
                    event_id: message_event_id.clone(),
                    room_id: room_id.clone(),
                    user_id: creator.clone(),
                    event_type: "m.room.message".to_string(),
                    content: serde_json::json!({ "body": "hello", "msgtype": "m.text" }),
                    state_key: None,
                    origin_server_ts: 200_i64,
                },
                None,
            )
            .await
            .expect("Failed to create message event fixture");

        event_storage
            .create_event(
                CreateEventParams {
                    event_id: other_event_id.clone(),
                    room_id: room_id.clone(),
                    user_id: creator.clone(),
                    event_type: "org.example.signal".to_string(),
                    content: serde_json::json!({ "body": "signal" }),
                    state_key: None,
                    origin_server_ts: 300_i64,
                },
                None,
            )
            .await
            .expect("Failed to create other event fixture");

        insert_queue_item(
            &pool,
            &room_id,
            &state_name_event_id,
            "m.room.name",
            Some(""),
            10,
            1,
        )
        .await;
        insert_queue_item(
            &pool,
            &room_id,
            &state_topic_event_id,
            "m.room.topic",
            Some(""),
            10,
            2,
        )
        .await;
        insert_queue_item(
            &pool,
            &room_id,
            &message_event_id,
            "m.room.message",
            None,
            10,
            3,
        )
        .await;
        insert_queue_item(
            &pool,
            &room_id,
            &other_event_id,
            "org.example.signal",
            None,
            0,
            4,
        )
        .await;

        let processed_first = service
            .process_pending_updates(2)
            .await
            .expect("Failed to process first batch");
        assert_eq!(processed_first, 2);

        let first_batch_rows = sqlx::query(
            r#"
            SELECT event_id, status
            FROM room_summary_update_queue
            WHERE room_id = $1
            ORDER BY created_ts ASC
            "#,
        )
        .bind(&room_id)
        .fetch_all(&*pool)
        .await
        .expect("Failed to query first batch queue rows");
        assert_eq!(first_batch_rows.len(), 4);
        assert_eq!(first_batch_rows[0].get::<String, _>("status"), "processed");
        assert_eq!(first_batch_rows[1].get::<String, _>("status"), "processed");
        assert_eq!(first_batch_rows[2].get::<String, _>("status"), "pending");
        assert_eq!(first_batch_rows[3].get::<String, _>("status"), "pending");

        let processed_second = service
            .process_pending_updates(1)
            .await
            .expect("Failed to process second batch");
        assert_eq!(processed_second, 1);

        let processed_third = service
            .process_pending_updates(10)
            .await
            .expect("Failed to process third batch");
        assert_eq!(processed_third, 1);

        let final_rows = sqlx::query(
            r#"
            SELECT event_id, status
            FROM room_summary_update_queue
            WHERE room_id = $1
            ORDER BY created_ts ASC
            "#,
        )
        .bind(&room_id)
        .fetch_all(&*pool)
        .await
        .expect("Failed to query final queue rows");
        assert_eq!(final_rows.len(), 4);
        for row in final_rows {
            assert_eq!(row.get::<String, _>("status"), "processed");
        }

        let summary = storage
            .get_summary(&room_id)
            .await
            .expect("Failed to fetch final summary")
            .expect("Expected summary to exist");
        assert_eq!(summary.name.as_deref(), Some("Driver Updated"));
        assert_eq!(summary.topic.as_deref(), Some("Driver Topic"));
        assert_eq!(
            summary.last_event_id.as_deref(),
            Some(other_event_id.as_str())
        );
        assert_eq!(summary.last_event_ts, Some(300_i64));
        assert_eq!(summary.last_message_ts, Some(200_i64));

        cleanup_room_summary_fixtures(&pool, &room_id, &[creator, hero]).await;
    }

    #[tokio::test]
    async fn test_schema_contract_room_summary_queue_failed_items_are_not_reprocessed() {
        let pool = match connect_pool().await {
            Some(pool) => pool,
            None => return,
        };

        let storage = Arc::new(RoomSummaryStorage::new(&pool));
        let event_storage = Arc::new(EventStorage::new(&pool, "localhost".to_string()));
        let service = RoomSummaryService::new(storage.clone(), event_storage.clone(), None);

        let suffix = Uuid::new_v4().to_string();
        let (creator, hero, room_id) = seed_users_and_room(&pool, &suffix).await;
        create_summary_fixture(&pool, &room_id).await;

        let missing_event_id = format!("$summary-driver-missing-{suffix}");
        service
            .queue_update(&room_id, &missing_event_id, "m.room.name", Some(""))
            .await
            .expect("Failed to queue missing event update");

        let processed_first = service
            .process_pending_updates(10)
            .await
            .expect("Failed to process queue with missing event");
        assert_eq!(processed_first, 0);

        let row = sqlx::query(
            r#"
            SELECT status, error_message, retry_count
            FROM room_summary_update_queue
            WHERE room_id = $1 AND event_id = $2
            "#,
        )
        .bind(&room_id)
        .bind(&missing_event_id)
        .fetch_one(&*pool)
        .await
        .expect("Failed to query missing event queue row");
        assert_eq!(row.get::<String, _>("status"), "failed");
        assert_eq!(
            row.get::<Option<String>, _>("error_message").as_deref(),
            Some("Not found: Event not found")
        );
        assert_eq!(row.get::<i32, _>("retry_count"), 1);

        let processed_second = service
            .process_pending_updates(10)
            .await
            .expect("Failed to process queue a second time");
        assert_eq!(processed_second, 0);

        let row_after = sqlx::query(
            r#"
            SELECT status, retry_count
            FROM room_summary_update_queue
            WHERE room_id = $1 AND event_id = $2
            "#,
        )
        .bind(&room_id)
        .bind(&missing_event_id)
        .fetch_one(&*pool)
        .await
        .expect("Failed to query missing event queue row after second run");
        assert_eq!(row_after.get::<String, _>("status"), "failed");
        assert_eq!(row_after.get::<i32, _>("retry_count"), 1);

        cleanup_room_summary_fixtures(&pool, &room_id, &[creator, hero]).await;
    }
}
