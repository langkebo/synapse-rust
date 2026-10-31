//! P1-2.3: `is_private` contract on `POST /_matrix/vendor/v1/friends/dm/{user_id}`.
//!
//! `is_private: true` must, on **newly created** rooms:
//! 1. set `m.room.history_visibility` to `invited`;
//! 2. write the `com.hula.privacy` flag (`is_private: true`);
//! 3. install the default 7-day retention policy.
//!
//! None of the above applies to rooms that already existed (`created: false`),
//! so re-calling the endpoint cannot silently rewrite an existing DM's policy.

use axum::body::Body;
use hyper::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

const DEFAULT_PRIVATE_RETENTION_MS: i64 = 7 * 24 * 60 * 60 * 1000;

async fn register_user(app: &axum::Router, username: &str) -> (String, String) {
    let suffix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "username": format!("{username}_{suffix}"),
                "password": "Password123!",
                "auth": { "type": "m.login.dummy" }
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1 << 20).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    (json["access_token"].as_str().unwrap().to_string(), json["user_id"].as_str().unwrap().to_string())
}

async fn create_friend_dm(app: &axum::Router, token: &str, peer: &str, body: Option<Value>) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/vendor/v1/friends/dm/{peer}"))
        .header("Authorization", format!("Bearer {token}"));
    if body.is_some() {
        builder = builder.header("Content-Type", "application/json");
    }
    let request = builder.body(body.map_or_else(Body::empty, |body| Body::from(body.to_string()))).unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

/// Read a state event's content; `None` when the event does not exist.
///
/// Uses the full-state endpoint with the MSC4497 exact `type` filter rather than
/// `/state/{type}`: the latter normalizes non-`m.` event types into
/// `m.room.<type>`, so custom types such as `com.hula.privacy` are unreachable
/// through it.
async fn state_content(app: &axum::Router, token: &str, room_id: &str, event_type: &str) -> Option<Value> {
    let request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/rooms/{room_id}/state?type={event_type}"))
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    if response.status() != StatusCode::OK {
        return None;
    }
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).ok()?;
    // CS-API: the full-state endpoint answers with a bare array of state events.
    body.as_array()?
        .iter()
        .find(|event| event["type"].as_str() == Some(event_type))
        .map(|event| event["content"].clone())
}

async fn retention_policy_count(pool: &sqlx::PgPool, room_id: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM room_retention_policies WHERE room_id = $1")
        .bind(room_id)
        .fetch_one(pool)
        .await
        .expect("count retention policies")
}

#[tokio::test]
async fn private_dm_installs_privacy_history_and_default_retention() {
    let Some(ctx) = super::TestContext::new().await else {
        return;
    };
    let app = ctx.app.clone();

    let (token_a, _user_a) = register_user(&app, "priv_dm_creator").await;
    let (_token_b, user_b) = register_user(&app, "priv_dm_peer").await;

    let (status, body) = create_friend_dm(&app, &token_a, &user_b, Some(json!({ "is_private": true }))).await;
    assert_eq!(status, StatusCode::OK, "friends/dm(is_private) failed: {body}");
    assert_eq!(body["is_private"], true, "response must echo is_private: {body}");
    assert_eq!(body["created"], true, "first call must create the room: {body}");
    let room_id = body["room_id"].as_str().expect("room_id");

    let history = state_content(&app, &token_a, room_id, "m.room.history_visibility")
        .await
        .expect("m.room.history_visibility must exist");
    assert_eq!(history["history_visibility"], "invited", "private DM history visibility: {history}");

    let privacy =
        state_content(&app, &token_a, room_id, "com.hula.privacy").await.expect("com.hula.privacy must exist");
    assert_eq!(privacy["is_private"], true, "privacy flag content: {privacy}");

    let policy: (Option<i64>, Option<i64>, Option<bool>) = sqlx::query_as(
        "SELECT max_lifetime, min_lifetime, is_expire_on_clients \
         FROM room_retention_policies WHERE room_id = $1",
    )
    .bind(room_id)
    .fetch_one(&*ctx.pool)
    .await
    .expect("private DM must have a default retention policy");
    assert_eq!(policy.0, Some(DEFAULT_PRIVATE_RETENTION_MS), "default max lifetime");
    // `create_room_policy` materializes an absent minimum as 0 (no lower bound),
    // which is the storage layer's existing convention for "no minimum".
    assert_eq!(policy.1, Some(0), "default min lifetime");
    assert_eq!(policy.2, Some(true), "expire on clients");
}

#[tokio::test]
async fn non_private_dm_keeps_defaults_and_has_no_retention_policy() {
    let Some(ctx) = super::TestContext::new().await else {
        return;
    };
    let app = ctx.app.clone();

    let (token_a, _user_a) = register_user(&app, "plain_dm_creator").await;
    let (_token_b, user_b) = register_user(&app, "plain_dm_peer").await;

    let (status, body) = create_friend_dm(&app, &token_a, &user_b, Some(json!({}))).await;
    assert_eq!(status, StatusCode::OK, "friends/dm failed: {body}");
    assert_eq!(body["created"], true, "{body}");
    assert_eq!(body["is_private"], false, "{body}");
    let room_id = body["room_id"].as_str().expect("room_id");

    let history = state_content(&app, &token_a, room_id, "m.room.history_visibility")
        .await
        .expect("m.room.history_visibility must exist");
    assert_ne!(history["history_visibility"], "invited", "non-private DM keeps preset defaults: {history}");

    assert!(
        state_content(&app, &token_a, room_id, "com.hula.privacy").await.is_none(),
        "non-private DM must not carry com.hula.privacy"
    );

    assert_eq!(
        retention_policy_count(&ctx.pool, room_id).await,
        0,
        "non-private DM must not get a default retention policy"
    );
}

#[tokio::test]
async fn private_flag_on_existing_dm_does_not_rewrite_retention() {
    let Some(ctx) = super::TestContext::new().await else {
        return;
    };
    let app = ctx.app.clone();

    let (token_a, _user_a) = register_user(&app, "reuse_dm_creator").await;
    let (_token_b, user_b) = register_user(&app, "reuse_dm_peer").await;

    // First call creates a plain DM (no retention policy, shared history).
    let (status, created) = create_friend_dm(&app, &token_a, &user_b, None).await;
    assert_eq!(status, StatusCode::OK, "first friends/dm failed: {created}");
    assert_eq!(created["created"], true, "{created}");
    let room_id = created["room_id"].as_str().expect("room_id").to_string();

    // Re-calling with is_private must reuse the room and leave it untouched.
    let (status, reused) = create_friend_dm(&app, &token_a, &user_b, Some(json!({ "is_private": true }))).await;
    assert_eq!(status, StatusCode::OK, "second friends/dm failed: {reused}");
    assert_eq!(reused["created"], false, "second call must reuse the room: {reused}");
    assert_eq!(reused["room_id"], room_id);

    let history = state_content(&app, &token_a, &room_id, "m.room.history_visibility")
        .await
        .expect("m.room.history_visibility must exist");
    assert_ne!(history["history_visibility"], "invited", "existing room history must not be rewritten: {history}");
    assert!(
        state_content(&app, &token_a, &room_id, "com.hula.privacy").await.is_none(),
        "existing room must not gain com.hula.privacy"
    );

    assert_eq!(retention_policy_count(&ctx.pool, &room_id).await, 0, "existing room must not gain a retention policy");
}
