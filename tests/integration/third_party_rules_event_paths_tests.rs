//! M-4: the in-process third-party rule evaluator on the two event paths.
//!
//! Exercises the real wiring, not a stub: a rule is registered on the
//! container's `ModuleService` (the same `Arc` the admin routes use), and the
//! assertions drive the HTTP surface — the client send route for the local path
//! and the signed federation `/send` transaction for the ingress path.
//!
//! Semantics pinned here:
//! * empty registry ⇒ unchanged behaviour (the pre-M-4 regression guard);
//! * `is_allowed == false` ⇒ `403 M_FORBIDDEN` **carrying the rule's reason**,
//!   event not persisted — on the client send route *and* on room creation;
//! * rule `Err` ⇒ rejected as well (fail closed), event not persisted;
//! * `modified_content` ⇒ honoured on the local paths, where we author the bytes;
//! * `modified_content` ⇒ **ignored** for inbound federation PDUs, which arrive
//!   already signed and content-hashed by their origin: persisting rewritten
//!   bytes would desync the event from its signature.
//!
//! Two of these were only found by driving the real HTTP surface (2026-10-02):
//! the send route used to convert the gate's `403` into `500 M_UNKNOWN`, and the
//! creation sequence is gated too (so a rule that errors on everything refuses
//! `createRoom` before any message can be sent).

use async_trait::async_trait;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use base64::engine::general_purpose::STANDARD_NO_PAD;
use base64::Engine as _;
use ed25519_dalek::Signer;
use serde_json::{json, Value};
use std::sync::Arc;
use synapse_common::error::ApiError;
use synapse_common::event_id::resolve_received_event_id;
use synapse_common::pdu::{build_pdu, PduParts};
use synapse_federation::event_finalize::finalize_local_pdu;
use synapse_federation::signing::{sign_and_hash_event, verify_event_content_hash};
use synapse_services::module_service::{ModuleRegistry, ThirdPartyRule, ThirdPartyRuleContext, ThirdPartyRuleOutput};
use synapse_services::ServiceContainer;
use synapse_storage::event::EventStorage;
use synapse_web::federation::signing::canonical_federation_request_bytes;
use synapse_web::routes::state::AppState;
use tower::ServiceExt;

/// The rule registry the admin surface writes and both event paths read.
///
/// `ModuleService::registry()` hands out exactly this `Arc`, and the container
/// injects that same `ModuleService` as the `EventAdmissionGate`, so a rule
/// installed here is the rule the send paths enforce — there is no second
/// registry that could diverge.
type ThirdPartyRuleRegistry = Arc<tokio::sync::RwLock<ModuleRegistry>>;

/// Room version used by the federation fixture. v12 is the only creatable
/// version (`synapse_common::room_versions::SUPPORTED_ROOM_VERSIONS`), and
/// `build_pdu` omits the `room_id` for its `m.room.create` as MSC4291 requires.
const FIXTURE_ROOM_VERSION: &str = "12";

/// Denies one event type. `is_allowed == false` is the deny signal.
struct DenyEventTypeRule {
    blocked_event_type: String,
}

#[async_trait]
impl ThirdPartyRule for DenyEventTypeRule {
    fn name(&self) -> &str {
        "m4_deny_event_type"
    }

    async fn check(&self, context: &ThirdPartyRuleContext) -> Result<ThirdPartyRuleOutput, ApiError> {
        if context.event_type == self.blocked_event_type {
            return Ok(ThirdPartyRuleOutput {
                is_allowed: false,
                reason: Some("blocked by m4_deny_event_type".to_string()),
                modified_content: None,
            });
        }
        Ok(ThirdPartyRuleOutput { is_allowed: true, reason: None, modified_content: None })
    }
}

/// Always errors, to pin the fail-closed branch.
struct ErroringRule;

#[async_trait]
impl ThirdPartyRule for ErroringRule {
    fn name(&self) -> &str {
        "m4_erroring_rule"
    }

    async fn check(&self, _context: &ThirdPartyRuleContext) -> Result<ThirdPartyRuleOutput, ApiError> {
        Err(ApiError::internal("m4_erroring_rule exploded".to_string()))
    }
}

/// Rewrites `content.body`, to pin the "rewritten content is persisted" branch.
struct RewriteBodyRule;

#[async_trait]
impl ThirdPartyRule for RewriteBodyRule {
    fn name(&self) -> &str {
        "m4_rewrite_body"
    }

    async fn check(&self, context: &ThirdPartyRuleContext) -> Result<ThirdPartyRuleOutput, ApiError> {
        let mut content = context.content.clone();
        if let Some(body) = content.get("body").and_then(Value::as_str) {
            content["body"] = json!(format!("[rewritten] {body}"));
        }
        Ok(ThirdPartyRuleOutput { is_allowed: true, reason: None, modified_content: Some(content) })
    }
}

/// Registers `rule` on the same registry both event paths evaluate.
async fn register_rule(registry: &ThirdPartyRuleRegistry, rule: Arc<dyn ThirdPartyRule>) {
    registry.write().await.register_third_party_rule(rule);
}

/// Build a real container + router, returning the registry so the test can
/// install rules after construction (the admin surface's only entry point).
async fn build_app() -> (axum::Router, Arc<sqlx::PgPool>, ThirdPartyRuleRegistry) {
    let pool = super::require_test_pool().await;
    let cache = Arc::new(synapse_rust::cache::CacheManager::new(&synapse_rust::cache::CacheConfig::default()));
    let container = ServiceContainer::new_test_with_pool_and_cache(pool.clone(), cache.clone()).await;
    let registry = container.admin.modules.module_service.registry();
    let state = AppState::new(container, cache);
    (synapse_web::create_router(state), pool, registry)
}

async fn register_user_via_client(app: &axum::Router) -> (String, String) {
    let username = format!("m4_{}", uuid::Uuid::new_v4().as_simple());
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
        .expect("request builds");

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.expect("register responds");
    assert_eq!(response.status(), StatusCode::OK, "test user registration must succeed");
    let bytes = axum::body::to_bytes(response.into_body(), 4096).await.expect("body");
    let body: Value = serde_json::from_slice(&bytes).expect("register body is JSON");
    (
        body["access_token"].as_str().expect("registration returns a token").to_string(),
        body["user_id"].as_str().expect("registration returns a user id").to_string(),
    )
}

/// `POST /createRoom` without asserting the verdict (the gate can refuse it).
async fn create_room_raw(app: &axum::Router, token: &str) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "preset": "public_chat", "room_version": FIXTURE_ROOM_VERSION }).to_string()))
        .expect("request builds");
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.expect("createRoom responds");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 4096).await.expect("body");
    let body: Value = serde_json::from_slice(&bytes).expect("createRoom body is JSON");
    (status, body)
}

async fn create_room(app: &axum::Router, token: &str) -> String {
    let (status, body) = create_room_raw(app, token).await;
    assert_eq!(status, StatusCode::OK, "createRoom must succeed: {body}");
    body["room_id"].as_str().expect("createRoom returns a room_id").to_string()
}

/// `PUT /rooms/{roomId}/send/{eventType}/{txnId}`; returns status + parsed body.
async fn send_message(
    app: &axum::Router,
    token: &str,
    room_id: &str,
    event_type: &str,
    txn_id: &str,
    content: Value,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/rooms/{room_id}/send/{event_type}/{txn_id}"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(content.to_string()))
        .expect("request builds");
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.expect("send responds");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 8192).await.expect("body");
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

/// A `{ "body": body }` `m.room.message` send that must succeed, returning the
/// persisted event id.
async fn send_body(app: &axum::Router, token: &str, room_id: &str, body: &str) -> String {
    let (status, response) = send_message(
        app,
        token,
        room_id,
        "m.room.message",
        &uuid::Uuid::new_v4().as_simple().to_string(),
        json!({ "msgtype": "m.text", "body": body }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the send must succeed when no rule denies it: {response}");
    response["event_id"].as_str().expect("a successful send returns an event_id").to_string()
}

// ============================================================================
// (a) Empty registry ⇒ behaviour unchanged
// ============================================================================

#[tokio::test]
async fn test_m4_empty_registry_send_still_succeeds_and_persists() {
    let (app, pool, _registry) = build_app().await;
    let (token, _user_id) = register_user_via_client(&app).await;
    let room_id = create_room(&app, &token).await;

    let event_id = send_body(&app, &token, &room_id, "no rules are registered").await;

    let storage = EventStorage::new(&pool, "localhost".to_string());
    let stored = storage.get_event(&event_id).await.expect("events table readable");
    let stored =
        stored.unwrap_or_else(|| panic!("with an empty registry the event must persist as before: {event_id}"));
    assert_eq!(stored.content["body"], json!("no rules are registered"), "content must be untouched");
}

// ============================================================================
// (b) Deny rule ⇒ 403 M_FORBIDDEN and nothing persisted
// ============================================================================

#[tokio::test]
async fn test_m4_deny_rule_rejects_local_send_with_m_forbidden() {
    let (app, pool, registry) = build_app().await;
    register_rule(&registry, Arc::new(DenyEventTypeRule { blocked_event_type: "m.room.message".to_string() })).await;
    let (token, _user_id) = register_user_via_client(&app).await;
    let room_id = create_room(&app, &token).await;

    let (status, body) = send_message(
        &app,
        &token,
        &room_id,
        "m.room.message",
        &uuid::Uuid::new_v4().as_simple().to_string(),
        json!({ "msgtype": "m.text", "body": "denied" }),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN, "a deny verdict must be 403: {body}");
    assert_eq!(body["errcode"], json!("M_FORBIDDEN"), "the Matrix code must be M_FORBIDDEN: {body}");
    assert_eq!(
        body["error"],
        json!("blocked by m4_deny_event_type"),
        "the rule's reason must be surfaced to the client: {body}"
    );

    let persisted: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE room_id = $1 AND event_type = $2")
        .bind(&room_id)
        .bind("m.room.message")
        .fetch_one(&*pool)
        .await
        .expect("events table readable");
    assert_eq!(persisted, 0, "a denied event must not be persisted");
}

// ============================================================================
// (b2) Deny rule ⇒ the room-creation sequence is gated too
// ============================================================================

/// A rule that refuses `m.room.create` must refuse `createRoom` itself: the
/// admission gate covers the creation sequence, so no room row may be created.
#[tokio::test]
async fn test_m4_deny_rule_rejects_room_creation() {
    let (app, _pool, registry) = build_app().await;
    register_rule(&registry, Arc::new(DenyEventTypeRule { blocked_event_type: "m.room.create".to_string() })).await;
    let (token, _user_id) = register_user_via_client(&app).await;

    let (status, body) = create_room_raw(&app, &token).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "a denied m.room.create must be 403: {body}");
    assert_eq!(body["errcode"], json!("M_FORBIDDEN"), "the Matrix code must be M_FORBIDDEN: {body}");
}

// ============================================================================
// (c) Rule error ⇒ fail-closed rejection, nothing persisted
// ============================================================================

#[tokio::test]
async fn test_m4_rule_error_rejects_local_send_fail_closed() {
    let (app, pool, registry) = build_app().await;
    let (token, _user_id) = register_user_via_client(&app).await;
    // Create the room *before* registering the rule. The admission gate also
    // covers the room-creation sequence (`create_events.rs`), so an
    // always-erroring rule would otherwise refuse `m.room.create` and this test
    // could never reach the send path it is about. Creation gating itself is
    // pinned by `test_m4_deny_rule_rejects_room_creation` below.
    let room_id = create_room(&app, &token).await;
    register_rule(&registry, Arc::new(ErroringRule)).await;

    let (status, body) = send_message(
        &app,
        &token,
        &room_id,
        "m.room.message",
        &uuid::Uuid::new_v4().as_simple().to_string(),
        json!({ "msgtype": "m.text", "body": "must fail closed" }),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN, "an evaluation failure must reject the send: {body}");
    assert_eq!(body["errcode"], json!("M_FORBIDDEN"), "fail-closed rejection keeps M_FORBIDDEN: {body}");

    let persisted: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE room_id = $1 AND event_type = $2")
        .bind(&room_id)
        .bind("m.room.message")
        .fetch_one(&*pool)
        .await
        .expect("events table readable");
    assert_eq!(persisted, 0, "an event whose rules could not be evaluated must not be persisted");
}

// ============================================================================
// (d) Rewrite rule ⇒ the rewritten content is what is persisted
// ============================================================================

#[tokio::test]
async fn test_m4_rewrite_rule_content_is_persisted() {
    let (app, pool, registry) = build_app().await;
    register_rule(&registry, Arc::new(RewriteBodyRule)).await;
    let (token, _user_id) = register_user_via_client(&app).await;
    let room_id = create_room(&app, &token).await;

    let event_id = send_body(&app, &token, &room_id, "original body").await;

    let storage = EventStorage::new(&pool, "localhost".to_string());
    let stored = storage.get_event(&event_id).await.expect("events table readable");
    let stored = stored.unwrap_or_else(|| panic!("a rewritten event is still allowed, so it must persist: {event_id}"));
    assert_eq!(
        stored.content["body"],
        json!("[rewritten] original body"),
        "the persisted content must be the rule's rewrite, not the submitted body"
    );
}

// ============================================================================
// Federation ingress path: same evaluator, same fail-closed semantics
// ============================================================================

fn signed_federation_request(
    method: &str,
    uri: &str,
    origin: &str,
    key_id: &str,
    signing_key: &ed25519_dalek::SigningKey,
    content: Option<&Value>,
) -> Request<Body> {
    let signed_bytes = canonical_federation_request_bytes(method, uri, origin, origin, content).expect("signable");
    let signature = signing_key.sign(&signed_bytes);
    let signature_b64 = STANDARD_NO_PAD.encode(signature.to_bytes());

    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("Authorization", format!("X-Matrix origin=\"{origin}\",key=\"{key_id}\",sig=\"{signature_b64}\""));
    if content.is_some() {
        builder = builder.header("Content-Type", "application/json");
    }
    builder.body(Body::from(content.map(Value::to_string).unwrap_or_default())).expect("request builds")
}

/// Build + sign one `org.example.tombstone` PDU in `room_id`.
fn build_signed_tombstone_pdu(
    room_id: &str,
    sender: &str,
    create_event_id: &str,
    key_id: &str,
    signing_key_b64: &str,
) -> Value {
    let content = json!({ "body": "m4 federation fixture" });
    let parts = PduParts {
        room_version: FIXTURE_ROOM_VERSION,
        event_id: None,
        room_id,
        sender,
        event_type: "org.example.tombstone",
        content: &content,
        state_key: None,
        origin_server_ts: 1_750_000_000_000,
        origin: "localhost",
        depth: 2,
        prev_events: &[create_event_id.to_string()],
        auth_events: &[create_event_id.to_string()],
        redacts: None,
    };
    let mut pdu = build_pdu(&parts);
    let finalized = finalize_local_pdu(&parts).expect("the assembled PDU is finalizable");
    pdu.as_object_mut().expect("PDU is an object").insert("hashes".to_string(), finalized.hashes);
    sign_and_hash_event(FIXTURE_ROOM_VERSION, "localhost", key_id, signing_key_b64, &mut pdu)
        .expect("sign_and_hash_event signs our own PDU");
    assert!(verify_event_content_hash(&pdu).is_ok(), "the fixture PDU must carry a valid content hash");
    pdu
}

/// A live federation-ingress harness: real container + router, a local room to
/// receive PDUs, and the shared rule registry.
struct FederationHarness {
    app: axum::Router,
    pool: Arc<sqlx::PgPool>,
    registry: ThirdPartyRuleRegistry,
    key_id: String,
    signing_key_b64: String,
    signing_key: ed25519_dalek::SigningKey,
    room_id: String,
    creator: String,
    create_event_id: String,
}

impl FederationHarness {
    async fn setup(key_id: &str, seed: u8) -> Self {
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&[seed; 32]);
        let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key.to_bytes());

        let pool = super::require_test_pool().await;
        let cache = Arc::new(synapse_rust::cache::CacheManager::new(&synapse_rust::cache::CacheConfig::default()));
        let mut container = ServiceContainer::new_test_with_pool_and_cache(pool.clone(), cache.clone()).await;
        super::config_mut(&mut container).server.name = "localhost".to_string();
        container.core.server_name = "localhost".to_string();
        super::config_mut(&mut container).federation.enabled = true;
        super::config_mut(&mut container).federation.allow_ingress = true;
        super::config_mut(&mut container).federation.server_name = "localhost".to_string();
        super::config_mut(&mut container).federation.key_id = Some(key_id.to_string());
        super::config_mut(&mut container).federation.signing_key = Some(signing_key_b64.clone());
        // The `KeyRotationManager` is built inside `ServiceContainer::new`, so
        // the config override above never reaches it; install the key directly.
        let init_result = container.federation.key_rotation_manager.initialize(&signing_key_b64, key_id).await;
        assert!(
            container.federation.key_rotation_manager.get_current_key().await.expect("key read").is_some(),
            "the deterministic signing key must be installed (initialize returned {init_result:?})"
        );

        let registry = container.admin.modules.module_service.registry();
        let state = AppState::new(container, cache);
        let app = synapse_web::create_router(state);

        let (token, creator) = register_user_via_client(&app).await;
        let room_id = create_room(&app, &token).await;
        let create_event_id = create_event_id_of(&pool, &room_id).await;

        Self {
            app,
            pool,
            registry,
            key_id: key_id.to_string(),
            signing_key_b64,
            signing_key,
            room_id,
            creator,
            create_event_id,
        }
    }

    /// PUT one signed `org.example.tombstone` PDU for `room_id`; returns the
    /// parsed transaction response.
    async fn send_tombstone(&self, txn_prefix: &str) -> Value {
        let pdu = build_signed_tombstone_pdu(
            &self.room_id,
            &self.creator,
            &self.create_event_id,
            &self.key_id,
            &self.signing_key_b64,
        );
        let event_id =
            resolve_received_event_id(FIXTURE_ROOM_VERSION, &pdu).expect("the receiver must derive the event id");
        let body = json!({ "origin": "localhost", "pdus": [pdu] });
        let request = signed_federation_request(
            "PUT",
            &format!("/_matrix/federation/v1/send/{txn_prefix}_{}", uuid::Uuid::new_v4().as_simple()),
            "localhost",
            &self.key_id,
            &self.signing_key,
            Some(&body),
        );
        let response = ServiceExt::<Request<Body>>::oneshot(self.app.clone(), request).await.expect("txn responds");
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "the transaction endpoint answers 200 even for rejected PDUs; the verdict is in `results`"
        );
        let bytes = axum::body::to_bytes(response.into_body(), 8192).await.expect("body");
        let parsed: Value = serde_json::from_slice(&bytes).expect("response is JSON");
        // One PDU in -> one result out.
        let results = parsed["results"].as_array().expect("results must be an array");
        assert_eq!(results.len(), 1, "one PDU in -> one result out: {parsed}");
        json!({ "event_id": event_id, "result": results[0].clone() })
    }
}

/// The `m.room.create` event ID of `room_id`, read back from the real schema.
async fn create_event_id_of(pool: &Arc<sqlx::PgPool>, room_id: &str) -> String {
    sqlx::query_scalar(
        "SELECT event_id FROM events WHERE room_id = $1 AND event_type = 'm.room.create' ORDER BY origin_server_ts ASC LIMIT 1",
    )
    .bind(room_id)
    .fetch_one(&**pool)
    .await
    .expect("every created room must have an m.room.create event")
}

#[tokio::test]
async fn test_m4_deny_rule_rejects_federation_pdu_and_does_not_persist_it() {
    let harness = FederationHarness::setup("ed25519:m4_deny", 71).await;
    register_rule(
        &harness.registry,
        Arc::new(DenyEventTypeRule { blocked_event_type: "org.example.tombstone".to_string() }),
    )
    .await;

    let response = harness.send_tombstone("m4_deny").await;
    let derived_event_id = response["event_id"].as_str().expect("event id");
    let result = &response["result"];

    assert!(result.get("success").is_none(), "a denied PDU must not report success: {result}");
    let error = result["error"].as_str().unwrap_or_else(|| panic!("a denied PDU must carry an error: {result}"));
    assert!(
        error.contains("blocked by m4_deny_event_type"),
        "the rejection must surface the rule's reason, got: {error}"
    );

    let storage = EventStorage::new(&harness.pool, "localhost".to_string());
    assert!(
        storage.get_event(derived_event_id).await.expect("events table readable").is_none(),
        "a PDU denied by third-party rules must not be persisted"
    );
}

#[tokio::test]
async fn test_m4_rewrite_rule_is_not_applied_to_federation_pdu() {
    let harness = FederationHarness::setup("ed25519:m4_rewrite", 72).await;
    register_rule(&harness.registry, Arc::new(RewriteBodyRule)).await;

    let response = harness.send_tombstone("m4_rewrite").await;
    let derived_event_id = response["event_id"].as_str().expect("event id");
    let result = &response["result"];
    assert_eq!(result["success"], json!(true), "an allowed PDU must be accepted: {result}");

    let storage = EventStorage::new(&harness.pool, "localhost".to_string());
    let stored = storage
        .get_event(derived_event_id)
        .await
        .expect("events table readable")
        .unwrap_or_else(|| panic!("the accepted PDU must be persisted: {derived_event_id}"));
    // An inbound PDU is already signed and content-hashed by its origin. A rule
    // rewrite is therefore **not** applied on this path: persisting rewritten
    // bytes would desync the event from its signature and content hash. The
    // local send paths (where we author the bytes) still honour rewrites.
    assert_eq!(
        stored.content["body"],
        json!("m4 federation fixture"),
        "an inbound PDU's content must be persisted verbatim, not rewritten"
    );
}
