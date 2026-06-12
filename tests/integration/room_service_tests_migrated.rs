#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::common::room_versions::DEFAULT_ROOM_VERSION;
use synapse_rust::common::validation::Validator;
use synapse_rust::services::application_service::ApplicationServiceManager;
use synapse_rust::services::room_service::{CreateRoomConfig, RoomService};
use synapse_rust::services::room_summary_service::RoomSummaryService;
use synapse_rust::storage::application_service::{ApplicationServiceStorage, RegisterApplicationServiceRequest};
use synapse_rust::storage::event::EventStorage;
use synapse_rust::storage::membership::RoomMemberStorage;
use synapse_rust::storage::relations::RelationsStorage;
use synapse_rust::storage::room::RoomStorage;
use synapse_rust::storage::room_summary::RoomSummaryStorage;
use synapse_rust::storage::user::UserStorage;
use synapse_rust::storage::CreateEventParams;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

async fn setup_test_database(pool: &Arc<sqlx::PgPool>) {
    sqlx::query(
        r#"
        CREATE TABLE users (
            user_id VARCHAR(255) PRIMARY KEY,
            username TEXT NOT NULL UNIQUE,
            password_hash TEXT,
            is_admin BOOLEAN DEFAULT FALSE,
            is_guest BOOLEAN DEFAULT FALSE,
            is_shadow_banned BOOLEAN DEFAULT FALSE,
            is_deactivated BOOLEAN DEFAULT FALSE,
            created_ts BIGINT NOT NULL,
            updated_ts BIGINT,
            displayname TEXT,
            avatar_url TEXT,
            email TEXT,
            phone TEXT,
            generation BIGINT DEFAULT 0,
            consent_version TEXT,
            appservice_id TEXT,
            user_type TEXT,
            invalid_update_at BIGINT,
            migration_state TEXT,
            password_changed_ts BIGINT,
            is_password_change_required BOOLEAN DEFAULT FALSE,
            password_expires_at BIGINT,
            failed_login_attempts INT DEFAULT 0,
            locked_until BIGINT,
            must_change_password BOOLEAN DEFAULT FALSE
        )
    "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create users table");

    sqlx::query(
        r#"
        CREATE TABLE rooms (
            room_id VARCHAR(255) PRIMARY KEY,
            is_public BOOLEAN DEFAULT FALSE,
            room_version TEXT DEFAULT '6',
            created_ts BIGINT NOT NULL,
            last_activity_ts BIGINT,
            join_rules TEXT DEFAULT 'invite',
            history_visibility TEXT DEFAULT 'shared',
            name TEXT,
            topic TEXT,
            avatar_url TEXT,
            canonical_alias TEXT,
            visibility TEXT DEFAULT 'private',
            creator TEXT,
            encryption TEXT,
            member_count BIGINT DEFAULT 0
        )
    "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create rooms table");

    sqlx::query(
        r#"
        CREATE TABLE room_memberships (
            room_id VARCHAR(255) NOT NULL,
            user_id VARCHAR(255) NOT NULL,
            sender TEXT,
            membership TEXT NOT NULL,
            event_id TEXT,
            event_type TEXT,
            display_name TEXT,
            avatar_url TEXT,
            is_banned BOOLEAN DEFAULT FALSE,
            invite_token TEXT,
            updated_ts BIGINT,
            joined_ts BIGINT,
            left_ts BIGINT,
            reason TEXT,
            banned_by TEXT,
            ban_reason TEXT,
            banned_ts BIGINT,
            join_reason TEXT,
            PRIMARY KEY (room_id, user_id)
        )
    "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create room_memberships table");

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
            stream_ordering BIGSERIAL,
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
    .execute(pool.as_ref())
    .await
    .expect("Failed to create events table");
}

async fn setup_appservice_test_database(pool: &Arc<sqlx::PgPool>) {
    sqlx::query(
        r#"
        CREATE TABLE application_services (
            id BIGSERIAL PRIMARY KEY,
            as_id TEXT NOT NULL UNIQUE,
            url TEXT NOT NULL,
            as_token TEXT NOT NULL,
            hs_token TEXT NOT NULL,
            sender_localpart TEXT NOT NULL,
            is_enabled BOOLEAN NOT NULL DEFAULT TRUE,
            is_rate_limited BOOLEAN NOT NULL DEFAULT FALSE,
            protocols TEXT[] NOT NULL DEFAULT '{}',
            namespaces JSONB NOT NULL DEFAULT '{"users":[],"aliases":[],"rooms":[]}',
            created_ts BIGINT NOT NULL,
            updated_ts BIGINT,
            description TEXT,
            api_key TEXT,
            config JSONB NOT NULL DEFAULT '{}'
        )
    "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create application_services table");

    sqlx::query(
        r#"
        CREATE TABLE application_service_user_namespaces (
            id BIGSERIAL PRIMARY KEY,
            as_id TEXT NOT NULL,
            namespace TEXT NOT NULL,
            is_exclusive BOOLEAN NOT NULL DEFAULT FALSE,
            created_ts BIGINT NOT NULL
        )
    "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create application_service_user_namespaces table");

    sqlx::query(
        r#"
        CREATE TABLE application_service_room_alias_namespaces (
            id BIGSERIAL PRIMARY KEY,
            as_id TEXT NOT NULL,
            namespace TEXT NOT NULL,
            is_exclusive BOOLEAN NOT NULL DEFAULT FALSE,
            created_ts BIGINT NOT NULL
        )
    "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create application_service_room_alias_namespaces table");

    sqlx::query(
        r#"
        CREATE TABLE application_service_room_namespaces (
            id BIGSERIAL PRIMARY KEY,
            as_id TEXT NOT NULL,
            namespace TEXT NOT NULL,
            is_exclusive BOOLEAN NOT NULL DEFAULT FALSE,
            created_ts BIGINT NOT NULL
        )
    "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create application_service_room_namespaces table");

    sqlx::query(
        r#"
        CREATE TABLE application_service_events (
            event_id TEXT PRIMARY KEY,
            as_id TEXT NOT NULL,
            room_id TEXT NOT NULL,
            event_type TEXT NOT NULL,
            is_processed BOOLEAN NOT NULL DEFAULT FALSE,
            processed_ts BIGINT,
            created_ts BIGINT NOT NULL
        )
    "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create application_service_events table");

    sqlx::query(
        r#"
        CREATE TABLE application_service_transactions (
            id BIGSERIAL PRIMARY KEY,
            as_id TEXT NOT NULL,
            transaction_id TEXT NOT NULL,
            events JSONB NOT NULL,
            sent_ts BIGINT NOT NULL,
            completed_ts BIGINT,
            retry_count INTEGER NOT NULL DEFAULT 0,
            last_error TEXT
        )
    "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create application_service_transactions table");

    sqlx::query(
        r#"
        CREATE TABLE application_service_state (
            as_id TEXT NOT NULL,
            state_key TEXT NOT NULL,
            state_value TEXT NOT NULL,
            updated_ts BIGINT NOT NULL,
            PRIMARY KEY (as_id, state_key)
        )
    "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create application_service_state table");
}

async fn create_test_user(pool: &sqlx::PgPool, user_id: &str, username: &str) {
    sqlx::query(
        r#"
        INSERT INTO users (user_id, username, created_ts)
        VALUES ($1, $2, $3)
        ON CONFLICT (user_id) DO NOTHING
        "#,
    )
    .bind(user_id)
    .bind(username)
    .bind(chrono::Utc::now().timestamp_millis())
    .execute(pool)
    .await
    .expect("Failed to create test user");
}

fn create_room_service(pool: &Arc<sqlx::PgPool>, cache: Arc<CacheManager>) -> RoomService {
    let member_storage = RoomMemberStorage::new(pool, "localhost");
    let event_storage = EventStorage::new(pool, "localhost".to_string());
    let canonical_cache = Arc::new(cache.to_synapse_cache_manager());
    let room_summary_storage = Arc::new(RoomSummaryStorage::new(pool));
    let room_summary_service = Arc::new(RoomSummaryService::new(
        room_summary_storage,
        Arc::new(event_storage.clone()),
        Some(Arc::new(member_storage.clone())),
    ));

    RoomService::new(synapse_rust::services::room_service::RoomServiceConfig {
        room_storage: RoomStorage::new(pool),
        member_storage,
        event_storage,
        user_storage: UserStorage::new(pool, canonical_cache),
        auth_service: synapse_rust::auth::AuthService::new(
            pool,
            cache,
            Arc::new(synapse_rust::common::metrics::MetricsCollector::new()),
            &synapse_rust::common::config::SecurityConfig::default(),
            "localhost",
        ),
        room_summary_service,
        validator: Arc::new(Validator::default()),
        server_name: "localhost".to_string(),
        task_queue: None,
        relations_storage: RelationsStorage::new(pool),
        event_broadcaster: Some(Arc::new(synapse_rust::federation::event_broadcaster::EventBroadcaster::new(
            "localhost".to_string(),
        ))),
        app_service_manager: None,
        beacon_service: None,
    })
}

async fn attach_test_appservice(
    pool: &Arc<sqlx::PgPool>,
    room_service: &RoomService,
    as_id: &str,
) -> Arc<ApplicationServiceManager> {
    let manager = Arc::new(ApplicationServiceManager::new(
        Arc::new(ApplicationServiceStorage::new(pool)),
        Arc::new(EventStorage::new(pool, "localhost".to_string())),
        "localhost".to_string(),
    ));

    manager
        .register(RegisterApplicationServiceRequest {
            as_id: as_id.to_string(),
            url: "http://localhost:9999".to_string(),
            as_token: format!("as_token_{as_id}"),
            hs_token: format!("hs_token_{as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("room-service test bridge".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [{"exclusive": false, "regex": "@.*:localhost"}],
                "aliases": [],
                "rooms": []
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("Failed to register test application service");

    room_service.set_app_service_manager(manager.clone()).await;
    manager
}

#[tokio::test]
async fn test_room_service_creation() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let room_service = create_room_service(&pool, cache);

    assert_eq!(room_service.server_name, "localhost");
}

#[tokio::test]
async fn test_create_room_success() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let id = unique_id();
    let alice_id = format!("@alice_{id}:localhost");
    let alice_name = format!("alice_{id}");
    create_test_user(&pool, &alice_id, &alice_name).await;

    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let room_service = create_room_service(&pool, cache.clone());

    let config = CreateRoomConfig {
        name: Some("Test Room".to_string()),
        topic: Some("Test Topic".to_string()),
        visibility: Some("public".to_string()),
        ..Default::default()
    };

    let result = room_service.create_room(&alice_id, config).await;
    assert!(result.is_ok());
    let val = result.unwrap();
    assert!(val["room_id"].as_str().unwrap().starts_with('!'));

    let room_id = val["room_id"].as_str().unwrap();
    let room = room_service.get_room(room_id).await.unwrap();
    assert_eq!(room["name"], "Test Room");
    assert_eq!(room["topic"], "Test Topic");
    assert_eq!(room["is_public"], true);
    assert_eq!(room["creator"], alice_id);
}

#[tokio::test]
async fn test_create_room_enqueues_appservice_events_after_commit() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup_appservice_test_database(&pool).await;

    let id = unique_id();
    let alice_id = format!("@alice_{id}:localhost");
    let alice_name = format!("alice_{id}");
    create_test_user(&pool, &alice_id, &alice_name).await;

    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let room_service = create_room_service(&pool, cache);
    let as_id = format!("room-create-bridge-{id}");
    let storage = ApplicationServiceStorage::new(&pool);
    attach_test_appservice(&pool, &room_service, &as_id).await;

    let room_val = room_service
        .create_room(
            &alice_id,
            CreateRoomConfig {
                name: Some("Appservice Create Room".to_string()),
                topic: Some("Initial metadata should enqueue".to_string()),
                visibility: Some("public".to_string()),
                ..Default::default()
            },
        )
        .await
        .expect("room creation should succeed");

    let room_id = room_val["room_id"].as_str().expect("room_id should be present");
    let pending = storage.get_pending_events(&as_id, 64).await.expect("pending events should load");
    let pending_types: Vec<&str> = pending.iter().map(|event| event.event_type.as_str()).collect();

    assert!(pending.iter().all(|event| event.room_id == room_id));
    assert!(pending_types.contains(&"m.room.create"));
    assert!(pending_types.contains(&"m.room.member"));
    assert!(pending_types.contains(&"m.room.power_levels"));
    assert!(pending_types.contains(&"m.room.join_rules"));
    assert!(pending_types.contains(&"m.room.history_visibility"));
    assert!(pending_types.contains(&"m.room.guest_access"));
    assert!(pending_types.contains(&"m.room.name"));
    assert!(pending_types.contains(&"m.room.topic"));
}

#[tokio::test]
async fn test_create_room_ignores_protected_creation_content_fields() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let id = unique_id();
    let alice_id = format!("@alice_{id}:localhost");
    let alice_name = format!("alice_{id}");
    create_test_user(&pool, &alice_id, &alice_name).await;

    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let room_service = create_room_service(&pool, cache.clone());

    let config = CreateRoomConfig {
        room_version: Some("11".to_string()),
        creation_content: Some(json!({
            "creator": "@mallory:localhost",
            "room_version": "1",
            "m.federate": false,
        })),
        ..Default::default()
    };
    let room_val = room_service.create_room(&alice_id, config).await.unwrap();
    let room_id = room_val["room_id"].as_str().unwrap();

    let event_storage = EventStorage::new(&pool, "localhost".to_string());
    let create_events = event_storage.get_state_events_by_type(room_id, "m.room.create").await.unwrap();
    let create_event = create_events
        .iter()
        .find(|event| event.state_key.as_deref() == Some(""))
        .expect("room should have create state");

    assert_eq!(create_event.content["creator"].as_str(), Some(alice_id.as_str()));
    assert_eq!(create_event.content["room_version"].as_str(), Some("11"));
    assert_eq!(create_event.content["m.federate"].as_bool(), Some(false));
}

#[tokio::test]
async fn test_join_room_success() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let id = unique_id();
    let alice_id = format!("@alice_{id}:localhost");
    let alice_name = format!("alice_{id}");
    let bob_id = format!("@bob_{id}:localhost");
    let bob_name = format!("bob_{id}");
    create_test_user(&pool, &alice_id, &alice_name).await;
    create_test_user(&pool, &bob_id, &bob_name).await;

    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let room_service = create_room_service(&pool, cache.clone());

    let config = CreateRoomConfig { visibility: Some("public".to_string()), ..Default::default() };
    let room_val = room_service.create_room(&alice_id, config).await.unwrap();
    let room_id = room_val["room_id"].as_str().unwrap();

    let result = room_service.join_room(room_id, &bob_id).await;
    assert!(result.is_ok(), "join_room failed: {:?}", result.err());

    let members = room_service.get_room_members(room_id, &alice_id).await.unwrap();
    let chunk = members["chunk"].as_array().unwrap();
    assert!(chunk.iter().any(|m| m["state_key"] == bob_id || m["user_id"] == bob_id));
}

#[tokio::test]
async fn test_join_room_enqueues_appservice_membership_event() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup_appservice_test_database(&pool).await;

    let id = unique_id();
    let alice_id = format!("@alice_{id}:localhost");
    let alice_name = format!("alice_{id}");
    let bob_id = format!("@bob_{id}:localhost");
    let bob_name = format!("bob_{id}");
    create_test_user(&pool, &alice_id, &alice_name).await;
    create_test_user(&pool, &bob_id, &bob_name).await;

    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let room_service = create_room_service(&pool, cache);
    let as_id = format!("join-bridge-{id}");
    let storage = ApplicationServiceStorage::new(&pool);
    attach_test_appservice(&pool, &room_service, &as_id).await;

    let room_val = room_service
        .create_room(&alice_id, CreateRoomConfig { visibility: Some("public".to_string()), ..Default::default() })
        .await
        .expect("room creation should succeed");
    let room_id = room_val["room_id"].as_str().expect("room_id should be present");

    let before_member_events = storage
        .get_pending_events(&as_id, 64)
        .await
        .expect("pending events should load")
        .into_iter()
        .filter(|event| event.event_type == "m.room.member")
        .count();

    room_service.join_room(room_id, &bob_id).await.expect("join_room should succeed");

    let after_member_events = storage
        .get_pending_events(&as_id, 64)
        .await
        .expect("pending events should load")
        .into_iter()
        .filter(|event| event.event_type == "m.room.member")
        .count();

    assert_eq!(after_member_events, before_member_events + 1);
}

#[tokio::test]
async fn test_send_message_success() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let id = unique_id();
    let alice_id = format!("@alice_{id}:localhost");
    let alice_name = format!("alice_{id}");
    create_test_user(&pool, &alice_id, &alice_name).await;

    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let room_service = create_room_service(&pool, cache.clone());

    let config = CreateRoomConfig::default();
    let room_val = room_service.create_room(&alice_id, config).await.unwrap();
    let room_id = room_val["room_id"].as_str().unwrap();

    let content = json!({"msgtype": "m.text", "body": "Hello world"});
    let result = room_service.send_message(room_id, &alice_id, "m.room.message", &content).await;
    assert!(result.is_ok());
    let val = result.unwrap();
    assert!(val["event_id"].as_str().unwrap().starts_with('$'));

    let messages = room_service.get_room_messages(room_id, &alice_id, 0, 10, "b").await.unwrap();
    let chunk = messages["chunk"].as_array().unwrap();
    let event = chunk
        .iter()
        .find(|event| event["type"] == "m.room.message")
        .expect("timeline should include m.room.message event");
    assert_eq!(event["content"]["body"], "Hello world");
    assert_eq!(event["sender"], alice_id);
}

#[tokio::test]
async fn test_get_room_messages_supports_sync_prev_batch_token() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let id = unique_id();
    let alice_id = format!("@alice_{id}:localhost");
    let alice_name = format!("alice_{id}");
    create_test_user(&pool, &alice_id, &alice_name).await;

    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let room_service = create_room_service(&pool, cache.clone());

    let config = CreateRoomConfig::default();
    let room_val = room_service.create_room(&alice_id, config).await.unwrap();
    let room_id = room_val["room_id"].as_str().unwrap();
    let base_ts = chrono::Utc::now().timestamp_millis() + 10_000;

    for ts in [base_ts + 1000, base_ts + 2000, base_ts + 3000] {
        room_service
            .event_storage
            .create_event(
                CreateEventParams {
                    event_id: format!("$timeline_{id}_{ts}"),
                    room_id: room_id.to_string(),
                    user_id: alice_id.clone(),
                    event_type: "m.room.message".to_string(),
                    content: json!({"msgtype": "m.text", "body": format!("msg-{ts}")}),
                    state_key: None,
                    origin_server_ts: ts,
                },
                None,
            )
            .await
            .unwrap();
    }

    let messages = room_service.get_room_messages(room_id, &alice_id, base_ts + 3000, 2, "b").await.unwrap();

    assert_eq!(messages["start"], format!("t{}", base_ts + 3000));
    assert_eq!(messages["end"], format!("t{}", base_ts + 1000));

    let chunk = messages["chunk"].as_array().unwrap();
    assert_eq!(chunk.len(), 2);
    assert_eq!(chunk[0]["origin_server_ts"], base_ts + 2000);
    assert_eq!(chunk[1]["origin_server_ts"], base_ts + 1000);
    assert_eq!(chunk[0]["content"]["body"], format!("msg-{}", base_ts + 2000));
    assert_eq!(chunk[1]["content"]["body"], format!("msg-{}", base_ts + 1000));
}

#[tokio::test]
async fn test_get_room_messages_supports_forward_pagination_from_stream_token() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let id = unique_id();
    let alice_id = format!("@alice_{id}:localhost");
    let alice_name = format!("alice_{id}");
    create_test_user(&pool, &alice_id, &alice_name).await;

    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let room_service = create_room_service(&pool, cache.clone());

    let config = CreateRoomConfig::default();
    let room_val = room_service.create_room(&alice_id, config).await.unwrap();
    let room_id = room_val["room_id"].as_str().unwrap();
    let base_ts = chrono::Utc::now().timestamp_millis() + 10_000;

    for ts in [base_ts + 1000, base_ts + 2000, base_ts + 3000] {
        room_service
            .event_storage
            .create_event(
                CreateEventParams {
                    event_id: format!("$forward_timeline_{id}_{ts}"),
                    room_id: room_id.to_string(),
                    user_id: alice_id.clone(),
                    event_type: "m.room.message".to_string(),
                    content: json!({"msgtype": "m.text", "body": format!("msg-{ts}")}),
                    state_key: None,
                    origin_server_ts: ts,
                },
                None,
            )
            .await
            .unwrap();
    }

    let messages = room_service.get_room_messages(room_id, &alice_id, base_ts + 1000, 2, "f").await.unwrap();

    assert_eq!(messages["start"], format!("t{}", base_ts + 1000));
    assert_eq!(messages["end"], format!("t{}", base_ts + 3000));

    let chunk = messages["chunk"].as_array().unwrap();
    assert_eq!(chunk.len(), 2);
    assert_eq!(chunk[0]["origin_server_ts"], base_ts + 2000);
    assert_eq!(chunk[1]["origin_server_ts"], base_ts + 3000);
    assert_eq!(chunk[0]["content"]["body"], format!("msg-{}", base_ts + 2000));
    assert_eq!(chunk[1]["content"]["body"], format!("msg-{}", base_ts + 3000));
}

#[tokio::test]
async fn test_invite_user_success() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let id = unique_id();
    let alice_id = format!("@alice_{id}:localhost");
    let alice_name = format!("alice_{id}");
    let bob_id = format!("@bob_{id}:localhost");
    let bob_name = format!("bob_{id}");
    create_test_user(&pool, &alice_id, &alice_name).await;
    create_test_user(&pool, &bob_id, &bob_name).await;

    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let room_service = create_room_service(&pool, cache.clone());

    let config = CreateRoomConfig::default();
    let room_val = room_service.create_room(&alice_id, config).await.unwrap();
    let room_id = room_val["room_id"].as_str().unwrap();

    let result = room_service.invite_user(room_id, &alice_id, &bob_id).await;
    assert!(result.is_ok(), "invite_user failed: {:?}", result.err());

    let member_storage = RoomMemberStorage::new(&pool, "localhost");
    let member = member_storage.get_member(room_id, &bob_id).await.unwrap().unwrap();
    assert_eq!(member.membership, "invite");
}

#[tokio::test]
async fn test_invite_user_enqueues_appservice_membership_event() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup_appservice_test_database(&pool).await;

    let id = unique_id();
    let alice_id = format!("@alice_{id}:localhost");
    let alice_name = format!("alice_{id}");
    let bob_id = format!("@bob_{id}:localhost");
    let bob_name = format!("bob_{id}");
    create_test_user(&pool, &alice_id, &alice_name).await;
    create_test_user(&pool, &bob_id, &bob_name).await;

    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let room_service = create_room_service(&pool, cache);
    let as_id = format!("invite-bridge-{id}");
    let storage = ApplicationServiceStorage::new(&pool);
    attach_test_appservice(&pool, &room_service, &as_id).await;

    let room_val =
        room_service.create_room(&alice_id, CreateRoomConfig::default()).await.expect("room creation should succeed");
    let room_id = room_val["room_id"].as_str().expect("room_id should be present");

    let before_member_events = storage
        .get_pending_events(&as_id, 64)
        .await
        .expect("pending events should load")
        .into_iter()
        .filter(|event| event.event_type == "m.room.member")
        .count();

    room_service.invite_user(room_id, &alice_id, &bob_id).await.expect("invite_user should succeed");

    let after_member_events = storage
        .get_pending_events(&as_id, 64)
        .await
        .expect("pending events should load")
        .into_iter()
        .filter(|event| event.event_type == "m.room.member")
        .count();

    assert_eq!(after_member_events, before_member_events + 1);
}

#[tokio::test]
async fn test_ban_user_success() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let id = unique_id();
    let alice_id = format!("@alice_{id}:localhost");
    let alice_name = format!("alice_{id}");
    let bob_id = format!("@bob_{id}:localhost");
    let bob_name = format!("bob_{id}");
    create_test_user(&pool, &alice_id, &alice_name).await;
    create_test_user(&pool, &bob_id, &bob_name).await;

    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let room_service = create_room_service(&pool, cache.clone());

    let config = CreateRoomConfig::default();
    let room_val = room_service.create_room(&alice_id, config).await.unwrap();
    let room_id = room_val["room_id"].as_str().unwrap();

    let result = room_service.ban_user(room_id, &bob_id, &alice_id, Some("Spam")).await;
    assert!(result.is_ok(), "ban_user failed: {:?}", result.err());

    let member_storage = RoomMemberStorage::new(&pool, "localhost");
    let member = member_storage.get_member(room_id, &bob_id).await.unwrap().unwrap();
    assert_eq!(member.membership, "ban");
    assert_eq!(member.banned_by, Some(alice_id));
}

#[tokio::test]
async fn test_upgrade_room_success() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let id = unique_id();
    let alice_id = format!("@alice_{id}:localhost");
    let alice_name = format!("alice_{id}");
    create_test_user(&pool, &alice_id, &alice_name).await;

    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let room_service = create_room_service(&pool, cache.clone());

    let config = CreateRoomConfig::default();
    let room_val = room_service.create_room(&alice_id, config).await.unwrap();
    let old_room_id = room_val["room_id"].as_str().unwrap();

    let result = room_service.upgrade_room(old_room_id, "11", &alice_id).await;

    assert!(result.is_ok());
    let new_room_id = result.unwrap();
    assert!(!new_room_id.is_empty());
    assert_ne!(new_room_id, old_room_id);

    let room_storage = RoomStorage::new(&pool);
    let old_room = room_storage.get_room(old_room_id).await.unwrap().expect("old room should still exist");
    assert_eq!(old_room.room_version, DEFAULT_ROOM_VERSION);

    let new_room = room_storage.get_room(&new_room_id).await.unwrap().expect("replacement room should exist");
    assert_eq!(new_room.room_version, "11");

    let event_storage = EventStorage::new(&pool, "localhost".to_string());
    let tombstone_events = event_storage.get_state_events_by_type(old_room_id, "m.room.tombstone").await.unwrap();
    let tombstone = tombstone_events
        .iter()
        .find(|event| event.state_key.as_deref() == Some(""))
        .expect("old room should have tombstone state");
    assert_eq!(tombstone.content["replacement_room"].as_str(), Some(new_room_id.as_str()));

    let create_events = event_storage.get_state_events_by_type(&new_room_id, "m.room.create").await.unwrap();
    let create_event = create_events
        .iter()
        .find(|event| event.state_key.as_deref() == Some(""))
        .expect("replacement room should have create state");
    assert_eq!(create_event.content["predecessor"]["room_id"].as_str(), Some(old_room_id));
    assert_eq!(create_event.content["predecessor"]["event_id"].as_str(), Some(tombstone.event_id.as_str()));
}

#[tokio::test]
async fn test_upgrade_room_enqueues_tombstone_and_replacement_create_events() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup_appservice_test_database(&pool).await;

    let id = unique_id();
    let alice_id = format!("@alice_{id}:localhost");
    let alice_name = format!("alice_{id}");
    create_test_user(&pool, &alice_id, &alice_name).await;

    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let room_service = create_room_service(&pool, cache);
    let as_id = format!("upgrade-bridge-{id}");
    let storage = ApplicationServiceStorage::new(&pool);
    attach_test_appservice(&pool, &room_service, &as_id).await;

    let room_val =
        room_service.create_room(&alice_id, CreateRoomConfig::default()).await.expect("room creation should succeed");
    let old_room_id = room_val["room_id"].as_str().expect("room_id should be present").to_string();

    let before_pending = storage.get_pending_events(&as_id, 256).await.expect("pending events should load");
    let before_create_events = before_pending.iter().filter(|event| event.event_type == "m.room.create").count();
    let before_tombstone_events = before_pending.iter().filter(|event| event.event_type == "m.room.tombstone").count();

    let new_room_id =
        room_service.upgrade_room(&old_room_id, "11", &alice_id).await.expect("upgrade_room should succeed");

    let after_pending = storage.get_pending_events(&as_id, 256).await.expect("pending events should load");
    let after_create_events = after_pending.iter().filter(|event| event.event_type == "m.room.create").count();
    let after_tombstone_events = after_pending.iter().filter(|event| event.event_type == "m.room.tombstone").count();

    assert_eq!(after_create_events, before_create_events + 1);
    assert_eq!(after_tombstone_events, before_tombstone_events + 1);
    assert!(after_pending.iter().any(|event| event.event_type == "m.room.tombstone" && event.room_id == old_room_id));
    assert!(after_pending.iter().any(|event| event.event_type == "m.room.create" && event.room_id == new_room_id));
}

#[tokio::test]
async fn test_upgrade_room_not_found() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let id = unique_id();
    let alice_id = format!("@alice_{id}:localhost");
    let alice_name = format!("alice_{id}");
    create_test_user(&pool, &alice_id, &alice_name).await;

    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let room_service = create_room_service(&pool, cache.clone());

    let result = room_service.upgrade_room("!nonexistent:localhost", "11", &alice_id).await;

    assert!(result.is_err());
}
