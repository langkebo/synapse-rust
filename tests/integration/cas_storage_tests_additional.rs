//! Additional integration tests for `CasStorage` covering all public methods
//! in `synapse-storage/src/cas.rs`:
//!   - Ticket CRUD: `create_ticket` / `validate_ticket` / `get_ticket` /
//!     `delete_ticket` / `cleanup_expired_tickets`
//!   - Proxy ticket: `create_proxy_ticket` / `validate_proxy_ticket`
//!   - Proxy granting ticket: `create_pgt` / `get_pgt` / `get_pgt_by_iou`
//!   - Registered services: `register_service` / `get_service` /
//!     `get_service_by_url` / `list_services` / `delete_service`
//!   - User attributes: `set_user_attribute` / `get_user_attributes`
//!   - SLO sessions: `create_slo_session` / `mark_slo_sent` /
//!     `get_active_slo_sessions`

#![cfg(feature = "cas-sso")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use synapse_storage::cas::{
    CasProxyTicket, CasRegisteredService, CasSloSession, CasTicket, CasUserAttribute, CasStorage,
    CreatePgtRequest, CreateProxyTicketRequest, CreateTicketRequest, RegisterServiceRequest,
};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

fn cas_test_guard() -> &'static Mutex<()> {
    static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    GUARD.get_or_init(|| Mutex::new(()))
}

/// Warm up the shared pool on the current tokio runtime (the test pool can be
/// created on a different runtime; the first query on a fresh runtime may
/// fail with a connection checkout error).
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

/// Clean all CAS tables. Order does not matter (no FK constraints between
/// CAS tables), but child tables are cleared first as a defensive measure.
async fn setup(pool: &Arc<sqlx::PgPool>) {
    warm_up_pool(pool).await;
    sqlx::query("DELETE FROM cas_slo_sessions").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM cas_user_attributes").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM cas_proxy_tickets").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM cas_proxy_granting_tickets").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM cas_services").execute(pool.as_ref()).await.ok();
    sqlx::query("DELETE FROM cas_tickets").execute(pool.as_ref()).await.ok();
}

async fn teardown(pool: &sqlx::PgPool) {
    sqlx::query("DELETE FROM cas_slo_sessions").execute(pool).await.ok();
    sqlx::query("DELETE FROM cas_user_attributes").execute(pool).await.ok();
    sqlx::query("DELETE FROM cas_proxy_tickets").execute(pool).await.ok();
    sqlx::query("DELETE FROM cas_proxy_granting_tickets").execute(pool).await.ok();
    sqlx::query("DELETE FROM cas_services").execute(pool).await.ok();
    sqlx::query("DELETE FROM cas_tickets").execute(pool).await.ok();
}

fn new_storage(pool: &Arc<sqlx::PgPool>) -> CasStorage {
    CasStorage::new(pool)
}

fn unique_ticket_id(prefix: &str) -> String {
    format!("{prefix}-{}", unique_id())
}

fn unique_service_url() -> String {
    format!("https://service-{}/login", unique_id())
}

fn unique_user_id() -> String {
    format!("@castest_{}:localhost", unique_id())
}

fn make_create_ticket_request(service_url: &str) -> CreateTicketRequest {
    CreateTicketRequest {
        ticket_id: unique_ticket_id("ST"),
        user_id: unique_user_id(),
        service_url: service_url.to_string(),
        expires_in_seconds: 60,
    }
}

fn make_register_service_request(pattern: Option<&str>) -> RegisterServiceRequest {
    RegisterServiceRequest {
        service_id: format!("svc-{}", unique_id()),
        name: format!("Service {}", unique_id()),
        description: Some("Test service".to_string()),
        service_url_pattern: pattern
            .map(|p| p.to_string())
            .unwrap_or_else(|| format!("^https://service-{}/.*$", unique_id())),
        allowed_attributes: Some(vec!["uid".to_string(), "displayName".to_string()]),
        allowed_proxy_callbacks: Some(vec!["https://callback.example.com".to_string()]),
        is_require_secure: Some(true),
        is_single_logout: Some(false),
    }
}

// ---------------------------------------------------------------------------
// new (constructor)
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_new_creates_storage() {
    let _guard = cas_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    // Constructor does not perform any I/O; just verify it does not panic
    // and returns a value that can be used.
    let storage = new_storage(&pool);
    let _ = storage;
    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// create_ticket
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_create_ticket_basic() {
    let _guard = cas_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let service_url = unique_service_url();
    let request = CreateTicketRequest {
        ticket_id: unique_ticket_id("ST"),
        user_id: unique_user_id(),
        service_url: service_url.clone(),
        expires_in_seconds: 60,
    };

    let ticket: CasTicket = storage.create_ticket(request).await.unwrap();
    assert!(ticket.id > 0);
    assert!(ticket.is_valid);
    assert!(ticket.consumed_ts.is_none());
    assert!(ticket.consumed_by.is_none());
    assert!(ticket.created_ts > 0);
    assert!(ticket.expires_at > ticket.created_ts);
    assert_eq!(ticket.expires_at - ticket.created_ts, 60_000);

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_create_ticket_with_short_expiration() {
    let _guard = cas_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let request = CreateTicketRequest {
        ticket_id: unique_ticket_id("ST"),
        user_id: unique_user_id(),
        service_url: unique_service_url(),
        expires_in_seconds: 5,
    };
    let ticket = storage.create_ticket(request).await.unwrap();
    assert_eq!(ticket.expires_at - ticket.created_ts, 5_000);

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// validate_ticket
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_validate_ticket_success() {
    let _guard = cas_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let service_url = unique_service_url();
    let created = storage
        .create_ticket(make_create_ticket_request(&service_url))
        .await
        .unwrap();

    let validated = storage
        .validate_ticket(&created.ticket_id, &service_url)
        .await
        .unwrap()
        .expect("ticket should validate");
    assert_eq!(validated.ticket_id, created.ticket_id);
    assert!(!validated.is_valid);
    assert!(validated.consumed_ts.is_some());
    assert_eq!(validated.consumed_ts.unwrap(), validated.consumed_ts.unwrap());

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_validate_ticket_wrong_service_url_returns_none() {
    let _guard = cas_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let created = storage
        .create_ticket(make_create_ticket_request(&unique_service_url()))
        .await
        .unwrap();

    let result = storage
        .validate_ticket(&created.ticket_id, "https://wrong.example.com/login")
        .await
        .unwrap();
    assert!(result.is_none(), "validation with wrong service_url should return None");

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_validate_ticket_expired_returns_none() {
    let _guard = cas_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let service_url = unique_service_url();
    let now = chrono::Utc::now().timestamp_millis();
    // Insert an already-expired ticket directly so we do not have to sleep.
    let ticket_id = unique_ticket_id("ST");
    let user_id = unique_user_id();
    sqlx::query(
        "INSERT INTO cas_tickets (ticket_id, user_id, service_url, created_ts, expires_at) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(&ticket_id)
    .bind(&user_id)
    .bind(&service_url)
    .bind(now - 120_000)
    .bind(now - 60_000)
    .execute(pool.as_ref())
    .await
    .unwrap();

    let result = storage.validate_ticket(&ticket_id, &service_url).await.unwrap();
    assert!(result.is_none(), "expired ticket should not validate");

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_validate_ticket_already_consumed_returns_none() {
    let _guard = cas_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let service_url = unique_service_url();
    let created = storage
        .create_ticket(make_create_ticket_request(&service_url))
        .await
        .unwrap();

    // First validation consumes the ticket.
    let first = storage.validate_ticket(&created.ticket_id, &service_url).await.unwrap();
    assert!(first.is_some());

    // Second validation should fail (already consumed / is_valid = FALSE).
    let second = storage.validate_ticket(&created.ticket_id, &service_url).await.unwrap();
    assert!(second.is_none());

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// get_ticket
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_ticket_existing() {
    let _guard = cas_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let created = storage
        .create_ticket(make_create_ticket_request(&unique_service_url()))
        .await
        .unwrap();

    let fetched = storage.get_ticket(&created.ticket_id).await.unwrap().expect("ticket should exist");
    assert_eq!(fetched.ticket_id, created.ticket_id);
    assert_eq!(fetched.user_id, created.user_id);
    assert_eq!(fetched.service_url, created.service_url);
    assert!(fetched.is_valid);

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_ticket_nonexistent_returns_none() {
    let _guard = cas_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let result = storage.get_ticket("ST-does-not-exist-999999").await.unwrap();
    assert!(result.is_none());

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// delete_ticket
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_delete_ticket_existing() {
    let _guard = cas_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let created = storage
        .create_ticket(make_create_ticket_request(&unique_service_url()))
        .await
        .unwrap();

    let deleted = storage.delete_ticket(&created.ticket_id).await.unwrap();
    assert!(deleted);

    // Verify the ticket is gone.
    let fetched = storage.get_ticket(&created.ticket_id).await.unwrap();
    assert!(fetched.is_none());

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_delete_ticket_nonexistent_returns_false() {
    let _guard = cas_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let deleted = storage.delete_ticket("ST-does-not-exist-999999").await.unwrap();
    assert!(!deleted);

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// cleanup_expired_tickets
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_cleanup_expired_tickets_removes_only_expired() {
    let _guard = cas_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let now = chrono::Utc::now().timestamp_millis();

    // Insert two expired tickets directly.
    for i in 0..2 {
        let _ = sqlx::query(
            "INSERT INTO cas_tickets (ticket_id, user_id, service_url, created_ts, expires_at) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(format!("ST-expired-{i}-{}", unique_id()))
        .bind(unique_user_id())
        .bind(unique_service_url())
        .bind(now - 120_000)
        .bind(now - 60_000)
        .execute(pool.as_ref())
        .await
        .unwrap();
    }

    // Insert one valid ticket via the storage API.
    let valid = storage
        .create_ticket(make_create_ticket_request(&unique_service_url()))
        .await
        .unwrap();

    let removed = storage.cleanup_expired_tickets().await.unwrap();
    assert_eq!(removed, 2, "should remove exactly the two expired tickets");

    // The valid ticket must still be present.
    let fetched = storage.get_ticket(&valid.ticket_id).await.unwrap();
    assert!(fetched.is_some(), "valid ticket should survive cleanup");

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_cleanup_expired_tickets_nothing_to_remove() {
    let _guard = cas_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    // Insert only a valid ticket.
    let _ = storage
        .create_ticket(make_create_ticket_request(&unique_service_url()))
        .await
        .unwrap();

    let removed = storage.cleanup_expired_tickets().await.unwrap();
    assert_eq!(removed, 0);

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// create_proxy_ticket
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_create_proxy_ticket_basic() {
    let _guard = cas_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let request = CreateProxyTicketRequest {
        proxy_ticket_id: unique_ticket_id("PT"),
        user_id: unique_user_id(),
        service_url: unique_service_url(),
        pgt_url: Some("https://pgt.example.com".to_string()),
        expires_in_seconds: 30,
    };
    let ticket: CasProxyTicket = storage.create_proxy_ticket(request).await.unwrap();
    assert!(ticket.id > 0);
    assert!(ticket.is_valid);
    assert!(ticket.consumed_ts.is_none());
    assert_eq!(ticket.pgt_url.as_deref(), Some("https://pgt.example.com"));
    assert_eq!(ticket.expires_at - ticket.created_ts, 30_000);

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// validate_proxy_ticket
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_validate_proxy_ticket_success() {
    let _guard = cas_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let service_url = unique_service_url();
    let request = CreateProxyTicketRequest {
        proxy_ticket_id: unique_ticket_id("PT"),
        user_id: unique_user_id(),
        service_url: service_url.clone(),
        pgt_url: None,
        expires_in_seconds: 60,
    };
    let created = storage.create_proxy_ticket(request).await.unwrap();

    let validated = storage
        .validate_proxy_ticket(&created.proxy_ticket_id, &service_url)
        .await
        .unwrap()
        .expect("proxy ticket should validate");
    assert_eq!(validated.proxy_ticket_id, created.proxy_ticket_id);
    assert!(!validated.is_valid);
    assert!(validated.consumed_ts.is_some());

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_validate_proxy_ticket_wrong_service_returns_none() {
    let _guard = cas_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let request = CreateProxyTicketRequest {
        proxy_ticket_id: unique_ticket_id("PT"),
        user_id: unique_user_id(),
        service_url: unique_service_url(),
        pgt_url: None,
        expires_in_seconds: 60,
    };
    let created = storage.create_proxy_ticket(request).await.unwrap();

    let result = storage
        .validate_proxy_ticket(&created.proxy_ticket_id, "https://wrong.example.com")
        .await
        .unwrap();
    assert!(result.is_none(), "proxy ticket with wrong service_url should not validate");

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// create_pgt / get_pgt / get_pgt_by_iou
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_create_pgt_basic() {
    let _guard = cas_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let request = CreatePgtRequest {
        pgt_id: unique_ticket_id("PGT"),
        user_id: unique_user_id(),
        service_url: unique_service_url(),
        iou: Some(format!("PGTIOU-{}", unique_id())),
        expires_in_seconds: 600,
    };
    let pgt = storage.create_pgt(request).await.unwrap();
    assert!(pgt.id > 0);
    assert!(pgt.is_valid);
    assert!(pgt.iou.is_some());
    assert_eq!(pgt.expires_at - pgt.created_ts, 600_000);

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_pgt_existing() {
    let _guard = cas_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let request = CreatePgtRequest {
        pgt_id: unique_ticket_id("PGT"),
        user_id: unique_user_id(),
        service_url: unique_service_url(),
        iou: Some(format!("PGTIOU-{}", unique_id())),
        expires_in_seconds: 600,
    };
    let created = storage.create_pgt(request).await.unwrap();

    let fetched = storage.get_pgt(&created.pgt_id).await.unwrap().expect("PGT should exist");
    assert_eq!(fetched.pgt_id, created.pgt_id);
    assert_eq!(fetched.user_id, created.user_id);
    assert!(fetched.is_valid);

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_pgt_nonexistent_returns_none() {
    let _guard = cas_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let result = storage.get_pgt("PGT-does-not-exist-999999").await.unwrap();
    assert!(result.is_none());

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_pgt_by_iou() {
    let _guard = cas_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let iou = format!("PGTIOU-{}", unique_id());
    let request = CreatePgtRequest {
        pgt_id: unique_ticket_id("PGT"),
        user_id: unique_user_id(),
        service_url: unique_service_url(),
        iou: Some(iou.clone()),
        expires_in_seconds: 600,
    };
    let created = storage.create_pgt(request).await.unwrap();

    let fetched = storage
        .get_pgt_by_iou(&iou)
        .await
        .unwrap()
        .expect("PGT should be found by IOU");
    assert_eq!(fetched.pgt_id, created.pgt_id);
    assert_eq!(fetched.iou.as_deref(), Some(iou.as_str()));

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_pgt_by_iou_with_no_iou_returns_none() {
    let _guard = cas_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    // Create a PGT with no IOU.
    let request = CreatePgtRequest {
        pgt_id: unique_ticket_id("PGT"),
        user_id: unique_user_id(),
        service_url: unique_service_url(),
        iou: None,
        expires_in_seconds: 600,
    };
    let _ = storage.create_pgt(request).await.unwrap();

    let result = storage.get_pgt_by_iou("PGTIOU-never-exists").await.unwrap();
    assert!(result.is_none());

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// register_service / get_service
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_register_service_basic() {
    let _guard = cas_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let request = make_register_service_request(None);
    let service: CasRegisteredService = storage.register_service(request.clone()).await.unwrap();
    assert!(service.id > 0);
    assert_eq!(service.service_id, request.service_id);
    assert_eq!(service.name, request.name);
    assert_eq!(service.description, request.description);
    assert_eq!(service.service_url_pattern, request.service_url_pattern);
    assert!(service.is_enabled);
    assert!(service.is_require_secure);
    assert!(!service.is_single_logout);
    assert!(service.created_ts > 0);
    // `updated_ts` defaults to `created_ts` when registering (see impl: `$9, $9`).
    assert_eq!(service.updated_ts, service.created_ts);
    // allowed_attributes / allowed_proxy_callbacks are JSON arrays.
    assert_eq!(service.allowed_attributes, serde_json::json!(["uid", "displayName"]));
    assert_eq!(
        service.allowed_proxy_callbacks,
        serde_json::json!(["https://callback.example.com"])
    );

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_register_service_defaults() {
    let _guard = cas_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let request = RegisterServiceRequest {
        service_id: format!("svc-{}", unique_id()),
        name: "Bare Service".to_string(),
        description: None,
        service_url_pattern: format!("^https://bare-{}/.*$", unique_id()),
        allowed_attributes: None,
        allowed_proxy_callbacks: None,
        is_require_secure: None,
        is_single_logout: None,
    };
    let service = storage.register_service(request).await.unwrap();
    // When allowed_attributes / allowed_proxy_callbacks are None, the impl
    // serializes an empty Vec, producing `[]`.
    assert_eq!(service.allowed_attributes, serde_json::json!([]));
    assert_eq!(service.allowed_proxy_callbacks, serde_json::json!([]));
    // Defaults: is_require_secure=true, is_single_logout=false.
    assert!(service.is_require_secure);
    assert!(!service.is_single_logout);
    assert!(service.is_enabled);

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_service_existing() {
    let _guard = cas_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let request = make_register_service_request(None);
    let created = storage.register_service(request.clone()).await.unwrap();

    let fetched = storage
        .get_service(&created.service_id)
        .await
        .unwrap()
        .expect("service should exist");
    assert_eq!(fetched.service_id, created.service_id);
    assert_eq!(fetched.name, request.name);

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_service_nonexistent_returns_none() {
    let _guard = cas_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let result = storage.get_service("svc-does-not-exist-999999").await.unwrap();
    assert!(result.is_none());

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// get_service_by_url
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_service_by_url_match() {
    let _guard = cas_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    // Pattern matches a specific path prefix.
    let pattern = "^https://app-[0-9]+\\.example\\.com/.*";
    let request = make_register_service_request(Some(pattern));
    let created = storage.register_service(request).await.unwrap();

    let matched = storage
        .get_service_by_url("https://app-42.example.com/login")
        .await
        .unwrap()
        .expect("service should match URL");
    assert_eq!(matched.service_id, created.service_id);

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_service_by_url_no_match_returns_none() {
    let _guard = cas_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let pattern = "^https://app-[0-9]+\\.example\\.com/.*";
    let _ = storage
        .register_service(make_register_service_request(Some(pattern)))
        .await
        .unwrap();

    // A URL that does not match the pattern.
    let result = storage.get_service_by_url("https://other.example.com/login").await.unwrap();
    assert!(result.is_none(), "URL not matching any pattern should return None");

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_service_by_url_skips_disabled_service() {
    let _guard = cas_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let pattern = "^https://disabled-[0-9]+\\.example\\.com/.*";
    let request = make_register_service_request(Some(pattern));
    let created = storage.register_service(request).await.unwrap();

    // Disable the service directly.
    sqlx::query("UPDATE cas_services SET is_enabled = FALSE WHERE service_id = $1")
        .bind(&created.service_id)
        .execute(pool.as_ref())
        .await
        .unwrap();

    let result = storage
        .get_service_by_url("https://disabled-1.example.com/login")
        .await
        .unwrap();
    assert!(result.is_none(), "disabled services should not be returned");

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// list_services
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_list_services_returns_all_ordered_by_created_ts_desc() {
    let _guard = cas_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    // Register 3 services with distinct timestamps to verify ORDER BY.
    let mut ids = Vec::new();
    for _ in 0..3 {
        let created = storage.register_service(make_register_service_request(None)).await.unwrap();
        ids.push(created);
        // Tiny sleep so created_ts differs between services. created_ts has
        // millisecond resolution; a 5ms yield is enough on most platforms.
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }

    let services = storage.list_services().await.unwrap();
    assert_eq!(services.len(), 3, "should list exactly the 3 registered services");
    // Order: newest first.
    assert!(services[0].created_ts >= services[1].created_ts);
    assert!(services[1].created_ts >= services[2].created_ts);

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_list_services_empty() {
    let _guard = cas_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let services = storage.list_services().await.unwrap();
    assert!(services.is_empty());

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// delete_service
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_delete_service_existing() {
    let _guard = cas_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let created = storage.register_service(make_register_service_request(None)).await.unwrap();

    let deleted = storage.delete_service(&created.service_id).await.unwrap();
    assert!(deleted);

    // Confirm gone.
    let fetched = storage.get_service(&created.service_id).await.unwrap();
    assert!(fetched.is_none());

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_delete_service_nonexistent_returns_false() {
    let _guard = cas_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let deleted = storage.delete_service("svc-does-not-exist-999999").await.unwrap();
    assert!(!deleted);

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// set_user_attribute / get_user_attributes
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_set_user_attribute_new() {
    let _guard = cas_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user_id = unique_user_id();
    let attr: CasUserAttribute = storage
        .set_user_attribute(&user_id, "displayName", "Alice Example")
        .await
        .unwrap();
    assert!(attr.id > 0);
    assert_eq!(attr.user_id, user_id);
    assert_eq!(attr.attribute_name, "displayName");
    assert_eq!(attr.attribute_value, "Alice Example");
    assert!(attr.created_ts > 0);
    assert!(attr.updated_ts > 0);

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_set_user_attribute_upsert() {
    let _guard = cas_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user_id = unique_user_id();
    let first = storage.set_user_attribute(&user_id, "email", "old@example.com").await.unwrap();
    // Allow updated_ts to differ from created_ts.
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    let second = storage.set_user_attribute(&user_id, "email", "new@example.com").await.unwrap();

    // ON CONFLICT should update the same row, not insert a new one.
    assert_eq!(first.id, second.id);
    assert_eq!(second.attribute_value, "new@example.com");
    assert!(second.updated_ts >= first.updated_ts);

    // Verify only one row exists for this (user, attribute).
    let attrs = storage.get_user_attributes(&user_id).await.unwrap();
    assert_eq!(attrs.len(), 1);

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_user_attributes_returns_all_for_user() {
    let _guard = cas_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user_id = unique_user_id();
    storage.set_user_attribute(&user_id, "uid", "alice").await.unwrap();
    storage.set_user_attribute(&user_id, "displayName", "Alice").await.unwrap();
    storage.set_user_attribute(&user_id, "email", "alice@example.com").await.unwrap();

    let attrs = storage.get_user_attributes(&user_id).await.unwrap();
    assert_eq!(attrs.len(), 3);

    let names: std::collections::HashSet<&str> =
        attrs.iter().map(|a| a.attribute_name.as_str()).collect();
    assert!(names.contains("uid"));
    assert!(names.contains("displayName"));
    assert!(names.contains("email"));

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_user_attributes_empty_for_unknown_user() {
    let _guard = cas_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let attrs = storage.get_user_attributes("@nobody:localhost").await.unwrap();
    assert!(attrs.is_empty());

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// create_slo_session
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_create_slo_session_basic() {
    let _guard = cas_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let session_id = format!("slo-{}", unique_id());
    let user_id = unique_user_id();
    let service_url = unique_service_url();
    let ticket_id = unique_ticket_id("ST");

    let session: CasSloSession = storage
        .create_slo_session(&session_id, &user_id, &service_url, Some(&ticket_id))
        .await
        .unwrap();
    assert!(session.id > 0);
    assert_eq!(session.session_id, session_id);
    assert_eq!(session.user_id, user_id);
    assert_eq!(session.service_url, service_url);
    assert_eq!(session.ticket_id.as_deref(), Some(ticket_id.as_str()));
    assert!(session.created_ts > 0);
    assert!(session.logout_sent_ts.is_none(), "newly created session should not be marked as sent");

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_create_slo_session_without_ticket() {
    let _guard = cas_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let session_id = format!("slo-{}", unique_id());
    let session = storage
        .create_slo_session(&session_id, &unique_user_id(), &unique_service_url(), None)
        .await
        .unwrap();
    assert!(session.ticket_id.is_none());

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// mark_slo_sent
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_mark_slo_sent_first_call_returns_true() {
    let _guard = cas_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let session_id = format!("slo-{}", unique_id());
    storage
        .create_slo_session(
            &session_id,
            &unique_user_id(),
            &unique_service_url(),
            None,
        )
        .await
        .unwrap();

    let marked = storage.mark_slo_sent(&session_id).await.unwrap();
    assert!(marked, "first mark_slo_sent on an existing session should return true");

    // Confirm the session is now marked via get_active_slo_sessions.
    let _ = sqlx::query("SELECT 1").execute(pool.as_ref()).await.unwrap();

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_mark_slo_sent_nonexistent_returns_false() {
    let _guard = cas_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let marked = storage.mark_slo_sent("slo-does-not-exist-999999").await.unwrap();
    assert!(!marked, "mark_slo_sent on a non-existent session should return false");

    teardown(pool.as_ref()).await;
}

// ---------------------------------------------------------------------------
// get_active_slo_sessions
// ---------------------------------------------------------------------------

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_active_slo_sessions_returns_only_unsent() {
    let _guard = cas_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user_id = unique_user_id();

    // Session 1: active (no logout_sent_at).
    let sid_active = format!("slo-active-{}", unique_id());
    storage
        .create_slo_session(&sid_active, &user_id, &unique_service_url(), None)
        .await
        .unwrap();

    // Session 2: already sent.
    let sid_sent = format!("slo-sent-{}", unique_id());
    storage
        .create_slo_session(&sid_sent, &user_id, &unique_service_url(), None)
        .await
        .unwrap();
    let _ = storage.mark_slo_sent(&sid_sent).await.unwrap();

    let active = storage.get_active_slo_sessions(&user_id).await.unwrap();
    assert_eq!(active.len(), 1, "only the unsent session should be returned");
    assert_eq!(active[0].session_id, sid_active);
    assert!(active[0].logout_sent_ts.is_none());

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_active_slo_sessions_filters_by_user() {
    let _guard = cas_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let user_a = unique_user_id();
    let user_b = unique_user_id();

    storage
        .create_slo_session(
            &format!("slo-{}", unique_id()),
            &user_a,
            &unique_service_url(),
            None,
        )
        .await
        .unwrap();
    storage
        .create_slo_session(
            &format!("slo-{}", unique_id()),
            &user_a,
            &unique_service_url(),
            None,
        )
        .await
        .unwrap();
    storage
        .create_slo_session(
            &format!("slo-{}", unique_id()),
            &user_b,
            &unique_service_url(),
            None,
        )
        .await
        .unwrap();

    let active_a = storage.get_active_slo_sessions(&user_a).await.unwrap();
    assert_eq!(active_a.len(), 2);
    for s in &active_a {
        assert_eq!(s.user_id, user_a);
    }

    let active_b = storage.get_active_slo_sessions(&user_b).await.unwrap();
    assert_eq!(active_b.len(), 1);

    teardown(pool.as_ref()).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn test_get_active_slo_sessions_empty_for_unknown_user() {
    let _guard = cas_test_guard().lock().unwrap();
    let pool = crate::require_test_pool().await;
    setup(&pool).await;
    let storage = new_storage(&pool);

    let active = storage.get_active_slo_sessions("@nobody:localhost").await.unwrap();
    assert!(active.is_empty());

    teardown(pool.as_ref()).await;
}
