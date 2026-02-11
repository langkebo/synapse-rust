#[cfg(test)]
mod protocol_compliance_tests {
    use serde_json::json;
    use sqlx::{Pool, Postgres};
    use std::sync::Arc;
    use synapse_rust::cache::{CacheConfig, CacheManager};
    use synapse_rust::services::{PresenceStorage, ServiceContainer};
    use synapse_rust::storage::{CreateEventParams, EventStorage, RoomStorage};
    use tokio::runtime::Runtime;

    async fn setup_test_database() -> Option<Pool<Postgres>> {
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://synapse:synapse@localhost:5432/synapse_test".to_string()
        });
        let pool = match sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(std::time::Duration::from_secs(5))
            .connect(&database_url)
            .await
        {
            Ok(pool) => pool,
            Err(error) => {
                eprintln!("Skipping protocol tests; database unavailable: {}", error);
                return None;
            }
        };

        // Run database initialization to ensure all tables exist
        let init_service = synapse_rust::services::DatabaseInitService::new(std::sync::Arc::new(pool.clone()));
        if let Err(e) = init_service.initialize().await {
            eprintln!("Database initialization failed: {}", e);
        }

        // Manually ensure typing table exists (in case init failed silently)
        let _ = sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS typing (
                user_id TEXT NOT NULL,
                room_id TEXT NOT NULL,
                typing BOOLEAN DEFAULT FALSE,
                last_active_ts BIGINT NOT NULL,
                UNIQUE (user_id, room_id)
            )
            "#,
        ).execute(&pool).await;

        // Manually ensure is_guest column exists
        let _ = sqlx::query("ALTER TABLE users ADD COLUMN IF NOT EXISTS is_guest BOOLEAN DEFAULT FALSE")
            .execute(&pool).await;

        // Clean up any existing test data to avoid conflicts
        sqlx::query("DELETE FROM read_markers WHERE user_id LIKE '@%:localhost'")
            .execute(&pool).await.ok();
        sqlx::query("DELETE FROM receipts WHERE sender LIKE '@%:localhost' OR sent_to LIKE '@%:localhost'")
            .execute(&pool).await.ok();
        sqlx::query("DELETE FROM typing WHERE user_id LIKE '@%:localhost'")
            .execute(&pool).await.ok();
        sqlx::query("DELETE FROM events WHERE room_id = '!room:test'")
            .execute(&pool).await.ok();
        sqlx::query("DELETE FROM rooms WHERE room_id = '!room:test'")
            .execute(&pool).await.ok();
        sqlx::query("DELETE FROM users WHERE user_id LIKE '@%:localhost'")
            .execute(&pool).await.ok();

        Some(pool)
    }

    async fn create_test_user(pool: &Pool<Postgres>, user_id: &str) {
        // Extract username from user_id (e.g., "@alice:localhost" -> "alice")
        let username = user_id.trim_start_matches('@').split(':').next().unwrap_or("unknown");
        let now = chrono::Utc::now().timestamp_millis();

        // Use the actual schema from master_unified_schema.sql
        sqlx::query(
            "INSERT INTO users (user_id, username, creation_ts, generation) VALUES ($1, $2, $3, $4)
             ON CONFLICT (user_id) DO NOTHING"
        )
        .bind(user_id)
        .bind(username)
        .bind(now)
        .bind(1)  // generation
        .execute(pool)
        .await
        .ok();
    }

    async fn create_test_room(pool: &Pool<Postgres>, room_id: &str) {
        // First create the creator user if it doesn't exist
        create_test_user(pool, "@creator:localhost").await;

        let now = chrono::Utc::now().timestamp_millis();

        // Use the actual schema from master_unified_schema.sql
        // Required NOT NULL fields: room_id, creator, creation_ts, last_activity_ts
        sqlx::query(
            "INSERT INTO rooms (room_id, creator, creation_ts, last_activity_ts) VALUES ($1, $2, $3, $4)
             ON CONFLICT (room_id) DO NOTHING"
        )
        .bind(room_id)
        .bind("@creator:localhost")
        .bind(now)
        .bind(now)  // last_activity_ts
        .execute(pool)
        .await
        .ok();
    }

    #[test]
    fn test_typing_set_and_clear() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let pool = match setup_test_database().await {
                Some(p) => p,
                None => return,
            };
            let arc_pool = Arc::new(pool.clone());
            let cache = Arc::new(CacheManager::new(CacheConfig::default()));
            let presence = PresenceStorage::new(arc_pool.clone(), cache.clone());

            let room_id = "!room:test";
            let user_id = "@alice:localhost";
            create_test_user(&pool, user_id).await;
            create_test_room(&pool, room_id).await;

            presence.set_typing(room_id, user_id, true).await.unwrap();
            let count: (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM typing WHERE room_id = $1 AND user_id = $2 AND typing = TRUE",
            )
            .bind(room_id)
            .bind(user_id)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(count.0, 1);

            presence.set_typing(room_id, user_id, false).await.unwrap();
            let count: (i64,) =
                sqlx::query_as("SELECT COUNT(*) FROM typing WHERE room_id = $1 AND user_id = $2")
                    .bind(room_id)
                    .bind(user_id)
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(count.0, 0);
        });
    }

    #[test]
    fn test_read_marker_update() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let pool = match setup_test_database().await {
                Some(p) => p,
                None => return,
            };
            let arc_pool = Arc::new(pool.clone());
            let room_storage = RoomStorage::new(&arc_pool);

            let room_id = "!room:test";
            let user_id = "@bob:localhost";
            let event_id = "$event:test";
            create_test_user(&pool, user_id).await;
            create_test_room(&pool, room_id).await;

            room_storage
                .update_read_marker(room_id, user_id, event_id)
                .await
                .unwrap();
            let row: (String,) = sqlx::query_as(
                "SELECT event_id FROM read_markers WHERE room_id = $1 AND user_id = $2",
            )
            .bind(room_id)
            .bind(user_id)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(row.0, event_id);
        });
    }

    #[test]
    fn test_receipt_insert() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let pool = match setup_test_database().await { Some(p) => p, None => return };
            let arc_pool = Arc::new(pool.clone());
            let room_storage = RoomStorage::new(&arc_pool);
            let event_storage = EventStorage::new(&arc_pool);
            let services = ServiceContainer::new_test();

            let room_id = "!room:test";
            let sender = "@alice:localhost";
            let target = "@bob:localhost";
            create_test_user(&pool, sender).await;
            create_test_user(&pool, target).await;
            create_test_room(&pool, room_id).await;

            let event_id = "$ev_read:test";
            let now = chrono::Utc::now().timestamp_millis();
            EventStorage::new(&arc_pool)
                .create_event(CreateEventParams {
                    event_id: event_id.to_string(),
                    room_id: room_id.to_string(),
                    user_id: target.to_string(),
                    event_type: "m.room.message".to_string(),
                    content: json!({"body":"hi"}),
                    state_key: None,
                    origin_server_ts: now,
                })
                .await
                .unwrap();

            room_storage.add_receipt(sender, target, room_id, event_id, "m.read").await.unwrap();
            // Query from event_receipts table which is what add_receipt actually uses
            let row: (String, String, String,) = sqlx::query_as("SELECT room_id, receipt_type, event_id FROM event_receipts WHERE room_id = $1 AND event_id = $2 AND user_id = $3")
                .bind(room_id).bind(event_id).bind(target).fetch_one(&pool).await.unwrap();
            assert_eq!(row.0, room_id);
            assert_eq!(row.1, "m.read".to_string());
            assert_eq!(row.2, event_id);
            let _ = services; // ensure services construct in tests
            let _ = event_storage; // ensure storage is usable
        });
    }
}
