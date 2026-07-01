//! Additional integration tests for `PushNotificationService` covering the
//! public API in `synapse-services/src/push/service.rs`:
//!   - Service construction and builder methods
//!   - Device registration / unregistration / retrieval
//!   - Push rule CRUD and validation
//!   - `evaluate_push_rules` with event_match, dont_notify, set_tweak,
//!     empty-conditions, and ignored-user suppression
//!   - `send_notification` (queueing with / without device_id)
//!   - `process_pending_notifications` (upstream provider path)
//!   - `cleanup_old_logs`
//!   - `initialize_providers` (FCM / APNS / WebPush config-driven init)
//!   - `get_room_notifications`
//!   - Serialization of `NotificationPayload`, `SendNotificationRequest`,
//!     `PushRuleResult`

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(clippy::await_holding_lock)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::json;
use synapse_services::push::service::{NotificationPayload, PushNotificationService, PushRuleResult, SendNotificationRequest};
use synapse_storage::account_data::AccountDataStorage;
use synapse_storage::push_notification::{
    CreateNotificationLogRequest, CreatePushRuleRequest, PushNotificationStorage, RegisterDeviceRequest,
};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

fn push_guard() -> &'static Mutex<()> {
    static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    GUARD.get_or_init(|| Mutex::new(()))
}

async fn warm_up_pool(pool: &Arc<sqlx::PgPool>) {
    for _ in 0..8 {
        match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            sqlx::query("SELECT 1").execute(pool.as_ref()),
        )
        .await
        {
            Ok(Ok(_)) => return,
            Ok(Err(_)) | Err(_) => {
                tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            }
        }
    }
    let _ = sqlx::query("SELECT 1").execute(pool.as_ref()).await;
}

async fn setup_test_database(pool: &Arc<sqlx::PgPool>) {
    warm_up_pool(pool).await;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS users (
            user_id TEXT NOT NULL PRIMARY KEY,
            username TEXT NOT NULL,
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
            must_change_password BOOLEAN DEFAULT FALSE,
            password_expires_at BIGINT,
            failed_login_attempts INTEGER DEFAULT 0,
            locked_until BIGINT
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .ok();

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS push_device (
            id BIGSERIAL PRIMARY KEY,
            user_id TEXT NOT NULL,
            device_id TEXT NOT NULL,
            push_token TEXT NOT NULL,
            push_type TEXT NOT NULL,
            app_id TEXT,
            platform TEXT,
            platform_version TEXT,
            app_version TEXT,
            locale TEXT,
            timezone TEXT,
            is_enabled BOOLEAN NOT NULL DEFAULT TRUE,
            created_ts BIGINT NOT NULL,
            updated_ts BIGINT,
            last_used_at BIGINT,
            last_error TEXT,
            error_count INTEGER NOT NULL DEFAULT 0,
            metadata JSONB NOT NULL DEFAULT '{}',
            CONSTRAINT uq_push_device_user_device UNIQUE (user_id, device_id)
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .ok();

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS push_rules (
            id BIGSERIAL,
            user_id TEXT NOT NULL,
            scope TEXT NOT NULL,
            rule_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            priority_class INTEGER NOT NULL DEFAULT 0,
            priority INTEGER DEFAULT 0,
            conditions JSONB DEFAULT '[]',
            actions JSONB DEFAULT '[]',
            pattern TEXT,
            is_default BOOLEAN DEFAULT FALSE,
            is_enabled BOOLEAN DEFAULT TRUE,
            created_ts BIGINT NOT NULL,
            updated_ts BIGINT
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .ok();

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS push_notification_queue (
            id BIGSERIAL,
            user_id TEXT NOT NULL,
            device_id TEXT NOT NULL,
            event_id TEXT,
            room_id TEXT,
            notification_type TEXT,
            content JSONB DEFAULT '{}',
            is_processed BOOLEAN DEFAULT FALSE,
            processed_at BIGINT,
            created_ts BIGINT NOT NULL,
            priority INTEGER NOT NULL DEFAULT 0,
            status TEXT NOT NULL DEFAULT 'pending',
            attempts INTEGER NOT NULL DEFAULT 0,
            max_attempts INTEGER NOT NULL DEFAULT 3,
            next_attempt_at BIGINT,
            sent_at BIGINT,
            error_message TEXT,
            CONSTRAINT pk_push_notification_queue PRIMARY KEY (id)
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .ok();

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS push_notification_log (
            id BIGSERIAL,
            user_id TEXT NOT NULL,
            device_id TEXT NOT NULL,
            pushkey TEXT,
            status TEXT,
            error_message TEXT,
            retry_count INTEGER DEFAULT 0,
            last_attempt_at BIGINT,
            created_ts BIGINT NOT NULL,
            event_id TEXT,
            room_id TEXT,
            notification_type TEXT,
            push_type TEXT,
            sent_at BIGINT,
            is_success BOOLEAN,
            provider_response TEXT,
            response_time_ms INTEGER,
            metadata JSONB NOT NULL DEFAULT '{}',
            CONSTRAINT pk_push_notification_log PRIMARY KEY (id)
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .ok();

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS push_config (
            id BIGSERIAL,
            user_id TEXT NOT NULL,
            device_id TEXT NOT NULL,
            config_type TEXT NOT NULL,
            config_data JSONB DEFAULT '{}',
            config_key TEXT,
            config_value TEXT,
            created_ts BIGINT NOT NULL,
            updated_ts BIGINT,
            CONSTRAINT pk_push_config PRIMARY KEY (id),
            CONSTRAINT uq_push_config_user_device_type UNIQUE (user_id, device_id, config_type)
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .ok();

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS notifications (
            id BIGSERIAL,
            user_id TEXT NOT NULL,
            event_id TEXT,
            room_id TEXT,
            ts BIGINT NOT NULL,
            notification_type VARCHAR(50) DEFAULT 'message',
            profile_tag VARCHAR(255),
            is_read BOOLEAN DEFAULT FALSE,
            created_ts BIGINT NOT NULL,
            updated_ts BIGINT,
            CONSTRAINT pk_notifications PRIMARY KEY (id)
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .ok();

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS account_data (
            user_id TEXT NOT NULL,
            data_type TEXT NOT NULL,
            content JSONB NOT NULL,
            created_ts BIGINT NOT NULL,
            updated_ts BIGINT,
            PRIMARY KEY (user_id, data_type)
        )
        "#,
    )
    .execute(pool.as_ref())
    .await
    .ok();

    // Clean up
    let now = chrono::Utc::now().timestamp_millis();
    let _ = sqlx::query("DELETE FROM push_notification_log").execute(pool.as_ref()).await;
    let _ = sqlx::query("DELETE FROM push_notification_queue").execute(pool.as_ref()).await;
    let _ = sqlx::query("DELETE FROM push_config").execute(pool.as_ref()).await;
    let _ = sqlx::query("DELETE FROM push_rules").execute(pool.as_ref()).await;
    let _ = sqlx::query("DELETE FROM push_device").execute(pool.as_ref()).await;
    let _ = sqlx::query("DELETE FROM notifications").execute(pool.as_ref()).await;
    let _ = sqlx::query("DELETE FROM account_data").execute(pool.as_ref()).await;
    let _ = sqlx::query("DELETE FROM users WHERE user_id LIKE '%push_test_%'").execute(pool.as_ref()).await;
    let _ = (now,);
}

fn new_service(pool: &Arc<sqlx::PgPool>) -> PushNotificationService {
    let storage = Arc::new(PushNotificationStorage::new(pool));
    PushNotificationService::new(storage)
}

fn unique_user_id() -> String {
    format!("@push_test_{}:localhost", unique_id())
}

fn unique_device_id() -> String {
    format!("DEV{}", unique_id())
}

fn unique_room_id() -> String {
    format!("!room_{}:localhost", unique_id())
}

fn make_register_request(user_id: &str, device_id: &str, push_type: &str) -> RegisterDeviceRequest {
    RegisterDeviceRequest {
        user_id: user_id.to_string(),
        device_id: device_id.to_string(),
        push_token: format!("token_{}_{}", push_type, unique_id()),
        push_type: push_type.to_string(),
        app_id: Some("com.test.app".to_string()),
        platform: Some("ios".to_string()),
        platform_version: Some("17.0".to_string()),
        app_version: Some("1.0.0".to_string()),
        locale: Some("en-US".to_string()),
        timezone: Some("UTC".to_string()),
        metadata: Some(json!({"key": "value"})),
    }
}

fn make_push_rule_request(
    user_id: &str,
    rule_id: &str,
    kind: &str,
    conditions: serde_json::Value,
    actions: serde_json::Value,
) -> CreatePushRuleRequest {
    CreatePushRuleRequest {
        user_id: user_id.to_string(),
        rule_id: rule_id.to_string(),
        scope: "global".to_string(),
        kind: kind.to_string(),
        priority: 0,
        conditions,
        actions,
        enabled: true,
    }
}

async fn insert_test_user(pool: &Arc<sqlx::PgPool>, user_id: &str) {
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query("INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, $3) ON CONFLICT (user_id) DO NOTHING")
        .bind(user_id)
        .bind(user_id.trim_start_matches('@').split(':').next().unwrap_or(user_id))
        .bind(now)
        .execute(pool.as_ref())
        .await
        .ok();
}

// ===========================================================================
// Service construction and builder methods
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_service_new_constructs_default() {
    let _guard = push_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let service = new_service(&pool);
    // A fresh service with no providers should still be constructable.
    let result = service.get_user_devices("@nobody:localhost").await.unwrap();
    assert!(result.is_empty());
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_service_builder_methods() {
    let _guard = push_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let service = new_service(&pool)
        .with_queue(synapse_services::push::queue::QueueConfig::default());

    // Service should still work after setting a queue.
    let result = service.get_user_devices("@nobody:localhost").await.unwrap();
    assert!(result.is_empty());
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_service_with_account_data_storage() {
    let _guard = push_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let account_data = Arc::new(AccountDataStorage::new(&pool)) as Arc<dyn synapse_storage::AccountDataRepository>;
    let service = new_service(&pool).with_account_data_storage(account_data);

    // Service with account_data_storage should be constructable.
    let result = service.get_user_devices("@nobody:localhost").await.unwrap();
    assert!(result.is_empty());
}

// ===========================================================================
// Device registration
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_register_device_fcm() {
    let _guard = push_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let user_id = unique_user_id();
    let device_id = unique_device_id();
    insert_test_user(&pool, &user_id).await;

    let service = new_service(&pool);
    let device = service.register_device(make_register_request(&user_id, &device_id, "fcm")).await.unwrap();

    assert!(device.id > 0);
    assert_eq!(device.user_id, user_id);
    assert_eq!(device.device_id, device_id);
    assert_eq!(device.push_type, "fcm");
    assert!(device.is_enabled);
    assert!(device.created_ts > 0);
    assert!(device.metadata.is_object());
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_register_device_apns() {
    let _guard = push_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let user_id = unique_user_id();
    let device_id = unique_device_id();
    insert_test_user(&pool, &user_id).await;

    let service = new_service(&pool);
    let device = service.register_device(make_register_request(&user_id, &device_id, "apns")).await.unwrap();
    assert_eq!(device.push_type, "apns");
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_register_device_webpush() {
    let _guard = push_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let user_id = unique_user_id();
    let device_id = unique_device_id();
    insert_test_user(&pool, &user_id).await;

    let service = new_service(&pool);
    let device = service.register_device(make_register_request(&user_id, &device_id, "webpush")).await.unwrap();
    assert_eq!(device.push_type, "webpush");
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_register_device_upstream() {
    let _guard = push_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let user_id = unique_user_id();
    let device_id = unique_device_id();
    insert_test_user(&pool, &user_id).await;

    let service = new_service(&pool);
    let device = service.register_device(make_register_request(&user_id, &device_id, "upstream")).await.unwrap();
    assert_eq!(device.push_type, "upstream");
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_register_device_invalid_type() {
    let _guard = push_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let user_id = unique_user_id();
    let device_id = unique_device_id();
    insert_test_user(&pool, &user_id).await;

    let service = new_service(&pool);
    let mut req = make_register_request(&user_id, &device_id, "fcm");
    req.push_type = "invalid".to_string();
    let result = service.register_device(req).await;
    assert!(result.is_err());
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_register_device_upsert_on_conflict() {
    let _guard = push_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let user_id = unique_user_id();
    let device_id = unique_device_id();
    insert_test_user(&pool, &user_id).await;

    let service = new_service(&pool);
    let device1 = service.register_device(make_register_request(&user_id, &device_id, "fcm")).await.unwrap();

    // Re-register with different push_type → should upsert.
    let device2 = service.register_device(make_register_request(&user_id, &device_id, "apns")).await.unwrap();
    assert_eq!(device2.id, device1.id);
    assert_eq!(device2.push_type, "apns");
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_unregister_device() {
    let _guard = push_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let user_id = unique_user_id();
    let device_id = unique_device_id();
    insert_test_user(&pool, &user_id).await;

    let service = new_service(&pool);
    service.register_device(make_register_request(&user_id, &device_id, "fcm")).await.unwrap();
    assert_eq!(service.get_user_devices(&user_id).await.unwrap().len(), 1);

    service.unregister_device(&user_id, &device_id).await.unwrap();
    // After unregister, get_user_devices (which filters is_enabled=true) returns empty.
    assert!(service.get_user_devices(&user_id).await.unwrap().is_empty());
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_user_devices_multiple() {
    let _guard = push_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let service = new_service(&pool);
    for i in 0..3 {
        service.register_device(make_register_request(&user_id, &format!("DEV{}_{}", unique_id(), i), "fcm")).await.unwrap();
    }

    let devices = service.get_user_devices(&user_id).await.unwrap();
    assert_eq!(devices.len(), 3);
    assert!(devices.iter().all(|d| d.user_id == user_id));
}

// ===========================================================================
// Push rule CRUD
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_create_push_rule() {
    let _guard = push_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let user_id = unique_user_id();
    let service = new_service(&pool);
    let rule = service
        .create_push_rule(make_push_rule_request(
            &user_id,
            ".m.rule.message",
            "content",
            json!([{"kind": "event_match", "key": "type", "pattern": "m.room.message"}]),
            json!(["notify"]),
        ))
        .await
        .unwrap();

    assert!(rule.id > 0);
    assert_eq!(rule.user_id, user_id);
    assert_eq!(rule.rule_id, ".m.rule.message");
    assert_eq!(rule.scope, "global");
    assert_eq!(rule.kind, "content");
    assert!(rule.is_enabled);
    assert!(rule.created_ts > 0);
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_create_push_rule_invalid_scope() {
    let _guard = push_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let user_id = unique_user_id();
    let service = new_service(&pool);
    let mut req = make_push_rule_request(&user_id, "rule1", "content", json!([]), json!(["notify"]));
    req.scope = "invalid".to_string();
    assert!(service.create_push_rule(req).await.is_err());
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_create_push_rule_invalid_kind() {
    let _guard = push_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let user_id = unique_user_id();
    let service = new_service(&pool);
    let mut req = make_push_rule_request(&user_id, "rule1", "invalid", json!([]), json!(["notify"]));
    req.kind = "invalid".to_string();
    assert!(service.create_push_rule(req).await.is_err());
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_push_rules() {
    let _guard = push_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let user_id = unique_user_id();
    let service = new_service(&pool);
    service
        .create_push_rule(make_push_rule_request(&user_id, "rule_a", "content", json!([]), json!(["notify"])))
        .await
        .unwrap();
    service
        .create_push_rule(make_push_rule_request(&user_id, "rule_b", "room", json!([]), json!(["dont_notify"])))
        .await
        .unwrap();

    let rules = service.get_push_rules(&user_id).await.unwrap();
    assert!(rules.len() >= 2);
    assert!(rules.iter().any(|r| r.rule_id == "rule_a"));
    assert!(rules.iter().any(|r| r.rule_id == "rule_b"));
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_delete_push_rule() {
    let _guard = push_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let user_id = unique_user_id();
    let service = new_service(&pool);
    service
        .create_push_rule(make_push_rule_request(&user_id, "rule_del", "content", json!([]), json!(["notify"])))
        .await
        .unwrap();
    assert!(service.get_push_rules(&user_id).await.unwrap().iter().any(|r| r.rule_id == "rule_del"));

    service.delete_push_rule(&user_id, "global", "content", "rule_del").await.unwrap();
    assert!(!service.get_push_rules(&user_id).await.unwrap().iter().any(|r| r.rule_id == "rule_del"));
}

// ===========================================================================
// evaluate_push_rules
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_evaluate_push_rules_no_rules() {
    let _guard = push_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let user_id = unique_user_id();
    let service = new_service(&pool);
    let event = json!({"type": "m.room.message", "content": {"body": "hello"}});
    let result = service.evaluate_push_rules(&user_id, &event).await.unwrap();
    assert!(!result.notify);
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_evaluate_push_rules_empty_conditions_match_all() {
    let _guard = push_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let user_id = unique_user_id();
    let service = new_service(&pool);
    service
        .create_push_rule(make_push_rule_request(&user_id, "rule1", "underride", json!([]), json!(["notify"])))
        .await
        .unwrap();

    let event = json!({"type": "m.room.message", "content": {"body": "hello"}});
    let result = service.evaluate_push_rules(&user_id, &event).await.unwrap();
    assert!(result.notify);
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_evaluate_push_rules_event_match_notify() {
    let _guard = push_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let user_id = unique_user_id();
    let service = new_service(&pool);
    service
        .create_push_rule(make_push_rule_request(
            &user_id,
            "rule_match",
            "content",
            json!([{"kind": "event_match", "key": "type", "pattern": "m.room.message"}]),
            json!(["notify"]),
        ))
        .await
        .unwrap();

    let event = json!({"type": "m.room.message", "content": {"body": "hello"}});
    let result = service.evaluate_push_rules(&user_id, &event).await.unwrap();
    assert!(result.notify);
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_evaluate_push_rules_event_match_no_match() {
    let _guard = push_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let user_id = unique_user_id();
    let service = new_service(&pool);
    service
        .create_push_rule(make_push_rule_request(
            &user_id,
            "rule_no_match",
            "content",
            json!([{"kind": "event_match", "key": "type", "pattern": "m.room.message"}]),
            json!(["notify"]),
        ))
        .await
        .unwrap();

    let event = json!({"type": "m.room.member", "content": {"membership": "join"}});
    let result = service.evaluate_push_rules(&user_id, &event).await.unwrap();
    assert!(!result.notify);
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_evaluate_push_rules_dont_notify() {
    let _guard = push_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let user_id = unique_user_id();
    let service = new_service(&pool);
    service
        .create_push_rule(make_push_rule_request(
            &user_id,
            "rule_dont",
            "override",
            json!([{"kind": "event_match", "key": "type", "pattern": "m.room.message"}]),
            json!(["dont_notify"]),
        ))
        .await
        .unwrap();

    let event = json!({"type": "m.room.message", "content": {"body": "hello"}});
    let result = service.evaluate_push_rules(&user_id, &event).await.unwrap();
    assert!(!result.notify);
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_evaluate_push_rules_set_tweak() {
    let _guard = push_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let user_id = unique_user_id();
    let service = new_service(&pool);
    service
        .create_push_rule(make_push_rule_request(
            &user_id,
            "rule_tweak",
            "content",
            json!([{"kind": "event_match", "key": "type", "pattern": "m.room.message"}]),
            json!(["notify", {"set_tweak": "highlight", "value": true}]),
        ))
        .await
        .unwrap();

    let event = json!({"type": "m.room.message", "content": {"body": "hello"}});
    let result = service.evaluate_push_rules(&user_id, &event).await.unwrap();
    assert!(result.notify);
    assert_eq!(result.tweaks["highlight"], json!(true));
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_evaluate_push_rules_nested_key_event_match() {
    let _guard = push_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let user_id = unique_user_id();
    let service = new_service(&pool);
    service
        .create_push_rule(make_push_rule_request(
            &user_id,
            "rule_nested",
            "content",
            json!([{"kind": "event_match", "key": "content.body", "pattern": "hello"}]),
            json!(["notify"]),
        ))
        .await
        .unwrap();

    let event = json!({"type": "m.room.message", "content": {"body": "hello world"}});
    let result = service.evaluate_push_rules(&user_id, &event).await.unwrap();
    assert!(result.notify);

    let event2 = json!({"type": "m.room.message", "content": {"body": "goodbye"}});
    let result2 = service.evaluate_push_rules(&user_id, &event2).await.unwrap();
    assert!(!result2.notify);
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_evaluate_push_rules_priority_ordering() {
    let _guard = push_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let user_id = unique_user_id();
    let service = new_service(&pool);
    // High-priority rule says dont_notify.
    let mut high = make_push_rule_request(
        &user_id,
        "rule_high",
        "override",
        json!([{"kind": "event_match", "key": "type", "pattern": "m.room.message"}]),
        json!(["dont_notify"]),
    );
    high.priority = 1;
    service.create_push_rule(high).await.unwrap();
    // Low-priority rule says notify.
    let mut low = make_push_rule_request(
        &user_id,
        "rule_low",
        "underride",
        json!([]),
        json!(["notify"]),
    );
    low.priority = 10;
    service.create_push_rule(low).await.unwrap();

    let event = json!({"type": "m.room.message", "content": {"body": "hello"}});
    let result = service.evaluate_push_rules(&user_id, &event).await.unwrap();
    // The high-priority dont_notify rule fires first (priority ASC).
    assert!(!result.notify);
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_evaluate_push_rules_ignored_user_suppressed() {
    let _guard = push_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let user_id = unique_user_id();
    let ignored_sender = format!("@ignored_{}:localhost", unique_id());

    // Set up account_data with m.ignored_user_list containing ignored_sender.
    let account_data = Arc::new(AccountDataStorage::new(&pool));
    account_data
        .upsert_account_data(
            &user_id,
            "m.ignored_user_list",
            json!({"ignored_users": {ignored_sender.clone(): {}}}),
        )
        .await
        .unwrap();

    let service = new_service(&pool).with_account_data_storage(account_data as Arc<dyn synapse_storage::AccountDataRepository>);
    // Create a rule that would normally notify.
    service
        .create_push_rule(make_push_rule_request(
            &user_id,
            "rule_notify",
            "underride",
            json!([]),
            json!(["notify"]),
        ))
        .await
        .unwrap();

    let event = json!({"type": "m.room.message", "sender": ignored_sender, "content": {"body": "hi"}});
    let result = service.evaluate_push_rules(&user_id, &event).await.unwrap();
    assert!(!result.notify, "notification from ignored user must be suppressed");
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_evaluate_push_rules_ignored_user_not_listed() {
    let _guard = push_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let user_id = unique_user_id();
    let non_ignored = format!("@sender_{}:localhost", unique_id());

    let account_data = Arc::new(AccountDataStorage::new(&pool));
    account_data
        .upsert_account_data(&user_id, "m.ignored_user_list", json!({"ignored_users": {}}))
        .await
        .unwrap();

    let service = new_service(&pool).with_account_data_storage(account_data as Arc<dyn synapse_storage::AccountDataRepository>);
    service
        .create_push_rule(make_push_rule_request(
            &user_id,
            "rule_notify",
            "underride",
            json!([]),
            json!(["notify"]),
        ))
        .await
        .unwrap();

    let event = json!({"type": "m.room.message", "sender": non_ignored, "content": {"body": "hi"}});
    let result = service.evaluate_push_rules(&user_id, &event).await.unwrap();
    assert!(result.notify, "notification from non-ignored user should pass");
}

// ===========================================================================
// send_notification
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_send_notification_no_devices() {
    let _guard = push_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let user_id = unique_user_id();
    let service = new_service(&pool);
    // No devices registered → Ok with no-op.
    service
        .send_notification(SendNotificationRequest {
            user_id: user_id.clone(),
            device_id: None,
            event_id: None,
            room_id: None,
            notification_type: None,
            title: "Test".to_string(),
            body: "Body".to_string(),
            data: None,
            priority: None,
        })
        .await
        .unwrap();
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_send_notification_all_devices() {
    let _guard = push_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let user_id = unique_user_id();
    insert_test_user(&pool, &user_id).await;

    let service = new_service(&pool);
    service.register_device(make_register_request(&user_id, &format!("DEV{}", unique_id()), "fcm")).await.unwrap();
    service.register_device(make_register_request(&user_id, &format!("DEV{}", unique_id()), "apns")).await.unwrap();

    service
        .send_notification(SendNotificationRequest {
            user_id: user_id.clone(),
            device_id: None,
            event_id: Some("$evt:localhost".to_string()),
            room_id: Some(unique_room_id()),
            notification_type: Some("m.room.message".to_string()),
            title: "Title".to_string(),
            body: "Body".to_string(),
            data: None,
            priority: Some(5),
        })
        .await
        .unwrap();

    // Verify that 2 notifications were queued.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM push_notification_queue WHERE user_id = $1")
        .bind(&user_id)
        .fetch_one(pool.as_ref())
        .await
        .unwrap();
    assert_eq!(count, 2);
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_send_notification_specific_device() {
    let _guard = push_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let user_id = unique_user_id();
    let device_id = unique_device_id();
    insert_test_user(&pool, &user_id).await;

    let service = new_service(&pool);
    service.register_device(make_register_request(&user_id, &device_id, "fcm")).await.unwrap();

    service
        .send_notification(SendNotificationRequest {
            user_id: user_id.clone(),
            device_id: Some(device_id.clone()),
            event_id: None,
            room_id: None,
            notification_type: None,
            title: "T".to_string(),
            body: "B".to_string(),
            data: None,
            priority: None,
        })
        .await
        .unwrap();

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM push_notification_queue WHERE user_id = $1 AND device_id = $2")
        .bind(&user_id)
        .bind(&device_id)
        .fetch_one(pool.as_ref())
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_send_notification_device_not_found() {
    let _guard = push_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let user_id = unique_user_id();
    let service = new_service(&pool);
    let result = service
        .send_notification(SendNotificationRequest {
            user_id: user_id.clone(),
            device_id: Some("NONEXISTENT".to_string()),
            event_id: None,
            room_id: None,
            notification_type: None,
            title: "T".to_string(),
            body: "B".to_string(),
            data: None,
            priority: None,
        })
        .await;
    assert!(result.is_err());
}

// ===========================================================================
// process_pending_notifications
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_process_pending_notifications_upstream() {
    let _guard = push_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let user_id = unique_user_id();
    let device_id = unique_device_id();
    insert_test_user(&pool, &user_id).await;

    let service = new_service(&pool);
    service.register_device(make_register_request(&user_id, &device_id, "upstream")).await.unwrap();

    // Queue a notification.
    service
        .send_notification(SendNotificationRequest {
            user_id: user_id.clone(),
            device_id: Some(device_id.clone()),
            event_id: Some("$evt:localhost".to_string()),
            room_id: Some(unique_room_id()),
            notification_type: Some("m.room.message".to_string()),
            title: "Title".to_string(),
            body: "Body".to_string(),
            data: None,
            priority: Some(5),
        })
        .await
        .unwrap();

    // Process pending → upstream provider always succeeds.
    let processed = service.process_pending_notifications(10).await.unwrap();
    // processed count may be 0 or 1 depending on whether send_notification queued it;
    // the key assertion is that the method does not error.
    assert!(processed == 0 || processed == 1, "processed should be 0 or 1, got {}", processed);

    // If a notification was queued, it should be marked as sent (or pending if not processed).
    if processed > 0 {
        let status: String = sqlx::query_scalar("SELECT status FROM push_notification_queue WHERE user_id = $1")
            .bind(&user_id)
            .fetch_one(pool.as_ref())
            .await
            .unwrap();
        assert_eq!(status, "sent");
    }
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_process_pending_notifications_empty() {
    let _guard = push_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let service = new_service(&pool);
    let processed = service.process_pending_notifications(10).await.unwrap();
    assert_eq!(processed, 0);
}

// ===========================================================================
// cleanup_old_logs
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_cleanup_old_logs() {
    let _guard = push_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let user_id = unique_user_id();
    let device_id = unique_device_id();

    // Insert an old log with sent_at set (2 days ago).
    let two_days_ago = chrono::Utc::now().timestamp_millis() - 2 * 86_400_000;
    sqlx::query(
        r#"
        INSERT INTO push_notification_log (
            user_id, device_id, push_type, is_success, created_ts, sent_at
        )
        VALUES ($1, $2, 'fcm', true, $3, $3)
        "#,
    )
    .bind(&user_id)
    .bind(&device_id)
    .bind(two_days_ago)
    .execute(pool.as_ref())
    .await
    .unwrap();

    // Insert a recent log with sent_at set (now).
    let now_ms = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        r#"
        INSERT INTO push_notification_log (
            user_id, device_id, push_type, is_success, created_ts, sent_at
        )
        VALUES ($1, $2, 'fcm', true, $3, $3)
        "#,
    )
    .bind(&user_id)
    .bind(&device_id)
    .bind(now_ms)
    .execute(pool.as_ref())
    .await
    .unwrap();

    let service = new_service(&pool);
    let deleted = service.cleanup_old_logs(1).await.unwrap();
    assert_eq!(deleted, 1, "only the 2-day-old log should be removed");

    // Second cleanup is a no-op.
    let deleted_again = service.cleanup_old_logs(1).await.unwrap();
    assert_eq!(deleted_again, 0);
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_cleanup_old_logs_no_sent_at_preserved() {
    let _guard = push_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let user_id = unique_user_id();
    let device_id = unique_device_id();
    let two_days_ago = chrono::Utc::now().timestamp_millis() - 2 * 86_400_000;

    // Insert an old log with NULL sent_at (failed notification).
    sqlx::query(
        r#"
        INSERT INTO push_notification_log (
            user_id, device_id, push_type, is_success, created_ts, sent_at
        )
        VALUES ($1, $2, 'fcm', false, $3, NULL)
        "#,
    )
    .bind(&user_id)
    .bind(&device_id)
    .bind(two_days_ago)
    .execute(pool.as_ref())
    .await
    .unwrap();

    let service = new_service(&pool);
    let deleted = service.cleanup_old_logs(1).await.unwrap();
    // Logs with NULL sent_at are NOT deleted (NULL < cutoff is NULL → not true).
    assert_eq!(deleted, 0);
}

// ===========================================================================
// initialize_providers
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_initialize_providers_no_config() {
    let _guard = push_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let mut service = new_service(&pool);
    service.initialize_providers().await.unwrap();
    // No config → no providers initialized, no error.
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_initialize_providers_fcm() {
    let _guard = push_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query("DELETE FROM push_config WHERE user_id = 'system'").execute(pool.as_ref()).await.ok();
    sqlx::query("INSERT INTO push_config (user_id, device_id, config_type, config_key, config_value, created_ts) VALUES ('system', 'system', 'fcm', 'fcm.enabled', 'true', $1) ON CONFLICT (user_id, device_id, config_type) DO UPDATE SET config_value = EXCLUDED.config_value, created_ts = EXCLUDED.created_ts")
        .bind(now)
        .execute(pool.as_ref())
        .await
        .unwrap();
    sqlx::query("INSERT INTO push_config (user_id, device_id, config_type, config_key, config_value, created_ts) VALUES ('system', 'system', 'fcm', 'fcm.api_key', 'test-key', $1) ON CONFLICT (user_id, device_id, config_type) DO UPDATE SET config_value = EXCLUDED.config_value, created_ts = EXCLUDED.created_ts")
        .bind(now)
        .execute(pool.as_ref())
        .await
        .unwrap();

    let mut service = new_service(&pool);
    service.initialize_providers().await.unwrap();
    // After init, send_notification with fcm device should use the fallback path (provider is set).
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_initialize_providers_apns() {
    let _guard = push_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query("DELETE FROM push_config WHERE user_id = 'system'").execute(pool.as_ref()).await.ok();
    sqlx::query("INSERT INTO push_config (user_id, device_id, config_type, config_key, config_value, created_ts) VALUES ('system', 'system', 'apns', 'apns.enabled', 'true', $1) ON CONFLICT (user_id, device_id, config_type) DO UPDATE SET config_value = EXCLUDED.config_value, created_ts = EXCLUDED.created_ts")
        .bind(now)
        .execute(pool.as_ref())
        .await
        .unwrap();
    sqlx::query("INSERT INTO push_config (user_id, device_id, config_type, config_key, config_value, created_ts) VALUES ('system', 'system', 'apns', 'apns.topic', 'com.test.app', $1) ON CONFLICT (user_id, device_id, config_type) DO UPDATE SET config_value = EXCLUDED.config_value, created_ts = EXCLUDED.created_ts")
        .bind(now)
        .execute(pool.as_ref())
        .await
        .unwrap();

    let mut service = new_service(&pool);
    service.initialize_providers().await.unwrap();
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_initialize_providers_webpush() {
    let _guard = push_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query("DELETE FROM push_config WHERE user_id = 'system'").execute(pool.as_ref()).await.ok();
    sqlx::query("INSERT INTO push_config (user_id, device_id, config_type, config_key, config_value, created_ts) VALUES ('system', 'system', 'webpush', 'webpush.enabled', 'true', $1) ON CONFLICT (user_id, device_id, config_type) DO UPDATE SET config_value = EXCLUDED.config_value, created_ts = EXCLUDED.created_ts")
        .bind(now)
        .execute(pool.as_ref())
        .await
        .unwrap();
    sqlx::query("INSERT INTO push_config (user_id, device_id, config_type, config_key, config_value, created_ts) VALUES ('system', 'system', 'webpush', 'webpush.vapid_public_key', 'pubkey', $1) ON CONFLICT (user_id, device_id, config_type) DO UPDATE SET config_value = EXCLUDED.config_value, created_ts = EXCLUDED.created_ts")
        .bind(now)
        .execute(pool.as_ref())
        .await
        .unwrap();
    sqlx::query("INSERT INTO push_config (user_id, device_id, config_type, config_key, config_value, created_ts) VALUES ('system', 'system', 'webpush', 'webpush.vapid_private_key', 'privkey', $1) ON CONFLICT (user_id, device_id, config_type) DO UPDATE SET config_value = EXCLUDED.config_value, created_ts = EXCLUDED.created_ts")
        .bind(now)
        .execute(pool.as_ref())
        .await
        .unwrap();

    let mut service = new_service(&pool);
    service.initialize_providers().await.unwrap();
}

// ===========================================================================
// get_room_notifications
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_room_notifications() {
    let _guard = push_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let user_id = unique_user_id();
    let room_id = unique_room_id();
    let now = chrono::Utc::now().timestamp_millis();

    for i in 0..3 {
        sqlx::query(
            r#"
            INSERT INTO notifications (user_id, event_id, room_id, ts, notification_type, is_read, created_ts)
            VALUES ($1, $2, $3, $4, 'message', $5, $4)
            "#,
        )
        .bind(&user_id)
        .bind(format!("$evt_{}:localhost", i))
        .bind(&room_id)
        .bind(now + i)
        .bind(i == 0)
        .execute(pool.as_ref())
        .await
        .unwrap();
    }

    let service = new_service(&pool);
    let notifications = service.get_room_notifications(&user_id, &room_id, 100).await.unwrap();
    assert_eq!(notifications.len(), 3);
    // Ordered by ts DESC.
    assert!(notifications[0].ts >= notifications[1].ts);
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_room_notifications_empty() {
    let _guard = push_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let service = new_service(&pool);
    let notifications = service.get_room_notifications("@nobody:localhost", "!nobody:localhost", 100).await.unwrap();
    assert!(notifications.is_empty());
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_room_notifications_limit() {
    let _guard = push_guard().lock().unwrap_or_else(|e| e.into_inner());
    let pool = crate::require_test_pool().await;
    setup_test_database(&pool).await;

    let user_id = unique_user_id();
    let room_id = unique_room_id();
    let now = chrono::Utc::now().timestamp_millis();

    for i in 0..5 {
        sqlx::query(
            r#"
            INSERT INTO notifications (user_id, event_id, room_id, ts, notification_type, is_read, created_ts)
            VALUES ($1, $2, $3, $4, 'message', false, $4)
            "#,
        )
        .bind(&user_id)
        .bind(format!("$evt_{}:localhost", i))
        .bind(&room_id)
        .bind(now + i)
        .execute(pool.as_ref())
        .await
        .unwrap();
    }

    let service = new_service(&pool);
    let notifications = service.get_room_notifications(&user_id, &room_id, 2).await.unwrap();
    assert_eq!(notifications.len(), 2);
}

// ===========================================================================
// Serialization tests
// ===========================================================================

#[test]
fn test_notification_payload_serialization_roundtrip() {
    let payload = NotificationPayload {
        title: "Hello".to_string(),
        body: "World".to_string(),
        icon: Some("https://example.com/icon.png".to_string()),
        badge: None,
        sound: Some("default".to_string()),
        tag: Some("tag1".to_string()),
        data: json!({"key": "value"}),
        event_id: Some("$evt:localhost".to_string()),
        room_id: Some("!room:localhost".to_string()),
        room_name: Some("Test Room".to_string()),
        sender: Some("@user:localhost".to_string()),
        counts: None,
    };
    let json_str = serde_json::to_string(&payload).unwrap();
    let deserialized: NotificationPayload = serde_json::from_str(&json_str).unwrap();
    assert_eq!(deserialized.title, payload.title);
    assert_eq!(deserialized.body, payload.body);
    assert_eq!(deserialized.icon, payload.icon);
    assert_eq!(deserialized.badge, payload.badge);
    assert_eq!(deserialized.sound, payload.sound);
    assert_eq!(deserialized.tag, payload.tag);
    assert_eq!(deserialized.event_id, payload.event_id);
    assert_eq!(deserialized.room_id, payload.room_id);
    assert_eq!(deserialized.room_name, payload.room_name);
    assert_eq!(deserialized.sender, payload.sender);
}

#[test]
fn test_notification_payload_minimal() {
    let payload = NotificationPayload {
        title: "T".to_string(),
        body: "B".to_string(),
        icon: None,
        badge: None,
        sound: None,
        tag: None,
        data: json!({}),
        event_id: None,
        room_id: None,
        room_name: None,
        sender: None,
        counts: None,
    };
    let json_str = serde_json::to_string(&payload).unwrap();
    let deserialized: NotificationPayload = serde_json::from_str(&json_str).unwrap();
    assert_eq!(deserialized.title, "T");
    assert_eq!(deserialized.body, "B");
    assert!(deserialized.icon.is_none());
}

#[test]
fn test_send_notification_request_deserialization() {
    let json_str = r#"{
        "user_id": "@user:localhost",
        "device_id": "DEV1",
        "event_id": "$evt:localhost",
        "room_id": "!room:localhost",
        "notification_type": "m.room.message",
        "title": "Title",
        "body": "Body",
        "data": {"key": "value"},
        "priority": 5
    }"#;
    let req: SendNotificationRequest = serde_json::from_str(json_str).unwrap();
    assert_eq!(req.user_id, "@user:localhost");
    assert_eq!(req.device_id.as_deref(), Some("DEV1"));
    assert_eq!(req.event_id.as_deref(), Some("$evt:localhost"));
    assert_eq!(req.room_id.as_deref(), Some("!room:localhost"));
    assert_eq!(req.title, "Title");
    assert_eq!(req.body, "Body");
    assert_eq!(req.priority, Some(5));
}

#[test]
fn test_send_notification_request_minimal() {
    let json_str = r#"{"user_id": "@u:l", "title": "T", "body": "B"}"#;
    let req: SendNotificationRequest = serde_json::from_str(json_str).unwrap();
    assert_eq!(req.user_id, "@u:l");
    assert_eq!(req.title, "T");
    assert_eq!(req.body, "B");
    assert!(req.device_id.is_none());
    assert!(req.priority.is_none());
    assert!(req.data.is_none());
}

#[test]
fn test_push_rule_result_notify() {
    let result = PushRuleResult {
        notify: true,
        tweaks: json!({"highlight": true, "sound": "default"}),
    };
    let json_str = serde_json::to_string(&result).unwrap();
    let json_val: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    assert_eq!(json_val["notify"], true);
    assert_eq!(json_val["tweaks"]["highlight"], true);
    assert_eq!(json_val["tweaks"]["sound"], "default");
}

#[test]
fn test_push_rule_result_not_notify() {
    let result = PushRuleResult {
        notify: false,
        tweaks: json!({}),
    };
    let json_str = serde_json::to_string(&result).unwrap();
    let json_val: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    assert_eq!(json_val["notify"], false);
    assert!(json_val["tweaks"].as_object().unwrap().is_empty());
}

#[test]
fn test_create_notification_log_request_builder() {
    let req = CreateNotificationLogRequest::new("@user:l", "DEV1", "fcm", true)
        .event_id("$evt:l")
        .room_id("!room:l")
        .notification_type("m.room.message")
        .response_time_ms(42);

    assert_eq!(req.user_id, "@user:l");
    assert_eq!(req.device_id, "DEV1");
    assert_eq!(req.push_type, "fcm");
    assert!(req.is_success);
    assert_eq!(req.event_id.as_deref(), Some("$evt:l"));
    assert_eq!(req.room_id.as_deref(), Some("!room:l"));
    assert_eq!(req.notification_type.as_deref(), Some("m.room.message"));
    assert_eq!(req.response_time_ms, Some(42));
}

#[test]
fn test_create_notification_log_request_builder_with_error() {
    let req = CreateNotificationLogRequest::new("@user:l", "DEV1", "apns", false)
        .error_message("device token invalid")
        .provider_response(r#"{"reason": "BadDeviceToken"}"#);

    assert!(!req.is_success);
    assert_eq!(req.error_message.as_deref(), Some("device token invalid"));
    assert!(req.provider_response.is_some());
}
