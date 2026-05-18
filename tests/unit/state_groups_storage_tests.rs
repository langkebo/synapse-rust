#![cfg(test)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use synapse_rust::storage::state_groups::{
    StateGroupStateEntry, StateGroupStorage,
};
use tokio::runtime::Runtime;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

async fn setup_test_database() -> Option<Arc<sqlx::PgPool>> {
    let pool = match synapse_rust::test_utils::prepare_empty_isolated_test_pool().await {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!(
                "Skipping state groups storage tests because test database is unavailable: {error}"
            );
            return None;
        }
    };

    sqlx::query(
        r#"
        CREATE TABLE rooms (
            room_id TEXT NOT NULL PRIMARY KEY,
            creator TEXT,
            is_public BOOLEAN DEFAULT FALSE,
            room_version TEXT DEFAULT '6',
            created_ts BIGINT NOT NULL,
            last_activity_ts BIGINT,
            is_federated BOOLEAN DEFAULT TRUE,
            has_guest_access BOOLEAN DEFAULT FALSE,
            join_rules TEXT DEFAULT 'invite',
            history_visibility TEXT DEFAULT 'shared',
            name TEXT,
            topic TEXT,
            avatar_url TEXT,
            canonical_alias TEXT,
            visibility TEXT DEFAULT 'private'
        )
        "#,
    )
    .execute(&*pool)
    .await
    .expect("Failed to create rooms table");

    sqlx::query(
        r#"
        CREATE TABLE events (
            event_id TEXT NOT NULL PRIMARY KEY,
            room_id TEXT NOT NULL,
            sender TEXT NOT NULL,
            event_type TEXT NOT NULL,
            content JSONB NOT NULL,
            origin_server_ts BIGINT NOT NULL,
            state_key TEXT,
            is_redacted BOOLEAN DEFAULT FALSE,
            redacted_at BIGINT,
            redacted_by TEXT,
            transaction_id TEXT,
            depth BIGINT,
            prev_events JSONB,
            auth_events JSONB,
            signatures JSONB,
            hashes JSONB,
            unsigned JSONB DEFAULT '{}',
            processed_at BIGINT,
            not_before BIGINT DEFAULT 0,
            status TEXT,
            reference_image TEXT,
            origin TEXT,
            user_id TEXT,
            stream_ordering BIGSERIAL,
            FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
        )
        "#,
    )
    .execute(&*pool)
    .await
    .expect("Failed to create events table");

    sqlx::query(
        r#"
        CREATE TABLE state_groups (
            id BIGSERIAL PRIMARY KEY,
            room_id TEXT NOT NULL,
            event_id TEXT NOT NULL,
            state_hash TEXT NOT NULL UNIQUE,
            created_ts BIGINT NOT NULL,
            FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
            FOREIGN KEY (event_id) REFERENCES events(event_id) ON DELETE CASCADE
        )
        "#,
    )
    .execute(&*pool)
    .await
    .expect("Failed to create state_groups table");

    sqlx::query(
        r#"
        CREATE TABLE state_group_edges (
            state_group_id BIGINT NOT NULL,
            prev_state_group_id BIGINT NOT NULL,
            PRIMARY KEY (state_group_id, prev_state_group_id),
            FOREIGN KEY (state_group_id) REFERENCES state_groups(id) ON DELETE CASCADE,
            FOREIGN KEY (prev_state_group_id) REFERENCES state_groups(id) ON DELETE CASCADE
        )
        "#,
    )
    .execute(&*pool)
    .await
    .expect("Failed to create state_group_edges table");

    sqlx::query(
        r#"
        CREATE TABLE event_to_state_groups (
            event_id TEXT NOT NULL PRIMARY KEY,
            state_group_id BIGINT NOT NULL,
            FOREIGN KEY (event_id) REFERENCES events(event_id) ON DELETE CASCADE,
            FOREIGN KEY (state_group_id) REFERENCES state_groups(id) ON DELETE CASCADE
        )
        "#,
    )
    .execute(&*pool)
    .await
    .expect("Failed to create event_to_state_groups table");

    sqlx::query(
        r#"
        CREATE TABLE state_group_state (
            state_group_id BIGINT NOT NULL,
            event_type TEXT NOT NULL,
            state_key TEXT NOT NULL,
            event_id TEXT NOT NULL,
            PRIMARY KEY (state_group_id, event_type, state_key),
            FOREIGN KEY (state_group_id) REFERENCES state_groups(id) ON DELETE CASCADE,
            FOREIGN KEY (event_id) REFERENCES events(event_id) ON DELETE CASCADE
        )
        "#,
    )
    .execute(&*pool)
    .await
    .expect("Failed to create state_group_state table");

    Some(pool)
}

async fn insert_room(pool: &sqlx::PgPool, room_id: &str) {
    sqlx::query(
        r#"INSERT INTO rooms (room_id, creator, created_ts) VALUES ($1, $2, $3)"#,
    )
    .bind(room_id)
    .bind("@creator:test")
    .bind(1000_i64)
    .execute(pool)
    .await
    .expect("Failed to insert room");
}

async fn insert_event(pool: &sqlx::PgPool, event_id: &str, room_id: &str) {
    sqlx::query(
        r#"INSERT INTO events (event_id, room_id, sender, event_type, content, origin_server_ts)
           VALUES ($1, $2, $3, $4, $5, $6)"#,
    )
    .bind(event_id)
    .bind(room_id)
    .bind("@sender:test")
    .bind("m.room.message")
    .bind(serde_json::json!({}))
    .bind(1000_i64)
    .execute(pool)
    .await
    .expect("Failed to insert event");
}

#[test]
fn test_create_and_get_state_group() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = StateGroupStorage::new(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:test");
        let event_id = format!("$event_{suffix}:test");
        let state_hash = format!("hash_{suffix}");

        insert_room(&pool, &room_id).await;
        insert_event(&pool, &event_id, &room_id).await;

        let id = storage
            .create_state_group(&room_id, &event_id, &state_hash, 1000)
            .await
            .unwrap();

        let sg = storage.get_state_group(id).await.unwrap().unwrap();
        assert_eq!(sg.id, id);
        assert_eq!(sg.room_id, room_id);
        assert_eq!(sg.event_id, event_id);
        assert_eq!(sg.state_hash, state_hash);
        assert_eq!(sg.created_ts, 1000);
    });
}

#[test]
fn test_get_state_group_not_found() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = StateGroupStorage::new(&pool);

        let result = storage.get_state_group(99999).await.unwrap();
        assert!(result.is_none());
    });
}

#[test]
fn test_create_state_group_upsert_on_conflict() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = StateGroupStorage::new(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:test");
        let event_id1 = format!("$event1_{suffix}:test");
        let event_id2 = format!("$event2_{suffix}:test");
        let state_hash = format!("hash_{suffix}");

        insert_room(&pool, &room_id).await;
        insert_event(&pool, &event_id1, &room_id).await;
        insert_event(&pool, &event_id2, &room_id).await;

        let id1 = storage
            .create_state_group(&room_id, &event_id1, &state_hash, 1000)
            .await
            .unwrap();

        let id2 = storage
            .create_state_group(&room_id, &event_id2, &state_hash, 2000)
            .await
            .unwrap();

        assert_eq!(id1, id2);

        let sg = storage.get_state_group(id1).await.unwrap().unwrap();
        assert_eq!(sg.event_id, event_id2);
    });
}

#[test]
fn test_get_state_group_by_event() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = StateGroupStorage::new(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:test");
        let event_id = format!("$event_{suffix}:test");
        let state_hash = format!("hash_{suffix}");

        insert_room(&pool, &room_id).await;
        insert_event(&pool, &event_id, &room_id).await;

        storage
            .create_state_group(&room_id, &event_id, &state_hash, 1000)
            .await
            .unwrap();

        let sg = storage.get_state_group_by_event(&event_id).await.unwrap().unwrap();
        assert_eq!(sg.event_id, event_id);
        assert_eq!(sg.room_id, room_id);
    });
}

#[test]
fn test_get_state_group_by_event_not_found() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = StateGroupStorage::new(&pool);

        let result = storage.get_state_group_by_event("$nonexistent:test").await.unwrap();
        assert!(result.is_none());
    });
}

#[test]
fn test_get_room_state_groups() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = StateGroupStorage::new(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:test");

        insert_room(&pool, &room_id).await;

        for i in 0..3 {
            let event_id = format!("$event_{suffix}_{i}:test");
            let state_hash = format!("hash_{suffix}_{i}");
            insert_event(&pool, &event_id, &room_id).await;
            storage
                .create_state_group(&room_id, &event_id, &state_hash, 1000 + i)
                .await
                .unwrap();
        }

        let groups = storage.get_room_state_groups(&room_id, 10).await.unwrap();
        assert_eq!(groups.len(), 3);

        let limited = storage.get_room_state_groups(&room_id, 2).await.unwrap();
        assert_eq!(limited.len(), 2);
    });
}

#[test]
fn test_add_and_get_state_group_edges() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = StateGroupStorage::new(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:test");

        insert_room(&pool, &room_id).await;

        let mut sg_ids = Vec::new();
        for i in 0..3 {
            let event_id = format!("$event_{suffix}_{i}:test");
            let state_hash = format!("hash_{suffix}_{i}");
            insert_event(&pool, &event_id, &room_id).await;
            let id = storage
                .create_state_group(&room_id, &event_id, &state_hash, 1000 + i)
                .await
                .unwrap();
            sg_ids.push(id);
        }

        storage.add_state_group_edge(sg_ids[1], sg_ids[0]).await.unwrap();
        storage.add_state_group_edge(sg_ids[2], sg_ids[1]).await.unwrap();

        let prev = storage.get_prev_state_groups(sg_ids[1]).await.unwrap();
        assert_eq!(prev, vec![sg_ids[0]]);

        let prev2 = storage.get_prev_state_groups(sg_ids[2]).await.unwrap();
        assert_eq!(prev2, vec![sg_ids[1]]);

        let next = storage.get_next_state_groups(sg_ids[0]).await.unwrap();
        assert_eq!(next, vec![sg_ids[1]]);

        let next2 = storage.get_next_state_groups(sg_ids[1]).await.unwrap();
        assert_eq!(next2, vec![sg_ids[2]]);
    });
}

#[test]
fn test_add_state_group_edges_batch() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = StateGroupStorage::new(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:test");

        insert_room(&pool, &room_id).await;

        let mut sg_ids = Vec::new();
        for i in 0..3 {
            let event_id = format!("$event_{suffix}_{i}:test");
            let state_hash = format!("hash_{suffix}_{i}");
            insert_event(&pool, &event_id, &room_id).await;
            let id = storage
                .create_state_group(&room_id, &event_id, &state_hash, 1000 + i)
                .await
                .unwrap();
            sg_ids.push(id);
        }

        storage
            .add_state_group_edges(sg_ids[2], &[sg_ids[0], sg_ids[1]])
            .await
            .unwrap();

        let prev = storage.get_prev_state_groups(sg_ids[2]).await.unwrap();
        assert_eq!(prev.len(), 2);
        assert!(prev.contains(&sg_ids[0]));
        assert!(prev.contains(&sg_ids[1]));
    });
}

#[test]
fn test_add_state_group_edge_duplicate_no_error() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = StateGroupStorage::new(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:test");

        insert_room(&pool, &room_id).await;

        let event_id1 = format!("$event_{suffix}_1:test");
        let event_id2 = format!("$event_{suffix}_2:test");
        let state_hash1 = format!("hash_{suffix}_1");
        let state_hash2 = format!("hash_{suffix}_2");
        insert_event(&pool, &event_id1, &room_id).await;
        insert_event(&pool, &event_id2, &room_id).await;
        let id1 = storage
            .create_state_group(&room_id, &event_id1, &state_hash1, 1000)
            .await
            .unwrap();
        let id2 = storage
            .create_state_group(&room_id, &event_id2, &state_hash2, 2000)
            .await
            .unwrap();

        storage.add_state_group_edge(id2, id1).await.unwrap();
        storage.add_state_group_edge(id2, id1).await.unwrap();

        let prev = storage.get_prev_state_groups(id2).await.unwrap();
        assert_eq!(prev.len(), 1);
    });
}

#[test]
fn test_bind_and_get_event_to_state_group() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = StateGroupStorage::new(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:test");
        let event_id = format!("$event_{suffix}:test");
        let state_hash = format!("hash_{suffix}");

        insert_room(&pool, &room_id).await;
        insert_event(&pool, &event_id, &room_id).await;

        let sg_id = storage
            .create_state_group(&room_id, &event_id, &state_hash, 1000)
            .await
            .unwrap();

        let bind_event_id = format!("$bind_event_{suffix}:test");
        insert_event(&pool, &bind_event_id, &room_id).await;

        storage
            .bind_event_to_state_group(&bind_event_id, sg_id)
            .await
            .unwrap();

        let result = storage.get_state_group_for_event(&bind_event_id).await.unwrap();
        assert_eq!(result, Some(sg_id));
    });
}

#[test]
fn test_get_state_group_for_event_not_found() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = StateGroupStorage::new(&pool);

        let result = storage.get_state_group_for_event("$nonexistent:test").await.unwrap();
        assert!(result.is_none());
    });
}

#[test]
fn test_bind_event_to_state_group_upsert() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = StateGroupStorage::new(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:test");

        insert_room(&pool, &room_id).await;

        let event_id1 = format!("$event1_{suffix}:test");
        let event_id2 = format!("$event2_{suffix}:test");
        let state_hash1 = format!("hash1_{suffix}");
        let state_hash2 = format!("hash2_{suffix}");
        insert_event(&pool, &event_id1, &room_id).await;
        insert_event(&pool, &event_id2, &room_id).await;
        let sg_id1 = storage
            .create_state_group(&room_id, &event_id1, &state_hash1, 1000)
            .await
            .unwrap();
        let sg_id2 = storage
            .create_state_group(&room_id, &event_id2, &state_hash2, 2000)
            .await
            .unwrap();

        let bind_event_id = format!("$bind_event_{suffix}:test");
        insert_event(&pool, &bind_event_id, &room_id).await;

        storage
            .bind_event_to_state_group(&bind_event_id, sg_id1)
            .await
            .unwrap();
        storage
            .bind_event_to_state_group(&bind_event_id, sg_id2)
            .await
            .unwrap();

        let result = storage.get_state_group_for_event(&bind_event_id).await.unwrap();
        assert_eq!(result, Some(sg_id2));
    });
}

#[test]
fn test_batch_bind_events_to_state_group() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = StateGroupStorage::new(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:test");
        let event_id = format!("$event_{suffix}:test");
        let state_hash = format!("hash_{suffix}");

        insert_room(&pool, &room_id).await;
        insert_event(&pool, &event_id, &room_id).await;

        let sg_id = storage
            .create_state_group(&room_id, &event_id, &state_hash, 1000)
            .await
            .unwrap();

        let bind_ids: Vec<String> = (0..3)
            .map(|i| {
                let id = format!("$batch_event_{suffix}_{i}:test");
                id
            })
            .collect();

        for id in &bind_ids {
            insert_event(&pool, id, &room_id).await;
        }

        storage
            .batch_bind_events_to_state_group(&bind_ids, sg_id)
            .await
            .unwrap();

        for id in &bind_ids {
            let result = storage.get_state_group_for_event(id).await.unwrap();
            assert_eq!(result, Some(sg_id));
        }
    });
}

#[test]
fn test_set_and_get_state_entry() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = StateGroupStorage::new(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:test");
        let event_id = format!("$event_{suffix}:test");
        let state_hash = format!("hash_{suffix}");

        insert_room(&pool, &room_id).await;
        insert_event(&pool, &event_id, &room_id).await;

        let sg_id = storage
            .create_state_group(&room_id, &event_id, &state_hash, 1000)
            .await
            .unwrap();

        let state_event_id = format!("$state_event_{suffix}:test");
        insert_event(&pool, &state_event_id, &room_id).await;

        storage
            .set_state_entry(sg_id, "m.room.member", "@user:test", &state_event_id)
            .await
            .unwrap();

        let result = storage
            .get_state_entry(sg_id, "m.room.member", "@user:test")
            .await
            .unwrap();
        assert_eq!(result, Some(state_event_id.clone()));
    });
}

#[test]
fn test_get_state_entry_not_found() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = StateGroupStorage::new(&pool);

        let result = storage
            .get_state_entry(99999, "m.room.member", "@user:test")
            .await
            .unwrap();
        assert!(result.is_none());
    });
}

#[test]
fn test_set_state_entry_upsert() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = StateGroupStorage::new(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:test");
        let event_id = format!("$event_{suffix}:test");
        let state_hash = format!("hash_{suffix}");

        insert_room(&pool, &room_id).await;
        insert_event(&pool, &event_id, &room_id).await;

        let sg_id = storage
            .create_state_group(&room_id, &event_id, &state_hash, 1000)
            .await
            .unwrap();

        let state_event_id1 = format!("$state_event1_{suffix}:test");
        let state_event_id2 = format!("$state_event2_{suffix}:test");
        insert_event(&pool, &state_event_id1, &room_id).await;
        insert_event(&pool, &state_event_id2, &room_id).await;

        storage
            .set_state_entry(sg_id, "m.room.member", "@user:test", &state_event_id1)
            .await
            .unwrap();
        storage
            .set_state_entry(sg_id, "m.room.member", "@user:test", &state_event_id2)
            .await
            .unwrap();

        let result = storage
            .get_state_entry(sg_id, "m.room.member", "@user:test")
            .await
            .unwrap();
        assert_eq!(result, Some(state_event_id2));
    });
}

#[test]
fn test_set_state_entries_batch() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = StateGroupStorage::new(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:test");
        let event_id = format!("$event_{suffix}:test");
        let state_hash = format!("hash_{suffix}");

        insert_room(&pool, &room_id).await;
        insert_event(&pool, &event_id, &room_id).await;

        let sg_id = storage
            .create_state_group(&room_id, &event_id, &state_hash, 1000)
            .await
            .unwrap();

        let entries = vec![
            StateGroupStateEntry {
                event_type: "m.room.member".to_string(),
                state_key: "@user1:test".to_string(),
                event_id: format!("$state1_{suffix}:test"),
            },
            StateGroupStateEntry {
                event_type: "m.room.member".to_string(),
                state_key: "@user2:test".to_string(),
                event_id: format!("$state2_{suffix}:test"),
            },
            StateGroupStateEntry {
                event_type: "m.room.name".to_string(),
                state_key: "".to_string(),
                event_id: format!("$state3_{suffix}:test"),
            },
        ];

        for entry in &entries {
            insert_event(&pool, &entry.event_id, &room_id).await;
        }

        storage.set_state_entries(sg_id, &entries).await.unwrap();

        let state = storage.get_state_at_group(sg_id).await.unwrap();
        assert_eq!(state.len(), 3);
    });
}

#[test]
fn test_get_state_at_group_empty() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = StateGroupStorage::new(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:test");
        let event_id = format!("$event_{suffix}:test");
        let state_hash = format!("hash_{suffix}");

        insert_room(&pool, &room_id).await;
        insert_event(&pool, &event_id, &room_id).await;

        let sg_id = storage
            .create_state_group(&room_id, &event_id, &state_hash, 1000)
            .await
            .unwrap();

        let state = storage.get_state_at_group(sg_id).await.unwrap();
        assert!(state.is_empty());
    });
}

#[test]
fn test_resolve_state_for_group_single() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = StateGroupStorage::new(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:test");
        let event_id = format!("$event_{suffix}:test");
        let state_hash = format!("hash_{suffix}");

        insert_room(&pool, &room_id).await;
        insert_event(&pool, &event_id, &room_id).await;

        let sg_id = storage
            .create_state_group(&room_id, &event_id, &state_hash, 1000)
            .await
            .unwrap();

        let state_event1 = format!("$state_member_{suffix}:test");
        let state_event2 = format!("$state_name_{suffix}:test");
        insert_event(&pool, &state_event1, &room_id).await;
        insert_event(&pool, &state_event2, &room_id).await;

        storage
            .set_state_entry(sg_id, "m.room.member", "@user:test", &state_event1)
            .await
            .unwrap();
        storage
            .set_state_entry(sg_id, "m.room.name", "", &state_event2)
            .await
            .unwrap();

        let resolved = storage.resolve_state_for_group(sg_id).await.unwrap();
        assert_eq!(resolved.len(), 2);
        assert_eq!(
            resolved.get(&("m.room.member".to_string(), "@user:test".to_string())),
            Some(&state_event1)
        );
        assert_eq!(
            resolved.get(&("m.room.name".to_string(), "".to_string())),
            Some(&state_event2)
        );
    });
}

#[test]
fn test_resolve_state_for_group_with_edges() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = StateGroupStorage::new(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:test");

        insert_room(&pool, &room_id).await;

        let event_id1 = format!("$event1_{suffix}:test");
        let event_id2 = format!("$event2_{suffix}:test");
        let state_hash1 = format!("hash1_{suffix}");
        let state_hash2 = format!("hash2_{suffix}");
        insert_event(&pool, &event_id1, &room_id).await;
        insert_event(&pool, &event_id2, &room_id).await;

        let sg_id1 = storage
            .create_state_group(&room_id, &event_id1, &state_hash1, 1000)
            .await
            .unwrap();
        let sg_id2 = storage
            .create_state_group(&room_id, &event_id2, &state_hash2, 2000)
            .await
            .unwrap();

        let state_event_a = format!("$state_a_{suffix}:test");
        let state_event_b = format!("$state_b_{suffix}:test");
        insert_event(&pool, &state_event_a, &room_id).await;
        insert_event(&pool, &state_event_b, &room_id).await;

        storage
            .set_state_entry(sg_id1, "m.room.member", "@alice:test", &state_event_a)
            .await
            .unwrap();
        storage
            .set_state_entry(sg_id2, "m.room.member", "@bob:test", &state_event_b)
            .await
            .unwrap();

        storage.add_state_group_edge(sg_id2, sg_id1).await.unwrap();

        let resolved = storage.resolve_state_for_group(sg_id2).await.unwrap();
        assert_eq!(resolved.len(), 2);
        assert_eq!(
            resolved.get(&("m.room.member".to_string(), "@alice:test".to_string())),
            Some(&state_event_a)
        );
        assert_eq!(
            resolved.get(&("m.room.member".to_string(), "@bob:test".to_string())),
            Some(&state_event_b)
        );
    });
}

#[test]
fn test_resolve_state_child_overrides_parent() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = StateGroupStorage::new(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:test");

        insert_room(&pool, &room_id).await;

        let event_id1 = format!("$event1_{suffix}:test");
        let event_id2 = format!("$event2_{suffix}:test");
        let state_hash1 = format!("hash1_{suffix}");
        let state_hash2 = format!("hash2_{suffix}");
        insert_event(&pool, &event_id1, &room_id).await;
        insert_event(&pool, &event_id2, &room_id).await;

        let sg_id1 = storage
            .create_state_group(&room_id, &event_id1, &state_hash1, 1000)
            .await
            .unwrap();
        let sg_id2 = storage
            .create_state_group(&room_id, &event_id2, &state_hash2, 2000)
            .await
            .unwrap();

        let old_event = format!("$old_name_{suffix}:test");
        let new_event = format!("$new_name_{suffix}:test");
        insert_event(&pool, &old_event, &room_id).await;
        insert_event(&pool, &new_event, &room_id).await;

        storage
            .set_state_entry(sg_id1, "m.room.name", "", &old_event)
            .await
            .unwrap();
        storage
            .set_state_entry(sg_id2, "m.room.name", "", &new_event)
            .await
            .unwrap();

        storage.add_state_group_edge(sg_id2, sg_id1).await.unwrap();

        let resolved = storage.resolve_state_for_group(sg_id2).await.unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(
            resolved.get(&("m.room.name".to_string(), "".to_string())),
            Some(&new_event)
        );
    });
}

#[test]
fn test_get_prev_state_groups_empty() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = StateGroupStorage::new(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_{suffix}:test");
        let event_id = format!("$event_{suffix}:test");
        let state_hash = format!("hash_{suffix}");

        insert_room(&pool, &room_id).await;
        insert_event(&pool, &event_id, &room_id).await;

        let sg_id = storage
            .create_state_group(&room_id, &event_id, &state_hash, 1000)
            .await
            .unwrap();

        let prev = storage.get_prev_state_groups(sg_id).await.unwrap();
        assert!(prev.is_empty());

        let next = storage.get_next_state_groups(sg_id).await.unwrap();
        assert!(next.is_empty());
    });
}
