#![cfg(test)]

use sqlx::{Pool, Postgres};
use std::sync::Arc;
use std::sync::{Mutex, OnceLock};
use tokio::runtime::Runtime;

use synapse_rust::storage::event::{CreateEventParams, EventStorage};

fn event_storage_test_guard() -> &'static Mutex<()> {
    static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    GUARD.get_or_init(|| Mutex::new(()))
}

async fn setup_test_database() -> Option<Pool<Postgres>> {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .unwrap_or_else(|_| "postgresql://synapse:secret@localhost:5432/synapse_test".to_string());

    let pool = match sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect(&database_url)
        .await
    {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!(
                "Skipping event storage tests because test database is unavailable: {}",
                error
            );
            return None;
        }
    };

    sqlx::query("DROP TABLE IF EXISTS events CASCADE")
        .execute(&pool)
        .await
        .ok();

    sqlx::query(
        r#"
        CREATE TABLE events (
            event_id VARCHAR(255) PRIMARY KEY,
            room_id VARCHAR(255) NOT NULL,
            user_id VARCHAR(255) NOT NULL,
            sender VARCHAR(255) NOT NULL,
            event_type TEXT NOT NULL,
            content JSONB NOT NULL,
            state_key TEXT,
            depth BIGINT,
            origin_server_ts BIGINT NOT NULL,
            processed_ts BIGINT,
            not_before BIGINT,
            is_redacted BOOLEAN DEFAULT FALSE,
            status TEXT,
            reference_image TEXT,
            origin TEXT,
            unsigned JSONB
        )
    "#,
    )
    .execute(&pool)
    .await
    .expect("Failed to create events table");

    Some(pool)
}

async fn teardown_test_database(pool: &Pool<Postgres>) {
    sqlx::query("DROP TABLE IF EXISTS events CASCADE")
        .execute(pool)
        .await
        .ok();
}

#[test]
fn test_create_event_success() {
    let _guard = event_storage_test_guard().lock().unwrap();
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => Arc::new(pool),
            None => return,
        };
        let storage = EventStorage::new(&pool, "localhost".to_string());

        let params = CreateEventParams {
            event_id: "$event1:localhost".to_string(),
            room_id: "!room1:localhost".to_string(),
            user_id: "@alice:localhost".to_string(),
            event_type: "m.room.message".to_string(),
            content: serde_json::json!({"body": "Hello"}),
            state_key: None,
            origin_server_ts: chrono::Utc::now().timestamp_millis(),
        };

        let result = storage.create_event(params, None).await;
        assert!(result.is_ok());
        let event = result.unwrap();
        assert_eq!(event.event_id, "$event1:localhost");
        assert_eq!(event.room_id, "!room1:localhost");
        teardown_test_database(&pool).await;
    });
}

#[test]
fn test_get_room_events_batch_empty() {
    let _guard = event_storage_test_guard().lock().unwrap();
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => Arc::new(pool),
            None => return,
        };
        let storage = EventStorage::new(&pool, "localhost".to_string());

        let room_ids: Vec<String> = vec![];
        let result = storage.get_room_events_batch(&room_ids, 10).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
        teardown_test_database(&pool).await;
    });
}

#[test]
fn test_get_room_events_batch_multiple_rooms() {
    let _guard = event_storage_test_guard().lock().unwrap();
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => Arc::new(pool),
            None => return,
        };
        let storage = EventStorage::new(&pool, "localhost".to_string());

        let ts = chrono::Utc::now().timestamp_millis();

        for i in 1..=3 {
            let params = CreateEventParams {
                event_id: format!("$event{}:localhost", i),
                room_id: format!("!room{}:localhost", i),
                user_id: "@alice:localhost".to_string(),
                event_type: "m.room.message".to_string(),
                content: serde_json::json!({"body": format!("Message {}", i)}),
                state_key: None,
                origin_server_ts: ts + i,
            };
            storage.create_event(params, None).await.unwrap();
        }

        let room_ids = vec![
            "!room1:localhost".to_string(),
            "!room2:localhost".to_string(),
            "!room3:localhost".to_string(),
        ];
        let result = storage.get_room_events_batch(&room_ids, 10).await;
        assert!(result.is_ok());

        let events_map = result.unwrap();
        assert_eq!(events_map.len(), 3);

        for room_id in &room_ids {
            assert!(events_map.contains_key(room_id));
            let events = events_map.get(room_id).unwrap();
            assert_eq!(events.len(), 1);
        }
        teardown_test_database(&pool).await;
    });
}

#[test]
fn test_get_room_events_since_batch() {
    let _guard = event_storage_test_guard().lock().unwrap();
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => Arc::new(pool),
            None => return,
        };
        let storage = EventStorage::new(&pool, "localhost".to_string());

        let base_ts = chrono::Utc::now().timestamp_millis();
        let room_id = "!room_batch:localhost".to_string();

        for i in 1..=5 {
            let params = CreateEventParams {
                event_id: format!("$event_batch{}:localhost", i),
                room_id: room_id.clone(),
                user_id: "@alice:localhost".to_string(),
                event_type: "m.room.message".to_string(),
                content: serde_json::json!({"body": format!("Message {}", i)}),
                state_key: None,
                origin_server_ts: base_ts + i * 1000,
            };
            storage.create_event(params, None).await.unwrap();
        }

        let room_ids = vec![room_id];

        let result = storage
            .get_room_events_since_batch(&room_ids, base_ts + 2500, 10)
            .await;
        assert!(result.is_ok());

        let events_map = result.unwrap();
        let events = events_map.values().next().unwrap();

        assert!(
            events.len() >= 2,
            "Should have events after the since timestamp"
        );
        teardown_test_database(&pool).await;
    });
}

#[test]
fn test_get_room_events_batch_limit_per_room() {
    let _guard = event_storage_test_guard().lock().unwrap();
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => Arc::new(pool),
            None => return,
        };
        let storage = EventStorage::new(&pool, "localhost".to_string());

        let base_ts = chrono::Utc::now().timestamp_millis();
        let room_id = "!room_limit:localhost".to_string();

        for i in 1..=10 {
            let params = CreateEventParams {
                event_id: format!("$event_limit{}:localhost", i),
                room_id: room_id.clone(),
                user_id: "@alice:localhost".to_string(),
                event_type: "m.room.message".to_string(),
                content: serde_json::json!({"body": format!("Message {}", i)}),
                state_key: None,
                origin_server_ts: base_ts + i,
            };
            storage.create_event(params, None).await.unwrap();
        }

        let room_ids = vec![room_id];
        let result = storage.get_room_events_batch(&room_ids, 3).await;
        assert!(result.is_ok());

        let events_map = result.unwrap();
        let events = events_map.values().next().unwrap();

        assert_eq!(events.len(), 3, "Should respect the limit per room");
        teardown_test_database(&pool).await;
    });
}

#[test]
fn test_encrypted_event_origin_decode_handles_null_boundary_and_malformed_values() {
    let _guard = event_storage_test_guard().lock().unwrap();
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => Arc::new(pool),
            None => return,
        };
        let storage = EventStorage::new(&pool, "localhost".to_string());
        let room_id = "!origin-room:localhost";
        let base_ts = chrono::Utc::now().timestamp_millis();
        let cases = [
            ("$origin_null:localhost", None, "self"),
            ("$origin_empty:localhost", Some(""), "self"),
            ("$origin_undefined:localhost", Some("undefined"), "self"),
            (
                "$origin_illegal_json:localhost",
                Some("{invalid-json"),
                "{invalid-json",
            ),
            (
                "$origin_encrypted:localhost",
                Some("encrypted:abcdef123456"),
                "encrypted:abcdef123456",
            ),
        ];

        for (offset, (event_id, origin, _expected)) in cases.iter().enumerate() {
            sqlx::query(
                r#"
                INSERT INTO events (
                    event_id, room_id, user_id, sender, event_type, content,
                    state_key, depth, origin_server_ts, processed_ts, not_before,
                    status, reference_image, origin, unsigned
                )
                VALUES (
                    $1, $2, $3, $4, 'm.room.encrypted', $5,
                    NULL, $6, $7, $7, 0,
                    'persisted', NULL, $8, '{}'::jsonb
                )
                "#,
            )
            .bind(*event_id)
            .bind(room_id)
            .bind("@alice:localhost")
            .bind("@alice:localhost")
            .bind(serde_json::json!({
                "algorithm": "m.megolm.v1.aes-sha2",
                "ciphertext": format!("ciphertext-{offset}")
            }))
            .bind(offset as i64)
            .bind(base_ts + offset as i64)
            .bind(*origin)
            .execute(&*pool)
            .await
            .expect("Failed to seed encrypted event");
        }

        let events = storage
            .get_room_events_by_type(room_id, "m.room.encrypted", 10)
            .await
            .expect("Failed to fetch encrypted events");

        assert_eq!(events.len(), cases.len());

        let origin_by_event_id = events
            .iter()
            .map(|event| (event.event_id.as_str(), event.origin.as_str()))
            .collect::<std::collections::HashMap<_, _>>();

        assert_eq!(
            origin_by_event_id.get("$origin_null:localhost"),
            Some(&"self")
        );
        assert_eq!(
            origin_by_event_id.get("$origin_empty:localhost"),
            Some(&"self")
        );
        assert_eq!(
            origin_by_event_id.get("$origin_undefined:localhost"),
            Some(&"self")
        );
        assert_eq!(
            origin_by_event_id.get("$origin_illegal_json:localhost"),
            Some(&"{invalid-json")
        );
        assert_eq!(
            origin_by_event_id.get("$origin_encrypted:localhost"),
            Some(&"encrypted:abcdef123456")
        );

        teardown_test_database(&pool).await;
    });
}
