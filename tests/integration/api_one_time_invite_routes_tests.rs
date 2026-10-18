//! B-9: one-time invite links
//! (`POST /_matrix/vendor/v1/invites/one-time` + `POST /_matrix/vendor/v1/invites/claim`).
//!
//! The token is minted for the creator and consumed exactly once by the
//! claimer; the room is created at claim time with the claimer invited.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

const CREATE_PATH: &str = "/_matrix/vendor/v1/invites/one-time";
const CLAIM_PATH: &str = "/_matrix/vendor/v1/invites/claim";

async fn register_user(app: &axum::Router, username: &str) -> (String, String) {
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
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1 << 20).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    (json["access_token"].as_str().unwrap().to_string(), json["user_id"].as_str().unwrap().to_string())
}

async fn post_json(app: &axum::Router, uri: &str, token: Option<&str>, body: Option<Value>) -> (StatusCode, Value) {
    let mut builder = Request::builder().method("POST").uri(uri);
    // An absent body must also omit the JSON content type: Axum's optional
    // extractor only yields `None` when the content type is missing — an empty
    // body *with* `application/json` is rejected as a syntax error.
    if body.is_some() {
        builder = builder.header("Content-Type", "application/json");
    }
    if let Some(token) = token {
        builder = builder.header("Authorization", format!("Bearer {token}"));
    }
    let request = builder.body(body.map_or_else(Body::empty, |body| Body::from(body.to_string()))).unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

#[tokio::test]
async fn claim_creates_room_and_invites_claimer_once() {
    let Some(ctx) = super::TestContext::new().await else {
        return;
    };
    let app = ctx.app.clone();
    let (owner_token, _owner_id) = register_user(&app, "oti_owner").await;
    let (claimer_token, claimer_id) = register_user(&app, "oti_claimer").await;

    let (status, created) = post_json(&app, CREATE_PATH, Some(&owner_token), None).await;
    assert_eq!(status, StatusCode::OK, "create failed: {created}");
    let invite_token = created["token"].as_str().expect("token").to_string();
    assert!(created["expires_at"].is_i64(), "expires_at missing: {created}");
    assert!(
        created["deep_link"].as_str().unwrap_or_default().contains(&invite_token),
        "deep_link must carry the token: {created}"
    );

    // Only the hash may be persisted: the plaintext token must not appear in
    // the table at all.
    let plaintext_rows: i64 = sqlx::query_scalar("SELECT count(*) FROM one_time_invites WHERE token_hash = $1")
        .bind(&invite_token)
        .fetch_one(&*ctx.pool)
        .await
        .expect("count plaintext rows");
    assert_eq!(plaintext_rows, 0, "plaintext token must never be stored");

    let (status, claimed) =
        post_json(&app, CLAIM_PATH, Some(&claimer_token), Some(json!({ "token": invite_token }))).await;
    assert_eq!(status, StatusCode::OK, "claim failed: {claimed}");
    let room_id = claimed["room_id"].as_str().expect("room_id").to_string();

    // The claimer is invited to the freshly created room.
    let member_uri =
        format!("/_matrix/client/v3/rooms/{}/state/m.room.member/{}", urlencoding(&room_id), urlencoding(&claimer_id));
    let request = Request::builder()
        .method("GET")
        .uri(member_uri)
        .header("Authorization", format!("Bearer {owner_token}"))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1 << 20).await.unwrap();
    let membership: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(membership["membership"], "invite", "claimer must be invited: {membership}");

    // Replay: the same token must be refused.
    let (status, replay) =
        post_json(&app, CLAIM_PATH, Some(&claimer_token), Some(json!({ "token": invite_token }))).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "replay must be rejected: {replay}");
    assert!(replay["error"].as_str().unwrap_or_default().contains("already been used"), "{replay}");
}

/// Percent-encode the characters that appear in MXIDs / room ids inside a path.
fn urlencoding(value: &str) -> String {
    value.replace('%', "%25").replace('#', "%23").replace('?', "%3F").replace('/', "%2F")
}

#[tokio::test]
async fn expired_token_is_rejected() {
    let Some(ctx) = super::TestContext::new().await else {
        return;
    };
    let app = ctx.app.clone();
    let (owner_token, _) = register_user(&app, "oti_expired_owner").await;
    let (claimer_token, _) = register_user(&app, "oti_expired_claimer").await;

    let (status, created) = post_json(&app, CREATE_PATH, Some(&owner_token), None).await;
    assert_eq!(status, StatusCode::OK, "create failed: {created}");
    let invite_token = created["token"].as_str().expect("token").to_string();

    // The API clamps TTLs to a one-minute floor, so expire the row directly.
    // A fresh isolated schema holds exactly this one invite.
    sqlx::query("UPDATE one_time_invites SET expires_at = 0").execute(&*ctx.pool).await.expect("expire invite");

    let (status, body) =
        post_json(&app, CLAIM_PATH, Some(&claimer_token), Some(json!({ "token": invite_token }))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "expired token must be refused: {body}");
    assert!(body["error"].as_str().unwrap_or_default().contains("expired"), "{body}");
}

#[tokio::test]
async fn unknown_token_is_not_found() {
    let Some(ctx) = super::TestContext::new().await else {
        return;
    };
    let app = ctx.app.clone();
    let (claimer_token, _) = register_user(&app, "oti_unknown_claimer").await;

    let (status, body) =
        post_json(&app, CLAIM_PATH, Some(&claimer_token), Some(json!({ "token": "not-a-real-token" }))).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "unknown token must 404: {body}");
}

#[tokio::test]
async fn endpoints_require_authentication() {
    let Some(ctx) = super::TestContext::new().await else {
        return;
    };
    let app = ctx.app.clone();

    let (create_status, _) = post_json(&app, CREATE_PATH, None, None).await;
    assert_eq!(create_status, StatusCode::UNAUTHORIZED);

    let (claim_status, _) = post_json(&app, CLAIM_PATH, None, Some(json!({ "token": "whatever" }))).await;
    assert_eq!(claim_status, StatusCode::UNAUTHORIZED);
}
