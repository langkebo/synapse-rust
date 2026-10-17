//! Burn-after-read is a per-reader time registry, not a room-wide deleter.
//!
//! A room event has exactly one copy shared by every member, so redacting it
//! when *one* member's timer expires would delete the message for everyone
//! (the "one reader deletes for all" escalation). The server therefore only
//! records the burn (pending row + audit log) and never touches the event;
//! each reader's client performs the local deletion. Physical deletion stays
//! with the retention service, which is a room-wide policy.
//!
//! These tests run against the real (isolated, migrated) schema and the real
//! `BurnAfterReadStorage` implementation.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use sqlx::Row;
use std::sync::Arc;
use synapse_services::burn_after_read_service::BurnAfterReadService;
use synapse_storage::burn_after_read::{BurnAfterReadStorage, BurnAfterReadStoreApi};
use tower::ServiceExt;

/// Register a client user, returning `(access_token, user_id)`.
async fn burn_register_user(app: &axum::Router, prefix: &str) -> (String, String) {
    let username = format!("{prefix}_{}", rand::random::<u32>());
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "username": username,
                "password": "Password123!",
                "auth": { "type": "m.login.dummy" }
            })
            .to_string(),
        ))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "registration must succeed");
    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    (
        json["access_token"].as_str().expect("access_token").to_string(),
        json["user_id"].as_str().expect("user_id").to_string(),
    )
}

/// Create a room owned by `token` and return its id.
async fn burn_create_room(app: &axum::Router, token: &str, name: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "name": name, "preset": "private_chat" }).to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "createRoom must succeed");
    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["room_id"].as_str().expect("room_id").to_string()
}

/// Send a plain message and return its event id.
async fn burn_send_message(app: &axum::Router, token: &str, room_id: &str) -> String {
    let txn = format!("txn_{}", rand::random::<u32>());
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/rooms/{room_id}/send/m.room.message/{txn}"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "msgtype": "m.text", "body": "burn me" }).to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "send must succeed");
    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["event_id"].as_str().expect("event_id").to_string()
}

/// Build a real `BurnAfterReadService` over the isolated test pool.
fn burn_service(pool: &Arc<sqlx::PgPool>) -> BurnAfterReadService {
    let storage: Arc<dyn BurnAfterReadStoreApi> = Arc::new(BurnAfterReadStorage::new(pool));
    BurnAfterReadService::new(storage)
}

/// `(is_redacted, redacted_by)` for one event.
async fn burn_redaction_state(pool: &sqlx::PgPool, event_id: &str) -> (bool, Option<String>) {
    let row = sqlx::query("SELECT is_redacted, redacted_by FROM events WHERE event_id = $1")
        .bind(event_id)
        .fetch_one(pool)
        .await
        .expect("the event must exist");
    (row.get::<bool, _>("is_redacted"), row.get::<Option<String>, _>("redacted_by"))
}

/// Number of audit rows recorded for one `(user, event)` pair.
async fn burn_log_count(pool: &sqlx::PgPool, user_id: &str, event_id: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM burn_after_read_log WHERE user_id = $1 AND event_id = $2")
        .bind(user_id)
        .bind(event_id)
        .fetch_one(pool)
        .await
        .expect("counting the burn log must succeed")
}

/// The sweep must mark the row processed and write the audit entry, but leave
/// the shared room event untouched.
#[tokio::test]
async fn burn_expired_sweep_logs_and_marks_without_redacting_the_event() {
    let Some((app, pool, _cache)) = super::setup_fresh_test_app_with_pool().await else {
        return;
    };
    let (token, user_id) = burn_register_user(&app, "burnsweep").await;
    let room_id = burn_create_room(&app, &token, "burn sweep").await;
    let event_id = burn_send_message(&app, &token, &room_id).await;

    let service = burn_service(&pool);
    // Negative delay ⇒ `delete_ts` is already in the past, so the row is expired
    // on the first sweep.
    service.schedule_burn(&user_id, &room_id, &event_id, -60_000).await.expect("schedule_burn must succeed");

    let expired = service.process_expired_burns().await.expect("the sweep must not fail");
    assert_eq!(expired.len(), 1, "the expired burn must be reported to the caller");
    assert_eq!(expired[0].user_id, user_id);
    assert_eq!(expired[0].event_id, event_id);

    let (is_redacted, redacted_by) = burn_redaction_state(&pool, &event_id).await;
    assert!(!is_redacted, "one reader's burn must not redact the event for the whole room");
    assert_eq!(redacted_by, None, "nothing may point at a redaction event");

    assert_eq!(burn_log_count(&pool, &user_id, &event_id).await, 1, "the burn must be audited");
}

/// The single-message path has the same contract: audit only, no redaction.
#[tokio::test]
async fn burn_delete_burned_message_logs_without_redacting_the_event() {
    let Some((app, pool, _cache)) = super::setup_fresh_test_app_with_pool().await else {
        return;
    };
    let (token, user_id) = burn_register_user(&app, "burnsingle").await;
    let room_id = burn_create_room(&app, &token, "burn single").await;
    let event_id = burn_send_message(&app, &token, &room_id).await;

    let service = burn_service(&pool);
    service
        .delete_burned_message(&user_id, &room_id, &event_id)
        .await
        .expect("delete_burned_message only writes the audit row, so it must succeed");

    let (is_redacted, redacted_by) = burn_redaction_state(&pool, &event_id).await;
    assert!(!is_redacted, "a per-user burn must not redact the shared room event");
    assert_eq!(redacted_by, None, "nothing may point at a redaction event");

    assert_eq!(burn_log_count(&pool, &user_id, &event_id).await, 1, "the burn must be audited");
}
