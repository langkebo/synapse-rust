//! MSC3912 ("Redaction of related events") regression tests.
//!
//! Two gaps in the client redaction cascade are pinned here:
//!
//! * **Per-event authorization (security).** `with_rel_types` used to redact
//!   *every* related event in the room, so an unprivileged actor could wipe
//!   another user's `m.annotation` by redacting their own message. The cascade
//!   must reuse the ordinary `can_redact_event` check for each child.
//! * **Empty `with_rel_types`.** An empty list is equivalent to not cascading;
//!   it used to be rejected with `400 M_BAD_JSON`. The target must still be
//!   redacted, and no related event may be touched.
//!
//! The cascade runs in a best-effort background task, so the tests use
//! `wait_until_redacted` on a child that *is* permitted before asserting that a
//! forbidden sibling survived — otherwise "the child survived" could just mean
//! "the background task had not run yet".

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

async fn setup_test_app() -> Option<(axum::Router, Arc<sqlx::PgPool>)> {
    super::setup_fresh_test_app_with_pool().await.map(|(app, pool, _cache)| (app, pool))
}

async fn register_user_with_id(app: &axum::Router, username: &str) -> (String, String) {
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
    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    (
        json["access_token"].as_str().expect("access_token").to_string(),
        json["user_id"].as_str().expect("user_id").to_string(),
    )
}

async fn create_room(app: &axum::Router, token: &str, name: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "name": name }).to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "createRoom must succeed");
    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["room_id"].as_str().expect("room_id").to_string()
}

async fn invite_user(app: &axum::Router, token: &str, room_id: &str, user_id: &str) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/rooms/{room_id}/invite"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "user_id": user_id }).to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "invite must succeed");
}

async fn join_room(app: &axum::Router, token: &str, room_id: &str) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/rooms/{room_id}/join"))
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "join must succeed");
}

fn put_json(uri: String, token: &str, body: &Value) -> Request<Body> {
    Request::builder()
        .method("PUT")
        .uri(uri)
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

/// Send a plain message and return its event id (the cascade target).
async fn send_message(app: &axum::Router, token: &str, room_id: &str) -> String {
    let txn = format!("txn_{}", rand::random::<u32>());
    let request = put_json(
        format!("/_matrix/client/v3/rooms/{room_id}/send/m.room.message/{txn}"),
        token,
        &json!({ "msgtype": "m.text", "body": "hello" }),
    );
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "send must succeed");
    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["event_id"].as_str().expect("event_id").to_string()
}

/// Add an `m.annotation` child event to `target_event_id`.
///
/// Deliberately uses the *generic* send route rather than `/send/m.reaction`:
/// the dedicated reaction compat route (`add_reaction`) only indexes the
/// annotation in the relations tables, so it never creates a row in `events` —
/// and the cascade's `find_related_events_single_layer` query reads
/// `events.content`. The generic route persists the event with its
/// `m.relates_to` relation, which is what the cascade matches on.
async fn send_annotation(app: &axum::Router, token: &str, room_id: &str, target_event_id: &str, key: &str) -> String {
    let txn = format!("txn_annot_{}", rand::random::<u32>());
    let request = put_json(
        format!("/_matrix/client/v3/rooms/{room_id}/send/m.room.message/{txn}"),
        token,
        &json!({
            "msgtype": "m.text",
            "body": key,
            "m.relates_to": {
                "rel_type": "m.annotation",
                "event_id": target_event_id,
                "key": key
            }
        }),
    );
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "annotation send must succeed");
    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["event_id"].as_str().expect("event_id").to_string()
}

/// Add a reaction through the *dedicated* compat route
/// `PUT /rooms/{id}/send/m.reaction/{txn}` (U-20).
async fn add_reaction_via_compat_route(
    app: &axum::Router,
    token: &str,
    room_id: &str,
    target_event_id: &str,
    key: &str,
) -> String {
    let txn = format!("txn_react_{}", rand::random::<u32>());
    let request = put_json(
        format!("/_matrix/client/v3/rooms/{room_id}/send/m.reaction/{txn}"),
        token,
        &json!({
            "body": key,
            "m.relates_to": {
                "rel_type": "m.annotation",
                "event_id": target_event_id
            }
        }),
    );
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "reaction send must succeed");
    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["event_id"].as_str().expect("event_id").to_string()
}

/// Issue a client redaction with the given JSON body.
async fn redact(
    app: &axum::Router,
    token: &str,
    room_id: &str,
    event_id: &str,
    body: &Value,
) -> axum::response::Response {
    let txn = format!("txn_redact_{}", rand::random::<u32>());
    let request = put_json(format!("/_matrix/client/v3/rooms/{room_id}/redact/{event_id}/{txn}"), token, body);
    ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap()
}

async fn is_redacted(pool: &sqlx::PgPool, event_id: &str) -> bool {
    sqlx::query_scalar::<_, Option<bool>>("SELECT is_redacted FROM events WHERE event_id = $1")
        .bind(event_id)
        .fetch_optional(pool)
        .await
        .expect("query is_redacted")
        .flatten()
        .unwrap_or(false)
}

/// Wait (bounded) for a *permitted* child to be redacted — proof that the
/// best-effort background cascade actually ran to completion.
async fn wait_until_redacted(pool: &sqlx::PgPool, event_id: &str) -> bool {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        if is_redacted(pool, event_id).await {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// The three participants of a cascade test room.
struct CascadeRoom {
    /// Creator (power level 100) — the privileged actor.
    owner_token: String,
    /// Power-level-0 member that performs the redactions in tests (a) and (c).
    actor_token: String,
    /// Power-level-0 member whose events must not be touched by test (a).
    other_token: String,
    /// The room both redactions happen in.
    room_id: String,
}

/// Owner (creator, power 100) + actor (power 0) + other (power 0), all joined.
async fn three_member_room(app: &axum::Router, suffix: u32, tag: &str) -> CascadeRoom {
    let (owner_token, _owner_id) = register_user_with_id(app, &format!("m3912_{tag}_owner_{suffix}")).await;
    let (actor_token, actor_id) = register_user_with_id(app, &format!("m3912_{tag}_actor_{suffix}")).await;
    let (other_token, other_id) = register_user_with_id(app, &format!("m3912_{tag}_other_{suffix}")).await;

    let room_id = create_room(app, &owner_token, "MSC3912 cascade authz").await;
    invite_user(app, &owner_token, &room_id, &actor_id).await;
    join_room(app, &actor_token, &room_id).await;
    invite_user(app, &owner_token, &room_id, &other_id).await;
    join_room(app, &other_token, &room_id).await;

    CascadeRoom { owner_token, actor_token, other_token, room_id }
}

/// SECURITY: an unprivileged actor redacting their own message must not be able
/// to cascade-redact another user's annotation on it.
#[tokio::test]
async fn msc3912_redact_cascade_denies_unprivileged_actor_on_foreign_child() {
    let Some((app, pool)) = setup_test_app().await else {
        return;
    };
    let suffix = rand::random::<u32>();
    let room = three_member_room(&app, suffix, "deny").await;

    let target = send_message(&app, &room.actor_token, &room.room_id).await;
    // The actor's OWN annotation is permitted and doubles as the "cascade ran" signal.
    let own_child = send_annotation(&app, &room.actor_token, &room.room_id, &target, "👍").await;
    // Carol's annotation is NOT the actor's event and the actor has power level 0.
    let foreign_child = send_annotation(&app, &room.other_token, &room.room_id, &target, "🎉").await;

    let response =
        redact(&app, &room.actor_token, &room.room_id, &target, &json!({ "with_rel_types": ["m.annotation"] })).await;
    assert_eq!(response.status(), StatusCode::OK, "redacting one's own event must succeed");

    assert!(
        wait_until_redacted(&pool, &own_child).await,
        "the cascade must redact the actor's own annotation (otherwise this test could pass vacuously)"
    );
    assert!(
        !is_redacted(&pool, &foreign_child).await,
        "SECURITY: an unprivileged actor must not cascade-redact another user's annotation"
    );
}

/// The happy path: a moderator's cascade still reaches another user's child.
#[tokio::test]
async fn msc3912_redact_cascade_allows_privileged_actor_on_foreign_child() {
    let Some((app, pool)) = setup_test_app().await else {
        return;
    };
    let suffix = rand::random::<u32>();
    let room = three_member_room(&app, suffix, "allow").await;

    let target = send_message(&app, &room.actor_token, &room.room_id).await;
    let foreign_child = send_annotation(&app, &room.other_token, &room.room_id, &target, "🎉").await;

    // The creator has power level 100, the default `redact` level is 50.
    let response =
        redact(&app, &room.owner_token, &room.room_id, &target, &json!({ "with_rel_types": ["m.annotation"] })).await;
    assert_eq!(response.status(), StatusCode::OK, "a moderator redaction must succeed");

    assert!(
        wait_until_redacted(&pool, &foreign_child).await,
        "a moderator must be able to cascade-redact another user's annotation"
    );
}

/// An empty `with_rel_types` list means "do not cascade" — not a 400.
#[tokio::test]
async fn msc3912_redact_empty_rel_types_succeeds_without_cascade() {
    let Some((app, pool)) = setup_test_app().await else {
        return;
    };
    let suffix = rand::random::<u32>();
    let room = three_member_room(&app, suffix, "empty").await;

    let target = send_message(&app, &room.actor_token, &room.room_id).await;
    // The actor's own annotation would be redacted by any cascade, so its
    // survival proves no cascade happened (not merely that auth denied one).
    let child = send_annotation(&app, &room.actor_token, &room.room_id, &target, "👍").await;

    let response = redact(&app, &room.actor_token, &room.room_id, &target, &json!({ "with_rel_types": [] })).await;
    assert_eq!(response.status(), StatusCode::OK, "an empty with_rel_types array must not be rejected with 400");
    assert!(is_redacted(&pool, &target).await, "the target event itself must still be redacted");
    assert!(!is_redacted(&pool, &child).await, "an empty with_rel_types array must not cascade");
}

/// The unstable spelling (`org.matrix.msc3912.with_relations`) follows the same rule.
#[tokio::test]
async fn msc3912_redact_empty_unstable_rel_types_succeeds_without_cascade() {
    let Some((app, pool)) = setup_test_app().await else {
        return;
    };
    let suffix = rand::random::<u32>();
    let room = three_member_room(&app, suffix, "emptyunstable").await;

    let target = send_message(&app, &room.actor_token, &room.room_id).await;
    let child = send_annotation(&app, &room.actor_token, &room.room_id, &target, "👍").await;

    let response =
        redact(&app, &room.actor_token, &room.room_id, &target, &json!({ "org.matrix.msc3912.with_relations": [] }))
            .await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "an empty org.matrix.msc3912.with_relations array must not be rejected with 400"
    );
    assert!(is_redacted(&pool, &target).await, "the target event itself must still be redacted");
    assert!(!is_redacted(&pool, &child).await, "an empty relation list must not cascade");
}

/// U-20: a reaction sent through the *dedicated* `/send/m.reaction` route must
/// be a real room event, so the MSC3912 cascade (which matches on
/// `events.content->'m.relates_to'`) can see and redact it.
///
/// On the pre-fix code `RelationsService::send_annotation` only inserted an
/// `event_relations` index row and never wrote `events`, so both assertions
/// below failed: the returned event id had no `events` row at all, and the
/// cascade could never reach it.
#[tokio::test]
async fn msc3912_cascade_reaches_reaction_sent_via_dedicated_route() {
    let Some((app, pool)) = setup_test_app().await else {
        return;
    };
    let suffix = rand::random::<u32>();
    let room = three_member_room(&app, suffix, "reactcascade").await;

    let target = send_message(&app, &room.actor_token, &room.room_id).await;
    let reaction = add_reaction_via_compat_route(&app, &room.actor_token, &room.room_id, &target, "👍").await;

    // (a) The dedicated route must persist a real `m.reaction` row in `events`,
    //     carrying the relation fields the read/cascade paths look for.
    let row = sqlx::query("SELECT event_type, content FROM events WHERE event_id = $1")
        .bind(&reaction)
        .fetch_optional(&*pool)
        .await
        .expect("query events");
    let row = row.unwrap_or_else(|| {
        panic!("the dedicated reaction route must persist an `events` row for the returned event_id {reaction}")
    });
    use sqlx::Row;
    assert_eq!(row.get::<String, _>("event_type"), "m.reaction");
    let content: Value = row.get("content");
    assert_eq!(content["m.relates_to"]["rel_type"], "m.annotation");
    assert_eq!(content["m.relates_to"]["event_id"], target);
    assert_eq!(content["m.relates_to"]["key"], "👍");
    assert_eq!(content["body"], "👍");

    // (b) ... and the index row must carry that same id.
    let indexed = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM event_relations WHERE event_id = $1 AND relation_type = 'm.annotation'",
    )
    .bind(&reaction)
    .fetch_one(&*pool)
    .await
    .expect("query event_relations");
    assert_eq!(indexed, 1, "the event_relations index row must carry the persisted event id");

    // (c) The MSC3912 cascade must now reach the reaction.
    let response =
        redact(&app, &room.actor_token, &room.room_id, &target, &json!({ "with_rel_types": ["m.annotation"] })).await;
    assert_eq!(response.status(), StatusCode::OK, "redacting one's own event must succeed");
    assert!(
        wait_until_redacted(&pool, &reaction).await,
        "the cascade must redact a reaction sent through the dedicated route"
    );
}

/// The dedicated route must source the emoji from the spec field
/// `m.relates_to.key`. It used to read only the top-level `body`, so a
/// key-only (spec-shaped) request was silently stored as the default 👍.
#[tokio::test]
async fn reaction_compat_route_reads_spec_key_from_relates_to() {
    let Some((app, pool)) = setup_test_app().await else {
        return;
    };
    let suffix = rand::random::<u32>();
    let (owner_token, _owner_id) = register_user_with_id(&app, &format!("reactkey_owner_{suffix}")).await;
    let room_id = create_room(&app, &owner_token, "reaction key shape").await;
    let target = send_message(&app, &owner_token, &room_id).await;

    let txn = format!("txn_key_{}", rand::random::<u32>());
    let request = put_json(
        format!("/_matrix/client/v3/rooms/{room_id}/send/m.reaction/{txn}"),
        &owner_token,
        &json!({
            "m.relates_to": { "rel_type": "m.annotation", "event_id": target, "key": "🎉" }
        }),
    );
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "spec-shaped reaction must be accepted");
    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let event_id = json["event_id"].as_str().expect("event_id").to_string();

    let content: Value = sqlx::query_scalar("SELECT content FROM events WHERE event_id = $1")
        .bind(&event_id)
        .fetch_one(&*pool)
        .await
        .expect("the reaction event must be persisted");
    assert_eq!(content["m.relates_to"]["key"], "🎉");
    assert_eq!(content["body"], "🎉");
}
