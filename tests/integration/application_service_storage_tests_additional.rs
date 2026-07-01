//! Additional integration tests for `ApplicationServiceStorage` covering all
//! public methods in `synapse-storage/src/application_service.rs` (which had 0%
//! dedicated coverage previously):
//!   - `register` / `upsert_registration` (with/without namespaces/protocols)
//!   - `get_by_id` / `get_by_token` / `get_by_hs_token` / `get_all_active`
//!   - `update` (url/description/is_rate_limited/protocols/is_enabled/api_key/config)
//!   - `update_timestamp` / `unregister`
//!   - `set_state` / `get_state` / `get_all_states` (insert + upsert)
//!   - `add_event` / `get_pending_events` / `count_pending_events` /
//!     `mark_event_processed` (insert + ON CONFLICT upsert)
//!   - `create_transaction` / `complete_transaction` / `fail_transaction` /
//!     `get_pending_transactions` / `count_pending_transactions`
//!   - `register_virtual_user` / `get_virtual_users` (insert + upsert)
//!   - Namespace matching: `has_exclusive_user_namespace_match`,
//!     `find_user_namespace_conflict` / `find_room_alias_namespace_conflict` /
//!     `find_room_namespace_conflict`, `is_user_in_namespace` /
//!     `is_room_alias_in_namespace` / `is_room_id_in_namespace`,
//!     `get_user_namespaces` / `get_room_alias_namespaces` / `get_room_namespaces`
//!   - `get_statistics` / `update_last_seen`
//!   - Empty-input edge cases (no events/transactions/users/states/namespaces)

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use synapse_storage::application_service::{
    ApplicationServiceStorage, RegisterApplicationServiceRequest, UpdateApplicationServiceRequest,
};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

fn appservice_test_guard() -> &'static Mutex<()> {
    static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    GUARD.get_or_init(|| Mutex::new(()))
}

/// Acquire the serialization guard, recovering from a poisoned mutex so a
/// single test panic does not cascade `PoisonError` failures into every
/// subsequent test (which would obscure the real failure).
fn lock_guard() -> std::sync::MutexGuard<'static, ()> {
    appservice_test_guard().lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Warm up the shared pool on the current tokio runtime.
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

async fn setup(pool: &Arc<sqlx::PgPool>) {
    warm_up_pool(pool).await;
    // Delete child tables first to respect FK constraints. Most namespace/state/
    // user/statistics tables have ON DELETE CASCADE from `application_services`,
    // but `application_service_events` and `application_service_transactions`
    // have NO foreign key, so they must be cleared explicitly.
    sqlx::query("DELETE FROM application_service_statistics").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM application_service_users").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM application_service_user_namespaces").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM application_service_room_alias_namespaces")
        .execute(pool.as_ref())
        .await
        .ok();
    sqlx::query("DELETE FROM application_service_room_namespaces").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM application_service_state").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM application_service_events").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM application_service_transactions").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM application_services").execute(pool.as_ref()).await.ok();
}

async fn teardown(pool: &sqlx::PgPool) {
    sqlx::query("DELETE FROM application_service_statistics").execute(pool).await.ok();
    sqlx::query("DELETE FROM application_service_users").execute(pool).await.ok();
    sqlx::query("DELETE FROM application_service_user_namespaces").execute(pool).await.ok();
    sqlx::query("DELETE FROM application_service_room_alias_namespaces").execute(pool).await.ok();
    sqlx::query("DELETE FROM application_service_room_namespaces").execute(pool).await.ok();
    sqlx::query("DELETE FROM application_service_state").execute(pool).await.ok();
    sqlx::query("DELETE FROM application_service_events").execute(pool).await.ok();
    sqlx::query("DELETE FROM application_service_transactions").execute(pool).await.ok();
    sqlx::query("DELETE FROM application_services").execute(pool).await.ok();
}

fn new_storage(pool: &Arc<sqlx::PgPool>) -> ApplicationServiceStorage {
    ApplicationServiceStorage::new(pool)
}

/// Build a `RegisterApplicationServiceRequest` with globally-unique as_id/tokens
/// so tests never collide on the shared `application_services` table.
fn make_request(as_id: &str) -> RegisterApplicationServiceRequest {
    let id = unique_id();
    RegisterApplicationServiceRequest {
        as_id: as_id.to_string(),
        url: format!("http://localhost:{}", 9000 + (id % 1000)),
        as_token: format!("ast_{}", id),
        hs_token: format!("hst_{}", id),
        sender: format!("@bot_{}:localhost", id),
        description: Some(format!("test appservice {}", id)),
        is_rate_limited: Some(false),
        protocols: Some(vec!["irc".to_string()]),
        namespaces: None,
        api_key: None,
        config: None,
    }
}

/// Build a request whose `namespaces` JSON uses the `exclusive`/`regex` shape
/// that `insert_namespaces` reads (note: the JSON key is `exclusive`, not
/// `is_exclusive`).
fn make_request_with_namespaces(as_id: &str) -> RegisterApplicationServiceRequest {
    let id = unique_id();
    RegisterApplicationServiceRequest {
        as_id: as_id.to_string(),
        url: format!("http://localhost:{}", 9000 + (id % 1000)),
        as_token: format!("ast_{}", id),
        hs_token: format!("hst_{}", id),
        sender: format!("@bot_{}:localhost", id),
        description: Some(format!("test appservice {}", id)),
        is_rate_limited: Some(false),
        protocols: Some(vec!["irc".to_string()]),
        namespaces: Some(serde_json::json!({
            "users": [{"exclusive": true, "regex": "@_irc_.*:localhost"}],
            "aliases": [{"exclusive": true, "regex": "#_irc_.*:localhost"}],
            "rooms": [{"exclusive": false, "regex": "!_irc_.*:localhost"}]
        })),
        api_key: None,
        config: None,
    }
}

// ---------------------------------------------------------------------------
// register / upsert_registration
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_register_basic() {
    let _guard = lock_guard();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let as_id = format!("as_{}", unique_id());
    let request = make_request(&as_id);
    let expected_url = request.url.clone();
    let expected_token = request.as_token.clone();
    let service = storage.register(request).await.unwrap();

    assert_eq!(service.as_id, as_id);
    assert!(service.is_enabled);
    assert!(!service.is_rate_limited);
    assert_eq!(service.protocols, vec!["irc".to_string()]);
    assert!(service.created_ts > 0);
    assert!(service.updated_ts.is_none());
    assert_eq!(service.url, expected_url);
    assert_eq!(service.as_token, expected_token);
    assert!(service.description.is_some());

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_register_empty_protocols_and_namespaces() {
    let _guard = lock_guard();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let as_id = format!("as_{}", unique_id());
    let request = RegisterApplicationServiceRequest {
        as_id: as_id.clone(),
        url: "http://localhost:9999".to_string(),
        as_token: format!("ast_{}", unique_id()),
        hs_token: format!("hst_{}", unique_id()),
        sender: "@bot:localhost".to_string(),
        description: None,
        is_rate_limited: None,
        protocols: None,
        namespaces: None,
        api_key: None,
        config: None,
    };
    let service = storage.register(request).await.unwrap();

    assert_eq!(service.as_id, as_id);
    assert!(service.protocols.is_empty());
    assert!(service.namespaces.get("users").is_some());
    assert!(service.description.is_none());
    // is_rate_limited defaults to false via unwrap_or(false)
    assert!(!service.is_rate_limited);

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_upsert_registration_new() {
    let _guard = lock_guard();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let as_id = format!("as_{}", unique_id());
    let request = make_request(&as_id);
    let service = storage.upsert_registration(request).await.unwrap();

    assert_eq!(service.as_id, as_id);
    assert!(service.is_enabled);

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_upsert_registration_existing_updates() {
    let _guard = lock_guard();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let as_id = format!("as_{}", unique_id());
    // First insert via register.
    let first = storage.register(make_request(&as_id)).await.unwrap();
    let original_url = first.url.clone();

    // Now upsert with a new URL/token.
    let mut updated_req = make_request(&as_id);
    updated_req.url = "http://localhost:7777".to_string();
    updated_req.as_token = format!("ast_new_{}", unique_id());
    let updated = storage.upsert_registration(updated_req).await.unwrap();

    assert_eq!(updated.as_id, as_id);
    assert_eq!(updated.url, "http://localhost:7777");
    assert_ne!(updated.url, original_url);
    assert!(updated.updated_ts.is_some());

    teardown(&pool).await;
}

// ---------------------------------------------------------------------------
// get_by_id / get_by_token / get_by_hs_token / get_all_active
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_by_id_found_and_not_found() {
    let _guard = lock_guard();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let as_id = format!("as_{}", unique_id());
    storage.register(make_request(&as_id)).await.unwrap();

    let found = storage.get_by_id(&as_id).await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().as_id, as_id);

    let missing = storage.get_by_id("does_not_exist").await.unwrap();
    assert!(missing.is_none());

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_by_token_found_and_not_found() {
    let _guard = lock_guard();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let as_id = format!("as_{}", unique_id());
    let request = make_request(&as_id);
    let as_token = request.as_token.clone();
    storage.register(request).await.unwrap();

    let found = storage.get_by_token(&as_token).await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().as_id, as_id);

    let missing = storage.get_by_token("no_such_token").await.unwrap();
    assert!(missing.is_none());

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_by_token_skips_disabled_service() {
    let _guard = lock_guard();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let as_id = format!("as_{}", unique_id());
    let request = make_request(&as_id);
    let as_token = request.as_token.clone();
    storage.register(request).await.unwrap();

    // Disable the service; get_by_token filters is_enabled = TRUE.
    storage
        .update(&as_id, &UpdateApplicationServiceRequest::new().is_enabled(false))
        .await
        .unwrap();

    let found = storage.get_by_token(&as_token).await.unwrap();
    assert!(found.is_none(), "disabled service should not be returned by token lookup");

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_by_hs_token_found_and_not_found() {
    let _guard = lock_guard();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let as_id = format!("as_{}", unique_id());
    let request = make_request(&as_id);
    let hs_token = request.hs_token.clone();
    storage.register(request).await.unwrap();

    let found = storage.get_by_hs_token(&hs_token).await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().as_id, as_id);

    let missing = storage.get_by_hs_token("no_such_hs_token").await.unwrap();
    assert!(missing.is_none());

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_all_active_filters_disabled() {
    let _guard = lock_guard();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let as_id_a = format!("as_{}", unique_id());
    let as_id_b = format!("as_{}", unique_id());
    storage.register(make_request(&as_id_a)).await.unwrap();
    storage.register(make_request(&as_id_b)).await.unwrap();

    // Both enabled by default.
    let active = storage.get_all_active().await.unwrap();
    assert_eq!(active.len(), 2);

    // Disable one.
    storage
        .update(&as_id_a, &UpdateApplicationServiceRequest::new().is_enabled(false))
        .await
        .unwrap();

    let active = storage.get_all_active().await.unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].as_id, as_id_b);

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_all_active_empty() {
    let _guard = lock_guard();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let active = storage.get_all_active().await.unwrap();
    assert!(active.is_empty());

    teardown(&pool).await;
}

// ---------------------------------------------------------------------------
// update / update_timestamp / unregister
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_update_all_fields() {
    let _guard = lock_guard();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let as_id = format!("as_{}", unique_id());
    storage.register(make_request(&as_id)).await.unwrap();

    let update_req = UpdateApplicationServiceRequest::new()
        .url("http://localhost:4242")
        .description("updated description")
        .is_rate_limited(true)
        .protocols(vec!["matrix".to_string(), "irc".to_string()])
        .is_enabled(false)
        .api_key("secret-key")
        .config(serde_json::json!({"key": "value"}));

    let updated = storage.update(&as_id, &update_req).await.unwrap();
    assert_eq!(updated.url, "http://localhost:4242");
    assert_eq!(updated.description.as_deref(), Some("updated description"));
    assert!(updated.is_rate_limited);
    assert_eq!(updated.protocols, vec!["matrix".to_string(), "irc".to_string()]);
    assert!(!updated.is_enabled);
    assert_eq!(updated.api_key.as_deref(), Some("secret-key"));
    assert_eq!(updated.config, serde_json::json!({"key": "value"}));
    assert!(updated.updated_ts.is_some());

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_update_partial_only_url() {
    let _guard = lock_guard();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let as_id = format!("as_{}", unique_id());
    let original = storage.register(make_request(&as_id)).await.unwrap();
    let original_description = original.description.clone();

    // Only update url; other fields must remain unchanged.
    let updated = storage
        .update(&as_id, &UpdateApplicationServiceRequest::new().url("http://localhost:5555"))
        .await
        .unwrap();

    assert_eq!(updated.url, "http://localhost:5555");
    assert_eq!(updated.description, original_description);
    assert!(updated.updated_ts.is_some());

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_update_timestamp() {
    let _guard = lock_guard();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let as_id = format!("as_{}", unique_id());
    let service = storage.register(make_request(&as_id)).await.unwrap();
    assert!(service.updated_ts.is_none());

    storage.update_timestamp(&as_id).await.unwrap();

    let fetched = storage.get_by_id(&as_id).await.unwrap().unwrap();
    assert!(fetched.updated_ts.is_some());

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_unregister_removes_service() {
    let _guard = lock_guard();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let as_id = format!("as_{}", unique_id());
    storage.register(make_request_with_namespaces(&as_id)).await.unwrap();

    // Service exists before unregister.
    assert!(storage.get_by_id(&as_id).await.unwrap().is_some());

    storage.unregister(&as_id).await.unwrap();

    // Service gone (the direct contract of `unregister`).
    let found = storage.get_by_id(&as_id).await.unwrap();
    assert!(found.is_none());

    // Re-unregistering a non-existent service is a no-op (no error).
    storage.unregister(&as_id).await.unwrap();

    teardown(&pool).await;
}

// ---------------------------------------------------------------------------
// set_state / get_state / get_all_states
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_set_and_get_state() {
    let _guard = lock_guard();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let as_id = format!("as_{}", unique_id());
    storage.register(make_request(&as_id)).await.unwrap();

    let state = storage.set_state(&as_id, "presence", "online").await.unwrap();
    assert_eq!(state.as_id, as_id);
    assert_eq!(state.state_key, "presence");
    assert_eq!(state.state_value, "online");
    assert!(state.updated_ts > 0);

    let fetched = storage.get_state(&as_id, "presence").await.unwrap();
    assert!(fetched.is_some());
    assert_eq!(fetched.unwrap().state_value, "online");

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_set_state_upserts_existing_key() {
    let _guard = lock_guard();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let as_id = format!("as_{}", unique_id());
    storage.register(make_request(&as_id)).await.unwrap();

    storage.set_state(&as_id, "status", "away").await.unwrap();
    storage.set_state(&as_id, "status", "online").await.unwrap();

    // Only one row for the (as_id, state_key) unique constraint.
    let all = storage.get_all_states(&as_id).await.unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].state_value, "online");

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_all_states_multiple_keys() {
    let _guard = lock_guard();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let as_id = format!("as_{}", unique_id());
    storage.register(make_request(&as_id)).await.unwrap();

    storage.set_state(&as_id, "key1", "v1").await.unwrap();
    storage.set_state(&as_id, "key2", "v2").await.unwrap();
    storage.set_state(&as_id, "key3", "v3").await.unwrap();

    let all = storage.get_all_states(&as_id).await.unwrap();
    assert_eq!(all.len(), 3);

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_state_missing_returns_none() {
    let _guard = lock_guard();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let as_id = format!("as_{}", unique_id());
    storage.register(make_request(&as_id)).await.unwrap();

    let missing = storage.get_state(&as_id, "no_such_key").await.unwrap();
    assert!(missing.is_none());

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_all_states_empty() {
    let _guard = lock_guard();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let as_id = format!("as_{}", unique_id());
    storage.register(make_request(&as_id)).await.unwrap();

    let all = storage.get_all_states(&as_id).await.unwrap();
    assert!(all.is_empty());

    teardown(&pool).await;
}

// ---------------------------------------------------------------------------
// add_event / get_pending_events / count_pending_events / mark_event_processed
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_add_event_and_get_pending() {
    let _guard = lock_guard();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let as_id = format!("as_{}", unique_id());
    storage.register(make_request(&as_id)).await.unwrap();

    let event_id = format!("$evt_{}:localhost", unique_id());
    let room_id = format!("!room_{}:localhost", unique_id());
    let event = storage
        .add_event(&event_id, &as_id, &room_id, "m.room.message", "@alice:localhost", serde_json::json!({}), None)
        .await
        .unwrap();

    assert_eq!(event.event_id, event_id);
    assert_eq!(event.as_id, as_id);
    assert_eq!(event.room_id, room_id);
    assert_eq!(event.event_type, "m.room.message");
    // add_event synthesizes sender/content/state_key as static literals.
    assert_eq!(event.sender, "");
    assert!(event.processed_ts.is_none());

    // Pending count is 1.
    assert_eq!(storage.count_pending_events(&as_id).await.unwrap(), 1);

    let pending = storage.get_pending_events(&as_id, 10).await.unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].event_id, event_id);

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_add_event_upsert_on_conflict() {
    let _guard = lock_guard();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let as_id = format!("as_{}", unique_id());
    storage.register(make_request(&as_id)).await.unwrap();

    let event_id = format!("$evt_{}:localhost", unique_id());
    let room_id = format!("!room_{}:localhost", unique_id());

    storage
        .add_event(&event_id, &as_id, &room_id, "m.room.message", "@alice:localhost", serde_json::json!({}), None)
        .await
        .unwrap();

    // Re-add with a different room/event_type: ON CONFLICT updates those fields.
    let updated = storage
        .add_event(&event_id, &as_id, &room_id, "m.room.name", "@bob:localhost", serde_json::json!({}), None)
        .await
        .unwrap();

    assert_eq!(updated.event_type, "m.room.name");
    // Still only one pending event (no duplicate).
    assert_eq!(storage.count_pending_events(&as_id).await.unwrap(), 1);

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_mark_event_processed_removes_from_pending() {
    let _guard = lock_guard();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let as_id = format!("as_{}", unique_id());
    storage.register(make_request(&as_id)).await.unwrap();

    let event_id = format!("$evt_{}:localhost", unique_id());
    let room_id = format!("!room_{}:localhost", unique_id());
    storage
        .add_event(&event_id, &as_id, &room_id, "m.room.message", "@alice:localhost", serde_json::json!({}), None)
        .await
        .unwrap();

    assert_eq!(storage.count_pending_events(&as_id).await.unwrap(), 1);

    storage.mark_event_processed(&event_id).await.unwrap();

    assert_eq!(storage.count_pending_events(&as_id).await.unwrap(), 0);
    let pending = storage.get_pending_events(&as_id, 10).await.unwrap();
    assert!(pending.is_empty());

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_pending_events_respects_limit() {
    let _guard = lock_guard();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let as_id = format!("as_{}", unique_id());
    storage.register(make_request(&as_id)).await.unwrap();

    let room_id = format!("!room_{}:localhost", unique_id());
    for i in 0..5 {
        let event_id = format!("$evt_{}_{}:localhost", unique_id(), i);
        storage
            .add_event(&event_id, &as_id, &room_id, "m.room.message", "@alice:localhost", serde_json::json!({}), None)
            .await
            .unwrap();
    }

    assert_eq!(storage.count_pending_events(&as_id).await.unwrap(), 5);
    let limited = storage.get_pending_events(&as_id, 2).await.unwrap();
    assert_eq!(limited.len(), 2);

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_count_pending_events_zero_when_none() {
    let _guard = lock_guard();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let as_id = format!("as_{}", unique_id());
    storage.register(make_request(&as_id)).await.unwrap();

    assert_eq!(storage.count_pending_events(&as_id).await.unwrap(), 0);
    let pending = storage.get_pending_events(&as_id, 10).await.unwrap();
    assert!(pending.is_empty());

    teardown(&pool).await;
}

// ---------------------------------------------------------------------------
// create_transaction / complete_transaction / fail_transaction
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_create_transaction_and_complete() {
    let _guard = lock_guard();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let as_id = format!("as_{}", unique_id());
    storage.register(make_request(&as_id)).await.unwrap();

    let txn_id = format!("txn_{}", unique_id());
    let events = vec![serde_json::json!({"type": "m.room.message", "content": {"body": "hi"}})];
    let txn = storage.create_transaction(&as_id, &txn_id, &events).await.unwrap();

    assert_eq!(txn.as_id, as_id);
    assert_eq!(txn.txn_id, txn_id);
    assert_eq!(txn.transaction_id.as_deref(), Some(txn_id.as_str()));
    assert!(txn.completed_ts.is_none());
    assert_eq!(txn.retry_count, 0);

    // Pending transaction present.
    assert_eq!(storage.count_pending_transactions(&as_id).await.unwrap(), 1);
    let pending = storage.get_pending_transactions(&as_id).await.unwrap();
    assert_eq!(pending.len(), 1);

    storage.complete_transaction(&as_id, &txn_id).await.unwrap();

    // No longer pending.
    assert_eq!(storage.count_pending_transactions(&as_id).await.unwrap(), 0);
    let pending = storage.get_pending_transactions(&as_id).await.unwrap();
    assert!(pending.is_empty());

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_fail_transaction_increments_retry() {
    let _guard = lock_guard();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let as_id = format!("as_{}", unique_id());
    storage.register(make_request(&as_id)).await.unwrap();

    let txn_id = format!("txn_{}", unique_id());
    storage
        .create_transaction(&as_id, &txn_id, &[serde_json::json!({"type": "m.room.message"})])
        .await
        .unwrap();

    let failed = storage.fail_transaction(&as_id, &txn_id, "connection refused").await.unwrap();
    assert_eq!(failed.retry_count, 1);
    assert_eq!(failed.last_error.as_deref(), Some("connection refused"));
    // Still pending (fail_transaction does not set completed_ts).
    assert!(failed.completed_ts.is_none());
    assert_eq!(storage.count_pending_transactions(&as_id).await.unwrap(), 1);

    let failed2 = storage.fail_transaction(&as_id, &txn_id, "timeout").await.unwrap();
    assert_eq!(failed2.retry_count, 2);
    assert_eq!(failed2.last_error.as_deref(), Some("timeout"));

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_count_pending_transactions_zero_when_none() {
    let _guard = lock_guard();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let as_id = format!("as_{}", unique_id());
    storage.register(make_request(&as_id)).await.unwrap();

    assert_eq!(storage.count_pending_transactions(&as_id).await.unwrap(), 0);
    let pending = storage.get_pending_transactions(&as_id).await.unwrap();
    assert!(pending.is_empty());

    teardown(&pool).await;
}

// ---------------------------------------------------------------------------
// register_virtual_user / get_virtual_users
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_register_virtual_user_and_get() {
    let _guard = lock_guard();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let as_id = format!("as_{}", unique_id());
    storage.register(make_request(&as_id)).await.unwrap();

    let user_id = format!("@vu_{}:localhost", unique_id());
    let user = storage.register_virtual_user(&as_id, &user_id, Some("Virtual User"), None).await.unwrap();

    assert_eq!(user.as_id, as_id);
    assert_eq!(user.user_id, user_id);
    assert_eq!(user.displayname.as_deref(), Some("Virtual User"));
    assert!(user.avatar_url.is_none());

    let users = storage.get_virtual_users(&as_id).await.unwrap();
    assert_eq!(users.len(), 1);
    assert_eq!(users[0].user_id, user_id);

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_register_virtual_user_upserts() {
    let _guard = lock_guard();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let as_id = format!("as_{}", unique_id());
    storage.register(make_request(&as_id)).await.unwrap();

    let user_id = format!("@vu_{}:localhost", unique_id());
    storage.register_virtual_user(&as_id, &user_id, Some("Name1"), None).await.unwrap();
    // Upsert: same (as_id, user_id), new displayname + avatar.
    storage
        .register_virtual_user(&as_id, &user_id, Some("Name2"), Some("mxc://avatar"))
        .await
        .unwrap();

    let users = storage.get_virtual_users(&as_id).await.unwrap();
    assert_eq!(users.len(), 1, "upsert should not duplicate the user");
    assert_eq!(users[0].displayname.as_deref(), Some("Name2"));
    assert_eq!(users[0].avatar_url.as_deref(), Some("mxc://avatar"));

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_virtual_users_empty() {
    let _guard = lock_guard();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let as_id = format!("as_{}", unique_id());
    storage.register(make_request(&as_id)).await.unwrap();

    let users = storage.get_virtual_users(&as_id).await.unwrap();
    assert!(users.is_empty());

    teardown(&pool).await;
}

// ---------------------------------------------------------------------------
// Namespace matching
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_has_exclusive_user_namespace_match() {
    let _guard = lock_guard();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let as_id = format!("as_{}", unique_id());
    storage.register(make_request_with_namespaces(&as_id)).await.unwrap();

    // The registered user namespace regex is "@_irc_.*:localhost".
    let matched = storage.has_exclusive_user_namespace_match(&as_id, "@_irc_bot:localhost").await.unwrap();
    assert!(matched, "user_id matching the exclusive regex should match");

    let not_matched = storage.has_exclusive_user_namespace_match(&as_id, "@plain:localhost").await.unwrap();
    assert!(!not_matched, "non-matching user_id should not match");

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_find_user_namespace_conflict_detects_other_owner() {
    let _guard = lock_guard();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let as_id_a = format!("as_{}", unique_id());
    let mut req_a = make_request(&as_id_a);
    req_a.namespaces = Some(serde_json::json!({
        "users": [{"exclusive": true, "regex": "@_conflict_.*:localhost"}],
        "aliases": [],
        "rooms": []
    }));
    storage.register(req_a).await.unwrap();

    let as_id_b = format!("as_{}", unique_id());
    storage.register(make_request(&as_id_b)).await.unwrap();

    // Looking from B's perspective: A owns the exclusive namespace → conflict.
    let conflict = storage.find_user_namespace_conflict(&as_id_b, "@_conflict_.*:localhost").await.unwrap();
    assert_eq!(conflict.as_deref(), Some(as_id_a.as_str()));

    // Looking from A's perspective: A itself owns it → no conflict.
    let no_conflict = storage.find_user_namespace_conflict(&as_id_a, "@_conflict_.*:localhost").await.unwrap();
    assert!(no_conflict.is_none());

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_find_room_alias_namespace_conflict_detects_other_owner() {
    let _guard = lock_guard();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let as_id_a = format!("as_{}", unique_id());
    let mut req_a = make_request(&as_id_a);
    req_a.namespaces = Some(serde_json::json!({
        "users": [],
        "aliases": [{"exclusive": true, "regex": "#_alias_conflict_.*:localhost"}],
        "rooms": []
    }));
    storage.register(req_a).await.unwrap();

    let as_id_b = format!("as_{}", unique_id());
    storage.register(make_request(&as_id_b)).await.unwrap();

    let conflict = storage
        .find_room_alias_namespace_conflict(&as_id_b, "#_alias_conflict_.*:localhost")
        .await
        .unwrap();
    assert_eq!(conflict.as_deref(), Some(as_id_a.as_str()));

    let no_conflict = storage
        .find_room_alias_namespace_conflict(&as_id_a, "#_alias_conflict_.*:localhost")
        .await
        .unwrap();
    assert!(no_conflict.is_none());

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_find_room_namespace_conflict_detects_other_owner() {
    let _guard = lock_guard();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let as_id_a = format!("as_{}", unique_id());
    let mut req_a = make_request(&as_id_a);
    req_a.namespaces = Some(serde_json::json!({
        "users": [],
        "aliases": [],
        "rooms": [{"exclusive": true, "regex": "!_room_conflict_.*:localhost"}]
    }));
    storage.register(req_a).await.unwrap();

    let as_id_b = format!("as_{}", unique_id());
    storage.register(make_request(&as_id_b)).await.unwrap();

    let conflict = storage.find_room_namespace_conflict(&as_id_b, "!_room_conflict_.*:localhost").await.unwrap();
    assert_eq!(conflict.as_deref(), Some(as_id_a.as_str()));

    let no_conflict = storage.find_room_namespace_conflict(&as_id_a, "!_room_conflict_.*:localhost").await.unwrap();
    assert!(no_conflict.is_none());

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_is_user_in_namespace() {
    let _guard = lock_guard();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let as_id = format!("as_{}", unique_id());
    storage.register(make_request_with_namespaces(&as_id)).await.unwrap();

    let owner = storage.is_user_in_namespace("@_irc_bot:localhost").await.unwrap();
    assert_eq!(owner.as_deref(), Some(as_id.as_str()));

    let none = storage.is_user_in_namespace("@nomatch:localhost").await.unwrap();
    assert!(none.is_none());

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_is_room_alias_in_namespace() {
    let _guard = lock_guard();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let as_id = format!("as_{}", unique_id());
    storage.register(make_request_with_namespaces(&as_id)).await.unwrap();

    let owner = storage.is_room_alias_in_namespace("#_irc_room:localhost").await.unwrap();
    assert_eq!(owner.as_deref(), Some(as_id.as_str()));

    let none = storage.is_room_alias_in_namespace("#nomatch:localhost").await.unwrap();
    assert!(none.is_none());

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_is_room_id_in_namespace() {
    let _guard = lock_guard();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let as_id = format!("as_{}", unique_id());
    storage.register(make_request_with_namespaces(&as_id)).await.unwrap();

    let owner = storage.is_room_id_in_namespace("!_irc_room:localhost").await.unwrap();
    assert_eq!(owner.as_deref(), Some(as_id.as_str()));

    let none = storage.is_room_id_in_namespace("!nomatch:localhost").await.unwrap();
    assert!(none.is_none());

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_user_room_alias_room_namespaces() {
    let _guard = lock_guard();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let as_id = format!("as_{}", unique_id());
    storage.register(make_request_with_namespaces(&as_id)).await.unwrap();

    let user_ns = storage.get_user_namespaces(&as_id).await.unwrap();
    assert_eq!(user_ns.len(), 1);
    assert!(user_ns[0].is_exclusive);
    assert_eq!(user_ns[0].regex, "@_irc_.*:localhost");
    assert_eq!(user_ns[0].namespace_pattern, "@_irc_.*:localhost");

    let alias_ns = storage.get_room_alias_namespaces(&as_id).await.unwrap();
    assert_eq!(alias_ns.len(), 1);
    assert!(alias_ns[0].is_exclusive);
    assert_eq!(alias_ns[0].regex, "#_irc_.*:localhost");

    let room_ns = storage.get_room_namespaces(&as_id).await.unwrap();
    assert_eq!(room_ns.len(), 1);
    assert!(!room_ns[0].is_exclusive);
    assert_eq!(room_ns[0].regex, "!_irc_.*:localhost");

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_namespaces_empty_when_none_registered() {
    let _guard = lock_guard();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let as_id = format!("as_{}", unique_id());
    storage.register(make_request(&as_id)).await.unwrap();

    assert!(storage.get_user_namespaces(&as_id).await.unwrap().is_empty());
    assert!(storage.get_room_alias_namespaces(&as_id).await.unwrap().is_empty());
    assert!(storage.get_room_namespaces(&as_id).await.unwrap().is_empty());

    teardown(&pool).await;
}

// ---------------------------------------------------------------------------
// get_statistics / update_last_seen
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_update_last_seen_creates_statistics_row() {
    let _guard = lock_guard();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let as_id = format!("as_{}", unique_id());
    storage.register(make_request(&as_id)).await.unwrap();

    // Before update_last_seen, statistics has last_seen_ts = null.
    let stats_before = storage.get_statistics().await.unwrap();
    assert_eq!(stats_before.len(), 1);
    assert!(stats_before[0]["last_seen_ts"].is_null());

    storage.update_last_seen(&as_id).await.unwrap();

    let stats_after = storage.get_statistics().await.unwrap();
    assert_eq!(stats_after.len(), 1);
    assert!(stats_after[0]["last_seen_ts"].is_i64());
    assert!(stats_after[0]["last_seen_ts"].as_i64().unwrap() > 0);

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_statistics_counts() {
    let _guard = lock_guard();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let as_id = format!("as_{}", unique_id());
    storage.register(make_request(&as_id)).await.unwrap();

    // Add a virtual user.
    let user_id = format!("@vu_{}:localhost", unique_id());
    storage.register_virtual_user(&as_id, &user_id, Some("Vu"), None).await.unwrap();

    // Add a pending event.
    let event_id = format!("$evt_{}:localhost", unique_id());
    let room_id = format!("!room_{}:localhost", unique_id());
    storage
        .add_event(&event_id, &as_id, &room_id, "m.room.message", "@alice:localhost", serde_json::json!({}), None)
        .await
        .unwrap();

    // Add a pending transaction.
    let txn_id = format!("txn_{}", unique_id());
    storage
        .create_transaction(&as_id, &txn_id, &[serde_json::json!({"type": "m.room.message"})])
        .await
        .unwrap();

    let stats = storage.get_statistics().await.unwrap();
    assert_eq!(stats.len(), 1);
    let row = &stats[0];
    assert_eq!(row["as_id"].as_str(), Some(as_id.as_str()));
    assert_eq!(row["virtual_user_count"].as_i64(), Some(1));
    assert_eq!(row["pending_event_count"].as_i64(), Some(1));
    assert_eq!(row["pending_transaction_count"].as_i64(), Some(1));
    assert_eq!(row["is_enabled"].as_bool(), Some(true));

    teardown(&pool).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_statistics_empty_when_no_services() {
    let _guard = lock_guard();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let stats = storage.get_statistics().await.unwrap();
    assert!(stats.is_empty());

    teardown(&pool).await;
}
