#![cfg(test)]

use sqlx::{Pool, Postgres};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use synapse_rust::storage::beacon::{
    BeaconStorage, CreateBeaconInfoParams, CreateBeaconLocationParams,
};
use tokio::runtime::Runtime;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

async fn setup_test_database() -> Option<Arc<Pool<Postgres>>> {
    let pool = match synapse_rust::test_utils::prepare_empty_isolated_test_pool().await {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!(
                "Skipping beacon storage tests because test database is unavailable: {error}"
            );
            return None;
        }
    };

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS beacon_info (
            id BIGSERIAL PRIMARY KEY,
            room_id TEXT NOT NULL,
            event_id TEXT NOT NULL UNIQUE,
            state_key TEXT NOT NULL,
            sender TEXT NOT NULL,
            description TEXT,
            timeout BIGINT NOT NULL,
            is_live BOOLEAN NOT NULL DEFAULT TRUE,
            asset_type TEXT NOT NULL,
            created_ts BIGINT NOT NULL,
            updated_ts BIGINT NOT NULL,
            expires_at BIGINT
        )
        "#,
    )
    .execute(&*pool)
    .await
    .expect("Failed to create beacon_info table");

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS beacon_locations (
            id BIGSERIAL PRIMARY KEY,
            room_id TEXT NOT NULL,
            event_id TEXT NOT NULL,
            beacon_info_id TEXT NOT NULL,
            sender TEXT NOT NULL,
            uri TEXT NOT NULL,
            description TEXT,
            timestamp BIGINT NOT NULL,
            accuracy BIGINT,
            created_ts BIGINT NOT NULL
        )
        "#,
    )
    .execute(&*pool)
    .await
    .expect("Failed to create beacon_locations table");

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS room_memberships (
            id BIGSERIAL PRIMARY KEY,
            room_id TEXT NOT NULL,
            user_id TEXT NOT NULL,
            membership TEXT NOT NULL,
            joined_ts BIGINT,
            invited_ts BIGINT,
            left_ts BIGINT,
            banned_ts BIGINT,
            sender TEXT,
            reason TEXT,
            event_id TEXT,
            event_type TEXT,
            display_name TEXT,
            avatar_url TEXT,
            is_banned BOOLEAN DEFAULT FALSE,
            invite_token TEXT,
            updated_ts BIGINT,
            join_reason TEXT,
            banned_by TEXT,
            ban_reason TEXT,
            UNIQUE (room_id, user_id)
        )
        "#,
    )
    .execute(&*pool)
    .await
    .expect("Failed to create room_memberships table");

    Some(pool)
}

fn create_storage(pool: &Arc<Pool<Postgres>>) -> BeaconStorage {
    BeaconStorage::new(pool.clone())
}

fn make_beacon_info_params(suffix: u64) -> CreateBeaconInfoParams {
    let now = chrono::Utc::now().timestamp_millis();
    CreateBeaconInfoParams {
        room_id: format!("!beacon_room_{suffix}:localhost"),
        event_id: format!("$beacon_info_{suffix}"),
        state_key: format!("@beacon_user_{suffix}:localhost"),
        sender: format!("@beacon_user_{suffix}:localhost"),
        description: Some(format!("Test beacon {suffix}")),
        timeout: 3_600_000,
        is_live: true,
        asset_type: "m.self".to_string(),
        created_ts: now,
    }
}

fn make_beacon_location_params(suffix: u64, beacon_info_id: &str) -> CreateBeaconLocationParams {
    let now = chrono::Utc::now().timestamp_millis();
    CreateBeaconLocationParams {
        room_id: format!("!beacon_room_{suffix}:localhost"),
        event_id: format!("$beacon_loc_{suffix}"),
        beacon_info_id: beacon_info_id.to_string(),
        sender: format!("@beacon_user_{suffix}:localhost"),
        uri: "geo:51.5008,0.1247;u=35".to_string(),
        description: Some("London".to_string()),
        timestamp: now,
        accuracy: Some(35),
        created_ts: now,
    }
}

#[test]
fn test_create_beacon_info() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let params = make_beacon_info_params(suffix);

        let beacon = storage.create_beacon_info(params).await.unwrap();

        assert_eq!(beacon.room_id, format!("!beacon_room_{suffix}:localhost"));
        assert_eq!(beacon.event_id, format!("$beacon_info_{suffix}"));
        assert_eq!(beacon.state_key, format!("@beacon_user_{suffix}:localhost"));
        assert_eq!(beacon.sender, format!("@beacon_user_{suffix}:localhost"));
        assert_eq!(beacon.description, Some(format!("Test beacon {suffix}")));
        assert_eq!(beacon.timeout, 3_600_000);
        assert!(beacon.is_live);
        assert_eq!(beacon.asset_type, "m.self");
        assert!(beacon.expires_at.is_some());
    });
}

#[test]
fn test_create_beacon_info_zero_timeout() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let now = chrono::Utc::now().timestamp_millis();
        let params = CreateBeaconInfoParams {
            room_id: format!("!zero_room_{suffix}:localhost"),
            event_id: format!("$zero_info_{suffix}"),
            state_key: format!("@zero_user_{suffix}:localhost"),
            sender: format!("@zero_user_{suffix}:localhost"),
            description: None,
            timeout: 0,
            is_live: true,
            asset_type: "m.self".to_string(),
            created_ts: now,
        };

        let beacon = storage.create_beacon_info(params).await.unwrap();

        assert_eq!(beacon.timeout, 0);
        assert!(beacon.expires_at.is_none());
    });
}

#[test]
fn test_create_beacon_info_not_live() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let now = chrono::Utc::now().timestamp_millis();
        let params = CreateBeaconInfoParams {
            room_id: format!("!notlive_room_{suffix}:localhost"),
            event_id: format!("$notlive_info_{suffix}"),
            state_key: format!("@notlive_user_{suffix}:localhost"),
            sender: format!("@notlive_user_{suffix}:localhost"),
            description: None,
            timeout: 3_600_000,
            is_live: false,
            asset_type: "m.self".to_string(),
            created_ts: now,
        };

        let beacon = storage.create_beacon_info(params).await.unwrap();

        assert!(!beacon.is_live);
    });
}

#[test]
fn test_get_beacon_info_found() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let params = make_beacon_info_params(suffix);
        let room_id = params.room_id.clone();
        let event_id = params.event_id.clone();

        storage.create_beacon_info(params).await.unwrap();

        let result = storage.get_beacon_info(&room_id, &event_id).await.unwrap();
        assert!(result.is_some());
        let beacon = result.unwrap();
        assert_eq!(beacon.room_id, room_id);
        assert_eq!(beacon.event_id, event_id);
    });
}

#[test]
fn test_get_beacon_info_not_found() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);

        let result = storage
            .get_beacon_info("!nonexistent:localhost", "$nonexistent")
            .await
            .unwrap();
        assert!(result.is_none());
    });
}

#[test]
fn test_get_beacon_info_by_state_key() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let state_key = format!("@state_user_{suffix}:localhost");
        let room_id = format!("!state_room_{suffix}:localhost");
        let now = chrono::Utc::now().timestamp_millis();

        let params1 = CreateBeaconInfoParams {
            room_id: room_id.clone(),
            event_id: format!("$state_info_1_{suffix}"),
            state_key: state_key.clone(),
            sender: state_key.clone(),
            description: None,
            timeout: 3_600_000,
            is_live: true,
            asset_type: "m.self".to_string(),
            created_ts: now - 1000,
        };
        let params2 = CreateBeaconInfoParams {
            room_id: room_id.clone(),
            event_id: format!("$state_info_2_{suffix}"),
            state_key: state_key.clone(),
            sender: state_key.clone(),
            description: None,
            timeout: 3_600_000,
            is_live: true,
            asset_type: "m.self".to_string(),
            created_ts: now,
        };

        storage.create_beacon_info(params1).await.unwrap();
        storage.create_beacon_info(params2).await.unwrap();

        let results = storage
            .get_beacon_info_by_state_key(&room_id, &state_key)
            .await
            .unwrap();
        assert_eq!(results.len(), 2);
        assert!(results[0].created_ts >= results[1].created_ts);
    });
}

#[test]
fn test_get_beacon_info_by_state_key_empty() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);

        let results = storage
            .get_beacon_info_by_state_key("!empty:localhost", "@nobody:localhost")
            .await
            .unwrap();
        assert!(results.is_empty());
    });
}

#[test]
fn test_get_active_beacons() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let room_id = format!("!active_room_{suffix}:localhost");
        let now = chrono::Utc::now().timestamp_millis();

        let live_params = CreateBeaconInfoParams {
            room_id: room_id.clone(),
            event_id: format!("$live_info_{suffix}"),
            state_key: format!("@live_user_{suffix}:localhost"),
            sender: format!("@live_user_{suffix}:localhost"),
            description: None,
            timeout: 3_600_000,
            is_live: true,
            asset_type: "m.self".to_string(),
            created_ts: now,
        };
        let not_live_params = CreateBeaconInfoParams {
            room_id: room_id.clone(),
            event_id: format!("$notlive_info_{suffix}"),
            state_key: format!("@notlive_user_{suffix}:localhost"),
            sender: format!("@notlive_user_{suffix}:localhost"),
            description: None,
            timeout: 3_600_000,
            is_live: false,
            asset_type: "m.self".to_string(),
            created_ts: now,
        };

        storage.create_beacon_info(live_params).await.unwrap();
        storage.create_beacon_info(not_live_params).await.unwrap();

        let active = storage.get_active_beacons(&room_id).await.unwrap();
        assert_eq!(active.len(), 1);
        assert!(active[0].is_live);
    });
}

#[test]
fn test_get_active_beacons_empty_room() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);

        let active = storage
            .get_active_beacons("!empty_active:localhost")
            .await
            .unwrap();
        assert!(active.is_empty());
    });
}

#[test]
fn test_deactivate_beacons_by_state_key() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let room_id = format!("!deact_room_{suffix}:localhost");
        let state_key = format!("@deact_user_{suffix}:localhost");
        let now = chrono::Utc::now().timestamp_millis();

        let params1 = CreateBeaconInfoParams {
            room_id: room_id.clone(),
            event_id: format!("$deact_info_1_{suffix}"),
            state_key: state_key.clone(),
            sender: state_key.clone(),
            description: None,
            timeout: 3_600_000,
            is_live: true,
            asset_type: "m.self".to_string(),
            created_ts: now,
        };
        let params2 = CreateBeaconInfoParams {
            room_id: room_id.clone(),
            event_id: format!("$deact_info_2_{suffix}"),
            state_key: state_key.clone(),
            sender: state_key.clone(),
            description: None,
            timeout: 3_600_000,
            is_live: true,
            asset_type: "m.self".to_string(),
            created_ts: now + 1000,
        };

        storage.create_beacon_info(params1).await.unwrap();
        storage.create_beacon_info(params2).await.unwrap();

        let deactivated = storage
            .deactivate_beacons_by_state_key(&room_id, &state_key)
            .await
            .unwrap();
        assert_eq!(deactivated, 2);

        let active = storage.get_active_beacons(&room_id).await.unwrap();
        assert!(active.is_empty());
    });
}

#[test]
fn test_deactivate_beacons_no_match() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);

        let deactivated = storage
            .deactivate_beacons_by_state_key("!nomatch:localhost", "@nobody:localhost")
            .await
            .unwrap();
        assert_eq!(deactivated, 0);
    });
}

#[test]
fn test_update_beacon_info_set_not_live() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let params = make_beacon_info_params(suffix);
        let room_id = params.room_id.clone();
        let event_id = params.event_id.clone();

        storage.create_beacon_info(params).await.unwrap();

        let updated = storage
            .update_beacon_info(&room_id, &event_id, false, None)
            .await
            .unwrap();
        assert!(updated.is_some());
        assert!(!updated.unwrap().is_live);
    });
}

#[test]
fn test_update_beacon_info_with_timeout() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let params = make_beacon_info_params(suffix);
        let room_id = params.room_id.clone();
        let event_id = params.event_id.clone();

        storage.create_beacon_info(params).await.unwrap();

        let updated = storage
            .update_beacon_info(&room_id, &event_id, true, Some(7_200_000))
            .await
            .unwrap();
        assert!(updated.is_some());
        let beacon = updated.unwrap();
        assert!(beacon.is_live);
        assert_eq!(beacon.timeout, 7_200_000);
    });
}

#[test]
fn test_update_beacon_info_not_found() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);

        let result = storage
            .update_beacon_info("!nonexistent:localhost", "$nonexistent", false, None)
            .await
            .unwrap();
        assert!(result.is_none());
    });
}

#[test]
fn test_delete_beacon_info() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let params = make_beacon_info_params(suffix);
        let room_id = params.room_id.clone();
        let event_id = params.event_id.clone();

        storage.create_beacon_info(params).await.unwrap();

        let deleted = storage.delete_beacon_info(&room_id, &event_id).await.unwrap();
        assert!(deleted);

        let result = storage.get_beacon_info(&room_id, &event_id).await.unwrap();
        assert!(result.is_none());
    });
}

#[test]
fn test_delete_beacon_info_not_found() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);

        let deleted = storage
            .delete_beacon_info("!nonexistent:localhost", "$nonexistent")
            .await
            .unwrap();
        assert!(!deleted);
    });
}

#[test]
fn test_create_beacon_location() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();

        let info_params = make_beacon_info_params(suffix);
        let beacon_info = storage.create_beacon_info(info_params).await.unwrap();

        let loc_params = make_beacon_location_params(suffix, &beacon_info.event_id);
        let location = storage.create_beacon_location(loc_params).await.unwrap();

        assert_eq!(location.beacon_info_id, beacon_info.event_id);
        assert_eq!(location.uri, "geo:51.5008,0.1247;u=35");
        assert_eq!(location.description, Some("London".to_string()));
        assert_eq!(location.accuracy, Some(35));
    });
}

#[test]
fn test_create_beacon_location_zero_timestamp() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();

        let info_params = make_beacon_info_params(suffix);
        let beacon_info = storage.create_beacon_info(info_params).await.unwrap();

        let now = chrono::Utc::now().timestamp_millis();
        let loc_params = CreateBeaconLocationParams {
            room_id: format!("!zero_loc_room_{suffix}:localhost"),
            event_id: format!("$zero_loc_{suffix}"),
            beacon_info_id: beacon_info.event_id.clone(),
            sender: format!("@zero_loc_user_{suffix}:localhost"),
            uri: "geo:0,0".to_string(),
            description: None,
            timestamp: 0,
            accuracy: None,
            created_ts: now,
        };

        let location = storage.create_beacon_location(loc_params).await.unwrap();
        assert_eq!(location.timestamp, 0);
        assert!(location.accuracy.is_none());
    });
}

#[test]
fn test_get_beacon_locations() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();

        let info_params = make_beacon_info_params(suffix);
        let beacon_info = storage.create_beacon_info(info_params).await.unwrap();

        let now = chrono::Utc::now().timestamp_millis();
        for i in 0..3 {
            let loc_params = CreateBeaconLocationParams {
                room_id: format!("!loc_room_{suffix}:localhost"),
                event_id: format!("$loc_{suffix}_{i}"),
                beacon_info_id: beacon_info.event_id.clone(),
                sender: format!("@loc_user_{suffix}:localhost"),
                uri: format!("geo:51.{i},0.{i}"),
                description: None,
                timestamp: now + i * 1000,
                accuracy: Some(10 + i),
                created_ts: now + i * 1000,
            };
            storage.create_beacon_location(loc_params).await.unwrap();
        }

        let locations = storage
            .get_beacon_locations(&beacon_info.event_id, None)
            .await
            .unwrap();
        assert_eq!(locations.len(), 3);
        assert!(locations[0].timestamp >= locations[1].timestamp);
    });
}

#[test]
fn test_get_beacon_locations_with_limit() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();

        let info_params = make_beacon_info_params(suffix);
        let beacon_info = storage.create_beacon_info(info_params).await.unwrap();

        let now = chrono::Utc::now().timestamp_millis();
        for i in 0..5 {
            let loc_params = CreateBeaconLocationParams {
                room_id: format!("!limit_room_{suffix}:localhost"),
                event_id: format!("$limit_loc_{suffix}_{i}"),
                beacon_info_id: beacon_info.event_id.clone(),
                sender: format!("@limit_user_{suffix}:localhost"),
                uri: format!("geo:51.{i},0.{i}"),
                description: None,
                timestamp: now + i * 1000,
                accuracy: None,
                created_ts: now + i * 1000,
            };
            storage.create_beacon_location(loc_params).await.unwrap();
        }

        let locations = storage
            .get_beacon_locations(&beacon_info.event_id, Some(2))
            .await
            .unwrap();
        assert_eq!(locations.len(), 2);
    });
}

#[test]
fn test_get_beacon_locations_empty() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);

        let locations = storage
            .get_beacon_locations("$nonexistent", None)
            .await
            .unwrap();
        assert!(locations.is_empty());
    });
}

#[test]
fn test_get_latest_location() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();

        let info_params = make_beacon_info_params(suffix);
        let beacon_info = storage.create_beacon_info(info_params).await.unwrap();

        let now = chrono::Utc::now().timestamp_millis();
        for i in 0..3 {
            let loc_params = CreateBeaconLocationParams {
                room_id: format!("!latest_room_{suffix}:localhost"),
                event_id: format!("$latest_loc_{suffix}_{i}"),
                beacon_info_id: beacon_info.event_id.clone(),
                sender: format!("@latest_user_{suffix}:localhost"),
                uri: format!("geo:51.{i},0.{i}"),
                description: None,
                timestamp: now + i * 1000,
                accuracy: None,
                created_ts: now + i * 1000,
            };
            storage.create_beacon_location(loc_params).await.unwrap();
        }

        let latest = storage
            .get_latest_location(&beacon_info.event_id)
            .await
            .unwrap();
        assert!(latest.is_some());
        assert_eq!(latest.unwrap().timestamp, now + 2000);
    });
}

#[test]
fn test_get_latest_location_none() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);

        let latest = storage.get_latest_location("$nonexistent").await.unwrap();
        assert!(latest.is_none());
    });
}

#[test]
fn test_count_locations_in_room_since() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let room_id = format!("!count_room_{suffix}:localhost");

        let info_params = CreateBeaconInfoParams {
            room_id: room_id.clone(),
            event_id: format!("$count_info_{suffix}"),
            state_key: format!("@count_user_{suffix}:localhost"),
            sender: format!("@count_user_{suffix}:localhost"),
            description: None,
            timeout: 3_600_000,
            is_live: true,
            asset_type: "m.self".to_string(),
            created_ts: chrono::Utc::now().timestamp_millis(),
        };
        let beacon_info = storage.create_beacon_info(info_params).await.unwrap();

        let now = chrono::Utc::now().timestamp_millis();
        for i in 0..3 {
            let loc_params = CreateBeaconLocationParams {
                room_id: room_id.clone(),
                event_id: format!("$count_loc_{suffix}_{i}"),
                beacon_info_id: beacon_info.event_id.clone(),
                sender: format!("@count_user_{suffix}:localhost"),
                uri: format!("geo:51.{i},0.{i}"),
                description: None,
                timestamp: now + i * 1000,
                accuracy: None,
                created_ts: now + i * 1000,
            };
            storage.create_beacon_location(loc_params).await.unwrap();
        }

        let count = storage
            .count_locations_in_room_since(&room_id, now)
            .await
            .unwrap();
        assert_eq!(count, 3);

        let count_after = storage
            .count_locations_in_room_since(&room_id, now + 3000)
            .await
            .unwrap();
        assert_eq!(count_after, 0);
    });
}

#[test]
fn test_count_locations_in_room_by_sender_since() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let room_id = format!("!sender_count_room_{suffix}:localhost");
        let sender = format!("@sender_count_user_{suffix}:localhost");
        let other_sender = format!("@other_sender_{suffix}:localhost");

        let info_params = CreateBeaconInfoParams {
            room_id: room_id.clone(),
            event_id: format!("$sender_count_info_{suffix}"),
            state_key: sender.clone(),
            sender: sender.clone(),
            description: None,
            timeout: 3_600_000,
            is_live: true,
            asset_type: "m.self".to_string(),
            created_ts: chrono::Utc::now().timestamp_millis(),
        };
        let beacon_info = storage.create_beacon_info(info_params).await.unwrap();

        let now = chrono::Utc::now().timestamp_millis();
        let loc1 = CreateBeaconLocationParams {
            room_id: room_id.clone(),
            event_id: format!("$sender_loc_1_{suffix}"),
            beacon_info_id: beacon_info.event_id.clone(),
            sender: sender.clone(),
            uri: "geo:51.5,0.1".to_string(),
            description: None,
            timestamp: now,
            accuracy: None,
            created_ts: now,
        };
        let loc2 = CreateBeaconLocationParams {
            room_id: room_id.clone(),
            event_id: format!("$sender_loc_2_{suffix}"),
            beacon_info_id: beacon_info.event_id.clone(),
            sender: other_sender.clone(),
            uri: "geo:51.6,0.2".to_string(),
            description: None,
            timestamp: now + 1000,
            accuracy: None,
            created_ts: now + 1000,
        };
        storage.create_beacon_location(loc1).await.unwrap();
        storage.create_beacon_location(loc2).await.unwrap();

        let count_sender = storage
            .count_locations_in_room_by_sender_since(&room_id, &sender, now)
            .await
            .unwrap();
        assert_eq!(count_sender, 1);

        let count_other = storage
            .count_locations_in_room_by_sender_since(&room_id, &other_sender, now)
            .await
            .unwrap();
        assert_eq!(count_other, 1);

        let count_all = storage
            .count_locations_in_room_since(&room_id, now)
            .await
            .unwrap();
        assert_eq!(count_all, 2);
    });
}

#[test]
fn test_get_joined_member_count() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let room_id = format!("!member_count_room_{suffix}:localhost");
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query(
            "INSERT INTO room_memberships (room_id, user_id, membership, joined_ts) VALUES ($1, $2, 'join', $3)",
        )
        .bind(&room_id)
        .bind(format!("@member1_{suffix}:localhost"))
        .bind(now)
        .execute(&*pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO room_memberships (room_id, user_id, membership, joined_ts) VALUES ($1, $2, 'join', $3)",
        )
        .bind(&room_id)
        .bind(format!("@member2_{suffix}:localhost"))
        .bind(now)
        .execute(&*pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO room_memberships (room_id, user_id, membership, invited_ts) VALUES ($1, $2, 'invite', $3)",
        )
        .bind(&room_id)
        .bind(format!("@invited_{suffix}:localhost"))
        .bind(now)
        .execute(&*pool)
        .await
        .unwrap();

        let count = storage.get_joined_member_count(&room_id).await.unwrap();
        assert_eq!(count, 2);
    });
}

#[test]
fn test_get_joined_member_count_empty() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);

        let count = storage
            .get_joined_member_count("!empty_member_room:localhost")
            .await
            .unwrap();
        assert_eq!(count, 0);
    });
}

#[test]
fn test_get_beacon_with_locations() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();

        let info_params = make_beacon_info_params(suffix);
        let room_id = info_params.room_id.clone();
        let event_id = info_params.event_id.clone();
        let beacon_info = storage.create_beacon_info(info_params).await.unwrap();

        let now = chrono::Utc::now().timestamp_millis();
        for i in 0..2 {
            let loc_params = CreateBeaconLocationParams {
                room_id: room_id.clone(),
                event_id: format!("$withloc_{suffix}_{i}"),
                beacon_info_id: beacon_info.event_id.clone(),
                sender: format!("@withloc_user_{suffix}:localhost"),
                uri: format!("geo:51.{i},0.{i}"),
                description: None,
                timestamp: now + i * 1000,
                accuracy: None,
                created_ts: now + i * 1000,
            };
            storage.create_beacon_location(loc_params).await.unwrap();
        }

        let result = storage
            .get_beacon_with_locations(&room_id, &event_id)
            .await
            .unwrap();
        assert!(result.is_some());
        let with_locs = result.unwrap();
        assert_eq!(with_locs.beacon_info.event_id, event_id);
        assert_eq!(with_locs.locations.len(), 2);
    });
}

#[test]
fn test_get_beacon_with_locations_not_found() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);

        let result = storage
            .get_beacon_with_locations("!nonexistent:localhost", "$nonexistent")
            .await
            .unwrap();
        assert!(result.is_none());
    });
}

#[test]
fn test_cleanup_expired_beacons() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let room_id = format!("!cleanup_room_{suffix}:localhost");
        let now = chrono::Utc::now().timestamp_millis();

        let expired_params = CreateBeaconInfoParams {
            room_id: room_id.clone(),
            event_id: format!("$expired_info_{suffix}"),
            state_key: format!("@expired_user_{suffix}:localhost"),
            sender: format!("@expired_user_{suffix}:localhost"),
            description: None,
            timeout: 1000,
            is_live: true,
            asset_type: "m.self".to_string(),
            created_ts: now - 10_000,
        };
        let valid_params = CreateBeaconInfoParams {
            room_id: room_id.clone(),
            event_id: format!("$valid_info_{suffix}"),
            state_key: format!("@valid_user_{suffix}:localhost"),
            sender: format!("@valid_user_{suffix}:localhost"),
            description: None,
            timeout: 3_600_000,
            is_live: true,
            asset_type: "m.self".to_string(),
            created_ts: now,
        };

        let expired_beacon = storage.create_beacon_info(expired_params).await.unwrap();
        storage.create_beacon_info(valid_params).await.unwrap();

        sqlx::query(
            "UPDATE beacon_info SET expires_at = $1 WHERE event_id = $2",
        )
        .bind(now - 5000)
        .bind(&expired_beacon.event_id)
        .execute(&*pool)
        .await
        .unwrap();

        let cleaned = storage.cleanup_expired_beacons().await.unwrap();
        assert_eq!(cleaned, 1);

        let remaining = storage
            .get_beacon_info(&room_id, &format!("$valid_info_{suffix}"))
            .await
            .unwrap();
        assert!(remaining.is_some());
    });
}

#[test]
fn test_cleanup_expired_beacons_none() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);

        let cleaned = storage.cleanup_expired_beacons().await.unwrap();
        assert_eq!(cleaned, 0);
    });
}

#[test]
fn test_get_room_beacons_include_expired() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let room_id = format!("!room_beacons_room_{suffix}:localhost");
        let now = chrono::Utc::now().timestamp_millis();

        let params1 = CreateBeaconInfoParams {
            room_id: room_id.clone(),
            event_id: format!("$rb_info_1_{suffix}"),
            state_key: format!("@rb_user_1_{suffix}:localhost"),
            sender: format!("@rb_user_1_{suffix}:localhost"),
            description: None,
            timeout: 3_600_000,
            is_live: true,
            asset_type: "m.self".to_string(),
            created_ts: now,
        };
        let params2 = CreateBeaconInfoParams {
            room_id: room_id.clone(),
            event_id: format!("$rb_info_2_{suffix}"),
            state_key: format!("@rb_user_2_{suffix}:localhost"),
            sender: format!("@rb_user_2_{suffix}:localhost"),
            description: None,
            timeout: 3_600_000,
            is_live: true,
            asset_type: "m.self".to_string(),
            created_ts: now + 1000,
        };

        storage.create_beacon_info(params1).await.unwrap();
        storage.create_beacon_info(params2).await.unwrap();

        let beacons = storage
            .get_room_beacons(&room_id, true)
            .await
            .unwrap();
        assert_eq!(beacons.len(), 2);
    });
}

#[test]
fn test_get_room_beacons_exclude_expired() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let room_id = format!("!exclude_room_{suffix}:localhost");
        let now = chrono::Utc::now().timestamp_millis();

        let valid_params = CreateBeaconInfoParams {
            room_id: room_id.clone(),
            event_id: format!("$valid_rb_{suffix}"),
            state_key: format!("@valid_rb_user_{suffix}:localhost"),
            sender: format!("@valid_rb_user_{suffix}:localhost"),
            description: None,
            timeout: 3_600_000,
            is_live: true,
            asset_type: "m.self".to_string(),
            created_ts: now,
        };

        storage.create_beacon_info(valid_params).await.unwrap();

        let beacons = storage
            .get_room_beacons(&room_id, false)
            .await
            .unwrap();
        assert_eq!(beacons.len(), 1);
    });
}

#[test]
fn test_get_room_beacons_empty() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);

        let beacons = storage
            .get_room_beacons("!empty_beacons:localhost", true)
            .await
            .unwrap();
        assert!(beacons.is_empty());
    });
}

#[test]
fn test_beacon_info_serialization() {
    let beacon = synapse_rust::storage::beacon::BeaconInfo {
        id: 1,
        room_id: "!room:example.com".to_string(),
        event_id: "$event1".to_string(),
        state_key: "@alice:example.com".to_string(),
        sender: "@alice:example.com".to_string(),
        description: Some("Test".to_string()),
        timeout: 3_600_000,
        is_live: true,
        asset_type: "m.self".to_string(),
        created_ts: 1234567890000,
        updated_ts: Some(1234567890000),
        expires_at: Some(1234571490000),
    };

    let json = serde_json::to_string(&beacon).unwrap();
    let deserialized: synapse_rust::storage::beacon::BeaconInfo =
        serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.room_id, beacon.room_id);
    assert_eq!(deserialized.timeout, beacon.timeout);
    assert_eq!(deserialized.is_live, beacon.is_live);
    assert_eq!(deserialized.expires_at, beacon.expires_at);
}

#[test]
fn test_beacon_location_serialization() {
    let location = synapse_rust::storage::beacon::BeaconLocation {
        id: 1,
        room_id: "!room:example.com".to_string(),
        event_id: "$loc1".to_string(),
        beacon_info_id: "$beacon_info_1".to_string(),
        sender: "@alice:example.com".to_string(),
        uri: "geo:51.5008,0.1247;u=35".to_string(),
        description: Some("London".to_string()),
        timestamp: 1234567890000,
        accuracy: Some(35),
        created_ts: 1234567890000,
    };

    let json = serde_json::to_string(&location).unwrap();
    let deserialized: synapse_rust::storage::beacon::BeaconLocation =
        serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.uri, location.uri);
    assert_eq!(deserialized.accuracy, location.accuracy);
    assert_eq!(deserialized.beacon_info_id, location.beacon_info_id);
}

#[test]
fn test_create_beacon_info_params_serialization() {
    let params = CreateBeaconInfoParams {
        room_id: "!room:example.com".to_string(),
        event_id: "$event1".to_string(),
        state_key: "@alice:example.com".to_string(),
        sender: "@alice:example.com".to_string(),
        description: Some("Test".to_string()),
        timeout: 3_600_000,
        is_live: true,
        asset_type: "m.self".to_string(),
        created_ts: 1234567890000,
    };

    let json = serde_json::to_string(&params).unwrap();
    let deserialized: CreateBeaconInfoParams = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.room_id, params.room_id);
    assert_eq!(deserialized.asset_type, params.asset_type);
}

#[test]
fn test_create_beacon_location_params_serialization() {
    let params = CreateBeaconLocationParams {
        room_id: "!room:example.com".to_string(),
        event_id: "$loc1".to_string(),
        beacon_info_id: "$beacon_info_1".to_string(),
        sender: "@alice:example.com".to_string(),
        uri: "geo:51.5008,0.1247;u=35".to_string(),
        description: Some("London".to_string()),
        timestamp: 1234567890000,
        accuracy: Some(35),
        created_ts: 1234567890000,
    };

    let json = serde_json::to_string(&params).unwrap();
    let deserialized: CreateBeaconLocationParams = serde_json::from_str(&json).unwrap();
    assert!(deserialized.uri.starts_with("geo:"));
    assert!(deserialized.accuracy.is_some());
}

#[test]
fn test_beacon_info_with_locations_structure() {
    let beacon_info = synapse_rust::storage::beacon::BeaconInfo {
        id: 1,
        room_id: "!room:example.com".to_string(),
        event_id: "$event1".to_string(),
        state_key: "@alice:example.com".to_string(),
        sender: "@alice:example.com".to_string(),
        description: None,
        timeout: 3_600_000,
        is_live: true,
        asset_type: "m.self".to_string(),
        created_ts: 1234567890000,
        updated_ts: Some(1234567890000),
        expires_at: None,
    };
    let locations = vec![
        synapse_rust::storage::beacon::BeaconLocation {
            id: 1,
            room_id: "!room:example.com".to_string(),
            event_id: "$loc1".to_string(),
            beacon_info_id: "$event1".to_string(),
            sender: "@alice:example.com".to_string(),
            uri: "geo:51.5,0.1".to_string(),
            description: None,
            timestamp: 1234567890000,
            accuracy: None,
            created_ts: 1234567890000,
        },
    ];

    let with_locs = synapse_rust::storage::beacon::BeaconInfoWithLocations {
        beacon_info: beacon_info.clone(),
        locations: locations.clone(),
    };

    assert_eq!(with_locs.beacon_info.event_id, "$event1");
    assert_eq!(with_locs.locations.len(), 1);
    assert_eq!(with_locs.locations[0].beacon_info_id, "$event1");
}

#[test]
fn test_beacon_storage_new() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let result = storage
            .get_beacon_info("!test:localhost", "$test")
            .await
            .unwrap();
        assert!(result.is_none());
    });
}

#[test]
fn test_full_beacon_lifecycle() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let storage = create_storage(&pool);
        let suffix = unique_id();
        let room_id = format!("!lifecycle_room_{suffix}:localhost");
        let state_key = format!("@lifecycle_user_{suffix}:localhost");
        let now = chrono::Utc::now().timestamp_millis();

        let params = CreateBeaconInfoParams {
            room_id: room_id.clone(),
            event_id: format!("$lifecycle_info_{suffix}"),
            state_key: state_key.clone(),
            sender: state_key.clone(),
            description: Some("Lifecycle test".to_string()),
            timeout: 3_600_000,
            is_live: true,
            asset_type: "m.self".to_string(),
            created_ts: now,
        };
        let event_id = params.event_id.clone();

        let beacon = storage.create_beacon_info(params).await.unwrap();
        assert!(beacon.is_live);

        let fetched = storage
            .get_beacon_info(&room_id, &event_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(fetched.event_id, event_id);

        let active = storage.get_active_beacons(&room_id).await.unwrap();
        assert_eq!(active.len(), 1);

        let loc_params = CreateBeaconLocationParams {
            room_id: room_id.clone(),
            event_id: format!("$lifecycle_loc_{suffix}"),
            beacon_info_id: event_id.clone(),
            sender: state_key.clone(),
            uri: "geo:51.5,0.1".to_string(),
            description: None,
            timestamp: now + 1000,
            accuracy: Some(10),
            created_ts: now + 1000,
        };
        let location = storage.create_beacon_location(loc_params).await.unwrap();
        assert_eq!(location.beacon_info_id, event_id);

        let latest = storage
            .get_latest_location(&event_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(latest.uri, "geo:51.5,0.1");

        let with_locs = storage
            .get_beacon_with_locations(&room_id, &event_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(with_locs.locations.len(), 1);

        storage
            .update_beacon_info(&room_id, &event_id, false, None)
            .await
            .unwrap();
        let updated = storage
            .get_beacon_info(&room_id, &event_id)
            .await
            .unwrap()
            .unwrap();
        assert!(!updated.is_live);

        let active_after = storage.get_active_beacons(&room_id).await.unwrap();
        assert!(active_after.is_empty());

        let deleted = storage.delete_beacon_info(&room_id, &event_id).await.unwrap();
        assert!(deleted);

        let gone = storage
            .get_beacon_info(&room_id, &event_id)
            .await
            .unwrap();
        assert!(gone.is_none());
    });
}
