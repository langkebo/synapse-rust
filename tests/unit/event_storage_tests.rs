#![cfg(test)]

use sqlx::{Pool, Postgres};
use std::sync::Arc;
use tokio::runtime::Runtime;

use synapse_rust::storage::event::{CreateEventParams, EventStorage};

async fn setup_test_database() -> Option<Pool<Postgres>> {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .unwrap_or_else(|_| {
            "postgresql://synapse:secret@localhost:5432/synapse_test".to_string()
        });

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

    sqlx::query(r#"
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
            processed_ts BIGINT NOT NULL,
            not_before BIGINT,
            status TEXT,
            reference_image TEXT,
            origin TEXT,
            unsigned JSONB
        )
    "#)
    .execute(&pool)
    .await
    .expect("Failed to create events table");

    Some(pool)
}

#[test]
fn test_create_event_success() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => Arc::new(pool),
            None => return,
        };
        let storage = EventStorage::new(&pool);

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
    });
}

#[test]
fn test_get_room_events_batch_empty() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => Arc::new(pool),
            None => return,
        };
        let storage = EventStorage::new(&pool);

        let room_ids: Vec<String> = vec![];
        let result = storage.get_room_events_batch(&room_ids, 10).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    });
}

#[test]
fn test_get_room_events_batch_multiple_rooms() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => Arc::new(pool),
            None => return,
        };
        let storage = EventStorage::new(&pool);

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
    });
}

#[test]
fn test_get_room_events_since_batch() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => Arc::new(pool),
            None => return,
        };
        let storage = EventStorage::new(&pool);

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
        
        let result = storage.get_room_events_since_batch(&room_ids, base_ts + 2500, 10).await;
        assert!(result.is_ok());
        
        let events_map = result.unwrap();
        let events = events_map.values().next().unwrap();
        
        assert!(events.len() >= 2, "Should have events after the since timestamp");
    });
}

#[test]
fn test_get_room_events_batch_limit_per_room() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => Arc::new(pool),
            None => return,
        };
        let storage = EventStorage::new(&pool);

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
    });
}
