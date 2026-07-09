use std::env;
use std::sync::Arc;

use sqlx::postgres::PgPoolOptions;

use super::*;

// ---- test infrastructure ----

async fn test_pool() -> Arc<sqlx::PgPool> {
    let db_url = env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
    let pool =
        PgPoolOptions::new().max_connections(2).connect(&db_url).await.expect("Failed to connect to test database");
    Arc::new(pool)
}

fn storage(pool: &Arc<sqlx::PgPool>) -> CasStorage {
    CasStorage::new(pool)
}

async fn cleanup_with_suffix(pool: &sqlx::PgPool, suffix: &str) {
    let pattern = format!("%{suffix}%");
    let _ = sqlx::query("DELETE FROM cas_tickets WHERE ticket_id LIKE $1").bind(&pattern).execute(pool).await;
    let _ =
        sqlx::query("DELETE FROM cas_proxy_tickets WHERE proxy_ticket_id LIKE $1").bind(&pattern).execute(pool).await;
    let _ =
        sqlx::query("DELETE FROM cas_proxy_granting_tickets WHERE pgt_id LIKE $1").bind(&pattern).execute(pool).await;
    let _ = sqlx::query("DELETE FROM cas_services WHERE service_id LIKE $1").bind(&pattern).execute(pool).await;
    let _ = sqlx::query("DELETE FROM cas_user_attributes WHERE user_id LIKE $1").bind(&pattern).execute(pool).await;
    let _ = sqlx::query("DELETE FROM cas_slo_sessions WHERE session_id LIKE $1").bind(&pattern).execute(pool).await;
}

async fn ensure_test_user(pool: &sqlx::PgPool, user_id: &str) {
    let username = user_id.strip_prefix('@').and_then(|u| u.split(':').next()).unwrap_or("testuser");
    let _ = sqlx::query(
        "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, EXTRACT(EPOCH FROM NOW()) * 1000) ON CONFLICT (user_id) DO NOTHING",
    )
    .bind(user_id)
    .bind(username)
    .execute(pool)
    .await;
}

// ---- CAS ticket tests ----

#[tokio::test]
async fn test_create_ticket() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@testuser-{suffix}:example.com");
    let ticket_id = format!("cas_ticket_{suffix}");
    let service_url = format!("https://svc-{suffix}.example.com");

    cleanup_with_suffix(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let cas = storage(&pool);
    let ticket = cas
        .create_ticket(CreateTicketRequest {
            ticket_id: ticket_id.clone(),
            user_id: user_id.clone(),
            service_url: service_url.clone(),
            expires_in_seconds: 3600,
        })
        .await
        .expect("should succeed");

    assert_eq!(ticket.ticket_id, ticket_id);
    assert_eq!(ticket.user_id, user_id);
    assert_eq!(ticket.service_url, service_url);
    assert!(ticket.is_valid);
    assert!(ticket.expires_at > ticket.created_ts);

    cleanup_with_suffix(&pool, &suffix).await;
}

#[tokio::test]
async fn test_create_duplicate_ticket() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@testuser-{suffix}:example.com");
    let ticket_id = format!("cas_ticket_{suffix}");
    let service_url = format!("https://svc-{suffix}.example.com");

    cleanup_with_suffix(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let cas = storage(&pool);
    let req = CreateTicketRequest {
        ticket_id: ticket_id.clone(),
        user_id: user_id.clone(),
        service_url: service_url.clone(),
        expires_in_seconds: 3600,
    };

    // First creation should succeed.
    cas.create_ticket(req.clone()).await.expect("should succeed");

    // Second creation with same ticket_id should fail (unique constraint).
    let result = cas.create_ticket(req).await;
    assert!(result.is_err(), "duplicate ticket_id should return error");

    cleanup_with_suffix(&pool, &suffix).await;
}

#[tokio::test]
async fn test_validate_ticket_valid() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@testuser-{suffix}:example.com");
    let ticket_id = format!("cas_ticket_{suffix}");
    let service_url = format!("https://svc-{suffix}.example.com");

    cleanup_with_suffix(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let cas = storage(&pool);
    cas.create_ticket(CreateTicketRequest {
        ticket_id: ticket_id.clone(),
        user_id: user_id.clone(),
        service_url: service_url.clone(),
        expires_in_seconds: 3600,
    })
    .await
    .expect("should succeed");

    // Validate the ticket — should succeed and mark consumed.
    let validated =
        cas.validate_ticket(&ticket_id, &service_url).await.expect("should succeed").expect("should return Some");

    assert_eq!(validated.ticket_id, ticket_id);
    assert_eq!(validated.user_id, user_id);
    assert!(!validated.is_valid, "should be marked invalid after consumption");

    // Validating the same ticket again should return None.
    let second = cas.validate_ticket(&ticket_id, &service_url).await.expect("should succeed");
    assert!(second.is_none(), "already-consumed ticket should return None");

    cleanup_with_suffix(&pool, &suffix).await;
}

#[tokio::test]
async fn test_validate_ticket_expired() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@testuser-{suffix}:example.com");
    let ticket_id = format!("cas_ticket_{suffix}");
    let service_url = format!("https://svc-{suffix}.example.com");

    cleanup_with_suffix(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let cas = storage(&pool);
    // expires_in_seconds negative = ticket is already expired.
    cas.create_ticket(CreateTicketRequest {
        ticket_id: ticket_id.clone(),
        user_id: user_id.clone(),
        service_url: service_url.clone(),
        expires_in_seconds: -3600,
    })
    .await
    .expect("should succeed");

    let result = cas.validate_ticket(&ticket_id, &service_url).await.expect("should succeed");
    assert!(result.is_none(), "expired ticket should return None");

    cleanup_with_suffix(&pool, &suffix).await;
}

#[tokio::test]
async fn test_validate_ticket_wrong_url() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@testuser-{suffix}:example.com");
    let ticket_id = format!("cas_ticket_{suffix}");
    let service_url = format!("https://svc-{suffix}.example.com");

    cleanup_with_suffix(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let cas = storage(&pool);
    cas.create_ticket(CreateTicketRequest {
        ticket_id: ticket_id.clone(),
        user_id: user_id.clone(),
        service_url: service_url.clone(),
        expires_in_seconds: 3600,
    })
    .await
    .expect("should succeed");

    // Validate with wrong service_url.
    let result = cas.validate_ticket(&ticket_id, "https://wrong.example.com").await.expect("should succeed");
    assert!(result.is_none(), "wrong service_url should return None");

    cleanup_with_suffix(&pool, &suffix).await;
}

#[tokio::test]
async fn test_get_ticket() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@testuser-{suffix}:example.com");
    let ticket_id = format!("cas_ticket_{suffix}");
    let service_url = format!("https://svc-{suffix}.example.com");

    cleanup_with_suffix(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let cas = storage(&pool);
    cas.create_ticket(CreateTicketRequest {
        ticket_id: ticket_id.clone(),
        user_id: user_id.clone(),
        service_url: service_url.clone(),
        expires_in_seconds: 3600,
    })
    .await
    .expect("should succeed");

    // Get existing ticket.
    let found = cas.get_ticket(&ticket_id).await.expect("should succeed").expect("should find ticket");
    assert_eq!(found.ticket_id, ticket_id);

    // Get non-existent ticket.
    let not_found = cas.get_ticket("nonexistent_ticket_id").await.expect("should succeed");
    assert!(not_found.is_none());

    cleanup_with_suffix(&pool, &suffix).await;
}

#[tokio::test]
async fn test_delete_ticket() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@testuser-{suffix}:example.com");
    let ticket_id = format!("cas_ticket_{suffix}");
    let service_url = format!("https://svc-{suffix}.example.com");

    cleanup_with_suffix(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let cas = storage(&pool);
    cas.create_ticket(CreateTicketRequest {
        ticket_id: ticket_id.clone(),
        user_id: user_id.clone(),
        service_url: service_url.clone(),
        expires_in_seconds: 3600,
    })
    .await
    .expect("should succeed");

    // Delete existing ticket.
    let deleted = cas.delete_ticket(&ticket_id).await.expect("should succeed");
    assert!(deleted, "should return true when ticket is deleted");

    // Verify it's gone.
    let after = cas.get_ticket(&ticket_id).await.expect("should succeed");
    assert!(after.is_none(), "ticket should be gone after delete");

    // Delete already-deleted ticket — should return false.
    let again = cas.delete_ticket(&ticket_id).await.expect("should succeed");
    assert!(!again, "should return false for already-deleted ticket");

    // Delete non-existent ticket.
    let never_existed = cas.delete_ticket("nonexistent_ticket_id").await.expect("should succeed");
    assert!(!never_existed);

    cleanup_with_suffix(&pool, &suffix).await;
}

#[tokio::test]
async fn test_cleanup_expired_tickets() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@testuser-{suffix}:example.com");
    let expired_id = format!("expired_ticket_{suffix}");
    let valid_id = format!("valid_ticket_{suffix}");
    let service_url = format!("https://svc-{suffix}.example.com");

    cleanup_with_suffix(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let cas = storage(&pool);

    // Create an already-expired ticket.
    cas.create_ticket(CreateTicketRequest {
        ticket_id: expired_id.clone(),
        user_id: user_id.clone(),
        service_url: service_url.clone(),
        expires_in_seconds: -3600,
    })
    .await
    .expect("should succeed");

    // Create a valid (not-expired) ticket.
    cas.create_ticket(CreateTicketRequest {
        ticket_id: valid_id.clone(),
        user_id: user_id.clone(),
        service_url: service_url.clone(),
        expires_in_seconds: 3600,
    })
    .await
    .expect("should succeed");

    // Cleanup should remove the expired one.
    let removed = cas.cleanup_expired_tickets().await.expect("should succeed");
    assert!(removed >= 1, "should remove at least 1 expired ticket");

    // Expired ticket should be gone.
    let expired_after = cas.get_ticket(&expired_id).await.expect("should succeed");
    assert!(expired_after.is_none(), "expired ticket should be removed");

    // Valid ticket should still exist.
    let valid_after =
        cas.get_ticket(&valid_id).await.expect("should succeed").expect("valid ticket should still exist");
    assert_eq!(valid_after.ticket_id, valid_id);

    cleanup_with_suffix(&pool, &suffix).await;
}

// ---- CAS proxy ticket tests ----

#[tokio::test]
async fn test_create_proxy_ticket() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@testuser-{suffix}:example.com");
    let proxy_ticket_id = format!("proxy_ticket_{suffix}");
    let service_url = format!("https://svc-{suffix}.example.com");

    cleanup_with_suffix(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let cas = storage(&pool);
    let ticket = cas
        .create_proxy_ticket(CreateProxyTicketRequest {
            proxy_ticket_id: proxy_ticket_id.clone(),
            user_id: user_id.clone(),
            service_url: service_url.clone(),
            pgt_url: Some(format!("https://pgt-{suffix}.example.com")),
            expires_in_seconds: 3600,
        })
        .await
        .expect("should succeed");

    assert_eq!(ticket.proxy_ticket_id, proxy_ticket_id);
    assert_eq!(ticket.user_id, user_id);
    assert_eq!(ticket.service_url, service_url);
    assert!(ticket.is_valid);
    assert!(ticket.expires_at > ticket.created_ts);

    cleanup_with_suffix(&pool, &suffix).await;
}

#[tokio::test]
async fn test_validate_proxy_ticket() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@testuser-{suffix}:example.com");
    let valid_pt_id = format!("valid_pt_{suffix}");
    let expired_pt_id = format!("expired_pt_{suffix}");
    let service_url = format!("https://svc-{suffix}.example.com");

    cleanup_with_suffix(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let cas = storage(&pool);

    // Create a valid proxy ticket.
    cas.create_proxy_ticket(CreateProxyTicketRequest {
        proxy_ticket_id: valid_pt_id.clone(),
        user_id: user_id.clone(),
        service_url: service_url.clone(),
        pgt_url: None,
        expires_in_seconds: 3600,
    })
    .await
    .expect("should succeed");

    // Create an already-expired proxy ticket.
    cas.create_proxy_ticket(CreateProxyTicketRequest {
        proxy_ticket_id: expired_pt_id.clone(),
        user_id: user_id.clone(),
        service_url: service_url.clone(),
        pgt_url: None,
        expires_in_seconds: -3600,
    })
    .await
    .expect("should succeed");

    // Validate valid ticket.
    let validated = cas
        .validate_proxy_ticket(&valid_pt_id, &service_url)
        .await
        .expect("should succeed")
        .expect("should return Some");
    assert_eq!(validated.proxy_ticket_id, valid_pt_id);
    assert!(!validated.is_valid, "should be consumed after validation");

    // Validate expired ticket.
    let expired = cas.validate_proxy_ticket(&expired_pt_id, &service_url).await.expect("should succeed");
    assert!(expired.is_none(), "expired proxy ticket should return None");

    // Validate with wrong service_url.
    let wrong_url = cas.validate_proxy_ticket(&valid_pt_id, "https://wrong.example.com").await.expect("should succeed");
    assert!(wrong_url.is_none(), "wrong service_url should return None");

    cleanup_with_suffix(&pool, &suffix).await;
}

// ---- CAS PGT tests ----

#[tokio::test]
async fn test_pgt_create_and_retrieve() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@testuser-{suffix}:example.com");
    let pgt_id = format!("pgt_{suffix}");
    let service_url = format!("https://svc-{suffix}.example.com");

    cleanup_with_suffix(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let cas = storage(&pool);
    let pgt = cas
        .create_pgt(CreatePgtRequest {
            pgt_id: pgt_id.clone(),
            user_id: user_id.clone(),
            service_url: service_url.clone(),
            iou: Some(format!("iou_{suffix}")),
            expires_in_seconds: 3600,
        })
        .await
        .expect("should succeed");

    assert_eq!(pgt.pgt_id, pgt_id);
    assert_eq!(pgt.user_id, user_id);
    assert!(pgt.is_valid);

    // Retrieve by pgt_id.
    let found = cas.get_pgt(&pgt_id).await.expect("should succeed").expect("should find PGT");
    assert_eq!(found.pgt_id, pgt_id);

    // Non-existent PGT should return None.
    let not_found = cas.get_pgt("nonexistent_pgt_id").await.expect("should succeed");
    assert!(not_found.is_none());

    cleanup_with_suffix(&pool, &suffix).await;
}

#[tokio::test]
async fn test_pgt_by_iou() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@testuser-{suffix}:example.com");
    let pgt_id = format!("pgt_{suffix}");
    let iou = format!("iou_{suffix}");
    let service_url = format!("https://svc-{suffix}.example.com");

    cleanup_with_suffix(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let cas = storage(&pool);
    cas.create_pgt(CreatePgtRequest {
        pgt_id: pgt_id.clone(),
        user_id: user_id.clone(),
        service_url: service_url.clone(),
        iou: Some(iou.clone()),
        expires_in_seconds: 3600,
    })
    .await
    .expect("should succeed");

    // Retrieve by IOU.
    let found = cas.get_pgt_by_iou(&iou).await.expect("should succeed").expect("should find PGT by IOU");
    assert_eq!(found.pgt_id, pgt_id);
    assert_eq!(found.iou.unwrap(), iou);

    // Non-existent IOU should return None.
    let not_found = cas.get_pgt_by_iou("nonexistent_iou").await.expect("should succeed");
    assert!(not_found.is_none());

    cleanup_with_suffix(&pool, &suffix).await;
}

// ---- CAS registered service tests ----

#[tokio::test]
async fn test_register_service() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let service_id = format!("svc_{suffix}");

    cleanup_with_suffix(&pool, &suffix).await;

    let cas = storage(&pool);
    let service = cas
        .register_service(RegisterServiceRequest {
            service_id: service_id.clone(),
            name: format!("Test Service {suffix}"),
            description: Some("A test service".to_string()),
            service_url_pattern: format!(".*svc-{suffix}.*"),
            allowed_attributes: Some(vec!["email".to_string(), "name".to_string()]),
            allowed_proxy_callbacks: Some(vec!["https://cb-{suffix}.example.com".to_string()]),
            is_require_secure: Some(false),
            is_single_logout: Some(true),
        })
        .await
        .expect("should succeed");

    assert_eq!(service.service_id, service_id);
    assert!(service.name.contains(&suffix));
    assert!(service.is_single_logout);
    assert!(!service.is_require_secure);
    assert!(service.is_enabled);

    cleanup_with_suffix(&pool, &suffix).await;
}

#[tokio::test]
async fn test_get_service() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let service_id = format!("svc_{suffix}");

    cleanup_with_suffix(&pool, &suffix).await;

    let cas = storage(&pool);
    cas.register_service(RegisterServiceRequest {
        service_id: service_id.clone(),
        name: format!("Test Service {suffix}"),
        description: None,
        service_url_pattern: format!(".*svc-{suffix}.*"),
        allowed_attributes: None,
        allowed_proxy_callbacks: None,
        is_require_secure: None,
        is_single_logout: None,
    })
    .await
    .expect("should succeed");

    // Get by service_id.
    let found = cas.get_service(&service_id).await.expect("should succeed").expect("should find service");
    assert_eq!(found.service_id, service_id);

    // Non-existent service.
    let not_found = cas.get_service("nonexistent_svc_id").await.expect("should succeed");
    assert!(not_found.is_none());

    cleanup_with_suffix(&pool, &suffix).await;
}

#[tokio::test]
async fn test_get_service_by_url() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let service_id = format!("svc_{suffix}");
    let service_url = format!("https://svc-{suffix}.example.com");

    cleanup_with_suffix(&pool, &suffix).await;

    let cas = storage(&pool);
    // Register with a regex pattern that matches the test URL.
    cas.register_service(RegisterServiceRequest {
        service_id: service_id.clone(),
        name: format!("Test Service {suffix}"),
        description: None,
        service_url_pattern: format!(".*svc-{suffix}.*"),
        allowed_attributes: None,
        allowed_proxy_callbacks: None,
        is_require_secure: None,
        is_single_logout: None,
    })
    .await
    .expect("should succeed");

    // Find by matching URL.
    let found = cas
        .get_service_by_url(&service_url)
        .await
        .expect("should succeed")
        .expect("should find service by URL pattern");
    assert_eq!(found.service_id, service_id);

    // Non-matching URL.
    let not_found = cas.get_service_by_url("https://unrelated.example.com").await.expect("should succeed");
    assert!(not_found.is_none());

    cleanup_with_suffix(&pool, &suffix).await;
}

#[tokio::test]
async fn test_list_services() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let service_id = format!("svc_{suffix}");

    cleanup_with_suffix(&pool, &suffix).await;

    let cas = storage(&pool);
    cas.register_service(RegisterServiceRequest {
        service_id: service_id.clone(),
        name: format!("Test Service {suffix}"),
        description: None,
        service_url_pattern: format!(".*svc-{suffix}.*"),
        allowed_attributes: None,
        allowed_proxy_callbacks: None,
        is_require_secure: None,
        is_single_logout: None,
    })
    .await
    .expect("should succeed");

    let services = cas.list_services().await.expect("should succeed");
    // The list contains all services (including pre-existing); verify ours is present.
    assert!(services.iter().any(|s| s.service_id == service_id), "registered service should appear in list");

    cleanup_with_suffix(&pool, &suffix).await;
}

#[tokio::test]
async fn test_delete_service() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let service_id = format!("svc_{suffix}");

    cleanup_with_suffix(&pool, &suffix).await;

    let cas = storage(&pool);
    cas.register_service(RegisterServiceRequest {
        service_id: service_id.clone(),
        name: format!("Test Service {suffix}"),
        description: None,
        service_url_pattern: format!(".*svc-{suffix}.*"),
        allowed_attributes: None,
        allowed_proxy_callbacks: None,
        is_require_secure: None,
        is_single_logout: None,
    })
    .await
    .expect("should succeed");

    // Delete it.
    let deleted = cas.delete_service(&service_id).await.expect("should succeed");
    assert!(deleted, "should return true when service is deleted");

    // Verify it's gone.
    let after = cas.get_service(&service_id).await.expect("should succeed");
    assert!(after.is_none(), "service should be gone after delete");

    // Delete again — should return false.
    let again = cas.delete_service(&service_id).await.expect("should succeed");
    assert!(!again, "should return false for already-deleted service");

    // Delete non-existent.
    let never = cas.delete_service("nonexistent_svc").await.expect("should succeed");
    assert!(!never);

    cleanup_with_suffix(&pool, &suffix).await;
}

// ---- CAS user attributes tests ----

#[tokio::test]
async fn test_user_attributes() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@testuser-{suffix}:example.com");

    cleanup_with_suffix(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let cas = storage(&pool);

    // Set an attribute.
    let attr = cas.set_user_attribute(&user_id, "email", "test@example.com").await.expect("should succeed");
    assert_eq!(attr.user_id, user_id);
    assert_eq!(attr.attribute_name, "email");
    assert_eq!(attr.attribute_value, "test@example.com");

    // Set another attribute.
    cas.set_user_attribute(&user_id, "display_name", "Test User").await.expect("should succeed");

    // Get all attributes.
    let attrs = cas.get_user_attributes(&user_id).await.expect("should succeed");
    assert_eq!(attrs.len(), 2);
    assert!(attrs.iter().any(|a| a.attribute_name == "email"));
    assert!(attrs.iter().any(|a| a.attribute_name == "display_name"));

    // Upsert existing attribute (update value).
    let updated = cas.set_user_attribute(&user_id, "email", "updated@example.com").await.expect("should succeed");
    assert_eq!(updated.attribute_value, "updated@example.com");

    // Verify update persisted.
    let attrs_after = cas.get_user_attributes(&user_id).await.expect("should succeed");
    let email_attr = attrs_after.iter().find(|a| a.attribute_name == "email").expect("email attribute should exist");
    assert_eq!(email_attr.attribute_value, "updated@example.com");

    // Still only 2 attributes after upsert (not a new row).
    assert_eq!(attrs_after.len(), 2);

    // Get attributes for user with no attributes.
    let empty = cas.get_user_attributes("@noattr:example.com").await.expect("should succeed");
    assert!(empty.is_empty());

    cleanup_with_suffix(&pool, &suffix).await;
}

// ---- CAS SLO session tests ----

#[tokio::test]
async fn test_slo_session_create_and_mark() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@testuser-{suffix}:example.com");
    let session_id = format!("slo_session_{suffix}");
    let service_url = format!("https://svc-{suffix}.example.com");

    cleanup_with_suffix(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let cas = storage(&pool);

    // Create SLO session.
    let session =
        cas.create_slo_session(&session_id, &user_id, &service_url, Some("ticket_ref")).await.expect("should succeed");
    assert_eq!(session.session_id, session_id);
    assert_eq!(session.user_id, user_id);
    assert_eq!(session.service_url, service_url);
    assert!(session.logout_sent_ts.is_none());

    // Mark as sent.
    let marked = cas.mark_slo_sent(&session_id).await.expect("should succeed");
    assert!(marked, "should return true for first mark_slo_sent");

    // Marking again should return false (logout_sent_at is already set).
    let again = cas.mark_slo_sent(&session_id).await.expect("should succeed");
    assert!(!again, "second mark_slo_sent should return false");

    // Mark non-existent session.
    let never = cas.mark_slo_sent("nonexistent_session").await.expect("should succeed");
    assert!(!never);

    cleanup_with_suffix(&pool, &suffix).await;
}

#[tokio::test]
async fn test_get_active_slo_sessions() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@testuser-{suffix}:example.com");
    let session_id_1 = format!("active_slo_{suffix}_1");
    let session_id_2 = format!("active_slo_{suffix}_2");
    let service_url = format!("https://svc-{suffix}.example.com");

    cleanup_with_suffix(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let cas = storage(&pool);

    // Create two SLO sessions.
    cas.create_slo_session(&session_id_1, &user_id, &service_url, None).await.expect("should succeed");
    cas.create_slo_session(&session_id_2, &user_id, &service_url, None).await.expect("should succeed");

    // Both should appear as active.
    let active = cas.get_active_slo_sessions(&user_id).await.expect("should succeed");
    assert_eq!(active.len(), 2);
    assert!(active.iter().any(|s| s.session_id == session_id_1));
    assert!(active.iter().any(|s| s.session_id == session_id_2));

    // Mark one as sent — it should no longer be active.
    cas.mark_slo_sent(&session_id_1).await.expect("should succeed");

    let active_after = cas.get_active_slo_sessions(&user_id).await.expect("should succeed");
    assert_eq!(active_after.len(), 1);
    assert_eq!(active_after[0].session_id, session_id_2);

    // User with no sessions.
    let empty = cas.get_active_slo_sessions("@nosessions:example.com").await.expect("should succeed");
    assert!(empty.is_empty());

    cleanup_with_suffix(&pool, &suffix).await;
}

// ---- End-to-end lifecycle tests ----

#[tokio::test]
async fn test_full_ticket_lifecycle() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@testuser-{suffix}:example.com");
    let ticket_id = format!("lifecycle_ticket_{suffix}");
    let service_url = format!("https://svc-{suffix}.example.com");

    cleanup_with_suffix(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let cas = storage(&pool);

    // Create.
    let created = cas
        .create_ticket(CreateTicketRequest {
            ticket_id: ticket_id.clone(),
            user_id: user_id.clone(),
            service_url: service_url.clone(),
            expires_in_seconds: 3600,
        })
        .await
        .expect("should succeed");
    assert!(created.is_valid);

    // Get — verify it exists.
    cas.get_ticket(&ticket_id).await.expect("should succeed").expect("should exist");

    // Validate — consume it.
    let validated =
        cas.validate_ticket(&ticket_id, &service_url).await.expect("should succeed").expect("should validate");
    assert!(!validated.is_valid);

    // Get — still exists in DB but is_valid = false.
    let after_validate = cas.get_ticket(&ticket_id).await.expect("should succeed").expect("should still exist in DB");
    assert!(!after_validate.is_valid);

    // Delete — remove it.
    let deleted = cas.delete_ticket(&ticket_id).await.expect("should succeed");
    assert!(deleted);

    // Get — should be gone.
    let after_delete = cas.get_ticket(&ticket_id).await.expect("should succeed");
    assert!(after_delete.is_none());

    cleanup_with_suffix(&pool, &suffix).await;
}

#[tokio::test]
async fn test_full_service_lifecycle() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let service_id = format!("lifecycle_svc_{suffix}");

    cleanup_with_suffix(&pool, &suffix).await;

    let cas = storage(&pool);

    // Register.
    let registered = cas
        .register_service(RegisterServiceRequest {
            service_id: service_id.clone(),
            name: format!("Lifecycle Test {suffix}"),
            description: Some("Lifecycle test service".to_string()),
            service_url_pattern: format!(".*lifecycle-{suffix}.*"),
            allowed_attributes: Some(vec!["email".to_string()]),
            allowed_proxy_callbacks: None,
            is_require_secure: Some(false),
            is_single_logout: Some(false),
        })
        .await
        .expect("should succeed");
    assert_eq!(registered.service_id, service_id);

    // Get by id.
    cas.get_service(&service_id).await.expect("should succeed").expect("should find by id");

    // Get by URL.
    let service_url = format!("https://lifecycle-{suffix}.example.com");
    cas.get_service_by_url(&service_url).await.expect("should succeed").expect("should find by URL");

    // List — should include this service.
    let list = cas.list_services().await.expect("should succeed");
    assert!(list.iter().any(|s| s.service_id == service_id));

    // Delete.
    let deleted = cas.delete_service(&service_id).await.expect("should succeed");
    assert!(deleted);

    // Verify gone.
    assert!(cas.get_service(&service_id).await.expect("should succeed").is_none());

    cleanup_with_suffix(&pool, &suffix).await;
}

#[tokio::test]
async fn test_slo_session_lifecycle() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@testuser-{suffix}:example.com");
    let session_id = format!("slo_lifecycle_{suffix}");
    let service_url = format!("https://svc-{suffix}.example.com");

    cleanup_with_suffix(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let cas = storage(&pool);

    // Create without optional ticket_id.
    let created = cas.create_slo_session(&session_id, &user_id, &service_url, None).await.expect("should succeed");
    assert_eq!(created.session_id, session_id);
    assert!(created.logout_sent_ts.is_none());
    assert!(created.ticket_id.is_none());

    // Should appear in active sessions.
    let active = cas.get_active_slo_sessions(&user_id).await.expect("should succeed");
    assert!(active.iter().any(|s| s.session_id == session_id));

    // Mark as sent.
    let marked = cas.mark_slo_sent(&session_id).await.expect("should succeed");
    assert!(marked);

    // Should no longer be active.
    let active_after = cas.get_active_slo_sessions(&user_id).await.expect("should succeed");
    assert!(!active_after.iter().any(|s| s.session_id == session_id));

    cleanup_with_suffix(&pool, &suffix).await;
}

#[tokio::test]
async fn test_pgt_full_lifecycle() {
    let pool = test_pool().await;
    let suffix = uuid::Uuid::new_v4().to_string().replace('-', "");
    let user_id = format!("@testuser-{suffix}:example.com");
    let pgt_id = format!("pgt_lifecycle_{suffix}");
    let iou = format!("iou_lifecycle_{suffix}");
    let service_url = format!("https://svc-{suffix}.example.com");

    cleanup_with_suffix(&pool, &suffix).await;
    ensure_test_user(&pool, &user_id).await;

    let cas = storage(&pool);

    // Create PGT with IOU.
    let created = cas
        .create_pgt(CreatePgtRequest {
            pgt_id: pgt_id.clone(),
            user_id: user_id.clone(),
            service_url: service_url.clone(),
            iou: Some(iou.clone()),
            expires_in_seconds: 3600,
        })
        .await
        .expect("should succeed");
    assert_eq!(created.pgt_id, pgt_id);
    assert_eq!(created.iou.as_deref(), Some(iou.as_str()));
    assert!(created.is_valid);

    // Retrieve by pgt_id.
    let by_id = cas.get_pgt(&pgt_id).await.expect("should succeed").expect("should find by pgt_id");
    assert_eq!(by_id.pgt_id, pgt_id);

    // Retrieve by IOU.
    let by_iou = cas.get_pgt_by_iou(&iou).await.expect("should succeed").expect("should find by IOU");
    assert_eq!(by_iou.pgt_id, pgt_id);

    // Create a proxy ticket using the PGT (common CAS flow).
    let proxy_ticket_id = format!("pt_from_pgt_{suffix}");
    cas.create_proxy_ticket(CreateProxyTicketRequest {
        proxy_ticket_id: proxy_ticket_id.clone(),
        user_id: user_id.clone(),
        service_url: service_url.clone(),
        pgt_url: Some(pgt_id.clone()),
        expires_in_seconds: 3600,
    })
    .await
    .expect("should succeed");

    // Validate the proxy ticket.
    let validated = cas
        .validate_proxy_ticket(&proxy_ticket_id, &service_url)
        .await
        .expect("should succeed")
        .expect("should validate proxy ticket");
    assert_eq!(validated.user_id, user_id);

    cleanup_with_suffix(&pool, &suffix).await;
}
