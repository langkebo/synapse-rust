//! Burn-after-read redaction must record the burn's own `m.room.redaction`
//! event, not the burning user's id.
//!
//! `events.redacted_by` is a self-referential FK to `events.event_id`
//! (`fk_events_redacted_by`). The burn-after-read service *does* persist a real
//! `m.room.redaction` event, so the correct value is that event's id — but the
//! service used to call `redact_event_content(..., Some(user_id))` **before**
//! creating the redaction event, so the UPDATE always failed with a foreign-key
//! violation and the burned message's content was never stripped.
//!
//! These tests run against the real (isolated, migrated) schema, where the FK
//! is live, and against the real `BurnAfterReadStorage` + `EventStorage`
//! implementations.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use sqlx::Row;
use std::sync::Arc;
use synapse_services::burn_after_read_service::BurnAfterReadService;
use synapse_storage::burn_after_read::{BurnAfterReadStorage, BurnAfterReadStoreApi};
use synapse_storage::event::{EventReader, EventStorage, EventWriter};
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
    let writer: Arc<dyn EventWriter> = Arc::new(EventStorage::new(pool, "localhost".to_string()));
    let reader: Arc<dyn EventReader> = Arc::new(EventStorage::new(pool, "localhost".to_string()));
    BurnAfterReadService::new(
        storage,
        writer,
        reader,
        Arc::new(synapse_services::test_mocks::FakeEventAdmissionGate::new()),
        "localhost".to_string(),
    )
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

/// The service's own redaction event id in `room_id`.
async fn burn_redaction_event_id(pool: &sqlx::PgPool, room_id: &str) -> String {
    sqlx::query_scalar(
        "SELECT event_id FROM events WHERE room_id = $1 AND event_type = 'm.room.redaction' \
         ORDER BY origin_server_ts DESC, event_id DESC LIMIT 1",
    )
    .bind(room_id)
    .fetch_one(pool)
    .await
    .expect("the burn service must have persisted an m.room.redaction event")
}

/// The batch sweep (`process_expired_burns`) must persist the redaction event
/// first, then point `redacted_by` at it — not at `row.user_id`.
#[tokio::test]
async fn burn_expired_sweep_records_its_redaction_event_id() {
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
    assert_eq!(expired.len(), 1, "the expired burn must be processed, not skipped on a failed redact");

    let redaction_event_id = burn_redaction_event_id(&pool, &room_id).await;
    let (is_redacted, redacted_by) = burn_redaction_state(&pool, &event_id).await;
    assert!(is_redacted, "the burned message's content must actually be redacted");
    assert_eq!(
        redacted_by.as_deref(),
        Some(redaction_event_id.as_str()),
        "redacted_by must be the burn's own m.room.redaction event id, not the burning user"
    );
}

/// The single-message path (`delete_burned_message`) has the same contract.
#[tokio::test]
async fn burn_delete_burned_message_records_its_redaction_event_id() {
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
        .expect("delete_burned_message must succeed (log_burned_event is the only fallible step)");

    let redaction_event_id = burn_redaction_event_id(&pool, &room_id).await;
    let (is_redacted, redacted_by) = burn_redaction_state(&pool, &event_id).await;
    assert!(is_redacted, "the burned message's content must actually be redacted");
    assert_eq!(
        redacted_by.as_deref(),
        Some(redaction_event_id.as_str()),
        "redacted_by must be the burn's own m.room.redaction event id, not the burning user"
    );
}
