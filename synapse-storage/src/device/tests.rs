use super::*;

#[test]
fn test_device_struct() {
    let device = Device {
        device_id: "DEVICE123".to_string(),
        user_id: "@alice:example.com".to_string(),
        display_name: Some("iPhone 15".to_string()),
        device_key: Some(serde_json::json!({"key": "value"})),
        last_seen_ts: Some(1234567890000),
        last_seen_ip: Some("192.168.1.1".to_string()),
        created_ts: 1234567890000,
        first_seen_ts: 1234567890000,
        user_agent: None,
        appservice_id: None,
        ignored_user_list: None,
    };

    assert_eq!(device.device_id, "DEVICE123");
    assert_eq!(device.user_id, "@alice:example.com");
    assert!(device.display_name.is_some());
}

#[test]
fn test_device_with_minimal_fields() {
    let device = Device {
        device_id: "MINIMAL".to_string(),
        user_id: "@bob:example.com".to_string(),
        display_name: None,
        device_key: None,
        last_seen_ts: None,
        last_seen_ip: None,
        created_ts: 0,
        first_seen_ts: 0,
        user_agent: None,
        appservice_id: None,
        ignored_user_list: None,
    };

    assert_eq!(device.device_id, "MINIMAL");
    assert!(device.display_name.is_none());
    assert!(device.device_key.is_none());
}

#[test]
fn test_device_with_appservice() {
    let device = Device {
        device_id: "APPDEVICE".to_string(),
        user_id: "@bot:example.com".to_string(),
        display_name: Some("Bot Device".to_string()),
        device_key: None,
        last_seen_ts: Some(1234567890000),
        last_seen_ip: None,
        created_ts: 1234567890000,
        first_seen_ts: 1234567890000,
        user_agent: None,
        appservice_id: Some("my_appservice".to_string()),
        ignored_user_list: None,
    };

    assert_eq!(device.appservice_id, Some("my_appservice".to_string()));
}

#[test]
fn test_device_key_json() {
    let device = Device {
        device_id: "KEYDEVICE".to_string(),
        user_id: "@charlie:example.com".to_string(),
        display_name: None,
        device_key: Some(serde_json::json!({
            "algorithms": ["m.megolm.v1.aes-sha2"],
            "device_id": "KEYDEVICE",
            "keys": {
                "ed25519:KEYDEVICE": "key_data"
            }
        })),
        last_seen_ts: None,
        last_seen_ip: None,
        created_ts: 0,
        first_seen_ts: 0,
        user_agent: None,
        appservice_id: None,
        ignored_user_list: None,
    };

    assert!(device.device_key.is_some());
    let key = device.device_key.unwrap();
    assert!(key.get("algorithms").is_some());
}

#[test]
fn test_device_with_display_name() {
    let device = Device {
        device_id: "TESTDEVICE".to_string(),
        user_id: "@alice:example.com".to_string(),
        display_name: Some("My Phone".to_string()),
        device_key: None,
        last_seen_ts: Some(1234567890),
        last_seen_ip: Some("192.168.1.1".to_string()),
        created_ts: 1234567800,
        first_seen_ts: 1234567800,
        user_agent: None,
        appservice_id: None,
        ignored_user_list: None,
    };

    assert!(device.display_name.is_some());
    assert_eq!(device.display_name.unwrap(), "My Phone");
    assert!(device.last_seen_ts.is_some());
}

#[test]
fn test_device_without_keys() {
    let device = Device {
        device_id: "NOKEYS".to_string(),
        user_id: "@bob:example.com".to_string(),
        display_name: None,
        device_key: None,
        last_seen_ts: None,
        last_seen_ip: None,
        created_ts: 0,
        first_seen_ts: 0,
        user_agent: None,
        appservice_id: None,
        ignored_user_list: None,
    };

    assert!(device.device_key.is_none());
    assert!(device.last_seen_ts.is_none());
}

#[test]
fn test_device_appservice_id() {
    let device = Device {
        device_id: "BRIDGE_DEVICE".to_string(),
        user_id: "@_irc_alice:example.com".to_string(),
        display_name: Some("IRC Bridge".to_string()),
        device_key: None,
        last_seen_ts: Some(1234567890),
        last_seen_ip: Some("10.0.0.1".to_string()),
        created_ts: 1234567800,
        first_seen_ts: 1234567800,
        user_agent: None,
        appservice_id: Some("irc-bridge".to_string()),
        ignored_user_list: None,
    };

    assert!(device.appservice_id.is_some());
    assert_eq!(device.appservice_id.unwrap(), "irc-bridge");
}

#[test]
fn test_device_ignored_user_list() {
    let device = Device {
        device_id: "IGNORED".to_string(),
        user_id: "@user:example.com".to_string(),
        display_name: None,
        device_key: None,
        last_seen_ts: None,
        last_seen_ip: None,
        created_ts: 0,
        first_seen_ts: 0,
        user_agent: None,
        appservice_id: None,
        ignored_user_list: Some("[\"@baduser:example.com\"]".to_string()),
    };

    assert!(device.ignored_user_list.is_some());
}

#[test]
fn test_device_ip_tracking() {
    let device = Device {
        device_id: "IPTRACK".to_string(),
        user_id: "@alice:example.com".to_string(),
        display_name: None,
        device_key: None,
        last_seen_ts: Some(1234567890),
        last_seen_ip: Some("203.0.113.1".to_string()),
        created_ts: 1234567800,
        first_seen_ts: 1234567800,
        user_agent: None,
        appservice_id: None,
        ignored_user_list: None,
    };

    assert!(device.last_seen_ip.is_some());
    assert!(device.last_seen_ts.is_some());
}

#[tokio::test]
async fn test_lazy_loaded_members_are_cleaned_up_with_device_lifecycle() {
    let pool = match crate::test_utils::prepare_empty_isolated_test_pool().await {
        Ok(guard) => guard.pool(),
        Err(error) => {
            tracing::warn!("Skipping device lazy-loaded-members test because test database is unavailable: {error}");
            return;
        }
    };

    sqlx::query(
        r#"
            CREATE TABLE users (
                user_id VARCHAR(255) PRIMARY KEY,
                username TEXT NOT NULL UNIQUE,
                created_ts BIGINT NOT NULL
            )
            "#,
    )
    .execute(&*pool)
    .await
    .expect("Failed to create users table");

    sqlx::query(
        r#"
            CREATE TABLE devices (
                device_id VARCHAR(255) PRIMARY KEY,
                user_id VARCHAR(255) NOT NULL,
                display_name TEXT,
                device_key JSONB,
                last_seen_ts BIGINT,
                last_seen_ip TEXT,
                created_ts BIGINT NOT NULL,
                first_seen_ts BIGINT NOT NULL,
                user_agent TEXT,
                appservice_id TEXT,
                ignored_user_list TEXT
            )
            "#,
    )
    .execute(&*pool)
    .await
    .expect("Failed to create devices table");

    sqlx::query(
        r#"
            CREATE TABLE device_lists_stream (
                stream_id BIGSERIAL PRIMARY KEY,
                user_id VARCHAR(255) NOT NULL,
                device_id VARCHAR(255),
                created_ts BIGINT NOT NULL
            )
            "#,
    )
    .execute(&*pool)
    .await
    .expect("Failed to create device_lists_stream table");

    sqlx::query(
        r#"
            CREATE TABLE device_lists_changes (
                id BIGSERIAL PRIMARY KEY,
                user_id VARCHAR(255) NOT NULL,
                device_id VARCHAR(255),
                change_type TEXT NOT NULL,
                stream_id BIGINT NOT NULL,
                created_ts BIGINT NOT NULL
            )
            "#,
    )
    .execute(&*pool)
    .await
    .expect("Failed to create device_lists_changes table");

    sqlx::query(
        r#"
            CREATE TABLE lazy_loaded_members (
                user_id TEXT NOT NULL,
                device_id TEXT NOT NULL,
                room_id TEXT NOT NULL,
                member_user_id TEXT NOT NULL,
                created_ts BIGINT NOT NULL,
                updated_ts BIGINT NOT NULL,
                last_sent_position BIGINT,
                PRIMARY KEY (user_id, device_id, room_id, member_user_id)
            )
            "#,
    )
    .execute(&*pool)
    .await
    .expect("Failed to create lazy_loaded_members table");

    sqlx::query(
        r#"
            INSERT INTO users (user_id, username, created_ts)
            VALUES ($1, $2, $3)
            "#,
    )
    .bind("@alice:localhost")
    .bind("alice")
    .bind(current_timestamp_millis())
    .execute(&*pool)
    .await
    .expect("Failed to insert test user");

    let storage = DeviceStorage::new(&pool);
    storage.create_device("DEVICE123", "@alice:localhost", Some("Alice phone")).await.expect("Failed to create device");

    // Distinct positions on purpose: the row carries the position of the
    // response that actually delivered *that* member, so a per-member
    // position must survive the round trip.
    let members = vec![
        LazyLoadedMemberRow { member_user_id: "@alice:localhost".to_string(), last_sent_position: Some(41) },
        LazyLoadedMemberRow { member_user_id: "@bob:localhost".to_string(), last_sent_position: Some(42) },
        // No position ⇒ nothing to prove ⇒ not written at all.
        LazyLoadedMemberRow { member_user_id: "@carol:localhost".to_string(), last_sent_position: None },
    ];
    let affected = storage
        .upsert_lazy_loaded_members("@alice:localhost", "DEVICE123", "!room:localhost", &members)
        .await
        .expect("Failed to store lazy-loaded members");
    assert_eq!(affected, 2, "a member without a position must not be stored");

    let mut before_delete = storage
        .get_lazy_loaded_members("@alice:localhost", "DEVICE123", "!room:localhost")
        .await
        .expect("Failed to fetch lazy-loaded members");
    before_delete.sort_by(|a, b| a.member_user_id.cmp(&b.member_user_id));
    let mut expected = vec![members[0].clone(), members[1].clone()];
    expected.sort_by(|a, b| a.member_user_id.cmp(&b.member_user_id));
    assert_eq!(before_delete, expected, "positions must round-trip per member");

    // A pre-existing row (written before the column existed) reads back as
    // `NULL`, which the sync layer treats as "not recorded" — so the two
    // states stay distinguishable in the database too.
    sqlx::query(
        r#"
            INSERT INTO lazy_loaded_members (user_id, device_id, room_id, member_user_id, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $5)
            "#,
    )
    .bind("@alice:localhost")
    .bind("DEVICE123")
    .bind("!room:localhost")
    .bind("@carol:localhost")
    .bind(current_timestamp_millis())
    .execute(&*pool)
    .await
    .expect("Failed to insert a legacy row");
    let legacy = storage
        .get_lazy_loaded_members("@alice:localhost", "DEVICE123", "!room:localhost")
        .await
        .expect("Failed to fetch lazy-loaded members");
    let carol =
        legacy.iter().find(|row| row.member_user_id == "@carol:localhost").expect("legacy row must be readable");
    assert_eq!(carol.last_sent_position, None, "an unrecorded position must stay NULL");

    let rows = storage.delete_user_device("@alice:localhost", "DEVICE123").await.expect("Failed to delete device");
    assert_eq!(rows, 1);

    let after_delete = storage
        .get_lazy_loaded_members("@alice:localhost", "DEVICE123", "!room:localhost")
        .await
        .expect("Failed to fetch lazy-loaded members after delete");
    assert!(after_delete.is_empty());
}
