#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};

use synapse_federation::event_broadcaster::EventBroadcaster;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::common::Validator;
use synapse_services::application_service::{ApplicationServiceManager, ApplicationServiceScheduler};
use synapse_services::room_service::{CreateRoomConfig, RoomService};
use synapse_services::room_summary_service::RoomSummaryService;
use synapse_storage::application_service::{ApplicationServiceStorage, RegisterApplicationServiceRequest};
use synapse_storage::event::EventStorage;
use synapse_storage::membership::RoomMemberStorage;
use synapse_storage::relations::RelationsStorage;
use synapse_storage::room::RoomStorage;
use synapse_storage::room_summary::RoomSummaryStorage;
use synapse_storage::user::UserStorage;
use synapse_storage::CreateEventParams;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

async fn setup_test_database(pool: &Arc<sqlx::PgPool>) {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS users (
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
        CREATE TABLE IF NOT EXISTS rooms (
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
        CREATE TABLE IF NOT EXISTS room_memberships (
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
        CREATE TABLE IF NOT EXISTS events (
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
        CREATE TABLE IF NOT EXISTS application_services (
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
        CREATE TABLE IF NOT EXISTS application_service_user_namespaces (
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
        CREATE TABLE IF NOT EXISTS application_service_room_alias_namespaces (
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
        CREATE TABLE IF NOT EXISTS application_service_room_namespaces (
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
        CREATE TABLE IF NOT EXISTS application_service_events (
            id BIGSERIAL PRIMARY KEY,
            event_id TEXT NOT NULL UNIQUE,
            as_id TEXT NOT NULL,
            room_id TEXT,
            event_type TEXT,
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
        CREATE TABLE IF NOT EXISTS application_service_transactions (
            id BIGSERIAL PRIMARY KEY,
            as_id TEXT NOT NULL,
            txn_id TEXT NOT NULL UNIQUE,
            transaction_id TEXT,
            data JSONB NOT NULL DEFAULT '{}',
            events JSONB,
            sent_ts BIGINT NOT NULL,
            is_processed BOOLEAN NOT NULL DEFAULT FALSE,
            processed_ts BIGINT,
            completed_ts BIGINT,
            retry_count INTEGER NOT NULL DEFAULT 0,
            last_error TEXT,
            created_ts BIGINT NOT NULL
        )
    "#,
    )
    .execute(pool.as_ref())
    .await
    .expect("Failed to create application_service_transactions table");

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS application_service_state (
            id BIGSERIAL PRIMARY KEY,
            as_id TEXT NOT NULL,
            state_key TEXT NOT NULL,
            value JSONB NOT NULL,
            state_value TEXT,
            updated_ts BIGINT NOT NULL,
            CONSTRAINT uq_application_service_state_as_key UNIQUE (as_id, state_key)
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
    build_room_service(pool, cache, None)
}

fn create_room_service_with_appservice(
    pool: &Arc<sqlx::PgPool>,
    cache: Arc<CacheManager>,
    app_service_manager: Arc<ApplicationServiceManager>,
) -> RoomService {
    build_room_service(pool, cache, Some(app_service_manager))
}

fn build_room_service(
    pool: &Arc<sqlx::PgPool>,
    cache: Arc<CacheManager>,
    app_service_manager: Option<Arc<ApplicationServiceManager>>,
) -> RoomService {
    let member_storage = Arc::new(RoomMemberStorage::new(pool, "localhost"));
    let event_storage: Arc<synapse_storage::event::EventStorage> =
        Arc::new(EventStorage::new(pool, "localhost".to_string()));
    let canonical_cache = cache;
    let room_summary_storage = Arc::new(RoomSummaryStorage::new(pool));
    let room_summary_service =
        Arc::new(RoomSummaryService::new(room_summary_storage, event_storage.clone(), Some(member_storage.clone())));

    RoomService::new(synapse_services::room_service::RoomServiceConfig {
        room_storage: Arc::new(RoomStorage::new(pool)),
        member_storage,
        event_storage,
        room_tag_storage: Arc::new(synapse_storage::room_tag::RoomTagStorage::new(pool.clone())),
        user_storage: Arc::new(UserStorage::new(pool, canonical_cache.clone())),
        auth_service: Arc::new(synapse_services::auth::AuthService::new(
            pool,
            canonical_cache,
            Arc::new(synapse_rust::common::metrics::MetricsCollector::new()),
            &synapse_rust::common::config::SecurityConfig::default(),
            "localhost",
        )),
        room_summary_service,
        validator: Arc::new(Validator::default()),
        server_name: "localhost".to_string(),
        task_queue: None,
        relations_storage: Arc::new(RelationsStorage::new(pool)),
        event_broadcaster: Some(Arc::new(EventBroadcaster::new("localhost".to_string()))),
        app_service_manager,
        key_rotation_manager: None,
        federation_client: None,
        beacon_service: None,
    })
}

async fn register_test_appservice(pool: &Arc<sqlx::PgPool>, as_id: &str) -> Arc<ApplicationServiceManager> {
    register_test_appservice_with(
        pool,
        RegisterApplicationServiceRequest {
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
        },
    )
    .await
}

async fn register_test_appservice_with(
    pool: &Arc<sqlx::PgPool>,
    request: RegisterApplicationServiceRequest,
) -> Arc<ApplicationServiceManager> {
    let manager = create_test_appservice_manager(pool);
    manager.register(request).await.expect("Failed to register test application service");
    manager
}

fn create_test_appservice_manager(pool: &Arc<sqlx::PgPool>) -> Arc<ApplicationServiceManager> {
    Arc::new(ApplicationServiceManager::new(
        Arc::new(ApplicationServiceStorage::new(pool)),
        Arc::new(EventStorage::new(pool, "localhost".to_string())),
        "localhost".to_string(),
    ))
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

    let result = room_service.lifecycle.create_room(&alice_id, config).await;
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
    let as_id = format!("room-create-bridge-{id}");
    let storage = ApplicationServiceStorage::new(&pool);
    let manager = register_test_appservice(&pool, &as_id).await;
    let room_service = create_room_service_with_appservice(&pool, cache, manager);

    let room_val = room_service
        .lifecycle
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
        room_version: Some("10".to_string()),
        creation_content: Some(json!({
            "creator": "@mallory:localhost",
            "room_version": "1",
            "m.federate": false,
        })),
        ..Default::default()
    };
    let room_val = room_service.lifecycle.create_room(&alice_id, config).await.unwrap();
    let room_id = room_val["room_id"].as_str().unwrap();

    let event_storage = EventStorage::new(&pool, "localhost".to_string());
    let create_events = event_storage.get_state_events_by_type(room_id, "m.room.create").await.unwrap();
    let create_event = create_events
        .iter()
        .find(|event| event.state_key.as_deref() == Some(""))
        .expect("room should have create state");

    assert_eq!(create_event.content["creator"].as_str(), Some(alice_id.as_str()));
    assert_eq!(create_event.content["room_version"].as_str(), Some("10"));
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
    let room_val = room_service.lifecycle.create_room(&alice_id, config).await.unwrap();
    let room_id = room_val["room_id"].as_str().unwrap();

    let result = room_service.membership.join_room(room_id, &bob_id).await;
    assert!(result.is_ok(), "join_room failed: {:?}", result.err());

    let members = room_service.membership.get_room_members(room_id, &alice_id).await.unwrap();
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
    let as_id = format!("join-bridge-{id}");
    let storage = ApplicationServiceStorage::new(&pool);
    let manager = register_test_appservice(&pool, &as_id).await;
    let room_service = create_room_service_with_appservice(&pool, cache, manager);

    let room_val = room_service
        .lifecycle
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

    room_service.membership.join_room(room_id, &bob_id).await.expect("join_room should succeed");

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
    let room_val = room_service.lifecycle.create_room(&alice_id, config).await.unwrap();
    let room_id = room_val["room_id"].as_str().unwrap();

    let content = json!({"msgtype": "m.text", "body": "Hello world"});
    let result = room_service.messaging.send_message(room_id, &alice_id, "m.room.message", &content).await;
    assert!(result.is_ok());
    let val = result.unwrap();
    assert!(val["event_id"].as_str().unwrap().starts_with('$'));

    let messages = room_service.messaging.get_room_messages(room_id, &alice_id, 0, 10, "b").await.unwrap();
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
    let room_val = room_service.lifecycle.create_room(&alice_id, config).await.unwrap();
    let room_id = room_val["room_id"].as_str().unwrap();
    let base_ts = chrono::Utc::now().timestamp_millis() + 10_000;

    for ts in [base_ts + 1000, base_ts + 2000, base_ts + 3000] {
        room_service
            .messaging
            .create_event(
                CreateEventParams {
                    event_id: format!("$timeline_{id}_{ts}"),
                    room_id: room_id.to_string(),
                    user_id: alice_id.clone(),
                    event_type: "m.room.message".to_string(),
                    content: json!({"msgtype": "m.text", "body": format!("msg-{ts}")}),
                    state_key: None,
                    origin_server_ts: ts,
                    redacts: None,
                },
                None,
            )
            .await
            .unwrap();
    }

    let messages = room_service.messaging.get_room_messages(room_id, &alice_id, base_ts + 3000, 2, "b").await.unwrap();

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
    let room_val = room_service.lifecycle.create_room(&alice_id, config).await.unwrap();
    let room_id = room_val["room_id"].as_str().unwrap();
    let base_ts = chrono::Utc::now().timestamp_millis() + 10_000;

    for ts in [base_ts + 1000, base_ts + 2000, base_ts + 3000] {
        room_service
            .messaging
            .create_event(
                CreateEventParams {
                    event_id: format!("$forward_timeline_{id}_{ts}"),
                    room_id: room_id.to_string(),
                    user_id: alice_id.clone(),
                    event_type: "m.room.message".to_string(),
                    content: json!({"msgtype": "m.text", "body": format!("msg-{ts}")}),
                    state_key: None,
                    origin_server_ts: ts,
                    redacts: None,
                },
                None,
            )
            .await
            .unwrap();
    }

    let messages = room_service.messaging.get_room_messages(room_id, &alice_id, base_ts + 1000, 2, "f").await.unwrap();

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
    let room_val = room_service.lifecycle.create_room(&alice_id, config).await.unwrap();
    let room_id = room_val["room_id"].as_str().unwrap();

    let result = room_service.membership.invite_user(room_id, &alice_id, &bob_id).await;
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
    let as_id = format!("invite-bridge-{id}");
    let storage = ApplicationServiceStorage::new(&pool);
    let manager = register_test_appservice(&pool, &as_id).await;
    let room_service = create_room_service_with_appservice(&pool, cache, manager);

    let room_val = room_service
        .lifecycle
        .create_room(&alice_id, CreateRoomConfig::default())
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

    room_service.membership.invite_user(room_id, &alice_id, &bob_id).await.expect("invite_user should succeed");

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
    let room_val = room_service.lifecycle.create_room(&alice_id, config).await.unwrap();
    let room_id = room_val["room_id"].as_str().unwrap();

    let result = room_service.membership.ban_user(room_id, &bob_id, &alice_id, Some("Spam")).await;
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

    let config = CreateRoomConfig { room_version: Some("9".to_string()), ..Default::default() };
    let room_val = room_service.lifecycle.create_room(&alice_id, config).await.unwrap();
    let old_room_id = room_val["room_id"].as_str().unwrap();

    let result = room_service.upgrade_room(old_room_id, "10", &alice_id).await;

    assert!(result.is_ok());
    let new_room_id = result.unwrap();
    assert!(!new_room_id.is_empty());
    assert_ne!(new_room_id, old_room_id);

    let room_storage = RoomStorage::new(&pool);
    let old_room = room_storage.get_room(old_room_id).await.unwrap().expect("old room should still exist");
    assert_eq!(old_room.room_version, "9");

    let new_room = room_storage.get_room(&new_room_id).await.unwrap().expect("replacement room should exist");
    assert_eq!(new_room.room_version, "10");

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
    let as_id = format!("upgrade-bridge-{id}");
    let storage = ApplicationServiceStorage::new(&pool);
    let manager = register_test_appservice(&pool, &as_id).await;
    let room_service = create_room_service_with_appservice(&pool, cache, manager);

    let room_val = room_service
        .lifecycle
        .create_room(&alice_id, CreateRoomConfig::default())
        .await
        .expect("room creation should succeed");
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
async fn test_appservice_successful_delivery_completes_transaction_and_marks_event_processed() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup_appservice_test_database(&pool).await;

    let mock_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&mock_server).await;

    let manager = create_test_appservice_manager(&pool);

    let as_id = format!("successful-delivery-bridge-{}", unique_id());
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: as_id.clone(),
            url: mock_server.uri(),
            as_token: format!("as_token_{as_id}"),
            hs_token: format!("hs_token_{as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("successful bridge".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [{"exclusive": false, "regex": "^@alice:localhost$"}],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": "^!background-test:localhost$"}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("appservice registration should succeed");

    manager
        .push_event(
            &as_id,
            "!success-test:localhost",
            "m.room.message",
            "@alice:localhost",
            json!({
                "msgtype": "m.text",
                "body": "successful delivery should complete transaction"
            }),
            None,
        )
        .await
        .expect("event enqueue should succeed");

    let dispatched = manager.process_pending_for_service(&as_id, 16).await.expect("delivery should succeed");
    assert_eq!(dispatched, 1);

    let pending_transactions = ApplicationServiceStorage::new(&pool)
        .get_pending_transactions(&as_id)
        .await
        .expect("pending transactions should load");
    assert!(pending_transactions.is_empty(), "successful delivery should complete the transaction");

    let pending_events =
        ApplicationServiceStorage::new(&pool).get_pending_events(&as_id, 10).await.expect("pending events should load");
    assert!(pending_events.is_empty(), "successful delivery should mark pending events as processed");

    let delivery_status = manager
        .get_state(&as_id, "delivery_status")
        .await
        .expect("delivery status lookup should succeed")
        .expect("delivery status should be persisted");
    assert_eq!(delivery_status.state_value, "up");

    let service =
        manager.get(&as_id).await.expect("service lookup should succeed").expect("service should still exist");
    assert!(service.is_enabled, "successful delivery should keep service enabled");

    let requests = mock_server.received_requests().await.expect("wiremock requests should be available");
    assert_eq!(requests.len(), 1, "sender should emit exactly one HTTP transaction");
    assert_eq!(requests[0].method.as_str(), "PUT");
    assert!(requests[0].url.path().starts_with("/transactions/"));

    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).expect("request body should be valid json");
    let events = body["events"].as_array().expect("events payload should be an array");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["room_id"], "!success-test:localhost");
    assert_eq!(events[0]["type"], "m.room.message");
}

#[tokio::test]
async fn test_bridge_e2e_send_message_delivers_real_room_event_payload() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup_appservice_test_database(&pool).await;

    let id = unique_id();
    let alice_id = format!("@alice_{id}:localhost");
    let alice_name = format!("alice_{id}");
    create_test_user(&pool, &alice_id, &alice_name).await;

    let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
    let room_service = create_room_service(&pool, cache.clone());
    let room_val = room_service
        .lifecycle
        .create_room(&alice_id, CreateRoomConfig::default())
        .await
        .expect("room creation should succeed");
    let room_id = room_val["room_id"].as_str().expect("room_id should be present").to_string();

    let mock_server = MockServer::start().await;
    let bridge_as_id = format!("bridge-e2e-{id}");
    let manager = register_test_appservice_with(
        &pool,
        RegisterApplicationServiceRequest {
            as_id: bridge_as_id.clone(),
            url: mock_server.uri(),
            as_token: format!("as_token_{bridge_as_id}"),
            hs_token: format!("hs_token_{bridge_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("bridge e2e".to_string()),
            is_rate_limited: Some(false),
            protocols: Some(vec!["example-bridge".to_string()]),
            namespaces: Some(json!({
                "users": [{"exclusive": false, "regex": format!("^{}$", regex::escape(&alice_id))}],
                "aliases": [],
                "rooms": []
            })),
            api_key: None,
            config: None,
        },
    )
    .await;
    let bridge_room_service = create_room_service_with_appservice(&pool, cache, manager.clone());
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&mock_server).await;

    let content = json!({
        "msgtype": "m.text",
        "body": format!("bridge-e2e-body-{id}")
    });
    let send_result = bridge_room_service
        .messaging
        .send_message(&room_id, &alice_id, "m.room.message", &content)
        .await
        .expect("send_message should succeed");
    let source_event_id = send_result["event_id"].as_str().expect("event id should be returned").to_string();

    manager.clone().start_sender(16, 1).await;

    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let requests = mock_server.received_requests().await.expect("wiremock requests should be available");
            if !requests.is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("bridge should receive appservice transaction");

    let requests = mock_server.received_requests().await.expect("wiremock requests should be available");
    assert_eq!(requests.len(), 1, "bridge should receive exactly one transaction");

    let request_body: serde_json::Value =
        serde_json::from_slice(&requests[0].body).expect("request body should be valid json");
    let events = request_body["events"].as_array().expect("events payload should be an array");
    assert_eq!(events.len(), 1);

    let bridge_event = &events[0];
    assert_eq!(bridge_event["event_id"], source_event_id);
    assert_eq!(bridge_event["room_id"], room_id);
    assert_eq!(bridge_event["type"], "m.room.message");
    assert_eq!(bridge_event["sender"], alice_id);
    assert_eq!(bridge_event["content"]["body"], format!("bridge-e2e-body-{id}"));
    assert!(bridge_event["queue_event_id"].as_str().is_some());

    let pending_transactions = ApplicationServiceStorage::new(&pool)
        .get_pending_transactions(&bridge_as_id)
        .await
        .expect("pending transactions should load");
    assert!(pending_transactions.is_empty(), "successful bridge delivery should complete transaction");

    let pending_events = ApplicationServiceStorage::new(&pool)
        .get_pending_events(&bridge_as_id, 10)
        .await
        .expect("pending events should load");
    assert!(pending_events.is_empty(), "successful bridge delivery should mark queue events processed");

    let delivery_status = manager
        .get_state(&bridge_as_id, "delivery_status")
        .await
        .expect("delivery status lookup should succeed")
        .expect("delivery status should be persisted");
    assert_eq!(delivery_status.state_value, "up");
}

#[tokio::test]
async fn test_bridge_e2e_membership_events_deliver_real_room_member_payloads() {
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
    let room_service = create_room_service(&pool, cache.clone());
    let room_val = room_service
        .lifecycle
        .create_room(&alice_id, CreateRoomConfig::default())
        .await
        .expect("room creation should succeed");
    let room_id = room_val["room_id"].as_str().expect("room_id should be present").to_string();

    let mock_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&mock_server).await;

    let bridge_as_id = format!("bridge-membership-e2e-{id}");
    let manager = register_test_appservice_with(
        &pool,
        RegisterApplicationServiceRequest {
            as_id: bridge_as_id.clone(),
            url: mock_server.uri(),
            as_token: format!("as_token_{bridge_as_id}"),
            hs_token: format!("hs_token_{bridge_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("bridge membership e2e".to_string()),
            is_rate_limited: Some(false),
            protocols: Some(vec!["example-bridge".to_string()]),
            namespaces: Some(json!({
                "users": [{"exclusive": false, "regex": format!("^{}$", regex::escape(&bob_id))}],
                "aliases": [],
                "rooms": []
            })),
            api_key: None,
            config: None,
        },
    )
    .await;
    let bridge_room_service = create_room_service_with_appservice(&pool, cache, manager.clone());

    bridge_room_service.membership.invite_user(&room_id, &alice_id, &bob_id).await.expect("invite_user should succeed");
    bridge_room_service.membership.join_room(&room_id, &bob_id).await.expect("join_room should succeed");

    let dispatched =
        manager.process_pending_for_service(&bridge_as_id, 16).await.expect("membership delivery should succeed");
    assert_eq!(dispatched, 2, "invite and join membership events should be dispatched in one batch");

    let requests = mock_server.received_requests().await.expect("wiremock requests should be available");
    assert_eq!(requests.len(), 1, "membership batch should produce exactly one transaction");

    let request_body: serde_json::Value =
        serde_json::from_slice(&requests[0].body).expect("request body should be valid json");
    let events = request_body["events"].as_array().expect("events payload should be an array");
    let membership_events: Vec<&serde_json::Value> = events
        .iter()
        .filter(|event| event["type"] == "m.room.member" && event["room_id"] == room_id && event["state_key"] == bob_id)
        .collect();
    assert_eq!(membership_events.len(), 2, "bridge should receive invite and join member events");
    assert!(membership_events
        .iter()
        .any(|event| { event["sender"] == alice_id && event["content"]["membership"] == "invite" }));
    assert!(membership_events
        .iter()
        .any(|event| { event["sender"] == bob_id && event["content"]["membership"] == "join" }));
    assert!(
        membership_events.iter().all(|event| event["queue_event_id"].as_str().is_some()),
        "membership payload should keep queue_event_id"
    );

    let pending_events = ApplicationServiceStorage::new(&pool)
        .get_pending_events(&bridge_as_id, 16)
        .await
        .expect("pending events should load");
    assert!(pending_events.is_empty(), "successful bridge delivery should drain membership pending queue");

    let delivery_status = manager
        .get_state(&bridge_as_id, "delivery_status")
        .await
        .expect("delivery status lookup should succeed")
        .expect("delivery status should be persisted");
    assert_eq!(delivery_status.state_value, "up");
}

#[tokio::test]
async fn test_appservice_background_sender_flushes_pending_queue() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup_appservice_test_database(&pool).await;

    let mock_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&mock_server).await;

    let manager = Arc::new(ApplicationServiceManager::new(
        Arc::new(ApplicationServiceStorage::new(&pool)),
        Arc::new(EventStorage::new(&pool, "localhost".to_string())),
        "localhost".to_string(),
    ));

    let as_id = format!("background-sender-bridge-{}", unique_id());
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: as_id.clone(),
            url: mock_server.uri(),
            as_token: format!("as_token_{as_id}"),
            hs_token: format!("hs_token_{as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("background sender bridge".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: None,
            api_key: None,
            config: None,
        })
        .await
        .expect("appservice registration should succeed");

    manager
        .push_event(
            &as_id,
            "!background-test:localhost",
            "m.room.message",
            "@alice:localhost",
            json!({
                "msgtype": "m.text",
                "body": "background sender should flush pending queue"
            }),
            None,
        )
        .await
        .expect("event enqueue should succeed");

    manager.clone().start_sender(16, 1).await;

    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let pending = ApplicationServiceStorage::new(&pool)
                .get_pending_events(&as_id, 10)
                .await
                .expect("pending events should load");
            let requests = mock_server.received_requests().await.expect("wiremock requests should be available");

            if pending.is_empty() && !requests.is_empty() {
                break;
            }

            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("background sender should flush queue within timeout");

    let pending_transactions = ApplicationServiceStorage::new(&pool)
        .get_pending_transactions(&as_id)
        .await
        .expect("pending transactions should load");
    assert!(pending_transactions.is_empty(), "background sender should complete transactions");

    let pending_events =
        ApplicationServiceStorage::new(&pool).get_pending_events(&as_id, 10).await.expect("pending events should load");
    assert!(pending_events.is_empty(), "background sender should clear pending events");

    let requests = mock_server.received_requests().await.expect("wiremock requests should be available");
    assert_eq!(requests.len(), 1, "background sender should emit exactly one HTTP transaction");

    let delivery_status = manager
        .get_state(&as_id, "delivery_status")
        .await
        .expect("delivery status lookup should succeed")
        .expect("delivery status should be persisted");
    assert_eq!(delivery_status.state_value, "up");
}

#[tokio::test]
async fn test_appservice_fatal_delivery_failures_disable_service_and_persist_state() {
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;
    setup_appservice_test_database(&pool).await;

    let failing_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(401)).mount(&failing_server).await;

    let manager = Arc::new(ApplicationServiceManager::new(
        Arc::new(ApplicationServiceStorage::new(&pool)),
        Arc::new(EventStorage::new(&pool, "localhost".to_string())),
        "localhost".to_string(),
    ));

    let as_id = format!("fatal-disable-bridge-{}", unique_id());
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: as_id.clone(),
            url: failing_server.uri(),
            as_token: format!("as_token_{as_id}"),
            hs_token: format!("hs_token_{as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("failing bridge".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: None,
            api_key: None,
            config: None,
        })
        .await
        .expect("appservice registration should succeed");

    manager
        .push_event(
            &as_id,
            "!fatal-test:localhost",
            "m.room.message",
            "@alice:localhost",
            json!({
                "msgtype": "m.text",
                "body": "fatal delivery should disable bridge"
            }),
            None,
        )
        .await
        .expect("event enqueue should succeed");

    let pending_events =
        ApplicationServiceStorage::new(&pool).get_pending_events(&as_id, 10).await.expect("pending events should load");
    assert_eq!(pending_events.len(), 1);

    for attempt in 1..=3 {
        let result = manager.process_pending_for_service(&as_id, 16).await;
        assert!(result.is_err(), "attempt {attempt} should fail");

        if attempt < 3 {
            sqlx::query(
                r"
                UPDATE application_service_transactions
                SET sent_ts = 0
                WHERE as_id = $1 AND completed_ts IS NULL
                ",
            )
            .bind(&as_id)
            .execute(pool.as_ref())
            .await
            .expect("transaction sent_ts rewind should succeed");
        }
    }

    let pending_transactions = ApplicationServiceStorage::new(&pool)
        .get_pending_transactions(&as_id)
        .await
        .expect("pending transactions should load");

    let delivery_status = manager
        .get_state(&as_id, "delivery_status")
        .await
        .expect("delivery status lookup should succeed")
        .expect("delivery status should be persisted");

    let service =
        manager.get(&as_id).await.expect("service lookup should succeed").expect("service should still exist");
    assert!(!service.is_enabled, "service should be disabled after repeated fatal failures");

    let active_services = manager.get_all_active().await.expect("active services lookup should succeed");
    assert!(!active_services.iter().any(|service| service.as_id == as_id));

    assert_eq!(delivery_status.state_value, "disabled");

    let failure_kind = manager
        .get_state(&as_id, "delivery_last_failure_kind")
        .await
        .expect("delivery failure kind lookup should succeed")
        .expect("delivery failure kind should be persisted");
    assert_eq!(failure_kind.state_value, "fatal");

    let disabled_reason = manager
        .get_state(&as_id, "delivery_disabled_reason")
        .await
        .expect("disabled reason lookup should succeed")
        .expect("disabled reason should be persisted");
    assert!(disabled_reason.state_value.contains("threshold reached"));
    assert!(disabled_reason.state_value.contains("401 Unauthorized"));

    assert_eq!(pending_transactions.len(), 1);
    assert_eq!(pending_transactions[0].retry_count, 3);
    assert_eq!(pending_transactions[0].last_error.as_deref(), Some("HTTP 401 Unauthorized: "));
}

#[tokio::test]
async fn test_appservice_scheduler_does_not_block_healthy_service_during_retry_backoff() {
    let pool = crate::require_test_pool().await;
    setup_appservice_test_database(&pool).await;

    let failing_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(503)).mount(&failing_server).await;

    let healthy_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&healthy_server).await;

    let manager = create_test_appservice_manager(&pool);
    let scheduler = ApplicationServiceScheduler::new(manager.clone());

    let failing_as_id = format!("scheduler-backoff-failing-{}", unique_id());
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: failing_as_id.clone(),
            url: failing_server.uri(),
            as_token: format!("as_token_{failing_as_id}"),
            hs_token: format!("hs_token_{failing_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("failing bridge".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": "^!scheduler-failing.*:localhost$"}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("failing appservice registration should succeed");

    let healthy_as_id = format!("scheduler-backoff-healthy-{}", unique_id());
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: healthy_as_id.clone(),
            url: healthy_server.uri(),
            as_token: format!("as_token_{healthy_as_id}"),
            hs_token: format!("hs_token_{healthy_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("healthy bridge".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": "^!scheduler-healthy.*:localhost$"}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("healthy appservice registration should succeed");

    manager
        .push_event(
            &failing_as_id,
            "!scheduler-failing-room:localhost",
            "m.room.message",
            "@bridge:localhost",
            json!({"msgtype": "m.text", "body": "fail once"}),
            None,
        )
        .await
        .expect("failing event enqueue should succeed");
    manager
        .push_event(
            &healthy_as_id,
            "!scheduler-healthy-room:localhost",
            "m.room.message",
            "@bridge:localhost",
            json!({"msgtype": "m.text", "body": "healthy first"}),
            None,
        )
        .await
        .expect("healthy event enqueue should succeed");

    scheduler.run_once().await.expect("first scheduler tick should complete");

    let failing_transactions = ApplicationServiceStorage::new(&pool)
        .get_pending_transactions(&failing_as_id)
        .await
        .expect("failing transactions should load");
    assert_eq!(failing_transactions.len(), 1);
    assert_eq!(failing_transactions[0].retry_count, 1);

    manager
        .push_event(
            &healthy_as_id,
            "!scheduler-healthy-room:localhost",
            "m.room.message",
            "@bridge:localhost",
            json!({"msgtype": "m.text", "body": "healthy second"}),
            None,
        )
        .await
        .expect("second healthy event enqueue should succeed");

    scheduler.run_once().await.expect("second scheduler tick should complete");

    let failing_requests = failing_server.received_requests().await.expect("failing server requests should load");
    let healthy_requests = healthy_server.received_requests().await.expect("healthy server requests should load");
    assert_eq!(failing_requests.len(), 1, "backoff should prevent an immediate second retry");
    assert_eq!(healthy_requests.len(), 2, "healthy AS should continue progressing while another AS is backing off");

    let failing_result = manager
        .get_state(&failing_as_id, "scheduler_last_result")
        .await
        .expect("failing result lookup should succeed")
        .expect("failing result should be persisted");
    assert_eq!(failing_result.state_value, "backoff");

    let failing_txn_state = manager
        .get_state(&failing_as_id, "scheduler_transaction_state")
        .await
        .expect("failing transaction state lookup should succeed")
        .expect("failing transaction state should be persisted");
    assert_eq!(failing_txn_state.state_value, "retry_backoff");

    let failing_backoff_count = manager
        .get_state(&failing_as_id, "scheduler_total_backoff_count")
        .await
        .expect("failing backoff count lookup should succeed")
        .expect("failing backoff count should be persisted");
    assert_eq!(failing_backoff_count.state_value, "1");

    let healthy_success_count = manager
        .get_state(&healthy_as_id, "scheduler_total_success_count")
        .await
        .expect("healthy success count lookup should succeed")
        .expect("healthy success count should be persisted");
    assert_eq!(healthy_success_count.state_value, "2");

    let remaining_healthy_pending = ApplicationServiceStorage::new(&pool)
        .get_pending_events(&healthy_as_id, 10)
        .await
        .expect("healthy pending events should load");
    assert!(remaining_healthy_pending.is_empty(), "healthy AS queue should be drained despite failing peer");
}

#[tokio::test]
async fn test_appservice_scheduler_keeps_single_pending_transaction_per_service() {
    let pool = crate::require_test_pool().await;
    setup_appservice_test_database(&pool).await;

    let failing_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(503)).mount(&failing_server).await;

    let manager = create_test_appservice_manager(&pool);
    let scheduler = ApplicationServiceScheduler::new(manager.clone());
    let as_id = format!("scheduler-txn-controller-{}", unique_id());

    manager
        .register(RegisterApplicationServiceRequest {
            as_id: as_id.clone(),
            url: failing_server.uri(),
            as_token: format!("as_token_{as_id}"),
            hs_token: format!("hs_token_{as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("txn controller bridge".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": "^!scheduler-controller.*:localhost$"}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("appservice registration should succeed");

    manager
        .push_event(
            &as_id,
            "!scheduler-controller-room:localhost",
            "m.room.message",
            "@bridge:localhost",
            json!({"msgtype": "m.text", "body": "first"}),
            None,
        )
        .await
        .expect("first event enqueue should succeed");

    scheduler.run_once().await.expect("first scheduler tick should complete");

    manager
        .push_event(
            &as_id,
            "!scheduler-controller-room:localhost",
            "m.room.message",
            "@bridge:localhost",
            json!({"msgtype": "m.text", "body": "second"}),
            None,
        )
        .await
        .expect("second event enqueue should succeed");

    scheduler.run_once().await.expect("second scheduler tick should complete");

    let pending_transactions = ApplicationServiceStorage::new(&pool)
        .get_pending_transactions(&as_id)
        .await
        .expect("pending transactions should load");
    assert_eq!(
        pending_transactions.len(),
        1,
        "scheduler should retry the existing transaction instead of creating a second one"
    );
    assert_eq!(pending_transactions[0].retry_count, 1, "backoff window should block immediate retry on second tick");

    let pending_events =
        ApplicationServiceStorage::new(&pool).get_pending_events(&as_id, 10).await.expect("pending events should load");
    assert_eq!(pending_events.len(), 2, "events should remain queued behind the single pending transaction");

    let requests = failing_server.received_requests().await.expect("failing server requests should load");
    assert_eq!(requests.len(), 1, "controller should avoid immediate duplicate sends for the same pending transaction");

    let txn_state = manager
        .get_state(&as_id, "scheduler_transaction_state")
        .await
        .expect("transaction state lookup should succeed")
        .expect("transaction state should be persisted");
    assert_eq!(txn_state.state_value, "retry_backoff");

    let failure_count = manager
        .get_state(&as_id, "scheduler_total_failure_count")
        .await
        .expect("failure count lookup should succeed")
        .expect("failure count should be persisted");
    assert_eq!(failure_count.state_value, "1");
}

#[tokio::test]
async fn test_appservice_scheduler_prioritizes_pending_transactions_over_pending_events() {
    let pool = crate::require_test_pool().await;
    setup_appservice_test_database(&pool).await;

    let transaction_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&transaction_server).await;

    let event_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&event_server).await;

    let manager = create_test_appservice_manager(&pool);
    let scheduler = ApplicationServiceScheduler::with_capacity_options(manager.clone(), 16, 500, 1, 50, 2);
    let storage = ApplicationServiceStorage::new(&pool);

    let transaction_as_id = format!("scheduler-priority-txn-{}", unique_id());
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: transaction_as_id.clone(),
            url: transaction_server.uri(),
            as_token: format!("as_token_{transaction_as_id}"),
            hs_token: format!("hs_token_{transaction_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("priority transaction bridge".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": "^!scheduler-priority-txn.*:localhost$"}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("transaction-priority appservice registration should succeed");

    let event_as_id = format!("scheduler-priority-event-{}", unique_id());
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: event_as_id.clone(),
            url: event_server.uri(),
            as_token: format!("as_token_{event_as_id}"),
            hs_token: format!("hs_token_{event_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("priority event bridge".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": "^!scheduler-priority-event.*:localhost$"}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("event-priority appservice registration should succeed");

    storage
        .create_transaction(
            &transaction_as_id,
            &format!("scheduler-priority-existing-txn-{}", unique_id()),
            &[json!({"type": "m.room.message", "content": {"body": "existing transaction"}})],
        )
        .await
        .expect("existing transaction should be created");
    sqlx::query(
        r"
        UPDATE application_service_transactions
        SET sent_ts = 0
        WHERE as_id = $1 AND completed_ts IS NULL
        ",
    )
    .bind(&transaction_as_id)
    .execute(pool.as_ref())
    .await
    .expect("existing transaction should be rewound into ready-to-retry state");

    manager
        .push_event(
            &event_as_id,
            "!scheduler-priority-event-room:localhost",
            "m.room.message",
            "@bridge:localhost",
            json!({"msgtype": "m.text", "body": "queued event"}),
            None,
        )
        .await
        .expect("pending event enqueue should succeed");

    scheduler.run_once().await.expect("priority scheduler tick should complete");

    let transaction_requests = transaction_server.received_requests().await.expect("transaction requests should load");
    let event_requests = event_server.received_requests().await.expect("event requests should load");
    assert_eq!(transaction_requests.len(), 1, "pending transaction should be dispatched first");
    assert_eq!(event_requests.len(), 0, "pending events service should be deferred when only one slot is available");

    let transaction_result = manager
        .get_state(&transaction_as_id, "scheduler_last_result")
        .await
        .expect("transaction result lookup should succeed")
        .expect("transaction result should be persisted");
    assert_eq!(transaction_result.state_value, "dispatched");

    let transaction_state = manager
        .get_state(&transaction_as_id, "scheduler_transaction_state")
        .await
        .expect("transaction state lookup should succeed")
        .expect("transaction state should be persisted");
    assert_eq!(transaction_state.state_value, "idle");

    let event_result = manager
        .get_state(&event_as_id, "scheduler_last_result")
        .await
        .expect("event result lookup should succeed")
        .expect("event result should be persisted");
    assert_eq!(event_result.state_value, "capacity_limited");

    let event_txn_state = manager
        .get_state(&event_as_id, "scheduler_transaction_state")
        .await
        .expect("event transaction state lookup should succeed")
        .expect("event transaction state should be persisted");
    assert_eq!(event_txn_state.state_value, "capacity_limited");

    let event_pending_events = manager
        .get_state(&event_as_id, "scheduler_pending_event_count")
        .await
        .expect("pending event count lookup should succeed")
        .expect("pending event count should be persisted");
    assert_eq!(event_pending_events.state_value, "1");
}

#[tokio::test]
async fn test_appservice_scheduler_keeps_pending_transactions_ahead_of_event_bucket_across_rotation() {
    let pool = crate::require_test_pool().await;
    setup_appservice_test_database(&pool).await;

    let txn_a_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&txn_a_server).await;

    let txn_b_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&txn_b_server).await;

    let event_a_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&event_a_server).await;

    let event_b_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&event_b_server).await;

    let manager = create_test_appservice_manager(&pool);
    let scheduler = ApplicationServiceScheduler::with_capacity_options(manager.clone(), 16, 500, 1, 50, 2);
    let storage = ApplicationServiceStorage::new(&pool);

    let scenario_id = unique_id();

    let txn_a_as_id = format!("scheduler-rotation-txn-a-{}", unique_id());
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: txn_a_as_id.clone(),
            url: txn_a_server.uri(),
            as_token: format!("as_token_{txn_a_as_id}"),
            hs_token: format!("hs_token_{txn_a_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("rotation transaction bridge a".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": format!("^!scheduler-rotation-txn-a-{scenario_id}.*:localhost$")}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("transaction bridge a should register");

    let txn_b_as_id = format!("scheduler-rotation-txn-b-{}", unique_id());
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: txn_b_as_id.clone(),
            url: txn_b_server.uri(),
            as_token: format!("as_token_{txn_b_as_id}"),
            hs_token: format!("hs_token_{txn_b_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("rotation transaction bridge b".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": format!("^!scheduler-rotation-txn-b-{scenario_id}.*:localhost$")}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("transaction bridge b should register");

    let event_a_as_id = format!("scheduler-rotation-event-a-{}", unique_id());
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: event_a_as_id.clone(),
            url: event_a_server.uri(),
            as_token: format!("as_token_{event_a_as_id}"),
            hs_token: format!("hs_token_{event_a_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("rotation event bridge a".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": format!("^!scheduler-rotation-event-a-{scenario_id}.*:localhost$")}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("event bridge a should register");

    let event_b_as_id = format!("scheduler-rotation-event-b-{}", unique_id());
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: event_b_as_id.clone(),
            url: event_b_server.uri(),
            as_token: format!("as_token_{event_b_as_id}"),
            hs_token: format!("hs_token_{event_b_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("rotation event bridge b".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": format!("^!scheduler-rotation-event-b-{scenario_id}.*:localhost$")}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("event bridge b should register");

    for transaction_index in 0..2 {
        storage
            .create_transaction(
                &txn_a_as_id,
                &format!("scheduler-rotation-a-{scenario_id}-{transaction_index}"),
                &[json!({"type": "m.room.message", "content": {"body": format!("txn-a-{transaction_index}")}})],
            )
            .await
            .expect("transaction bridge a pending transaction should be created");
        storage
            .create_transaction(
                &txn_b_as_id,
                &format!("scheduler-rotation-b-{scenario_id}-{transaction_index}"),
                &[json!({"type": "m.room.message", "content": {"body": format!("txn-b-{transaction_index}")}})],
            )
            .await
            .expect("transaction bridge b pending transaction should be created");
    }

    sqlx::query(
        r"
        UPDATE application_service_transactions
        SET sent_ts = 0
        WHERE as_id = ANY($1) AND completed_ts IS NULL
        ",
    )
    .bind(vec![txn_a_as_id.clone(), txn_b_as_id.clone()])
    .execute(pool.as_ref())
    .await
    .expect("pending transactions should be rewound into ready-to-retry state");

    manager
        .push_event(
            &event_a_as_id,
            &format!("!scheduler-rotation-event-a-{scenario_id}:localhost"),
            "m.room.message",
            "@bridge:localhost",
            json!({"msgtype": "m.text", "body": "event-a"}),
            None,
        )
        .await
        .expect("event bridge a pending event should enqueue");
    manager
        .push_event(
            &event_b_as_id,
            &format!("!scheduler-rotation-event-b-{scenario_id}:localhost"),
            "m.room.message",
            "@bridge:localhost",
            json!({"msgtype": "m.text", "body": "event-b"}),
            None,
        )
        .await
        .expect("event bridge b pending event should enqueue");

    scheduler.run_once().await.expect("rotation tick one should complete");
    scheduler.run_once().await.expect("rotation tick two should complete");
    scheduler.run_once().await.expect("rotation tick three should complete");

    let txn_a_requests = txn_a_server.received_requests().await.expect("transaction bridge a requests should load");
    let txn_b_requests = txn_b_server.received_requests().await.expect("transaction bridge b requests should load");
    let event_a_requests = event_a_server.received_requests().await.expect("event bridge a requests should load");
    let event_b_requests = event_b_server.received_requests().await.expect("event bridge b requests should load");

    assert_eq!(
        txn_a_requests.len() + txn_b_requests.len(),
        3,
        "first three single-slot ticks should all be spent on pending-transaction services"
    );
    assert!(
        !txn_a_requests.is_empty() && !txn_b_requests.is_empty(),
        "round-robin should rotate across the pending-transaction bucket before considering pending-events-only services"
    );
    assert_eq!(
        event_a_requests.len() + event_b_requests.len(),
        0,
        "pending-events-only services should remain deferred while any pending transactions are still ready"
    );

    let event_a_limited_count = manager
        .get_state(&event_a_as_id, "scheduler_total_capacity_limited_count")
        .await
        .expect("event bridge a capacity count lookup should succeed")
        .expect("event bridge a capacity count should be persisted");
    let event_b_limited_count = manager
        .get_state(&event_b_as_id, "scheduler_total_capacity_limited_count")
        .await
        .expect("event bridge b capacity count lookup should succeed")
        .expect("event bridge b capacity count should be persisted");
    assert!(
        event_a_limited_count.state_value.parse::<i64>().unwrap_or(0) >= 1
            || event_b_limited_count.state_value.parse::<i64>().unwrap_or(0) >= 1,
        "event-only services should still be observed as capacity-limited while the transaction bucket drains"
    );
}

#[tokio::test]
async fn test_appservice_scheduler_capacity_limit_persists_state() {
    let pool = crate::require_test_pool().await;
    setup_appservice_test_database(&pool).await;

    let first_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&first_server).await;

    let second_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&second_server).await;

    let manager = create_test_appservice_manager(&pool);
    let scheduler = ApplicationServiceScheduler::with_capacity_options(manager.clone(), 16, 500, 1, 1, 1);

    let first_as_id = format!("capacity-a-{}", unique_id());
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: first_as_id.clone(),
            url: first_server.uri(),
            as_token: format!("as_token_{first_as_id}"),
            hs_token: format!("hs_token_{first_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("capacity first".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": "^!capacity-a.*:localhost$"}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("first appservice registration should succeed");

    let second_as_id = format!("capacity-b-{}", unique_id());
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: second_as_id.clone(),
            url: second_server.uri(),
            as_token: format!("as_token_{second_as_id}"),
            hs_token: format!("hs_token_{second_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("capacity second".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": "^!capacity-b.*:localhost$"}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("second appservice registration should succeed");

    manager
        .push_event(
            &first_as_id,
            "!capacity-a-room:localhost",
            "m.room.message",
            "@bridge:localhost",
            json!({"msgtype": "m.text", "body": "first"}),
            None,
        )
        .await
        .expect("first event enqueue should succeed");
    manager
        .push_event(
            &second_as_id,
            "!capacity-b-room:localhost",
            "m.room.message",
            "@bridge:localhost",
            json!({"msgtype": "m.text", "body": "second"}),
            None,
        )
        .await
        .expect("second event enqueue should succeed");

    scheduler.run_once().await.expect("capacity-limited scheduler tick should complete");

    let first_requests = first_server.received_requests().await.expect("first requests should load");
    let second_requests = second_server.received_requests().await.expect("second requests should load");
    assert_eq!(
        first_requests.len() + second_requests.len(),
        1,
        "scheduler should only dispatch one service when max_services_per_tick=1"
    );

    let (dispatched_as_id, limited_as_id) = if first_requests.len() == 1 {
        (first_as_id.as_str(), second_as_id.as_str())
    } else {
        (second_as_id.as_str(), first_as_id.as_str())
    };

    let dispatched_result = manager
        .get_state(dispatched_as_id, "scheduler_last_result")
        .await
        .expect("dispatched result lookup should succeed")
        .expect("dispatched result should be persisted");
    assert_eq!(dispatched_result.state_value, "dispatched");

    let limited_result = manager
        .get_state(limited_as_id, "scheduler_last_result")
        .await
        .expect("limited result lookup should succeed")
        .expect("limited result should be persisted");
    assert_eq!(limited_result.state_value, "capacity_limited");

    let limited_txn_state = manager
        .get_state(limited_as_id, "scheduler_transaction_state")
        .await
        .expect("limited transaction state lookup should succeed")
        .expect("limited transaction state should be persisted");
    assert_eq!(limited_txn_state.state_value, "capacity_limited");

    let limited_pending_events = manager
        .get_state(limited_as_id, "scheduler_pending_event_count")
        .await
        .expect("pending event count lookup should succeed")
        .expect("pending event count should be persisted");
    assert_eq!(limited_pending_events.state_value, "1");

    let limited_backlog_state = manager
        .get_state(limited_as_id, "scheduler_backlog_state")
        .await
        .expect("backlog state lookup should succeed")
        .expect("backlog state should be persisted");
    assert_eq!(limited_backlog_state.state_value, "high");

    let limited_last_tick = manager
        .get_state(limited_as_id, "scheduler_last_tick_ts")
        .await
        .expect("last tick lookup should succeed")
        .expect("last tick should be persisted");
    assert!(
        limited_last_tick.state_value.parse::<i64>().is_ok(),
        "scheduler_last_tick_ts should persist a millisecond timestamp"
    );

    let limited_count = manager
        .get_state(limited_as_id, "scheduler_total_capacity_limited_count")
        .await
        .expect("capacity limited count lookup should succeed")
        .expect("capacity limited count should be persisted");
    assert_eq!(limited_count.state_value, "1");

    let dispatched_success_count = manager
        .get_state(dispatched_as_id, "scheduler_total_success_count")
        .await
        .expect("success count lookup should succeed")
        .expect("success count should be persisted");
    assert_eq!(dispatched_success_count.state_value, "1");
}

#[tokio::test]
async fn test_appservice_scheduler_rotates_capacity_limited_service_under_sustained_backlog() {
    let pool = crate::require_test_pool().await;
    setup_appservice_test_database(&pool).await;

    let first_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&first_server).await;

    let second_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&second_server).await;

    let third_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&third_server).await;

    let manager = create_test_appservice_manager(&pool);
    let scheduler = ApplicationServiceScheduler::with_capacity_options(manager.clone(), 1, 500, 2, 2, 2);

    let first_as_id = format!("sustained-a-{}", unique_id());
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: first_as_id.clone(),
            url: first_server.uri(),
            as_token: format!("as_token_{first_as_id}"),
            hs_token: format!("hs_token_{first_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("sustained first".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": "^!sustained-a.*:localhost$"}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("first appservice registration should succeed");

    let second_as_id = format!("sustained-b-{}", unique_id());
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: second_as_id.clone(),
            url: second_server.uri(),
            as_token: format!("as_token_{second_as_id}"),
            hs_token: format!("hs_token_{second_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("sustained second".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": "^!sustained-b.*:localhost$"}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("second appservice registration should succeed");

    let third_as_id = format!("sustained-c-{}", unique_id());
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: third_as_id.clone(),
            url: third_server.uri(),
            as_token: format!("as_token_{third_as_id}"),
            hs_token: format!("hs_token_{third_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("sustained third".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": "^!sustained-c.*:localhost$"}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("third appservice registration should succeed");

    for event_index in 0..3 {
        manager
            .push_event(
                &first_as_id,
                "!sustained-a-room:localhost",
                "m.room.message",
                "@bridge:localhost",
                json!({"msgtype": "m.text", "body": format!("first-{event_index}")}),
                None,
            )
            .await
            .expect("first event enqueue should succeed");
        manager
            .push_event(
                &second_as_id,
                "!sustained-b-room:localhost",
                "m.room.message",
                "@bridge:localhost",
                json!({"msgtype": "m.text", "body": format!("second-{event_index}")}),
                None,
            )
            .await
            .expect("second event enqueue should succeed");
        manager
            .push_event(
                &third_as_id,
                "!sustained-c-room:localhost",
                "m.room.message",
                "@bridge:localhost",
                json!({"msgtype": "m.text", "body": format!("third-{event_index}")}),
                None,
            )
            .await
            .expect("third event enqueue should succeed");
    }

    scheduler.run_once().await.expect("first sustained-backlog tick should complete");

    let first_tick_counts = [
        first_server.received_requests().await.expect("first requests should load").len(),
        second_server.received_requests().await.expect("second requests should load").len(),
        third_server.received_requests().await.expect("third requests should load").len(),
    ];
    assert_eq!(
        first_tick_counts.iter().sum::<usize>(),
        2,
        "scheduler should only dispatch two services per tick when max_services_per_tick=2"
    );
    assert_eq!(
        first_tick_counts.iter().filter(|count| **count == 0).count(),
        1,
        "exactly one service should be deferred by the capacity limit on the first tick"
    );

    let (initially_limited_as_id, initially_limited_server) = if first_tick_counts[0] == 0 {
        (&first_as_id, &first_server)
    } else if first_tick_counts[1] == 0 {
        (&second_as_id, &second_server)
    } else {
        (&third_as_id, &third_server)
    };

    let limited_result = manager
        .get_state(initially_limited_as_id, "scheduler_last_result")
        .await
        .expect("limited result lookup should succeed")
        .expect("limited result should be persisted");
    assert_eq!(limited_result.state_value, "capacity_limited");

    let limited_pending_events = manager
        .get_state(initially_limited_as_id, "scheduler_pending_event_count")
        .await
        .expect("limited pending events lookup should succeed")
        .expect("limited pending events should be persisted");
    assert_eq!(limited_pending_events.state_value, "3");

    let limited_backlog_state = manager
        .get_state(initially_limited_as_id, "scheduler_backlog_state")
        .await
        .expect("limited backlog state lookup should succeed")
        .expect("limited backlog state should be persisted");
    assert_eq!(limited_backlog_state.state_value, "high");

    scheduler.run_once().await.expect("second sustained-backlog tick should complete");
    scheduler.run_once().await.expect("third sustained-backlog tick should complete");

    let first_final_requests = first_server.received_requests().await.expect("first final requests should load");
    let second_final_requests = second_server.received_requests().await.expect("second final requests should load");
    let third_final_requests = third_server.received_requests().await.expect("third final requests should load");
    let final_counts = [first_final_requests.len(), second_final_requests.len(), third_final_requests.len()];

    assert_eq!(
        final_counts.iter().sum::<usize>(),
        6,
        "three ticks with a two-service capacity should emit six total transactions"
    );
    assert!(
        final_counts.iter().all(|count| *count >= 1),
        "each service should eventually receive a dispatch under sustained backlog"
    );
    assert!(
        !initially_limited_server.received_requests().await.expect("limited server requests should load").is_empty(),
        "the service deferred by the first tick should be rotated back into dispatch"
    );
}

#[tokio::test]
async fn test_appservice_scheduler_rotates_capacity_limited_service_under_sustained_transaction_backlog() {
    let pool = crate::require_test_pool().await;
    setup_appservice_test_database(&pool).await;

    let first_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&first_server).await;

    let second_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&second_server).await;

    let third_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&third_server).await;

    let manager = create_test_appservice_manager(&pool);
    let scheduler = ApplicationServiceScheduler::with_capacity_options(manager.clone(), 16, 500, 2, 50, 2);
    let storage = ApplicationServiceStorage::new(&pool);

    let scenario_id = unique_id();

    let first_as_id = format!("sustained-txn-a-{}", unique_id());
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: first_as_id.clone(),
            url: first_server.uri(),
            as_token: format!("as_token_{first_as_id}"),
            hs_token: format!("hs_token_{first_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("sustained txn first".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": format!("^!sustained-txn-a-{scenario_id}.*:localhost$")}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("first appservice registration should succeed");

    let second_as_id = format!("sustained-txn-b-{}", unique_id());
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: second_as_id.clone(),
            url: second_server.uri(),
            as_token: format!("as_token_{second_as_id}"),
            hs_token: format!("hs_token_{second_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("sustained txn second".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": format!("^!sustained-txn-b-{scenario_id}.*:localhost$")}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("second appservice registration should succeed");

    let third_as_id = format!("sustained-txn-c-{}", unique_id());
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: third_as_id.clone(),
            url: third_server.uri(),
            as_token: format!("as_token_{third_as_id}"),
            hs_token: format!("hs_token_{third_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("sustained txn third".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": format!("^!sustained-txn-c-{scenario_id}.*:localhost$")}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("third appservice registration should succeed");

    for transaction_index in 0..2 {
        storage
            .create_transaction(
                &first_as_id,
                &format!("sustained-txn-a-{scenario_id}-{transaction_index}"),
                &[json!({"type": "m.room.message", "content": {"body": format!("first-{transaction_index}")}})],
            )
            .await
            .expect("first pending transaction should be created");
        storage
            .create_transaction(
                &second_as_id,
                &format!("sustained-txn-b-{scenario_id}-{transaction_index}"),
                &[json!({"type": "m.room.message", "content": {"body": format!("second-{transaction_index}")}})],
            )
            .await
            .expect("second pending transaction should be created");
        storage
            .create_transaction(
                &third_as_id,
                &format!("sustained-txn-c-{scenario_id}-{transaction_index}"),
                &[json!({"type": "m.room.message", "content": {"body": format!("third-{transaction_index}")}})],
            )
            .await
            .expect("third pending transaction should be created");
    }

    scheduler.run_once().await.expect("first sustained transaction-backlog tick should complete");

    let first_tick_counts = [
        first_server.received_requests().await.expect("first requests should load").len(),
        second_server.received_requests().await.expect("second requests should load").len(),
        third_server.received_requests().await.expect("third requests should load").len(),
    ];
    assert_eq!(
        first_tick_counts.iter().sum::<usize>(),
        2,
        "scheduler should only dispatch two services per tick when max_services_per_tick=2"
    );
    assert_eq!(
        first_tick_counts.iter().filter(|count| **count == 0).count(),
        1,
        "exactly one service should be deferred by the capacity limit on the first tick"
    );

    let initially_limited_as_id = if first_tick_counts[0] == 0 {
        &first_as_id
    } else if first_tick_counts[1] == 0 {
        &second_as_id
    } else {
        &third_as_id
    };

    let limited_result = manager
        .get_state(initially_limited_as_id, "scheduler_last_result")
        .await
        .expect("limited result lookup should succeed")
        .expect("limited result should be persisted");
    assert_eq!(limited_result.state_value, "capacity_limited");

    let limited_pending_transactions = manager
        .get_state(initially_limited_as_id, "scheduler_pending_transaction_count")
        .await
        .expect("limited pending transaction lookup should succeed")
        .expect("limited pending transaction count should be persisted");
    assert_eq!(limited_pending_transactions.state_value, "2");

    let limited_backlog_state = manager
        .get_state(initially_limited_as_id, "scheduler_backlog_state")
        .await
        .expect("limited backlog state lookup should succeed")
        .expect("limited backlog state should be persisted");
    assert_eq!(
        limited_backlog_state.state_value, "high",
        "default transaction threshold should classify two pending transactions as high backlog"
    );

    let limited_transaction_state = manager
        .get_state(initially_limited_as_id, "scheduler_transaction_state")
        .await
        .expect("limited transaction state lookup should succeed")
        .expect("limited transaction state should be persisted");
    assert_eq!(limited_transaction_state.state_value, "capacity_limited");

    scheduler.run_once().await.expect("second sustained transaction-backlog tick should complete");
    scheduler.run_once().await.expect("third sustained transaction-backlog tick should complete");
    scheduler.run_once().await.expect("fourth sustained transaction-backlog tick should complete");

    let first_final_requests = first_server.received_requests().await.expect("first final requests should load");
    let second_final_requests = second_server.received_requests().await.expect("second final requests should load");
    let third_final_requests = third_server.received_requests().await.expect("third final requests should load");
    let final_counts = [first_final_requests.len(), second_final_requests.len(), third_final_requests.len()];

    assert_eq!(
        final_counts.iter().sum::<usize>(),
        6,
        "four ticks should be enough to drain six seeded pending transactions under a two-service capacity"
    );
    assert!(
        final_counts.iter().all(|count| *count >= 1),
        "each service should eventually receive a dispatch under sustained transaction backlog"
    );

    for as_id in [&first_as_id, &second_as_id, &third_as_id] {
        let pending_transactions = storage
            .get_pending_transactions(as_id)
            .await
            .expect("pending transactions should load after sustained dispatch");
        assert!(pending_transactions.is_empty(), "all seeded pending transactions should be drained after three ticks");
    }
}

#[tokio::test]
async fn test_appservice_scheduler_handles_mixed_event_and_transaction_backlog_under_capacity_limit() {
    let pool = crate::require_test_pool().await;
    setup_appservice_test_database(&pool).await;

    let transaction_heavy_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&transaction_heavy_server).await;

    let transaction_light_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&transaction_light_server).await;

    let event_heavy_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&event_heavy_server).await;

    let manager = create_test_appservice_manager(&pool);
    let scheduler = ApplicationServiceScheduler::with_capacity_options(manager.clone(), 100, 500, 2, 50, 2);
    let storage = ApplicationServiceStorage::new(&pool);

    let scenario_id = unique_id();

    let transaction_heavy_as_id = format!("mixed-txn-heavy-{}", unique_id());
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: transaction_heavy_as_id.clone(),
            url: transaction_heavy_server.uri(),
            as_token: format!("as_token_{transaction_heavy_as_id}"),
            hs_token: format!("hs_token_{transaction_heavy_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("mixed transaction heavy".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": format!("^!mixed-txn-heavy-{scenario_id}.*:localhost$")}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("transaction-heavy appservice registration should succeed");

    let transaction_light_as_id = format!("mixed-txn-light-{}", unique_id());
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: transaction_light_as_id.clone(),
            url: transaction_light_server.uri(),
            as_token: format!("as_token_{transaction_light_as_id}"),
            hs_token: format!("hs_token_{transaction_light_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("mixed transaction light".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": format!("^!mixed-txn-light-{scenario_id}.*:localhost$")}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("transaction-light appservice registration should succeed");

    let event_heavy_as_id = format!("mixed-event-heavy-{}", unique_id());
    let event_heavy_room_id = format!("!mixed-event-heavy-{scenario_id}-room:localhost");
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: event_heavy_as_id.clone(),
            url: event_heavy_server.uri(),
            as_token: format!("as_token_{event_heavy_as_id}"),
            hs_token: format!("hs_token_{event_heavy_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("mixed event heavy".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": format!("^!mixed-event-heavy-{scenario_id}.*:localhost$")}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("event-heavy appservice registration should succeed");

    for transaction_index in 0..2 {
        storage
            .create_transaction(
                &transaction_heavy_as_id,
                &format!("mixed-txn-heavy-{scenario_id}-{transaction_index}"),
                &[json!({"type": "m.room.message", "content": {"body": format!("heavy-{transaction_index}")}})],
            )
            .await
            .expect("transaction-heavy pending transaction should be created");
    }

    storage
        .create_transaction(
            &transaction_light_as_id,
            &format!("mixed-txn-light-{scenario_id}"),
            &[json!({"type": "m.room.message", "content": {"body": "light"}})],
        )
        .await
        .expect("transaction-light pending transaction should be created");

    for event_index in 0..60 {
        manager
            .push_event(
                &event_heavy_as_id,
                &event_heavy_room_id,
                "m.room.message",
                "@bridge:localhost",
                json!({"msgtype": "m.text", "body": format!("event-heavy-{event_index}")}),
                None,
            )
            .await
            .expect("event-heavy enqueue should succeed");
    }

    scheduler.run_once().await.expect("first mixed-backlog tick should complete");

    let first_tick_transaction_heavy_requests =
        transaction_heavy_server.received_requests().await.expect("transaction-heavy requests should load");
    let first_tick_transaction_light_requests =
        transaction_light_server.received_requests().await.expect("transaction-light requests should load");
    let first_tick_event_heavy_requests =
        event_heavy_server.received_requests().await.expect("event-heavy requests should load");

    assert_eq!(
        first_tick_transaction_heavy_requests.len() + first_tick_transaction_light_requests.len(),
        2,
        "first tick should spend both slots on pending-transaction services"
    );
    assert_eq!(
        first_tick_event_heavy_requests.len(),
        0,
        "event-heavy service should be deferred while transaction backlog still exists"
    );

    let limited_result = manager
        .get_state(&event_heavy_as_id, "scheduler_last_result")
        .await
        .expect("limited result lookup should succeed")
        .expect("limited result should be persisted");
    assert_eq!(limited_result.state_value, "capacity_limited");

    let limited_pending_events = manager
        .get_state(&event_heavy_as_id, "scheduler_pending_event_count")
        .await
        .expect("limited pending events lookup should succeed")
        .expect("limited pending events should be persisted");
    assert_eq!(limited_pending_events.state_value, "60");

    let limited_backlog_state = manager
        .get_state(&event_heavy_as_id, "scheduler_backlog_state")
        .await
        .expect("limited backlog state lookup should succeed")
        .expect("limited backlog state should be persisted");
    assert_eq!(limited_backlog_state.state_value, "high");

    let limited_transaction_state = manager
        .get_state(&event_heavy_as_id, "scheduler_transaction_state")
        .await
        .expect("limited transaction state lookup should succeed")
        .expect("limited transaction state should be persisted");
    assert_eq!(limited_transaction_state.state_value, "capacity_limited");

    scheduler.run_once().await.expect("second mixed-backlog tick should complete");

    let final_transaction_heavy_requests =
        transaction_heavy_server.received_requests().await.expect("final transaction-heavy requests should load");
    let final_transaction_light_requests =
        transaction_light_server.received_requests().await.expect("final transaction-light requests should load");
    let final_event_heavy_requests =
        event_heavy_server.received_requests().await.expect("final event-heavy requests should load");

    assert_eq!(
        final_transaction_heavy_requests.len(),
        2,
        "transaction-heavy service should consume its second pending transaction on the second tick"
    );
    assert_eq!(final_transaction_light_requests.len(), 1, "transaction-light service should finish on the first tick");
    assert_eq!(
        final_event_heavy_requests.len(),
        1,
        "event-heavy service should be rotated back into dispatch once transaction pressure drops"
    );

    let event_pending_events =
        storage.get_pending_events(&event_heavy_as_id, 100).await.expect("event-heavy pending events should load");
    assert!(event_pending_events.is_empty(), "event-heavy backlog should be drained once it gets a dispatch slot");
}

#[tokio::test]
async fn test_appservice_scheduler_mixed_backlog_does_not_block_healthy_services_during_retry_backoff() {
    let pool = crate::require_test_pool().await;
    setup_appservice_test_database(&pool).await;

    let failing_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(503)).mount(&failing_server).await;

    let healthy_transaction_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&healthy_transaction_server).await;

    let healthy_event_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&healthy_event_server).await;

    let manager = create_test_appservice_manager(&pool);
    let scheduler = ApplicationServiceScheduler::with_capacity_options(manager.clone(), 100, 500, 2, 50, 2);
    let storage = ApplicationServiceStorage::new(&pool);

    let scenario_id = unique_id();

    let failing_as_id = format!("mixed-backoff-failing-{}", unique_id());
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: failing_as_id.clone(),
            url: failing_server.uri(),
            as_token: format!("as_token_{failing_as_id}"),
            hs_token: format!("hs_token_{failing_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("mixed backoff failing".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": format!("^!mixed-backoff-failing-{scenario_id}.*:localhost$")}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("failing appservice registration should succeed");

    let healthy_transaction_as_id = format!("mixed-backoff-healthy-txn-{}", unique_id());
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: healthy_transaction_as_id.clone(),
            url: healthy_transaction_server.uri(),
            as_token: format!("as_token_{healthy_transaction_as_id}"),
            hs_token: format!("hs_token_{healthy_transaction_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("mixed backoff healthy txn".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": format!("^!mixed-backoff-healthy-txn-{scenario_id}.*:localhost$")}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("healthy transaction appservice registration should succeed");

    let healthy_event_as_id = format!("mixed-backoff-healthy-event-{}", unique_id());
    let healthy_event_room_id = format!("!mixed-backoff-healthy-event-{scenario_id}-room:localhost");
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: healthy_event_as_id.clone(),
            url: healthy_event_server.uri(),
            as_token: format!("as_token_{healthy_event_as_id}"),
            hs_token: format!("hs_token_{healthy_event_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("mixed backoff healthy event".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": format!("^!mixed-backoff-healthy-event-{scenario_id}.*:localhost$")}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("healthy event appservice registration should succeed");

    storage
        .create_transaction(
            &failing_as_id,
            &format!("mixed-backoff-failing-{scenario_id}"),
            &[json!({"type": "m.room.message", "content": {"body": "fail"}})],
        )
        .await
        .expect("failing pending transaction should be created");

    storage
        .create_transaction(
            &healthy_transaction_as_id,
            &format!("mixed-backoff-healthy-txn-{scenario_id}"),
            &[json!({"type": "m.room.message", "content": {"body": "healthy"}})],
        )
        .await
        .expect("healthy pending transaction should be created");

    for event_index in 0..60 {
        manager
            .push_event(
                &healthy_event_as_id,
                &healthy_event_room_id,
                "m.room.message",
                "@bridge:localhost",
                json!({"msgtype": "m.text", "body": format!("healthy-event-{event_index}")}),
                None,
            )
            .await
            .expect("healthy event enqueue should succeed");
    }

    scheduler.run_once().await.expect("first mixed backoff tick should complete");

    let failing_requests = failing_server.received_requests().await.expect("failing requests should load");
    let healthy_transaction_requests =
        healthy_transaction_server.received_requests().await.expect("healthy transaction requests should load");
    let healthy_event_requests =
        healthy_event_server.received_requests().await.expect("healthy event requests should load");

    assert_eq!(failing_requests.len(), 1, "failing service should consume one slot and enter backoff");
    assert_eq!(
        healthy_transaction_requests.len(),
        1,
        "healthy transaction service should still dispatch on the first tick"
    );
    assert_eq!(healthy_event_requests.len(), 0, "healthy event-heavy service should be deferred on the first tick");

    let limited_result = manager
        .get_state(&healthy_event_as_id, "scheduler_last_result")
        .await
        .expect("healthy event result lookup should succeed")
        .expect("healthy event result should be persisted");
    assert_eq!(limited_result.state_value, "capacity_limited");

    scheduler.run_once().await.expect("second mixed backoff tick should complete");

    let failing_requests_after_backoff =
        failing_server.received_requests().await.expect("failing requests after backoff should load");
    let healthy_transaction_requests_final =
        healthy_transaction_server.received_requests().await.expect("final healthy transaction requests should load");
    let healthy_event_requests_final =
        healthy_event_server.received_requests().await.expect("final healthy event requests should load");

    assert_eq!(
        failing_requests_after_backoff.len(),
        1,
        "retry backoff should suppress an immediate second retry for the failing service"
    );
    assert_eq!(
        healthy_transaction_requests_final.len(),
        1,
        "healthy transaction service should stay complete after its successful first dispatch"
    );
    assert_eq!(
        healthy_event_requests_final.len(),
        1,
        "healthy event-heavy service should progress on the second tick despite a peer being in backoff"
    );

    let failing_result = manager
        .get_state(&failing_as_id, "scheduler_last_result")
        .await
        .expect("failing result lookup should succeed")
        .expect("failing result should be persisted");
    assert_eq!(failing_result.state_value, "backoff");

    let failing_transaction_state = manager
        .get_state(&failing_as_id, "scheduler_transaction_state")
        .await
        .expect("failing transaction state lookup should succeed")
        .expect("failing transaction state should be persisted");
    assert_eq!(failing_transaction_state.state_value, "retry_backoff");

    let failing_backoff_count = manager
        .get_state(&failing_as_id, "scheduler_total_backoff_count")
        .await
        .expect("failing backoff count lookup should succeed")
        .expect("failing backoff count should be persisted");
    assert_eq!(failing_backoff_count.state_value, "1");

    let healthy_event_pending_events =
        storage.get_pending_events(&healthy_event_as_id, 100).await.expect("healthy event pending events should load");
    assert!(
        healthy_event_pending_events.is_empty(),
        "healthy event-heavy backlog should be drained despite another service being in retry backoff"
    );
}

#[tokio::test]
async fn test_appservice_scheduler_long_window_mixed_backlog_preserves_fairness_and_prevents_event_starvation() {
    let pool = crate::require_test_pool().await;
    setup_appservice_test_database(&pool).await;

    let failing_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(503)).mount(&failing_server).await;

    let healthy_txn_a_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&healthy_txn_a_server).await;

    let healthy_txn_b_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&healthy_txn_b_server).await;

    let event_heavy_a_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&event_heavy_a_server).await;

    let event_heavy_b_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&event_heavy_b_server).await;

    let manager = create_test_appservice_manager(&pool);
    let scheduler = ApplicationServiceScheduler::with_capacity_options(manager.clone(), 100, 500, 2, 50, 2);
    let storage = ApplicationServiceStorage::new(&pool);

    let scenario_id = unique_id();

    let failing_as_id = format!("long-window-failing-{}", unique_id());
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: failing_as_id.clone(),
            url: failing_server.uri(),
            as_token: format!("as_token_{failing_as_id}"),
            hs_token: format!("hs_token_{failing_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("long-window failing".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": format!("^!long-window-failing-{scenario_id}.*:localhost$")}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("failing appservice registration should succeed");

    let healthy_txn_a_as_id = format!("long-window-healthy-txn-a-{}", unique_id());
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: healthy_txn_a_as_id.clone(),
            url: healthy_txn_a_server.uri(),
            as_token: format!("as_token_{healthy_txn_a_as_id}"),
            hs_token: format!("hs_token_{healthy_txn_a_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("long-window healthy txn a".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": format!("^!long-window-healthy-txn-a-{scenario_id}.*:localhost$")}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("healthy transaction appservice a registration should succeed");

    let healthy_txn_b_as_id = format!("long-window-healthy-txn-b-{}", unique_id());
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: healthy_txn_b_as_id.clone(),
            url: healthy_txn_b_server.uri(),
            as_token: format!("as_token_{healthy_txn_b_as_id}"),
            hs_token: format!("hs_token_{healthy_txn_b_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("long-window healthy txn b".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": format!("^!long-window-healthy-txn-b-{scenario_id}.*:localhost$")}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("healthy transaction appservice b registration should succeed");

    let event_heavy_a_as_id = format!("long-window-event-heavy-a-{}", unique_id());
    let event_heavy_a_room_id = format!("!long-window-event-heavy-a-{scenario_id}-room:localhost");
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: event_heavy_a_as_id.clone(),
            url: event_heavy_a_server.uri(),
            as_token: format!("as_token_{event_heavy_a_as_id}"),
            hs_token: format!("hs_token_{event_heavy_a_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("long-window event heavy a".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": format!("^!long-window-event-heavy-a-{scenario_id}.*:localhost$")}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("event-heavy appservice a registration should succeed");

    let event_heavy_b_as_id = format!("long-window-event-heavy-b-{}", unique_id());
    let event_heavy_b_room_id = format!("!long-window-event-heavy-b-{scenario_id}-room:localhost");
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: event_heavy_b_as_id.clone(),
            url: event_heavy_b_server.uri(),
            as_token: format!("as_token_{event_heavy_b_as_id}"),
            hs_token: format!("hs_token_{event_heavy_b_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("long-window event heavy b".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": format!("^!long-window-event-heavy-b-{scenario_id}.*:localhost$")}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("event-heavy appservice b registration should succeed");

    storage
        .create_transaction(
            &failing_as_id,
            &format!("long-window-failing-{scenario_id}"),
            &[json!({"type": "m.room.message", "content": {"body": "fail"}})],
        )
        .await
        .expect("failing pending transaction should be created");

    for transaction_index in 0..2 {
        storage
            .create_transaction(
                &healthy_txn_a_as_id,
                &format!("long-window-healthy-a-{scenario_id}-{transaction_index}"),
                &[json!({"type": "m.room.message", "content": {"body": format!("healthy-a-{transaction_index}")}})],
            )
            .await
            .expect("healthy transaction a pending transaction should be created");
        storage
            .create_transaction(
                &healthy_txn_b_as_id,
                &format!("long-window-healthy-b-{scenario_id}-{transaction_index}"),
                &[json!({"type": "m.room.message", "content": {"body": format!("healthy-b-{transaction_index}")}})],
            )
            .await
            .expect("healthy transaction b pending transaction should be created");
    }

    for event_index in 0..60 {
        manager
            .push_event(
                &event_heavy_a_as_id,
                &event_heavy_a_room_id,
                "m.room.message",
                "@bridge:localhost",
                json!({"msgtype": "m.text", "body": format!("event-heavy-a-{event_index}")}),
                None,
            )
            .await
            .expect("event-heavy a enqueue should succeed");
        manager
            .push_event(
                &event_heavy_b_as_id,
                &event_heavy_b_room_id,
                "m.room.message",
                "@bridge:localhost",
                json!({"msgtype": "m.text", "body": format!("event-heavy-b-{event_index}")}),
                None,
            )
            .await
            .expect("event-heavy b enqueue should succeed");
    }

    scheduler.run_once().await.expect("long-window tick one should complete");
    scheduler.run_once().await.expect("long-window tick two should complete");
    scheduler.run_once().await.expect("long-window tick three should complete");
    scheduler.run_once().await.expect("long-window tick four should complete");

    let failing_requests = failing_server.received_requests().await.expect("failing requests should load");
    let healthy_txn_a_requests =
        healthy_txn_a_server.received_requests().await.expect("healthy transaction a requests should load");
    let healthy_txn_b_requests =
        healthy_txn_b_server.received_requests().await.expect("healthy transaction b requests should load");
    let event_heavy_a_requests =
        event_heavy_a_server.received_requests().await.expect("event-heavy a requests should load");
    let event_heavy_b_requests =
        event_heavy_b_server.received_requests().await.expect("event-heavy b requests should load");

    assert_eq!(
        failing_requests.len(),
        1,
        "failing service should attempt delivery once and then stay suppressed by retry_backoff during the long window"
    );
    assert_eq!(
        healthy_txn_a_requests.len() + healthy_txn_b_requests.len(),
        4,
        "healthy pending-transaction services should consume all four seeded transactions across the long window"
    );
    assert!(
        healthy_txn_a_requests.len() >= 2 && healthy_txn_b_requests.len() >= 2,
        "each healthy pending-transaction service should continue to make progress under sustained contention"
    );
    assert!(
        !event_heavy_a_requests.is_empty() && !event_heavy_b_requests.is_empty(),
        "both event-heavy services should eventually receive dispatch slots instead of starving behind retry_backoff and transaction pressure"
    );

    let healthy_txn_a_pending = storage
        .get_pending_transactions(&healthy_txn_a_as_id)
        .await
        .expect("healthy transaction a pending transactions should load");
    let healthy_txn_b_pending = storage
        .get_pending_transactions(&healthy_txn_b_as_id)
        .await
        .expect("healthy transaction b pending transactions should load");
    assert!(
        healthy_txn_a_pending.is_empty(),
        "healthy transaction a backlog should be drained by the end of the window"
    );
    assert!(
        healthy_txn_b_pending.is_empty(),
        "healthy transaction b backlog should be drained by the end of the window"
    );

    let event_heavy_a_pending =
        storage.get_pending_events(&event_heavy_a_as_id, 100).await.expect("event-heavy a pending events should load");
    let event_heavy_b_pending =
        storage.get_pending_events(&event_heavy_b_as_id, 100).await.expect("event-heavy b pending events should load");
    assert!(event_heavy_a_pending.is_empty(), "event-heavy a backlog should be drained once it eventually gets a slot");
    assert!(event_heavy_b_pending.is_empty(), "event-heavy b backlog should be drained once it eventually gets a slot");

    let failing_transaction_state = manager
        .get_state(&failing_as_id, "scheduler_transaction_state")
        .await
        .expect("failing transaction state lookup should succeed")
        .expect("failing transaction state should be persisted");
    assert_eq!(failing_transaction_state.state_value, "retry_backoff");

    let event_heavy_a_capacity_count = manager
        .get_state(&event_heavy_a_as_id, "scheduler_total_capacity_limited_count")
        .await
        .expect("event-heavy a capacity count lookup should succeed")
        .expect("event-heavy a capacity count should be persisted");
    let event_heavy_b_capacity_count = manager
        .get_state(&event_heavy_b_as_id, "scheduler_total_capacity_limited_count")
        .await
        .expect("event-heavy b capacity count lookup should succeed")
        .expect("event-heavy b capacity count should be persisted");
    assert!(
        event_heavy_a_capacity_count.state_value.parse::<i64>().unwrap_or(0) >= 1
            && event_heavy_b_capacity_count.state_value.parse::<i64>().unwrap_or(0) >= 1,
        "both event-heavy services should first be deferred under capacity pressure before later rotating into dispatch"
    );
}

#[tokio::test]
async fn test_appservice_scheduler_continuous_event_ingress_does_not_starve_event_bucket_after_transaction_backlog_drains(
) {
    let pool = crate::require_test_pool().await;
    setup_appservice_test_database(&pool).await;

    let healthy_txn_a_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&healthy_txn_a_server).await;

    let healthy_txn_b_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&healthy_txn_b_server).await;

    let event_heavy_a_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&event_heavy_a_server).await;

    let event_heavy_b_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&event_heavy_b_server).await;

    let manager = create_test_appservice_manager(&pool);
    let scheduler = ApplicationServiceScheduler::with_capacity_options(manager.clone(), 25, 500, 2, 50, 2);
    let storage = ApplicationServiceStorage::new(&pool);

    let scenario_id = unique_id();

    let healthy_txn_a_as_id = format!("continuous-ingress-healthy-txn-a-{}", unique_id());
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: healthy_txn_a_as_id.clone(),
            url: healthy_txn_a_server.uri(),
            as_token: format!("as_token_{healthy_txn_a_as_id}"),
            hs_token: format!("hs_token_{healthy_txn_a_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("continuous ingress healthy txn a".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": format!("^!continuous-ingress-healthy-txn-a-{scenario_id}.*:localhost$")}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("healthy transaction appservice a registration should succeed");

    let healthy_txn_b_as_id = format!("continuous-ingress-healthy-txn-b-{}", unique_id());
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: healthy_txn_b_as_id.clone(),
            url: healthy_txn_b_server.uri(),
            as_token: format!("as_token_{healthy_txn_b_as_id}"),
            hs_token: format!("hs_token_{healthy_txn_b_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("continuous ingress healthy txn b".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": format!("^!continuous-ingress-healthy-txn-b-{scenario_id}.*:localhost$")}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("healthy transaction appservice b registration should succeed");

    let event_heavy_a_as_id = format!("continuous-ingress-event-heavy-a-{}", unique_id());
    let event_heavy_a_room_id = format!("!continuous-ingress-event-heavy-a-{scenario_id}-room:localhost");
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: event_heavy_a_as_id.clone(),
            url: event_heavy_a_server.uri(),
            as_token: format!("as_token_{event_heavy_a_as_id}"),
            hs_token: format!("hs_token_{event_heavy_a_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("continuous ingress event heavy a".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": format!("^!continuous-ingress-event-heavy-a-{scenario_id}.*:localhost$")}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("event-heavy appservice a registration should succeed");

    let event_heavy_b_as_id = format!("continuous-ingress-event-heavy-b-{}", unique_id());
    let event_heavy_b_room_id = format!("!continuous-ingress-event-heavy-b-{scenario_id}-room:localhost");
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: event_heavy_b_as_id.clone(),
            url: event_heavy_b_server.uri(),
            as_token: format!("as_token_{event_heavy_b_as_id}"),
            hs_token: format!("hs_token_{event_heavy_b_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("continuous ingress event heavy b".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": format!("^!continuous-ingress-event-heavy-b-{scenario_id}.*:localhost$")}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("event-heavy appservice b registration should succeed");

    for transaction_index in 0..4 {
        storage
            .create_transaction(
                &healthy_txn_a_as_id,
                &format!("continuous-ingress-healthy-a-{scenario_id}-{transaction_index}"),
                &[json!({"type": "m.room.message", "content": {"body": format!("healthy-a-{transaction_index}")}})],
            )
            .await
            .expect("healthy transaction a pending transaction should be created");
        storage
            .create_transaction(
                &healthy_txn_b_as_id,
                &format!("continuous-ingress-healthy-b-{scenario_id}-{transaction_index}"),
                &[json!({"type": "m.room.message", "content": {"body": format!("healthy-b-{transaction_index}")}})],
            )
            .await
            .expect("healthy transaction b pending transaction should be created");
    }

    for event_index in 0..25 {
        manager
            .push_event(
                &event_heavy_a_as_id,
                &event_heavy_a_room_id,
                "m.room.message",
                "@bridge:localhost",
                json!({"msgtype": "m.text", "body": format!("continuous-ingress-event-a-initial-{event_index}")}),
                None,
            )
            .await
            .expect("event-heavy a initial enqueue should succeed");
        manager
            .push_event(
                &event_heavy_b_as_id,
                &event_heavy_b_room_id,
                "m.room.message",
                "@bridge:localhost",
                json!({"msgtype": "m.text", "body": format!("continuous-ingress-event-b-initial-{event_index}")}),
                None,
            )
            .await
            .expect("event-heavy b initial enqueue should succeed");
    }

    for tick in 0..4 {
        scheduler
            .run_once()
            .await
            .unwrap_or_else(|_| panic!("continuous-ingress transaction-pressure tick {tick} should complete"));

        if tick < 3 {
            for event_index in 0..25 {
                manager
                    .push_event(
                        &event_heavy_a_as_id,
                        &event_heavy_a_room_id,
                        "m.room.message",
                        "@bridge:localhost",
                        json!({"msgtype": "m.text", "body": format!("continuous-ingress-event-a-tick-{tick}-{event_index}")}),
                        None,
                    )
                    .await
                    .expect("event-heavy a continuous enqueue should succeed");
                manager
                    .push_event(
                        &event_heavy_b_as_id,
                        &event_heavy_b_room_id,
                        "m.room.message",
                        "@bridge:localhost",
                        json!({"msgtype": "m.text", "body": format!("continuous-ingress-event-b-tick-{tick}-{event_index}")}),
                        None,
                    )
                    .await
                    .expect("event-heavy b continuous enqueue should succeed");
            }
        }
    }

    let event_heavy_a_requests_during_transaction_pressure =
        event_heavy_a_server.received_requests().await.expect("event-heavy a intermediate requests should load");
    let event_heavy_b_requests_during_transaction_pressure =
        event_heavy_b_server.received_requests().await.expect("event-heavy b intermediate requests should load");
    assert_eq!(
        event_heavy_a_requests_during_transaction_pressure.len()
            + event_heavy_b_requests_during_transaction_pressure.len(),
        0,
        "event bucket should remain deferred while both pending-transaction services still occupy all dispatch slots"
    );

    for tick in 4..8 {
        scheduler
            .run_once()
            .await
            .unwrap_or_else(|_| panic!("continuous-ingress recovery tick {tick} should complete"));
    }

    let healthy_txn_a_requests =
        healthy_txn_a_server.received_requests().await.expect("healthy transaction a requests should load");
    let healthy_txn_b_requests =
        healthy_txn_b_server.received_requests().await.expect("healthy transaction b requests should load");
    let event_heavy_a_requests =
        event_heavy_a_server.received_requests().await.expect("event-heavy a requests should load");
    let event_heavy_b_requests =
        event_heavy_b_server.received_requests().await.expect("event-heavy b requests should load");

    assert_eq!(healthy_txn_a_requests.len(), 4, "healthy transaction a should drain all four seeded transactions");
    assert_eq!(healthy_txn_b_requests.len(), 4, "healthy transaction b should drain all four seeded transactions");
    assert_eq!(
        event_heavy_a_requests.len(),
        4,
        "event-heavy a should later drain four 25-event batches after transaction pressure releases"
    );
    assert_eq!(
        event_heavy_b_requests.len(),
        4,
        "event-heavy b should later drain four 25-event batches after transaction pressure releases"
    );

    let healthy_txn_a_pending = storage
        .get_pending_transactions(&healthy_txn_a_as_id)
        .await
        .expect("healthy transaction a pending transactions should load");
    let healthy_txn_b_pending = storage
        .get_pending_transactions(&healthy_txn_b_as_id)
        .await
        .expect("healthy transaction b pending transactions should load");
    assert!(
        healthy_txn_a_pending.is_empty(),
        "healthy transaction a backlog should be drained by the end of the window"
    );
    assert!(
        healthy_txn_b_pending.is_empty(),
        "healthy transaction b backlog should be drained by the end of the window"
    );

    let event_heavy_a_pending =
        storage.get_pending_events(&event_heavy_a_as_id, 120).await.expect("event-heavy a pending events should load");
    let event_heavy_b_pending =
        storage.get_pending_events(&event_heavy_b_as_id, 120).await.expect("event-heavy b pending events should load");
    assert!(event_heavy_a_pending.is_empty(), "event-heavy a backlog should be drained after sustained ingestion");
    assert!(event_heavy_b_pending.is_empty(), "event-heavy b backlog should be drained after sustained ingestion");

    let event_heavy_a_capacity_count = manager
        .get_state(&event_heavy_a_as_id, "scheduler_total_capacity_limited_count")
        .await
        .expect("event-heavy a capacity count lookup should succeed")
        .expect("event-heavy a capacity count should be persisted");
    let event_heavy_b_capacity_count = manager
        .get_state(&event_heavy_b_as_id, "scheduler_total_capacity_limited_count")
        .await
        .expect("event-heavy b capacity count lookup should succeed")
        .expect("event-heavy b capacity count should be persisted");
    assert!(
        event_heavy_a_capacity_count.state_value.parse::<i64>().unwrap_or(0) >= 4
            && event_heavy_b_capacity_count.state_value.parse::<i64>().unwrap_or(0) >= 4,
        "event bucket services should record repeated capacity pressure across the sustained transaction-heavy window"
    );
}

#[tokio::test]
async fn test_appservice_scheduler_super_event_heavy_service_begins_dispatch_within_two_ticks_under_light_transaction_bursts(
) {
    let pool = crate::require_test_pool().await;
    setup_appservice_test_database(&pool).await;

    let transaction_light_a_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&transaction_light_a_server).await;

    let transaction_light_b_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&transaction_light_b_server).await;

    let transaction_light_c_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&transaction_light_c_server).await;

    let super_event_heavy_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&super_event_heavy_server).await;

    let manager = create_test_appservice_manager(&pool);
    let scheduler = ApplicationServiceScheduler::with_capacity_options(manager.clone(), 50, 500, 2, 50, 2);
    let storage = ApplicationServiceStorage::new(&pool);

    let scenario_id = unique_id();

    let transaction_light_a_as_id = format!("super-event-heavy-light-a-{}", unique_id());
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: transaction_light_a_as_id.clone(),
            url: transaction_light_a_server.uri(),
            as_token: format!("as_token_{transaction_light_a_as_id}"),
            hs_token: format!("hs_token_{transaction_light_a_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("super event heavy light transaction a".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": format!("^!super-event-heavy-light-a-{scenario_id}.*:localhost$")}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("light transaction appservice a registration should succeed");

    let transaction_light_b_as_id = format!("super-event-heavy-light-b-{}", unique_id());
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: transaction_light_b_as_id.clone(),
            url: transaction_light_b_server.uri(),
            as_token: format!("as_token_{transaction_light_b_as_id}"),
            hs_token: format!("hs_token_{transaction_light_b_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("super event heavy light transaction b".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": format!("^!super-event-heavy-light-b-{scenario_id}.*:localhost$")}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("light transaction appservice b registration should succeed");

    let transaction_light_c_as_id = format!("super-event-heavy-light-c-{}", unique_id());
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: transaction_light_c_as_id.clone(),
            url: transaction_light_c_server.uri(),
            as_token: format!("as_token_{transaction_light_c_as_id}"),
            hs_token: format!("hs_token_{transaction_light_c_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("super event heavy light transaction c".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": format!("^!super-event-heavy-light-c-{scenario_id}.*:localhost$")}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("light transaction appservice c registration should succeed");

    let super_event_heavy_as_id = format!("super-event-heavy-{}", unique_id());
    let super_event_heavy_room_id = format!("!super-event-heavy-{scenario_id}-room:localhost");
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: super_event_heavy_as_id.clone(),
            url: super_event_heavy_server.uri(),
            as_token: format!("as_token_{super_event_heavy_as_id}"),
            hs_token: format!("hs_token_{super_event_heavy_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("super event heavy".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": format!("^!super-event-heavy-{scenario_id}.*:localhost$")}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("super event-heavy appservice registration should succeed");

    storage
        .create_transaction(
            &transaction_light_a_as_id,
            &format!("super-event-heavy-light-a-{scenario_id}"),
            &[json!({"type": "m.room.message", "content": {"body": "light-a-initial"}})],
        )
        .await
        .expect("light transaction a pending transaction should be created");
    storage
        .create_transaction(
            &transaction_light_b_as_id,
            &format!("super-event-heavy-light-b-{scenario_id}"),
            &[json!({"type": "m.room.message", "content": {"body": "light-b-initial"}})],
        )
        .await
        .expect("light transaction b pending transaction should be created");
    storage
        .create_transaction(
            &transaction_light_c_as_id,
            &format!("super-event-heavy-light-c-{scenario_id}"),
            &[json!({"type": "m.room.message", "content": {"body": "light-c-initial"}})],
        )
        .await
        .expect("light transaction c pending transaction should be created");

    for event_index in 0..160 {
        manager
            .push_event(
                &super_event_heavy_as_id,
                &super_event_heavy_room_id,
                "m.room.message",
                "@bridge:localhost",
                json!({"msgtype": "m.text", "body": format!("super-event-heavy-{event_index}")}),
                None,
            )
            .await
            .expect("super event-heavy enqueue should succeed");
    }

    scheduler.run_once().await.expect("super event-heavy tick one should complete");

    let tick_one_light_request_count = transaction_light_a_server
        .received_requests()
        .await
        .expect("light transaction a requests should load")
        .len()
        + transaction_light_b_server.received_requests().await.expect("light transaction b requests should load").len()
        + transaction_light_c_server.received_requests().await.expect("light transaction c requests should load").len();
    let tick_one_super_event_heavy_requests =
        super_event_heavy_server.received_requests().await.expect("super event-heavy tick one requests should load");
    assert_eq!(
        tick_one_light_request_count, 2,
        "first tick should spend both slots on light pending-transaction services"
    );
    assert_eq!(
        tick_one_super_event_heavy_requests.len(),
        0,
        "super event-heavy service should be capacity-limited on the first tick"
    );

    let super_event_heavy_last_result = manager
        .get_state(&super_event_heavy_as_id, "scheduler_last_result")
        .await
        .expect("super event-heavy last result lookup should succeed")
        .expect("super event-heavy last result should be persisted");
    assert_eq!(super_event_heavy_last_result.state_value, "capacity_limited");

    let super_event_heavy_backlog_state = manager
        .get_state(&super_event_heavy_as_id, "scheduler_backlog_state")
        .await
        .expect("super event-heavy backlog state lookup should succeed")
        .expect("super event-heavy backlog state should be persisted");
    assert_eq!(super_event_heavy_backlog_state.state_value, "high");

    scheduler.run_once().await.expect("super event-heavy tick two should complete");

    let tick_two_super_event_heavy_requests =
        super_event_heavy_server.received_requests().await.expect("super event-heavy tick two requests should load");
    assert_eq!(
        tick_two_super_event_heavy_requests.len(),
        1,
        "super event-heavy service should begin dispatching by the second tick once one light transaction remains"
    );

    let pending_after_tick_two = storage
        .get_pending_events(&super_event_heavy_as_id, 200)
        .await
        .expect("super event-heavy pending events after tick two should load");
    assert_eq!(
        pending_after_tick_two.len(),
        110,
        "one 50-event batch should have drained by the second tick from the initial 160-event backlog"
    );

    storage
        .create_transaction(
            &transaction_light_a_as_id,
            &format!("super-event-heavy-light-a-follow-up-{scenario_id}"),
            &[json!({"type": "m.room.message", "content": {"body": "light-a-follow-up"}})],
        )
        .await
        .expect("light transaction a follow-up pending transaction should be created");

    scheduler.run_once().await.expect("super event-heavy tick three should complete");
    scheduler.run_once().await.expect("super event-heavy tick four should complete");
    scheduler.run_once().await.expect("super event-heavy tick five should complete");

    let final_light_a_requests =
        transaction_light_a_server.received_requests().await.expect("final light transaction a requests should load");
    let final_light_b_requests =
        transaction_light_b_server.received_requests().await.expect("final light transaction b requests should load");
    let final_light_c_requests =
        transaction_light_c_server.received_requests().await.expect("final light transaction c requests should load");
    let final_super_event_heavy_requests =
        super_event_heavy_server.received_requests().await.expect("final super event-heavy requests should load");

    assert_eq!(
        final_light_a_requests.len(),
        2,
        "light transaction a should deliver its initial and follow-up transactions"
    );
    assert_eq!(final_light_b_requests.len(), 1, "light transaction b should deliver its single burst transaction");
    assert_eq!(final_light_c_requests.len(), 1, "light transaction c should deliver its single burst transaction");
    assert_eq!(
        final_super_event_heavy_requests.len(),
        4,
        "super event-heavy service should drain four 50-event batches without being re-starved by the follow-up light burst"
    );

    let final_pending_events = storage
        .get_pending_events(&super_event_heavy_as_id, 200)
        .await
        .expect("final super event-heavy pending events should load");
    assert!(final_pending_events.is_empty(), "super event-heavy backlog should be fully drained by the fifth tick");

    let super_event_heavy_capacity_count = manager
        .get_state(&super_event_heavy_as_id, "scheduler_total_capacity_limited_count")
        .await
        .expect("super event-heavy capacity count lookup should succeed")
        .expect("super event-heavy capacity count should be persisted");
    assert!(
        super_event_heavy_capacity_count.state_value.parse::<i64>().unwrap_or(0) >= 1,
        "super event-heavy service should record at least one real capacity-limited observation before it begins dispatching"
    );
}

#[tokio::test]
async fn test_appservice_scheduler_recovers_after_transient_failure_without_restarving_event_bucket() {
    let pool = crate::require_test_pool().await;
    setup_appservice_test_database(&pool).await;

    let failing_server = MockServer::start().await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&failing_server)
        .await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&failing_server).await;

    let healthy_txn_a_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&healthy_txn_a_server).await;

    let healthy_txn_b_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&healthy_txn_b_server).await;

    let event_heavy_a_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&event_heavy_a_server).await;

    let event_heavy_b_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&event_heavy_b_server).await;

    let manager = create_test_appservice_manager(&pool);
    let scheduler = ApplicationServiceScheduler::with_capacity_options(manager.clone(), 30, 500, 2, 50, 2);
    let storage = ApplicationServiceStorage::new(&pool);

    let scenario_id = unique_id();

    let failing_as_id = format!("recovery-window-failing-{}", unique_id());
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: failing_as_id.clone(),
            url: failing_server.uri(),
            as_token: format!("as_token_{failing_as_id}"),
            hs_token: format!("hs_token_{failing_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("recovery-window failing".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": format!("^!recovery-window-failing-{scenario_id}.*:localhost$")}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("failing appservice registration should succeed");

    let healthy_txn_a_as_id = format!("recovery-window-healthy-txn-a-{}", unique_id());
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: healthy_txn_a_as_id.clone(),
            url: healthy_txn_a_server.uri(),
            as_token: format!("as_token_{healthy_txn_a_as_id}"),
            hs_token: format!("hs_token_{healthy_txn_a_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("recovery-window healthy txn a".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": format!("^!recovery-window-healthy-txn-a-{scenario_id}.*:localhost$")}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("healthy transaction appservice a registration should succeed");

    let healthy_txn_b_as_id = format!("recovery-window-healthy-txn-b-{}", unique_id());
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: healthy_txn_b_as_id.clone(),
            url: healthy_txn_b_server.uri(),
            as_token: format!("as_token_{healthy_txn_b_as_id}"),
            hs_token: format!("hs_token_{healthy_txn_b_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("recovery-window healthy txn b".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": format!("^!recovery-window-healthy-txn-b-{scenario_id}.*:localhost$")}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("healthy transaction appservice b registration should succeed");

    let event_heavy_a_as_id = format!("recovery-window-event-heavy-a-{}", unique_id());
    let event_heavy_a_room_id = format!("!recovery-window-event-heavy-a-{scenario_id}-room:localhost");
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: event_heavy_a_as_id.clone(),
            url: event_heavy_a_server.uri(),
            as_token: format!("as_token_{event_heavy_a_as_id}"),
            hs_token: format!("hs_token_{event_heavy_a_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("recovery-window event heavy a".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": format!("^!recovery-window-event-heavy-a-{scenario_id}.*:localhost$")}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("event-heavy appservice a registration should succeed");

    let event_heavy_b_as_id = format!("recovery-window-event-heavy-b-{}", unique_id());
    let event_heavy_b_room_id = format!("!recovery-window-event-heavy-b-{scenario_id}-room:localhost");
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: event_heavy_b_as_id.clone(),
            url: event_heavy_b_server.uri(),
            as_token: format!("as_token_{event_heavy_b_as_id}"),
            hs_token: format!("hs_token_{event_heavy_b_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("recovery-window event heavy b".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": format!("^!recovery-window-event-heavy-b-{scenario_id}.*:localhost$")}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("event-heavy appservice b registration should succeed");

    storage
        .create_transaction(
            &failing_as_id,
            &format!("recovery-window-failing-{scenario_id}"),
            &[json!({"type": "m.room.message", "content": {"body": "failing-once-then-recovers"}})],
        )
        .await
        .expect("failing pending transaction should be created");

    for transaction_index in 0..2 {
        storage
            .create_transaction(
                &healthy_txn_a_as_id,
                &format!("recovery-window-healthy-a-{scenario_id}-{transaction_index}"),
                &[json!({"type": "m.room.message", "content": {"body": format!("healthy-a-{transaction_index}")}})],
            )
            .await
            .expect("healthy transaction a pending transaction should be created");
        storage
            .create_transaction(
                &healthy_txn_b_as_id,
                &format!("recovery-window-healthy-b-{scenario_id}-{transaction_index}"),
                &[json!({"type": "m.room.message", "content": {"body": format!("healthy-b-{transaction_index}")}})],
            )
            .await
            .expect("healthy transaction b pending transaction should be created");
    }

    for event_index in 0..60 {
        manager
            .push_event(
                &event_heavy_a_as_id,
                &event_heavy_a_room_id,
                "m.room.message",
                "@bridge:localhost",
                json!({"msgtype": "m.text", "body": format!("recovery-event-a-{event_index}")}),
                None,
            )
            .await
            .expect("event-heavy a enqueue should succeed");
        manager
            .push_event(
                &event_heavy_b_as_id,
                &event_heavy_b_room_id,
                "m.room.message",
                "@bridge:localhost",
                json!({"msgtype": "m.text", "body": format!("recovery-event-b-{event_index}")}),
                None,
            )
            .await
            .expect("event-heavy b enqueue should succeed");
    }

    scheduler.run_once().await.expect("recovery-window tick one should complete");
    scheduler.run_once().await.expect("recovery-window tick two should complete");
    scheduler.run_once().await.expect("recovery-window tick three should complete");

    tokio::time::sleep(std::time::Duration::from_millis(5_200)).await;

    scheduler.run_once().await.expect("recovery-window tick four should complete");
    scheduler.run_once().await.expect("recovery-window tick five should complete");
    scheduler.run_once().await.expect("recovery-window tick six should complete");

    let failing_requests = failing_server.received_requests().await.expect("failing requests should load");
    let healthy_txn_a_requests =
        healthy_txn_a_server.received_requests().await.expect("healthy transaction a requests should load");
    let healthy_txn_b_requests =
        healthy_txn_b_server.received_requests().await.expect("healthy transaction b requests should load");
    let event_heavy_a_requests =
        event_heavy_a_server.received_requests().await.expect("event-heavy a requests should load");
    let event_heavy_b_requests =
        event_heavy_b_server.received_requests().await.expect("event-heavy b requests should load");

    assert_eq!(
        failing_requests.len(),
        2,
        "failing service should fail once and then succeed once after the backoff window expires"
    );
    assert_eq!(
        healthy_txn_a_requests.len() + healthy_txn_b_requests.len(),
        4,
        "healthy pending-transaction services should drain all four seeded transactions despite recovery retries"
    );
    assert!(
        !event_heavy_a_requests.is_empty() && !event_heavy_b_requests.is_empty(),
        "event-heavy services should still keep making progress after the failing service becomes ready again"
    );

    let healthy_txn_a_pending = storage
        .get_pending_transactions(&healthy_txn_a_as_id)
        .await
        .expect("healthy transaction a pending transactions should load");
    let healthy_txn_b_pending = storage
        .get_pending_transactions(&healthy_txn_b_as_id)
        .await
        .expect("healthy transaction b pending transactions should load");
    assert!(healthy_txn_a_pending.is_empty(), "healthy transaction a backlog should be drained");
    assert!(healthy_txn_b_pending.is_empty(), "healthy transaction b backlog should be drained");

    let event_heavy_a_pending =
        storage.get_pending_events(&event_heavy_a_as_id, 100).await.expect("event-heavy a pending events should load");
    let event_heavy_b_pending =
        storage.get_pending_events(&event_heavy_b_as_id, 100).await.expect("event-heavy b pending events should load");
    assert!(
        event_heavy_a_pending.is_empty(),
        "event-heavy a backlog should be drained by the end of the recovery window"
    );
    assert!(
        event_heavy_b_pending.is_empty(),
        "event-heavy b backlog should be drained by the end of the recovery window"
    );

    let failing_last_result = manager
        .get_state(&failing_as_id, "scheduler_last_result")
        .await
        .expect("failing last result lookup should succeed")
        .expect("failing last result should be persisted");
    assert!(
        matches!(failing_last_result.state_value.as_str(), "success" | "dispatched"),
        "failing service should leave backoff and return to a healthy dispatched/success state after recovery"
    );

    let failing_transaction_state = manager
        .get_state(&failing_as_id, "scheduler_transaction_state")
        .await
        .expect("failing transaction state lookup should succeed")
        .expect("failing transaction state should be persisted");
    assert_eq!(failing_transaction_state.state_value, "idle");

    let failing_success_count = manager
        .get_state(&failing_as_id, "scheduler_total_success_count")
        .await
        .expect("failing success count lookup should succeed")
        .expect("failing success count should be persisted");
    assert_eq!(failing_success_count.state_value, "1");

    let failing_failure_count = manager
        .get_state(&failing_as_id, "scheduler_total_failure_count")
        .await
        .expect("failing failure count lookup should succeed")
        .expect("failing failure count should be persisted");
    assert_eq!(failing_failure_count.state_value, "1");

    if let Some(failing_backoff_count) = manager
        .get_state(&failing_as_id, "scheduler_total_backoff_count")
        .await
        .expect("failing backoff count lookup should succeed")
    {
        assert!(
            failing_backoff_count.state_value.parse::<i64>().unwrap_or(0) >= 1,
            "failing service should record retry_backoff observations when the scheduler revisits it before the retry window expires"
        );
    }

    let event_heavy_a_capacity_count = manager
        .get_state(&event_heavy_a_as_id, "scheduler_total_capacity_limited_count")
        .await
        .expect("event-heavy a capacity count lookup should succeed")
        .expect("event-heavy a capacity count should be persisted");
    let event_heavy_b_capacity_count = manager
        .get_state(&event_heavy_b_as_id, "scheduler_total_capacity_limited_count")
        .await
        .expect("event-heavy b capacity count lookup should succeed")
        .expect("event-heavy b capacity count should be persisted");
    assert!(
        event_heavy_a_capacity_count.state_value.parse::<i64>().unwrap_or(0) >= 1
            && event_heavy_b_capacity_count.state_value.parse::<i64>().unwrap_or(0) >= 1,
        "event bucket services should first experience real capacity pressure before recovering dispatch opportunity"
    );

    let statistics = manager.get_statistics().await.expect("scheduler statistics should load");
    let failing_statistics = statistics
        .iter()
        .find(|entry| entry["as_id"] == failing_as_id)
        .expect("failing appservice should be present in statistics");
    assert_eq!(failing_statistics["pending_transaction_count"], 0);
    assert_eq!(failing_statistics["scheduler"]["available"], true);
    assert!(
        matches!(failing_statistics["scheduler"]["last_result"].as_str(), Some("success" | "dispatched")),
        "statistics should report the recovering service in a healthy dispatched/success state"
    );
    assert_eq!(failing_statistics["scheduler"]["transaction_state"], "idle");
    assert_eq!(failing_statistics["scheduler"]["total_success_count"], 1);
    assert_eq!(failing_statistics["scheduler"]["total_failure_count"], 1);
}

#[tokio::test]
async fn test_appservice_scheduler_recovers_multiple_retry_backoff_services_without_restarving_event_bucket() {
    let pool = crate::require_test_pool().await;
    setup_appservice_test_database(&pool).await;

    let failing_a_server = MockServer::start().await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&failing_a_server)
        .await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&failing_a_server).await;

    let failing_b_server = MockServer::start().await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&failing_b_server)
        .await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&failing_b_server).await;

    let healthy_txn_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&healthy_txn_server).await;

    let event_heavy_a_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&event_heavy_a_server).await;

    let event_heavy_b_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&event_heavy_b_server).await;

    let manager = create_test_appservice_manager(&pool);
    let scheduler = ApplicationServiceScheduler::with_capacity_options(manager.clone(), 30, 500, 3, 50, 2);
    let storage = ApplicationServiceStorage::new(&pool);

    let scenario_id = unique_id();

    let failing_a_as_id = format!("multi-recovery-failing-a-{}", unique_id());
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: failing_a_as_id.clone(),
            url: failing_a_server.uri(),
            as_token: format!("as_token_{failing_a_as_id}"),
            hs_token: format!("hs_token_{failing_a_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("multi-recovery failing a".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": format!("^!multi-recovery-failing-a-{scenario_id}.*:localhost$")}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("failing appservice a registration should succeed");

    let failing_b_as_id = format!("multi-recovery-failing-b-{}", unique_id());
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: failing_b_as_id.clone(),
            url: failing_b_server.uri(),
            as_token: format!("as_token_{failing_b_as_id}"),
            hs_token: format!("hs_token_{failing_b_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("multi-recovery failing b".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": format!("^!multi-recovery-failing-b-{scenario_id}.*:localhost$")}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("failing appservice b registration should succeed");

    let healthy_txn_as_id = format!("multi-recovery-healthy-txn-{}", unique_id());
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: healthy_txn_as_id.clone(),
            url: healthy_txn_server.uri(),
            as_token: format!("as_token_{healthy_txn_as_id}"),
            hs_token: format!("hs_token_{healthy_txn_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("multi-recovery healthy txn".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": format!("^!multi-recovery-healthy-txn-{scenario_id}.*:localhost$")}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("healthy transaction appservice registration should succeed");

    let event_heavy_a_as_id = format!("multi-recovery-event-heavy-a-{}", unique_id());
    let event_heavy_a_room_id = format!("!multi-recovery-event-heavy-a-{scenario_id}-room:localhost");
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: event_heavy_a_as_id.clone(),
            url: event_heavy_a_server.uri(),
            as_token: format!("as_token_{event_heavy_a_as_id}"),
            hs_token: format!("hs_token_{event_heavy_a_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("multi-recovery event heavy a".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": format!("^!multi-recovery-event-heavy-a-{scenario_id}.*:localhost$")}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("event-heavy appservice a registration should succeed");

    let event_heavy_b_as_id = format!("multi-recovery-event-heavy-b-{}", unique_id());
    let event_heavy_b_room_id = format!("!multi-recovery-event-heavy-b-{scenario_id}-room:localhost");
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: event_heavy_b_as_id.clone(),
            url: event_heavy_b_server.uri(),
            as_token: format!("as_token_{event_heavy_b_as_id}"),
            hs_token: format!("hs_token_{event_heavy_b_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("multi-recovery event heavy b".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": format!("^!multi-recovery-event-heavy-b-{scenario_id}.*:localhost$")}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("event-heavy appservice b registration should succeed");

    storage
        .create_transaction(
            &failing_a_as_id,
            &format!("multi-recovery-failing-a-{scenario_id}"),
            &[json!({"type": "m.room.message", "content": {"body": "failing-a-once-then-recovers"}})],
        )
        .await
        .expect("failing a pending transaction should be created");
    storage
        .create_transaction(
            &failing_b_as_id,
            &format!("multi-recovery-failing-b-{scenario_id}"),
            &[json!({"type": "m.room.message", "content": {"body": "failing-b-once-then-recovers"}})],
        )
        .await
        .expect("failing b pending transaction should be created");

    for transaction_index in 0..2 {
        storage
            .create_transaction(
                &healthy_txn_as_id,
                &format!("multi-recovery-healthy-{scenario_id}-{transaction_index}"),
                &[json!({"type": "m.room.message", "content": {"body": format!("healthy-{transaction_index}")}})],
            )
            .await
            .expect("healthy transaction pending transaction should be created");
    }

    for event_index in 0..90 {
        manager
            .push_event(
                &event_heavy_a_as_id,
                &event_heavy_a_room_id,
                "m.room.message",
                "@bridge:localhost",
                json!({"msgtype": "m.text", "body": format!("multi-recovery-event-a-{event_index}")}),
                None,
            )
            .await
            .expect("event-heavy a enqueue should succeed");
        manager
            .push_event(
                &event_heavy_b_as_id,
                &event_heavy_b_room_id,
                "m.room.message",
                "@bridge:localhost",
                json!({"msgtype": "m.text", "body": format!("multi-recovery-event-b-{event_index}")}),
                None,
            )
            .await
            .expect("event-heavy b enqueue should succeed");
    }

    scheduler.run_once().await.expect("multi-recovery tick one should complete");
    scheduler.run_once().await.expect("multi-recovery tick two should complete");

    tokio::time::sleep(std::time::Duration::from_millis(5_200)).await;

    scheduler.run_once().await.expect("multi-recovery tick three should complete");
    scheduler.run_once().await.expect("multi-recovery tick four should complete");
    scheduler.run_once().await.expect("multi-recovery tick five should complete");

    let failing_a_requests = failing_a_server.received_requests().await.expect("failing a requests should load");
    let failing_b_requests = failing_b_server.received_requests().await.expect("failing b requests should load");
    let healthy_txn_requests =
        healthy_txn_server.received_requests().await.expect("healthy transaction requests should load");
    let event_heavy_a_requests =
        event_heavy_a_server.received_requests().await.expect("event-heavy a requests should load");
    let event_heavy_b_requests =
        event_heavy_b_server.received_requests().await.expect("event-heavy b requests should load");

    assert_eq!(
        failing_a_requests.len(),
        2,
        "failing service a should fail once and then succeed once after re-entering contention"
    );
    assert_eq!(
        failing_b_requests.len(),
        2,
        "failing service b should fail once and then succeed once after re-entering contention"
    );
    assert_eq!(
        healthy_txn_requests.len(),
        2,
        "healthy pending-transaction service should drain both seeded transactions despite two recovering contenders"
    );
    assert_eq!(
        event_heavy_a_requests.len(),
        3,
        "event-heavy a should still drain three batches instead of starving behind the recovering retry_backoff services"
    );
    assert_eq!(
        event_heavy_b_requests.len(),
        3,
        "event-heavy b should still drain three batches instead of starving behind the recovering retry_backoff services"
    );

    let healthy_txn_pending =
        storage.get_pending_transactions(&healthy_txn_as_id).await.expect("healthy pending transactions should load");
    assert!(healthy_txn_pending.is_empty(), "healthy transaction backlog should be drained");

    let event_heavy_a_pending =
        storage.get_pending_events(&event_heavy_a_as_id, 120).await.expect("event-heavy a pending events should load");
    let event_heavy_b_pending =
        storage.get_pending_events(&event_heavy_b_as_id, 120).await.expect("event-heavy b pending events should load");
    assert!(
        event_heavy_a_pending.is_empty(),
        "event-heavy a backlog should be drained by the end of the recovery window"
    );
    assert!(
        event_heavy_b_pending.is_empty(),
        "event-heavy b backlog should be drained by the end of the recovery window"
    );

    for failing_as_id in [&failing_a_as_id, &failing_b_as_id] {
        let failing_last_result = manager
            .get_state(failing_as_id, "scheduler_last_result")
            .await
            .expect("failing last result lookup should succeed")
            .expect("failing last result should be persisted");
        assert!(
            matches!(failing_last_result.state_value.as_str(), "success" | "dispatched"),
            "recovering retry_backoff service should end in a healthy dispatched/success state"
        );

        let failing_transaction_state = manager
            .get_state(failing_as_id, "scheduler_transaction_state")
            .await
            .expect("failing transaction state lookup should succeed")
            .expect("failing transaction state should be persisted");
        assert_eq!(failing_transaction_state.state_value, "idle");

        let failing_backoff_count = manager
            .get_state(failing_as_id, "scheduler_total_backoff_count")
            .await
            .expect("failing backoff count lookup should succeed")
            .expect("failing backoff count should be persisted");
        assert!(
            failing_backoff_count.state_value.parse::<i64>().unwrap_or(0) >= 1,
            "each recovering service should enter retry_backoff before becoming ready again"
        );

        let failing_statistics = manager
            .get_statistics()
            .await
            .expect("scheduler statistics should load")
            .into_iter()
            .find(|entry| entry["as_id"] == failing_as_id.as_str())
            .expect("recovering appservice should be present in statistics");
        assert_eq!(failing_statistics["pending_transaction_count"], 0);
        assert_eq!(failing_statistics["scheduler"]["transaction_state"], "idle");
    }

    let event_heavy_a_capacity_count = manager
        .get_state(&event_heavy_a_as_id, "scheduler_total_capacity_limited_count")
        .await
        .expect("event-heavy a capacity count lookup should succeed")
        .expect("event-heavy a capacity count should be persisted");
    let event_heavy_b_capacity_count = manager
        .get_state(&event_heavy_b_as_id, "scheduler_total_capacity_limited_count")
        .await
        .expect("event-heavy b capacity count lookup should succeed")
        .expect("event-heavy b capacity count should be persisted");
    assert!(
        event_heavy_a_capacity_count.state_value.parse::<i64>().unwrap_or(0) >= 1
            && event_heavy_b_capacity_count.state_value.parse::<i64>().unwrap_or(0) >= 1,
        "event bucket services should observe real capacity pressure before later rotating back into dispatch"
    );
}

#[tokio::test]
async fn test_appservice_scheduler_uses_custom_backlog_thresholds_for_limited_service() {
    let pool = crate::require_test_pool().await;
    setup_appservice_test_database(&pool).await;

    let first_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&first_server).await;

    let second_server = MockServer::start().await;
    Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&second_server).await;

    let manager = create_test_appservice_manager(&pool);
    let scheduler = ApplicationServiceScheduler::with_capacity_options(manager.clone(), 16, 500, 1, 3, 2);

    let first_as_id = format!("threshold-a-{}", unique_id());
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: first_as_id.clone(),
            url: first_server.uri(),
            as_token: format!("as_token_{first_as_id}"),
            hs_token: format!("hs_token_{first_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("threshold first".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": "^!threshold-a.*:localhost$"}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("first appservice registration should succeed");

    let second_as_id = format!("threshold-b-{}", unique_id());
    manager
        .register(RegisterApplicationServiceRequest {
            as_id: second_as_id.clone(),
            url: second_server.uri(),
            as_token: format!("as_token_{second_as_id}"),
            hs_token: format!("hs_token_{second_as_id}"),
            sender: "@bridge:localhost".to_string(),
            description: Some("threshold second".to_string()),
            is_rate_limited: Some(false),
            protocols: None,
            namespaces: Some(json!({
                "users": [],
                "aliases": [],
                "rooms": [{"exclusive": true, "regex": "^!threshold-b.*:localhost$"}]
            })),
            api_key: None,
            config: None,
        })
        .await
        .expect("second appservice registration should succeed");

    manager
        .push_event(
            &first_as_id,
            "!threshold-a-room:localhost",
            "m.room.message",
            "@bridge:localhost",
            json!({"msgtype": "m.text", "body": "first"}),
            None,
        )
        .await
        .expect("first event enqueue should succeed");

    for event_index in 0..2 {
        manager
            .push_event(
                &second_as_id,
                "!threshold-b-room:localhost",
                "m.room.message",
                "@bridge:localhost",
                json!({"msgtype": "m.text", "body": format!("second-{event_index}")}),
                None,
            )
            .await
            .expect("second event enqueue should succeed");
    }

    scheduler.run_once().await.expect("threshold-aware scheduler tick should complete");

    let first_requests = first_server.received_requests().await.expect("first requests should load");
    let second_requests = second_server.received_requests().await.expect("second requests should load");
    assert_eq!(
        first_requests.len() + second_requests.len(),
        1,
        "scheduler should still respect max_services_per_tick=1"
    );

    let limited_as_id = if first_requests.is_empty() { &first_as_id } else { &second_as_id };
    let limited_pending_events = manager
        .get_state(limited_as_id, "scheduler_pending_event_count")
        .await
        .expect("limited pending events lookup should succeed")
        .expect("limited pending events should be persisted");
    let limited_backlog_state = manager
        .get_state(limited_as_id, "scheduler_backlog_state")
        .await
        .expect("limited backlog state lookup should succeed")
        .expect("limited backlog state should be persisted");
    let limited_transaction_state = manager
        .get_state(limited_as_id, "scheduler_transaction_state")
        .await
        .expect("limited transaction state lookup should succeed")
        .expect("limited transaction state should be persisted");

    if limited_as_id == &first_as_id {
        assert_eq!(limited_pending_events.state_value, "1");
    } else {
        assert_eq!(limited_pending_events.state_value, "2");
    }
    assert_eq!(limited_backlog_state.state_value, "normal");
    assert_eq!(limited_transaction_state.state_value, "capacity_limited");
}

#[tokio::test]
async fn test_appservice_scheduler_default_capacity_limit_handles_ninth_service() {
    let pool = crate::require_test_pool().await;
    setup_appservice_test_database(&pool).await;

    let manager = create_test_appservice_manager(&pool);
    let scheduler = ApplicationServiceScheduler::new(manager.clone());

    let mut servers = Vec::new();
    let mut as_ids = Vec::new();

    for service_index in 0..9 {
        let server = MockServer::start().await;
        Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&server).await;

        let as_id = format!("default-capacity-{service_index}-{}", unique_id());
        manager
            .register(RegisterApplicationServiceRequest {
                as_id: as_id.clone(),
                url: server.uri(),
                as_token: format!("as_token_{as_id}"),
                hs_token: format!("hs_token_{as_id}"),
                sender: "@bridge:localhost".to_string(),
                description: Some(format!("default capacity service {service_index}")),
                is_rate_limited: Some(false),
                protocols: None,
                namespaces: Some(json!({
                    "users": [],
                    "aliases": [],
                    "rooms": [{"exclusive": true, "regex": format!("^!default-capacity-{service_index}.*:localhost$")}]
                })),
                api_key: None,
                config: None,
            })
            .await
            .expect("appservice registration should succeed");

        manager
            .push_event(
                &as_id,
                &format!("!default-capacity-{service_index}-room:localhost"),
                "m.room.message",
                "@bridge:localhost",
                json!({"msgtype": "m.text", "body": format!("service-{service_index}")}),
                None,
            )
            .await
            .expect("event enqueue should succeed");

        servers.push(server);
        as_ids.push(as_id);
    }

    scheduler.run_once().await.expect("default-capacity scheduler tick should complete");

    let mut request_counts = Vec::new();
    for server in &servers {
        request_counts.push(server.received_requests().await.expect("requests should load").len());
    }

    assert_eq!(
        request_counts.iter().sum::<usize>(),
        8,
        "default scheduler should dispatch at most eight services in one tick"
    );
    assert_eq!(
        request_counts.iter().filter(|count| **count == 0).count(),
        1,
        "exactly one service should be deferred when nine services compete for the default capacity of eight"
    );
    assert!(
        request_counts.iter().all(|count| *count <= 1),
        "each service should receive at most one transaction in the single-tick boundary test"
    );

    let limited_index = request_counts
        .iter()
        .position(|count| *count == 0)
        .expect("one service should remain limited by the default capacity");
    let limited_as_id = &as_ids[limited_index];

    let limited_result = manager
        .get_state(limited_as_id, "scheduler_last_result")
        .await
        .expect("limited result lookup should succeed")
        .expect("limited result should be persisted");
    assert_eq!(limited_result.state_value, "capacity_limited");

    let limited_txn_state = manager
        .get_state(limited_as_id, "scheduler_transaction_state")
        .await
        .expect("limited transaction state lookup should succeed")
        .expect("limited transaction state should be persisted");
    assert_eq!(limited_txn_state.state_value, "capacity_limited");

    let limited_pending_events = manager
        .get_state(limited_as_id, "scheduler_pending_event_count")
        .await
        .expect("limited pending events lookup should succeed")
        .expect("limited pending events should be persisted");
    assert_eq!(limited_pending_events.state_value, "1");

    let limited_backlog_state = manager
        .get_state(limited_as_id, "scheduler_backlog_state")
        .await
        .expect("limited backlog state lookup should succeed")
        .expect("limited backlog state should be persisted");
    assert_eq!(
        limited_backlog_state.state_value, "normal",
        "default pending thresholds should not classify a single deferred event as high backlog"
    );

    let limited_count = manager
        .get_state(limited_as_id, "scheduler_total_capacity_limited_count")
        .await
        .expect("limited count lookup should succeed")
        .expect("limited count should be persisted");
    assert_eq!(limited_count.state_value, "1");
}

#[tokio::test]
async fn test_appservice_scheduler_persists_different_backlog_state_for_default_vs_aggressive_event_thresholds() {
    async fn run_backlog_state_scenario(high_pending_event_threshold: i64) -> String {
        let pool = crate::require_test_pool().await;
        setup_appservice_test_database(&pool).await;

        let first_server = MockServer::start().await;
        Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&first_server).await;

        let second_server = MockServer::start().await;
        Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&second_server).await;

        let manager = create_test_appservice_manager(&pool);
        let scheduler = ApplicationServiceScheduler::with_capacity_options(
            manager.clone(),
            16,
            500,
            1,
            high_pending_event_threshold,
            2,
        );

        let scenario_id = unique_id();
        let first_as_id = format!("compare-threshold-a-{}", unique_id());
        let first_room_id = format!("!compare-threshold-a-{scenario_id}-room:localhost");
        let first_room_namespace = format!("^!compare-threshold-a-{scenario_id}.*:localhost$");
        manager
            .register(RegisterApplicationServiceRequest {
                as_id: first_as_id.clone(),
                url: first_server.uri(),
                as_token: format!("as_token_{first_as_id}"),
                hs_token: format!("hs_token_{first_as_id}"),
                sender: "@bridge:localhost".to_string(),
                description: Some("compare threshold first".to_string()),
                is_rate_limited: Some(false),
                protocols: None,
                namespaces: Some(json!({
                    "users": [],
                    "aliases": [],
                    "rooms": [{"exclusive": true, "regex": first_room_namespace}]
                })),
                api_key: None,
                config: None,
            })
            .await
            .expect("first appservice registration should succeed");

        let second_as_id = format!("compare-threshold-b-{}", unique_id());
        let second_room_id = format!("!compare-threshold-b-{scenario_id}-room:localhost");
        let second_room_namespace = format!("^!compare-threshold-b-{scenario_id}.*:localhost$");
        manager
            .register(RegisterApplicationServiceRequest {
                as_id: second_as_id.clone(),
                url: second_server.uri(),
                as_token: format!("as_token_{second_as_id}"),
                hs_token: format!("hs_token_{second_as_id}"),
                sender: "@bridge:localhost".to_string(),
                description: Some("compare threshold second".to_string()),
                is_rate_limited: Some(false),
                protocols: None,
                namespaces: Some(json!({
                    "users": [],
                    "aliases": [],
                    "rooms": [{"exclusive": true, "regex": second_room_namespace}]
                })),
                api_key: None,
                config: None,
            })
            .await
            .expect("second appservice registration should succeed");

        for event_index in 0..25 {
            manager
                .push_event(
                    &first_as_id,
                    &first_room_id,
                    "m.room.message",
                    "@bridge:localhost",
                    json!({"msgtype": "m.text", "body": format!("first-{event_index}")}),
                    None,
                )
                .await
                .expect("first event enqueue should succeed");
            manager
                .push_event(
                    &second_as_id,
                    &second_room_id,
                    "m.room.message",
                    "@bridge:localhost",
                    json!({"msgtype": "m.text", "body": format!("second-{event_index}")}),
                    None,
                )
                .await
                .expect("second event enqueue should succeed");
        }

        scheduler.run_once().await.expect("threshold comparison scheduler tick should complete");

        let first_requests = first_server.received_requests().await.expect("first requests should load");
        let second_requests = second_server.received_requests().await.expect("second requests should load");
        assert_eq!(
            first_requests.len() + second_requests.len(),
            1,
            "comparison scenario should still dispatch only one service"
        );

        let limited_as_id = if first_requests.is_empty() { &first_as_id } else { &second_as_id };
        manager
            .get_state(limited_as_id, "scheduler_backlog_state")
            .await
            .expect("limited backlog state lookup should succeed")
            .expect("limited backlog state should be persisted")
            .state_value
    }

    let default_backlog_state = run_backlog_state_scenario(50).await;
    let aggressive_backlog_state = run_backlog_state_scenario(25).await;

    assert_eq!(default_backlog_state, "normal");
    assert_eq!(aggressive_backlog_state, "high");
}

#[tokio::test]
async fn test_appservice_scheduler_persists_different_backlog_state_for_default_vs_aggressive_transaction_thresholds() {
    async fn run_backlog_state_scenario(high_pending_transaction_threshold: i64) -> String {
        let pool = crate::require_test_pool().await;
        setup_appservice_test_database(&pool).await;

        let first_server = MockServer::start().await;
        Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&first_server).await;

        let second_server = MockServer::start().await;
        Mock::given(method("PUT")).respond_with(ResponseTemplate::new(200)).mount(&second_server).await;

        let manager = create_test_appservice_manager(&pool);
        let scheduler = ApplicationServiceScheduler::with_capacity_options(
            manager.clone(),
            16,
            500,
            1,
            50,
            high_pending_transaction_threshold,
        );
        let storage = ApplicationServiceStorage::new(&pool);

        let scenario_id = unique_id();
        let first_as_id = format!("compare-transaction-threshold-a-{}", unique_id());
        let first_room_namespace = format!("^!compare-transaction-threshold-a-{scenario_id}.*:localhost$");
        manager
            .register(RegisterApplicationServiceRequest {
                as_id: first_as_id.clone(),
                url: first_server.uri(),
                as_token: format!("as_token_{first_as_id}"),
                hs_token: format!("hs_token_{first_as_id}"),
                sender: "@bridge:localhost".to_string(),
                description: Some("compare transaction threshold first".to_string()),
                is_rate_limited: Some(false),
                protocols: None,
                namespaces: Some(json!({
                    "users": [],
                    "aliases": [],
                    "rooms": [{"exclusive": true, "regex": first_room_namespace}]
                })),
                api_key: None,
                config: None,
            })
            .await
            .expect("first appservice registration should succeed");

        let second_as_id = format!("compare-transaction-threshold-b-{}", unique_id());
        let second_room_namespace = format!("^!compare-transaction-threshold-b-{scenario_id}.*:localhost$");
        manager
            .register(RegisterApplicationServiceRequest {
                as_id: second_as_id.clone(),
                url: second_server.uri(),
                as_token: format!("as_token_{second_as_id}"),
                hs_token: format!("hs_token_{second_as_id}"),
                sender: "@bridge:localhost".to_string(),
                description: Some("compare transaction threshold second".to_string()),
                is_rate_limited: Some(false),
                protocols: None,
                namespaces: Some(json!({
                    "users": [],
                    "aliases": [],
                    "rooms": [{"exclusive": true, "regex": second_room_namespace}]
                })),
                api_key: None,
                config: None,
            })
            .await
            .expect("second appservice registration should succeed");

        storage
            .create_transaction(
                &first_as_id,
                &format!("compare-transaction-a-{scenario_id}"),
                &[json!({"type": "m.room.message", "content": {"body": "first"}})],
            )
            .await
            .expect("first pending transaction should be created");
        storage
            .create_transaction(
                &second_as_id,
                &format!("compare-transaction-b-{scenario_id}"),
                &[json!({"type": "m.room.message", "content": {"body": "second"}})],
            )
            .await
            .expect("second pending transaction should be created");

        scheduler.run_once().await.expect("transaction threshold comparison scheduler tick should complete");

        let first_requests = first_server.received_requests().await.expect("first requests should load");
        let second_requests = second_server.received_requests().await.expect("second requests should load");
        assert_eq!(
            first_requests.len() + second_requests.len(),
            1,
            "comparison scenario should still dispatch only one service"
        );

        let limited_as_id = if first_requests.is_empty() { &first_as_id } else { &second_as_id };

        let limited_pending_transactions = manager
            .get_state(limited_as_id, "scheduler_pending_transaction_count")
            .await
            .expect("limited pending transaction count lookup should succeed")
            .expect("limited pending transaction count should be persisted");
        assert_eq!(limited_pending_transactions.state_value, "1");

        manager
            .get_state(limited_as_id, "scheduler_backlog_state")
            .await
            .expect("limited backlog state lookup should succeed")
            .expect("limited backlog state should be persisted")
            .state_value
    }

    let default_backlog_state = run_backlog_state_scenario(2).await;
    let aggressive_backlog_state = run_backlog_state_scenario(1).await;

    assert_eq!(default_backlog_state, "normal");
    assert_eq!(aggressive_backlog_state, "high");
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
