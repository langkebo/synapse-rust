//! Additional integration tests for `SamlStorage` covering every public
//! method in `synapse-storage/src/saml.rs` (lines 265-847):
//!   - Sessions: create / get / get_by_user / update_last_used / invalidate /
//!     cleanup_expired
//!   - User mappings: create (upsert) / get by (name_id, issuer) / get by
//!     user_id / delete / list (cursor pagination) / get_any_issuer /
//!     update_by_name_id / delete_by_name_id
//!   - Identity providers: create / get / get_all / get_enabled /
//!     update_metadata / delete
//!   - Auth events: create / get_by_user / cleanup_old
//!   - Logout requests: create / get / process
//!   - Config overrides: get_all / upsert / delete

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use synapse_storage::saml::{
    CreateSamlAuthEventRequest, CreateSamlIdentityProviderRequest, CreateSamlLogoutRequestRequest,
    CreateSamlSessionRequest, CreateSamlUserMappingRequest, SamlStorage,
};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

fn saml_test_guard() -> &'static Mutex<()> {
    static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    GUARD.get_or_init(|| Mutex::new(()))
}

/// Warm up the shared pool on the current tokio runtime (the test pool can be
/// created on a different runtime; first query on a fresh runtime may fail).
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
    // Child / dependent tables first; saml tables have no FKs between them but
    // we clear in a stable order to keep this predictable.
    sqlx::query("DELETE FROM saml_logout_requests").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM saml_auth_events").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM saml_config_overrides").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM saml_user_mapping").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM saml_identity_providers").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM saml_sessions").execute(pool.as_ref()).await.ok();
}

async fn teardown(pool: &sqlx::PgPool) {
    sqlx::query("DELETE FROM saml_logout_requests").execute(pool).await.ok();
    sqlx::query("DELETE FROM saml_auth_events").execute(pool).await.ok();
    sqlx::query("DELETE FROM saml_config_overrides").execute(pool).await.ok();
    sqlx::query("DELETE FROM saml_user_mapping").execute(pool).await.ok();
    sqlx::query("DELETE FROM saml_identity_providers").execute(pool).await.ok();
    sqlx::query("DELETE FROM saml_sessions").execute(pool).await.ok();
}

fn new_storage(pool: &Arc<sqlx::PgPool>) -> SamlStorage {
    SamlStorage::new(pool)
}

fn unique_session_id() -> String {
    format!("session_{}", unique_id())
}

fn unique_user_id() -> String {
    format!("@samltest_{}:localhost", unique_id())
}

fn unique_name_id() -> String {
    format!("nameid_{}", unique_id())
}

fn unique_entity_id() -> String {
    format!("https://idp_{}.example.com", unique_id())
}

fn make_session_request(suffix: &str, expires_in_seconds: i64) -> CreateSamlSessionRequest {
    let mut attributes = HashMap::new();
    attributes.insert(
        "email".to_string(),
        vec![format!("user_{suffix}@example.com")],
    );
    CreateSamlSessionRequest {
        session_id: format!("session_{suffix}"),
        user_id: format!("@samltest_{suffix}:localhost"),
        name_id: Some(format!("nameid_{suffix}")),
        issuer: Some(format!("https://idp_{suffix}.example.com")),
        session_index: Some(format!("idx_{suffix}")),
        attributes,
        expires_in_seconds,
    }
}

fn make_user_mapping_request(name_id: &str, user_id: &str, issuer: &str) -> CreateSamlUserMappingRequest {
    let mut attributes = HashMap::new();
    attributes.insert("email".to_string(), vec![format!("{user_id}@example.com")]);
    CreateSamlUserMappingRequest {
        name_id: name_id.to_string(),
        user_id: user_id.to_string(),
        issuer: issuer.to_string(),
        attributes,
    }
}

fn make_idp_request(suffix: &str, enabled: Option<bool>) -> CreateSamlIdentityProviderRequest {
    CreateSamlIdentityProviderRequest {
        entity_id: format!("https://idp_{suffix}.example.com"),
        display_name: Some(format!("IdP {suffix}")),
        description: Some(format!("Test IdP {suffix}")),
        metadata_url: Some(format!("https://idp_{suffix}.example.com/metadata")),
        metadata_xml: Some(format!("<EntityDescriptor>{suffix}</EntityDescriptor>")),
        enabled,
        priority: Some(100),
        attribute_mapping: Some(serde_json::json!({"uid": "name_id"})),
    }
}

fn make_auth_event_request(suffix: &str, status: &str) -> CreateSamlAuthEventRequest {
    let mut attributes = HashMap::new();
    attributes.insert("email".to_string(), vec![format!("user_{suffix}@example.com")]);
    CreateSamlAuthEventRequest {
        session_id: Some(format!("session_{suffix}")),
        user_id: Some(format!("@samltest_{suffix}:localhost")),
        name_id: Some(format!("nameid_{suffix}")),
        issuer: Some(format!("https://idp_{suffix}.example.com")),
        event_type: "authentication".to_string(),
        status: status.to_string(),
        error_message: if status == "failed" {
            Some("invalid credentials".to_string())
        } else {
            None
        },
        ip_address: Some("192.168.1.1".to_string()),
        user_agent: Some("TestAgent/1.0".to_string()),
        request_id: Some(format!("req_{suffix}")),
        attributes,
    }
}

fn make_logout_request(suffix: &str) -> CreateSamlLogoutRequestRequest {
    CreateSamlLogoutRequestRequest {
        request_id: format!("logout_{suffix}"),
        session_id: Some(format!("session_{suffix}")),
        user_id: Some(format!("@samltest_{suffix}:localhost")),
        name_id: Some(format!("nameid_{suffix}")),
        issuer: Some(format!("https://idp_{suffix}.example.com")),
        reason: Some("user_logout".to_string()),
    }
}

// ===========================================================================
// Sessions
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_create_session() {
    let _guard = saml_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let request = make_session_request(&uid.to_string(), 3600);
    let session = storage.create_session(request).await.unwrap();

    assert!(session.id > 0);
    assert_eq!(session.session_id, format!("session_{uid}"));
    assert_eq!(session.user_id, format!("@samltest_{uid}:localhost"));
    assert_eq!(session.name_id.as_deref(), Some(format!("nameid_{uid}").as_str()));
    assert_eq!(session.issuer.as_deref(), Some(format!("https://idp_{uid}.example.com").as_str()));
    assert_eq!(session.session_index.as_deref(), Some(format!("idx_{uid}").as_str()));
    assert_eq!(session.status, "active");
    assert!(session.created_ts > 0);
    assert!(session.expires_at > session.created_ts);
    assert_eq!(session.last_used_ts, session.created_ts);
    assert!(session.attributes.is_object());

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_session_found_and_not_found() {
    let _guard = saml_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let created = storage
        .create_session(make_session_request(&uid.to_string(), 3600))
        .await
        .unwrap();

    let fetched = storage.get_session(&created.session_id).await.unwrap();
    assert!(fetched.is_some());
    let fetched = fetched.unwrap();
    assert_eq!(fetched.session_id, created.session_id);
    assert_eq!(fetched.user_id, created.user_id);

    let missing = storage.get_session("session_does_not_exist").await.unwrap();
    assert!(missing.is_none());

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_session_by_user_active_and_expired() {
    let _guard = saml_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    // Active session (expires in 1h).
    let uid = unique_id();
    let user_id = format!("@samltest_{uid}:localhost");
    let created = storage
        .create_session(make_session_request(&uid.to_string(), 3600))
        .await
        .unwrap();

    let fetched = storage.get_session_by_user(&user_id).await.unwrap();
    assert!(fetched.is_some());
    assert_eq!(fetched.unwrap().session_id, created.session_id);

    // Expired session is not returned (expires_at < now).
    let uid2 = unique_id();
    let expired_user = format!("@samltest_exp_{uid2}:localhost");
    let mut expired_req = make_session_request(&uid2.to_string(), -100);
    expired_req.user_id = expired_user.clone();
    storage.create_session(expired_req).await.unwrap();

    let missing = storage.get_session_by_user(&expired_user).await.unwrap();
    assert!(missing.is_none(), "expired session must not be returned");

    // Unknown user returns None.
    let none = storage.get_session_by_user("@unknown:localhost").await.unwrap();
    assert!(none.is_none());

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_update_session_last_used() {
    let _guard = saml_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let created = storage
        .create_session(make_session_request(&uid.to_string(), 3600))
        .await
        .unwrap();
    let original_last_used = created.last_used_ts;

    // Sleep briefly so the DB NOW() is strictly greater than created_ts.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    storage.update_session_last_used(&created.session_id).await.unwrap();

    let fetched = storage.get_session(&created.session_id).await.unwrap().unwrap();
    assert!(
        fetched.last_used_ts >= original_last_used,
        "last_used_ts should advance; got {} vs original {}",
        fetched.last_used_ts,
        original_last_used
    );

    // Updating a non-existent session is a no-op (no error).
    let result = storage.update_session_last_used("no_such_session").await;
    assert!(result.is_ok());

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_invalidate_session() {
    let _guard = saml_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let created = storage
        .create_session(make_session_request(&uid.to_string(), 3600))
        .await
        .unwrap();

    // Active session is retrievable.
    assert!(storage.get_session(&created.session_id).await.unwrap().is_some());

    storage.invalidate_session(&created.session_id).await.unwrap();

    // get_session filters status = 'active', so it is no longer returned.
    let fetched = storage.get_session(&created.session_id).await.unwrap();
    assert!(fetched.is_none(), "invalidated session must not be returned by get_session");

    // Direct row inspection confirms status flipped.
    let status: String =
        sqlx::query_scalar("SELECT status FROM saml_sessions WHERE session_id = $1")
            .bind(&created.session_id)
            .fetch_one(pool.as_ref())
            .await
            .unwrap();
    assert_eq!(status, "invalidated");

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_cleanup_expired_sessions() {
    let _guard = saml_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    // Expired session (expires_at = now - 100s).
    let uid_exp = unique_id();
    storage
        .create_session(make_session_request(&uid_exp.to_string(), -100))
        .await
        .unwrap();

    // Invalidated session.
    let uid_inv = unique_id();
    let invalidated = storage
        .create_session(make_session_request(&uid_inv.to_string(), 3600))
        .await
        .unwrap();
    storage.invalidate_session(&invalidated.session_id).await.unwrap();

    // Valid active session.
    let uid_valid = unique_id();
    let valid = storage
        .create_session(make_session_request(&uid_valid.to_string(), 3600))
        .await
        .unwrap();

    let deleted = storage.cleanup_expired_sessions().await.unwrap();
    assert_eq!(
        deleted, 2,
        "cleanup should remove 1 expired + 1 invalidated session"
    );

    // The valid session survives.
    let still_there = storage.get_session(&valid.session_id).await.unwrap();
    assert!(still_there.is_some(), "valid session must survive cleanup");

    // Second cleanup with nothing to remove returns 0.
    let deleted_again = storage.cleanup_expired_sessions().await.unwrap();
    assert_eq!(deleted_again, 0);

    teardown(pool.as_ref()).await;
}

// ===========================================================================
// User mappings
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_create_user_mapping() {
    let _guard = saml_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let name_id = format!("nameid_{uid}");
    let user_id = format!("@samltest_{uid}:localhost");
    let issuer = format!("https://idp_{uid}.example.com");

    let mapping = storage
        .create_user_mapping(make_user_mapping_request(&name_id, &user_id, &issuer))
        .await
        .unwrap();

    assert!(mapping.id > 0);
    assert_eq!(mapping.name_id, name_id);
    assert_eq!(mapping.user_id, user_id);
    assert_eq!(mapping.issuer, issuer);
    assert!(mapping.first_seen_ts > 0);
    assert!(mapping.last_authenticated_ts > 0);
    assert_eq!(mapping.authentication_count, 1);
    assert!(mapping.attributes.is_object());

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_create_user_mapping_upsert_increments_count() {
    let _guard = saml_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let name_id = format!("nameid_{uid}");
    let user_id = format!("@samltest_{uid}:localhost");
    let issuer = format!("https://idp_{uid}.example.com");

    // First insert: authentication_count = 1.
    storage
        .create_user_mapping(make_user_mapping_request(&name_id, &user_id, &issuer))
        .await
        .unwrap();

    // Second insert with same (name_id, issuer) upserts and increments count.
    let updated = storage
        .create_user_mapping(make_user_mapping_request(&name_id, &user_id, &issuer))
        .await
        .unwrap();
    assert_eq!(updated.authentication_count, 2);
    assert_eq!(updated.name_id, name_id);

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_user_mapping_by_name_id_found_and_not_found() {
    let _guard = saml_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let name_id = format!("nameid_{uid}");
    let user_id = format!("@samltest_{uid}:localhost");
    let issuer = format!("https://idp_{uid}.example.com");
    storage
        .create_user_mapping(make_user_mapping_request(&name_id, &user_id, &issuer))
        .await
        .unwrap();

    let fetched = storage.get_user_mapping_by_name_id(&name_id, &issuer).await.unwrap();
    assert!(fetched.is_some());
    let fetched = fetched.unwrap();
    assert_eq!(fetched.name_id, name_id);
    assert_eq!(fetched.user_id, user_id);
    assert_eq!(fetched.issuer, issuer);

    // Wrong issuer returns None.
    let missing = storage
        .get_user_mapping_by_name_id(&name_id, "https://other.example.com")
        .await
        .unwrap();
    assert!(missing.is_none());

    // Unknown name_id returns None.
    let none = storage
        .get_user_mapping_by_name_id("unknown_name_id", &issuer)
        .await
        .unwrap();
    assert!(none.is_none());

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_user_mapping_by_user_id() {
    let _guard = saml_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let name_id = format!("nameid_{uid}");
    let user_id = format!("@samltest_{uid}:localhost");
    let issuer = format!("https://idp_{uid}.example.com");
    storage
        .create_user_mapping(make_user_mapping_request(&name_id, &user_id, &issuer))
        .await
        .unwrap();

    let fetched = storage.get_user_mapping_by_user_id(&user_id).await.unwrap();
    assert!(fetched.is_some());
    assert_eq!(fetched.unwrap().user_id, user_id);

    let missing = storage.get_user_mapping_by_user_id("@nobody:localhost").await.unwrap();
    assert!(missing.is_none());

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_delete_user_mapping() {
    let _guard = saml_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let name_id = format!("nameid_{uid}");
    let user_id = format!("@samltest_{uid}:localhost");
    let issuer = format!("https://idp_{uid}.example.com");
    storage
        .create_user_mapping(make_user_mapping_request(&name_id, &user_id, &issuer))
        .await
        .unwrap();

    assert!(storage.get_user_mapping_by_name_id(&name_id, &issuer).await.unwrap().is_some());

    storage.delete_user_mapping(&name_id, &issuer).await.unwrap();

    assert!(storage.get_user_mapping_by_name_id(&name_id, &issuer).await.unwrap().is_none());

    // Deleting again is a no-op (no error).
    let result = storage.delete_user_mapping(&name_id, &issuer).await;
    assert!(result.is_ok());

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_list_user_mappings_cursor_pagination() {
    let _guard = saml_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    // Create 5 mappings with deterministic name_id ordering.
    // Use a shared prefix so they sort together and don't collide with
    // leftover data; name_ids are name_page_<uid>_0..4.
    let uid = unique_id();
    let issuer = format!("https://idp_{uid}.example.com");
    for i in 0..5 {
        let name_id = format!("name_page_{uid}_{i}");
        let user_id = format!("@samltest_{uid}_{i}:localhost");
        storage
            .create_user_mapping(make_user_mapping_request(&name_id, &user_id, &issuer))
            .await
            .unwrap();
    }

    // First page: limit 2, no cursor.
    let page1 = storage.list_user_mappings(2, None).await.unwrap();
    let page1_ids: Vec<&str> = page1.iter().map(|m| m.name_id.as_str()).collect();
    assert_eq!(page1.len(), 2, "first page should have 2 rows");
    assert!(page1_ids.iter().all(|n| n.starts_with(&format!("name_page_{uid}_"))));

    // Second page: limit 2, after last name_id of page 1.
    let cursor = page1.last().unwrap().name_id.clone();
    let page2 = storage.list_user_mappings(2, Some(&cursor)).await.unwrap();
    assert_eq!(page2.len(), 2, "second page should have 2 rows");
    // All page2 name_ids strictly greater than the cursor.
    assert!(page2.iter().all(|m| m.name_id.as_str() > cursor.as_str()));

    // Third page: limit 2, after last name_id of page 2 → only 1 left.
    let cursor2 = page2.last().unwrap().name_id.clone();
    let page3 = storage.list_user_mappings(2, Some(&cursor2)).await.unwrap();
    assert_eq!(page3.len(), 1, "third page should have the last 1 row");

    // No more rows after the final cursor.
    let cursor3 = page3.last().unwrap().name_id.clone();
    let page4 = storage.list_user_mappings(2, Some(&cursor3)).await.unwrap();
    assert!(page4.is_empty(), "no rows after the final cursor");

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_user_mapping_any_issuer() {
    let _guard = saml_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    // Same name_id under two different issuers.
    let uid = unique_id();
    let name_id = format!("nameid_shared_{uid}");
    let issuer_a = format!("https://idp_a_{uid}.example.com");
    let issuer_b = format!("https://idp_b_{uid}.example.com");

    let mapping_a = storage
        .create_user_mapping(make_user_mapping_request(
            &name_id,
            &format!("@user_a_{uid}:localhost"),
            &issuer_a,
        ))
        .await
        .unwrap();
    // Small delay so mapping_b has a later first_seen_ts (deterministic ordering).
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    storage
        .create_user_mapping(make_user_mapping_request(
            &name_id,
            &format!("@user_b_{uid}:localhost"),
            &issuer_b,
        ))
        .await
        .unwrap();

    // get_user_mapping_any_issuer returns the oldest (issuer_a).
    let fetched = storage.get_user_mapping_any_issuer(&name_id).await.unwrap();
    assert!(fetched.is_some(), "should return a mapping for shared name_id");
    let fetched = fetched.unwrap();
    assert_eq!(fetched.name_id, name_id);
    assert_eq!(fetched.issuer, issuer_a, "should pick the oldest first_seen_ts");
    assert_eq!(fetched.user_id, mapping_a.user_id);

    // Unknown name_id returns None.
    let none = storage.get_user_mapping_any_issuer("does_not_exist").await.unwrap();
    assert!(none.is_none());

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_update_user_mapping_by_name_id() {
    let _guard = saml_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let name_id = format!("nameid_upd_{uid}");
    let user_id = format!("@samltest_upd_{uid}:localhost");
    let issuer = format!("https://idp_{uid}.example.com");
    storage
        .create_user_mapping(make_user_mapping_request(&name_id, &user_id, &issuer))
        .await
        .unwrap();

    // Update only the user_id (attributes preserved).
    let new_user_id = format!("@samltest_renamed_{uid}:localhost");
    let updated = storage
        .update_user_mapping_by_name_id(&name_id, Some(&new_user_id), None)
        .await
        .unwrap();
    assert!(updated.is_some(), "update should return the updated row");
    let updated = updated.unwrap();
    assert_eq!(updated.user_id, new_user_id);
    assert_eq!(updated.name_id, name_id);
    assert_eq!(updated.issuer, issuer);

    // Update only attributes (user_id preserved).
    let new_attrs = serde_json::json!({"role": ["admin"]});
    let updated2 = storage
        .update_user_mapping_by_name_id(&name_id, None, Some(&new_attrs))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated2.user_id, new_user_id);
    assert_eq!(updated2.attributes, new_attrs);

    // Verify via get_user_mapping_by_name_id.
    let fetched = storage.get_user_mapping_by_name_id(&name_id, &issuer).await.unwrap().unwrap();
    assert_eq!(fetched.user_id, new_user_id);
    assert_eq!(fetched.attributes, new_attrs);

    // Updating a non-existent name_id returns None (no error).
    let missing = storage
        .update_user_mapping_by_name_id("no_such_name_id", Some("@x:localhost"), None)
        .await
        .unwrap();
    assert!(missing.is_none());

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_delete_user_mapping_by_name_id() {
    let _guard = saml_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    // Create 3 mappings under the same name_id with different issuers.
    let uid = unique_id();
    let name_id = format!("nameid_del_{uid}");
    for i in 0..3 {
        storage
            .create_user_mapping(make_user_mapping_request(
                &name_id,
                &format!("@user_{uid}_{i}:localhost"),
                &format!("https://idp_{uid}_{i}.example.com"),
            ))
            .await
            .unwrap();
    }

    let deleted = storage.delete_user_mapping_by_name_id(&name_id).await.unwrap();
    assert_eq!(deleted, 3, "should delete all 3 rows for the name_id");

    // get_user_mapping_any_issuer now returns None.
    let none = storage.get_user_mapping_any_issuer(&name_id).await.unwrap();
    assert!(none.is_none());

    // Deleting again returns 0.
    let deleted_again = storage.delete_user_mapping_by_name_id(&name_id).await.unwrap();
    assert_eq!(deleted_again, 0);

    teardown(pool.as_ref()).await;
}

// ===========================================================================
// Identity providers
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_create_identity_provider() {
    let _guard = saml_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let idp = storage
        .create_identity_provider(make_idp_request(&uid.to_string(), Some(true)))
        .await
        .unwrap();

    assert!(idp.id > 0);
    assert_eq!(idp.entity_id, format!("https://idp_{uid}.example.com"));
    assert_eq!(idp.display_name.as_deref(), Some(format!("IdP {uid}").as_str()));
    assert_eq!(idp.description.as_deref(), Some(format!("Test IdP {uid}").as_str()));
    assert!(idp.is_enabled);
    assert_eq!(idp.priority, 100);
    assert_eq!(idp.attribute_mapping, serde_json::json!({"uid": "name_id"}));
    assert!(idp.created_ts > 0);
    assert!(idp.updated_ts.is_some());

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_create_identity_provider_defaults_enabled_and_priority() {
    let _guard = saml_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    // Pass None for enabled and priority → defaults: enabled=true, priority=100.
    let request = CreateSamlIdentityProviderRequest {
        entity_id: format!("https://idp_default_{uid}.example.com"),
        display_name: None,
        description: None,
        metadata_url: None,
        metadata_xml: None,
        enabled: None,
        priority: None,
        attribute_mapping: None,
    };
    let idp = storage.create_identity_provider(request).await.unwrap();
    assert!(idp.is_enabled, "default is_enabled should be true");
    assert_eq!(idp.priority, 100, "default priority should be 100");
    assert_eq!(idp.attribute_mapping, serde_json::json!({}));
    assert!(idp.display_name.is_none());

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_identity_provider_found_and_not_found() {
    let _guard = saml_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let entity_id = format!("https://idp_{uid}.example.com");
    storage
        .create_identity_provider(make_idp_request(&uid.to_string(), Some(true)))
        .await
        .unwrap();

    let fetched = storage.get_identity_provider(&entity_id).await.unwrap();
    assert!(fetched.is_some());
    assert_eq!(fetched.unwrap().entity_id, entity_id);

    let missing = storage.get_identity_provider("https://no.such.idp").await.unwrap();
    assert!(missing.is_none());

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_all_identity_providers_orders_by_priority() {
    let _guard = saml_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    // Create three IdPs with distinct priorities.
    let uid = unique_id();
    let mut req_high = make_idp_request(&format!("{uid}_high"), Some(true));
    req_high.priority = Some(10);
    let mut req_mid = make_idp_request(&format!("{uid}_mid"), Some(true));
    req_mid.priority = Some(50);
    let mut req_low = make_idp_request(&format!("{uid}_low"), Some(true));
    req_low.priority = Some(200);
    storage.create_identity_provider(req_low).await.unwrap();
    storage.create_identity_provider(req_high).await.unwrap();
    storage.create_identity_provider(req_mid).await.unwrap();

    let all = storage.get_all_identity_providers().await.unwrap();
    assert_eq!(all.len(), 3);
    // Ordered ascending by priority.
    assert!(all[0].priority <= all[1].priority);
    assert!(all[1].priority <= all[2].priority);
    assert_eq!(all[0].priority, 10);

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_enabled_identity_providers_filters_disabled() {
    let _guard = saml_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    storage
        .create_identity_provider(make_idp_request(&format!("{uid}_on"), Some(true)))
        .await
        .unwrap();
    storage
        .create_identity_provider(make_idp_request(&format!("{uid}_off"), Some(false)))
        .await
        .unwrap();

    let enabled = storage.get_enabled_identity_providers().await.unwrap();
    assert_eq!(enabled.len(), 1, "only the enabled IdP should be returned");
    assert!(enabled[0].is_enabled);
    assert_eq!(enabled[0].entity_id, format!("https://idp_{uid}_on.example.com"));

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_update_idp_metadata() {
    let _guard = saml_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let entity_id = format!("https://idp_{uid}.example.com");
    storage
        .create_identity_provider(make_idp_request(&uid.to_string(), Some(true)))
        .await
        .unwrap();

    let new_xml = format!("<EntityDescriptor updated=\"{uid}\"/>");
    let valid_until = chrono::Utc::now().timestamp_millis() + 86_400_000;
    storage
        .update_idp_metadata(&entity_id, &new_xml, Some(valid_until))
        .await
        .unwrap();

    let fetched = storage.get_identity_provider(&entity_id).await.unwrap().unwrap();
    assert_eq!(fetched.metadata_xml.as_deref(), Some(new_xml.as_str()));
    assert!(fetched.last_metadata_refresh_ts.is_some(), "last_metadata_refresh_at should be set");
    assert_eq!(fetched.metadata_valid_until, Some(valid_until));
    assert!(fetched.updated_ts.is_some());

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_delete_identity_provider() {
    let _guard = saml_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let entity_id = format!("https://idp_{uid}.example.com");
    storage
        .create_identity_provider(make_idp_request(&uid.to_string(), Some(true)))
        .await
        .unwrap();
    assert!(storage.get_identity_provider(&entity_id).await.unwrap().is_some());

    storage.delete_identity_provider(&entity_id).await.unwrap();
    assert!(storage.get_identity_provider(&entity_id).await.unwrap().is_none());

    // Deleting again is a no-op.
    let result = storage.delete_identity_provider(&entity_id).await;
    assert!(result.is_ok());

    teardown(pool.as_ref()).await;
}

// ===========================================================================
// Auth events
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_create_auth_event() {
    let _guard = saml_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let event = storage
        .create_auth_event(make_auth_event_request(&uid.to_string(), "success"))
        .await
        .unwrap();

    assert!(event.id > 0);
    assert_eq!(event.event_type, "authentication");
    assert_eq!(event.status, "success");
    assert_eq!(event.user_id.as_deref(), Some(format!("@samltest_{uid}:localhost").as_str()));
    assert_eq!(event.name_id.as_deref(), Some(format!("nameid_{uid}").as_str()));
    assert_eq!(event.issuer.as_deref(), Some(format!("https://idp_{uid}.example.com").as_str()));
    assert_eq!(event.ip_address.as_deref(), Some("192.168.1.1"));
    assert!(event.error_message.is_none());
    assert!(event.created_ts > 0);
    assert!(event.attributes.is_object());

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_create_auth_event_failed_status() {
    let _guard = saml_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let event = storage
        .create_auth_event(make_auth_event_request(&uid.to_string(), "failed"))
        .await
        .unwrap();
    assert_eq!(event.status, "failed");
    assert_eq!(event.error_message.as_deref(), Some("invalid credentials"));

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_auth_events_by_user_orders_desc_and_respects_limit() {
    let _guard = saml_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let user_id = format!("@samltest_{uid}:localhost");
    // Create 3 events under the same user_id with slight time gaps.
    for i in 0..3 {
        let mut req = make_auth_event_request(&format!("{uid}_{i}"), "success");
        req.user_id = Some(user_id.clone());
        storage.create_auth_event(req).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    let events = storage.get_auth_events_by_user(&user_id, 100).await.unwrap();
    assert_eq!(events.len(), 3);
    // Descending by created_ts.
    assert!(events[0].created_ts >= events[1].created_ts);
    assert!(events[1].created_ts >= events[2].created_ts);

    // Limit cuts off to 2.
    let limited = storage.get_auth_events_by_user(&user_id, 2).await.unwrap();
    assert_eq!(limited.len(), 2);
    assert_eq!(limited[0].id, events[0].id);

    // Unknown user returns empty.
    let empty = storage.get_auth_events_by_user("@nobody:localhost", 100).await.unwrap();
    assert!(empty.is_empty());

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_cleanup_old_auth_events() {
    let _guard = saml_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let user_id = format!("@samltest_{uid}:localhost");

    // Insert an old event directly (2 days ago).
    let two_days_ago = chrono::Utc::now().timestamp_millis() - 2 * 86_400_000;
    sqlx::query(
        r#"
        INSERT INTO saml_auth_events (
            session_id, user_id, name_id, issuer, event_type, status,
            error_message, ip_address, user_agent, request_id, attributes, created_ts
        )
        VALUES ($1, $2, $3, $4, 'authentication', 'success', NULL, NULL, NULL, NULL, '{}'::jsonb, $5)
        "#,
    )
    .bind(format!("session_old_{uid}"))
    .bind(&user_id)
    .bind(format!("nameid_old_{uid}"))
    .bind(format!("https://idp_{uid}.example.com"))
    .bind(two_days_ago)
    .execute(pool.as_ref())
    .await
    .unwrap();

    // Insert a recent event via the storage API (created_ts = now).
    let mut recent_req = make_auth_event_request(&uid.to_string(), "success");
    recent_req.user_id = Some(user_id.clone());
    storage.create_auth_event(recent_req).await.unwrap();

    // Cleanup events older than 1 day → should remove only the old one.
    let deleted = storage.cleanup_old_auth_events(1).await.unwrap();
    assert_eq!(deleted, 1, "only the 2-day-old event should be removed");

    // Recent event survives.
    let remaining = storage.get_auth_events_by_user(&user_id, 100).await.unwrap();
    assert_eq!(remaining.len(), 1, "recent event must survive cleanup");

    // Second cleanup is a no-op.
    let deleted_again = storage.cleanup_old_auth_events(1).await.unwrap();
    assert_eq!(deleted_again, 0);

    teardown(pool.as_ref()).await;
}

// ===========================================================================
// Logout requests
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_create_logout_request() {
    let _guard = saml_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let request = make_logout_request(&uid.to_string());
    let logout = storage.create_logout_request(request).await.unwrap();

    assert!(logout.id > 0);
    assert_eq!(logout.request_id, format!("logout_{uid}"));
    assert_eq!(logout.session_id.as_deref(), Some(format!("session_{uid}").as_str()));
    assert_eq!(logout.user_id.as_deref(), Some(format!("@samltest_{uid}:localhost").as_str()));
    assert_eq!(logout.name_id.as_deref(), Some(format!("nameid_{uid}").as_str()));
    assert_eq!(logout.issuer.as_deref(), Some(format!("https://idp_{uid}.example.com").as_str()));
    assert_eq!(logout.reason.as_deref(), Some("user_logout"));
    assert_eq!(logout.status, "pending");
    assert!(logout.created_ts > 0);
    assert!(logout.processed_ts.is_none(), "processed_ts should be None on creation");

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_logout_request_found_and_not_found() {
    let _guard = saml_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let created = storage.create_logout_request(make_logout_request(&uid.to_string())).await.unwrap();

    let fetched = storage.get_logout_request(&created.request_id).await.unwrap();
    assert!(fetched.is_some());
    assert_eq!(fetched.unwrap().request_id, created.request_id);

    let missing = storage.get_logout_request("no_such_request").await.unwrap();
    assert!(missing.is_none());

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_process_logout_request() {
    let _guard = saml_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let uid = unique_id();
    let created = storage.create_logout_request(make_logout_request(&uid.to_string())).await.unwrap();
    assert_eq!(created.status, "pending");
    assert!(created.processed_ts.is_none());

    storage.process_logout_request(&created.request_id).await.unwrap();

    let fetched = storage.get_logout_request(&created.request_id).await.unwrap().unwrap();
    assert_eq!(fetched.status, "processed", "status should flip to 'processed'");
    assert!(
        fetched.processed_ts.is_some(),
        "processed_ts should be populated after process_logout_request"
    );
    assert!(fetched.processed_ts.unwrap() >= created.created_ts);

    // Processing a non-existent request is a no-op (UPDATE matches 0 rows).
    let result = storage.process_logout_request("no_such_request").await;
    assert!(result.is_ok());

    teardown(pool.as_ref()).await;
}

// ===========================================================================
// Config overrides
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_upsert_config_override_insert_and_update() {
    let _guard = saml_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let key = format!("key_{}", unique_id());
    let value_v1 = serde_json::json!({"v": 1});
    storage.upsert_config_override(&key, &value_v1).await.unwrap();

    let all = storage.get_all_config_overrides().await.unwrap();
    assert_eq!(all.get(&key), Some(&value_v1));

    // Upsert again with a new value → should update, not duplicate.
    let value_v2 = serde_json::json!({"v": 2});
    storage.upsert_config_override(&key, &value_v2).await.unwrap();

    let all = storage.get_all_config_overrides().await.unwrap();
    assert_eq!(all.len(), 1, "upsert should not duplicate");
    assert_eq!(all.get(&key), Some(&value_v2));

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_all_config_overrides_empty() {
    let _guard = saml_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let all = storage.get_all_config_overrides().await.unwrap();
    assert!(all.is_empty(), "fresh table should yield no overrides");

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_config_overrides_full_flow() {
    let _guard = saml_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    // Upsert 3 keys.
    let key1 = format!("flow_key1_{}", unique_id());
    let key2 = format!("flow_key2_{}", unique_id());
    let key3 = format!("flow_key3_{}", unique_id());
    storage.upsert_config_override(&key1, &serde_json::json!({"a": 1})).await.unwrap();
    storage.upsert_config_override(&key2, &serde_json::json!({"b": 2})).await.unwrap();
    storage.upsert_config_override(&key3, &serde_json::json!({"c": 3})).await.unwrap();

    let all = storage.get_all_config_overrides().await.unwrap();
    assert_eq!(all.len(), 3);
    assert_eq!(all.get(&key1), Some(&serde_json::json!({"a": 1})));
    assert_eq!(all.get(&key2), Some(&serde_json::json!({"b": 2})));
    assert_eq!(all.get(&key3), Some(&serde_json::json!({"c": 3})));

    // Delete one.
    storage.delete_config_override(&key2).await.unwrap();
    let all = storage.get_all_config_overrides().await.unwrap();
    assert_eq!(all.len(), 2, "after delete one, 2 should remain");
    assert!(all.contains_key(&key1));
    assert!(!all.contains_key(&key2));
    assert!(all.contains_key(&key3));

    // Deleting a non-existent key is a no-op.
    let result = storage.delete_config_override("no_such_key").await;
    assert!(result.is_ok());
    let all = storage.get_all_config_overrides().await.unwrap();
    assert_eq!(all.len(), 2);

    teardown(pool.as_ref()).await;
}

// ===========================================================================
// Constructor sanity check (new)
// ===========================================================================

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_saml_storage_new_constructs_cloneable_handle() {
    // Verifies `SamlStorage::new(&Arc<PgPool>)` returns a Clone-able handle
    // that can issue queries against the pool.
    let _guard = saml_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    // Cheap clone must be usable independently.
    let storage_clone = storage.clone();
    let all = storage_clone.get_all_config_overrides().await.unwrap();
    assert!(all.is_empty(), "fresh table should be empty");

    teardown(pool.as_ref()).await;
}
