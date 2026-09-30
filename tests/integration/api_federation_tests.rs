use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use base64::engine::general_purpose::STANDARD_NO_PAD;
use base64::Engine as _;
use ed25519_dalek::Signer;
use serde_json::{json, Value};
use std::sync::Arc;
use synapse_common::room_versions::DEFAULT_ROOM_VERSION;
use synapse_web::federation::signing::canonical_federation_request_bytes;
use tower::ServiceExt;

async fn setup_test_app() -> Option<axum::Router> {
    super::setup_fresh_test_app().await
}

async fn setup_federation_test_app_with_pool(
    key_id: &str,
    signing_key_b64: &str,
) -> Option<(axum::Router, Arc<sqlx::PgPool>)> {
    // Use require_test_pool() for per-test schema isolation. These
    // directory-query tests create rooms and aliases that can be
    // interfered with by other tests sharing the same schema in full
    // suite runs. Each call clones a fresh schema from the template.
    let pool = super::require_test_pool().await;
    let mut container = synapse_services::ServiceContainer::new_test_with_pool(pool.clone()).await;
    super::config_mut(&mut container).server.name = "localhost".to_string();
    container.core.server_name = "localhost".to_string();
    super::config_mut(&mut container).federation.enabled = true;
    super::config_mut(&mut container).federation.allow_ingress = true;
    super::config_mut(&mut container).federation.server_name = "localhost".to_string();
    super::config_mut(&mut container).federation.key_id = Some(key_id.to_string());
    super::config_mut(&mut container).federation.signing_key = Some(signing_key_b64.to_string());
    let cache =
        std::sync::Arc::new(synapse_rust::cache::CacheManager::new(&synapse_rust::cache::CacheConfig::default()));
    let state = synapse_web::routes::state::AppState::new(container, cache);
    Some((synapse_web::create_router(state), pool))
}

fn signed_federation_request(
    method: &str,
    uri: &str,
    origin: &str,
    key_id: &str,
    signing_key: &ed25519_dalek::SigningKey,
    content: Option<&Value>,
) -> Request<Body> {
    let signed_bytes = canonical_federation_request_bytes(method, uri, origin, origin, content).unwrap();
    let sig = signing_key.sign(&signed_bytes);
    let sig_b64 = STANDARD_NO_PAD.encode(sig.to_bytes());

    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("Authorization", format!("X-Matrix origin=\"{}\",key=\"{}\",sig=\"{}\"", origin, key_id, sig_b64));

    if content.is_some() {
        builder = builder.header("Content-Type", "application/json");
    }

    builder.body(Body::from(content.map(Value::to_string).unwrap_or_default())).unwrap()
}

async fn register_user(app: &axum::Router, username: &str) -> (String, String) {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "username": format!("{}_{}", username, rand::random::<u32>()),
                "password": "Password123!",
                "auth": { "type": "m.login.dummy" }
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 2048).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    (json["access_token"].as_str().unwrap().to_string(), json["user_id"].as_str().unwrap().to_string())
}

async fn create_room(app: &axum::Router, token: &str, name: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "name": name }).to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 2048).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["room_id"].as_str().unwrap().to_string()
}

async fn set_room_alias(app: &axum::Router, token: &str, alias: &str, room_id: &str) {
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/_matrix/client/v3/directory/room/{}", urlencoding::encode(alias)))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "room_id": room_id }).to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn request_openid_token(app: &axum::Router, token: &str, user_id: &str) -> String {
    let request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/user/{}/openid/request_token", user_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 2048).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    json["access_token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn test_federation_version() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let request = Request::builder().uri("/_matrix/federation/v1/version").body(Body::empty()).unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(json["server"]["version"].is_string());
}

#[tokio::test]
async fn test_federation_queries() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    // 1. Query Profile
    let request = Request::builder()
        .uri("/_matrix/federation/v1/query/profile/@alice:localhost?field=displayname")
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    // Might be 404 if user doesn't exist, but the endpoint should exist
    assert!(response.status() == StatusCode::OK || response.status() == StatusCode::NOT_FOUND);

    // 2. Query Directory
    let request = Request::builder()
        .uri("/_matrix/federation/v1/query/directory?room_alias=#test:localhost")
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert!(response.status() == StatusCode::OK || response.status() == StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_federation_query_directory_returns_not_found_with_clear_message_for_missing_alias() {
    let key_id = "ed25519:test";
    let signing_key_seed = [17u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some((app, _pool)) = setup_federation_test_app_with_pool(key_id, &signing_key_b64).await else {
        return;
    };

    let alias = format!("#missing-alias-{}:localhost", rand::random::<u32>());
    let uri = format!("/_matrix/federation/v1/query/directory?room_alias={}", urlencoding::encode(&alias));
    let request = signed_federation_request("GET", &uri, "localhost", key_id, &signing_key, None);

    let response = ServiceExt::<Request<Body>>::oneshot(app, request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body = axum::body::to_bytes(response.into_body(), 2048).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_NOT_FOUND");
    assert!(
        json["error"]
            .as_str()
            .is_some_and(|message| message.contains("Create the alias before querying the federation directory.")),
        "Unexpected error payload: {json}"
    );
}

#[tokio::test]
async fn test_federation_query_directory_resolves_alias_after_creation() {
    let key_id = "ed25519:test";
    let signing_key_seed = [18u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some((app, pool)) = setup_federation_test_app_with_pool(key_id, &signing_key_b64).await else {
        return;
    };

    let (token, _) = register_user(&app, "federation_alias").await;
    let room_id = create_room(&app, &token, "Federation Alias").await;
    let alias = format!("#federation-query-{}:localhost", rand::random::<u32>());

    set_room_alias(&app, &token, &alias, &room_id).await;

    let uri = format!("/_matrix/federation/v1/query/directory?room_alias={}", urlencoding::encode(&alias));
    let request = signed_federation_request("GET", &uri, "localhost", key_id, &signing_key, None);
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 2048).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["room_id"], room_id);
    assert_eq!(json["servers"][0], "localhost");

    // 列名是 `room_alias`（baseline `CREATE TABLE room_aliases (room_alias TEXT …)`），
    // 不是 `alias`：写错会 `42703 column "alias" does not exist`。这个清理只在
    // 测试**跑完**时执行，而 integration 目标在 CI 里从未真正跑过（§14.14.1），
    // 所以这个错列名一直没被暴露。守卫：
    // `tests/unit/ci_test_scope_tests.rs::no_source_queries_a_non_existent_room_aliases_column`。
    sqlx::query("DELETE FROM room_aliases WHERE room_alias = $1")
        .bind(&alias)
        .execute(&*pool)
        .await
        .expect("clean up the federated room alias");
    sqlx::query("DELETE FROM rooms WHERE room_id = $1")
        .bind(&room_id)
        .execute(&*pool)
        .await
        .expect("clean up the federated room");
}

#[tokio::test]
async fn test_federation_public_rooms() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let request = Request::builder().uri("/_matrix/federation/v1/publicRooms").body(Body::empty()).unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_federation_query_destination_returns_minimal_payload() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let request = Request::builder().uri("/_matrix/federation/v1/query/destination").body(Body::empty()).unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(json["server_name"].is_string());
    assert!(json["destination"].is_string());
    assert_eq!(json["capabilities"]["m.change_password"], true);

    // G-2 / G-05: this surface used to have no assertion on its room-version
    // content at all, which hid a malformed shape (a flat `{version: {status}}`
    // map with no `available` wrapper). Assert the documented shape and the
    // federatable set, not just `default`.
    let versions = &json["capabilities"]["m.room_versions"];
    assert_eq!(versions["default"], DEFAULT_ROOM_VERSION);
    let available = versions["available"]
        .as_object()
        .unwrap_or_else(|| panic!("federation m.room_versions must carry an `available` object: {versions}"));
    // Every supported version is federatable (including v1-v11, which are no
    // longer creatable — the two sets legitimately differ since G-1).
    assert_eq!(available.len(), 12, "every supported version must be federatable: {available:?}");
    assert_eq!(available["12"], serde_json::json!("stable"));
    assert_eq!(available["11"], serde_json::json!("stable"), "v11 stays federatable though not creatable");
    assert!(available.get("13").is_none(), "room version 13 does not exist upstream");
}

/// The federation discovery endpoint (`GET /_matrix/federation/v1`) must
/// advertise the room versions this server federates with, in the documented
/// `{default, available}` shape. Untested before G-2.
#[tokio::test]
async fn test_federation_discovery_advertises_room_versions() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let request = Request::builder().uri("/_matrix/federation/v1").body(Body::empty()).unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(json["server_name"].is_string());

    let versions = &json["capabilities"]["m.room_versions"];
    assert_eq!(versions["default"], DEFAULT_ROOM_VERSION);
    let available = versions["available"]
        .as_object()
        .unwrap_or_else(|| panic!("federation discovery must carry `available`: {versions}"));
    assert_eq!(available.len(), 12);
    assert_eq!(available["12"], serde_json::json!("stable"));
}

#[tokio::test]
async fn test_federation_get_group_returns_not_found_without_placeholder() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let request = Request::builder()
        .uri("/_matrix/federation/v1/groups/%2Bexample%3Atest.example.com")
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_federation_key_clone_returns_server_keys() {
    let key_id = "ed25519:test";
    let signing_key_seed = [11u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some((app, _pool)) = setup_federation_test_app_with_pool(key_id, &signing_key_b64).await else {
        return;
    };

    let request = signed_federation_request(
        "POST",
        "/_synapse/federation/v2/key/clone",
        "localhost",
        key_id,
        &signing_key,
        Some(&json!({})),
    );

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(json["server_name"].is_string());
    assert!(json["verify_keys"].is_object());
}

#[tokio::test]
async fn test_server_keys_endpoint_returns_verify_keys_without_config_signing_key() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let request = Request::builder().uri("/_matrix/key/v2/server").body(Body::empty()).unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["server_name"], "test.example.com");
    assert!(json["verify_keys"].as_object().is_some_and(|keys| !keys.is_empty()));
}

#[tokio::test]
async fn test_local_key_query_reuses_server_key_response() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let request = Request::builder().uri("/_matrix/key/v2/server").body(Body::empty()).unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    let key_id = json["verify_keys"].as_object().and_then(|keys| keys.keys().next().cloned()).unwrap();

    let request = Request::builder()
        .uri(format!("/_matrix/key/v2/query/test.example.com/{}", key_id))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app, request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 8192).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    // P2-16: Notary query now returns the spec-compliant wrapped format.
    assert!(json["server_keys"].is_array(), "notary query must return wrapped format");
    let server_keys = json["server_keys"].as_array().unwrap();
    assert!(!server_keys.is_empty());
    let first = &server_keys[0];
    assert_eq!(first["server_name"], "test.example.com");
    assert!(first["verify_keys"].get(&key_id).is_some());
}

#[tokio::test]
async fn test_remote_key_query_fetches_real_remote_server_response() {
    // The key_query handler enforces HTTPS-only remote fetches and SSRF IP
    // blacklisting, so a wiremock HTTP server on localhost cannot be reached.
    // Instead, we pre-populate the federation key cache with a properly
    // signed, valid Ed25519 key response and verify the handler returns it.
    let Some((app, state)) = super::setup_fresh_test_app_with_state().await else {
        return;
    };

    let key_id = "ed25519:test";
    let server_name = "remote.example.com";

    // Generate a valid Ed25519 signing key and derive the verify key.
    let signing_key_seed = [42u8; 32];
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);
    let verifying_key = signing_key.verifying_key();
    let verify_key_b64 = STANDARD_NO_PAD.encode(verifying_key.as_bytes());

    // Build the response body without signatures.
    let mut body = json!({
        "server_name": server_name,
        "valid_until_ts": 4_102_444_800_000_i64,
        "verify_keys": {
            key_id: {
                "key": verify_key_b64
            }
        },
        "old_verify_keys": {},
        "signatures": {}
    });

    // Compute the canonical JSON of the body with signatures removed, then
    // sign it with the Ed25519 signing key.  The signature is base64-encoded
    // with STANDARD (padded) encoding to match the verification code.
    let mut body_without_sigs = body.clone();
    body_without_sigs.as_object_mut().unwrap().remove("signatures");
    let canonical = synapse_common::canonical_json::canonical_json(&body_without_sigs).unwrap();
    let signature = signing_key.sign(canonical.as_bytes());
    let signature_b64 = base64::engine::general_purpose::STANDARD.encode(signature.to_bytes());

    // Add the self-signature.
    body["signatures"][server_name][key_id] = json!(signature_b64);

    // Pre-populate the cache so the key query handler returns the cached
    // response without needing to fetch over HTTPS.
    let cache_key = format!("federation:server_keys:{}:{}", server_name, key_id);
    let _ = state.cache.set(&cache_key, &body, 3600).await;

    let request = Request::builder()
        .uri(format!("/_matrix/key/v2/query/{}/{}", server_name, key_id))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app, request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let resp_body = axum::body::to_bytes(response.into_body(), 8192).await.unwrap();
    let json: Value = serde_json::from_slice(&resp_body).unwrap();

    // P2-16: Notary query now returns the spec-compliant wrapped format.
    assert!(json["server_keys"].is_array(), "remote notary query must return wrapped format");
    let server_keys = json["server_keys"].as_array().unwrap();
    assert!(!server_keys.is_empty());
    let first = &server_keys[0];
    assert_eq!(first["server_name"], server_name);
    assert_eq!(first["verify_keys"][key_id]["key"], verify_key_b64);
}

#[tokio::test]
async fn test_federation_openid_userinfo_validates_openid_token_without_placeholder() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (access_token, user_id) = register_user(&app, "federation_openid").await;
    let openid_token = request_openid_token(&app, &access_token, &user_id).await;

    let request = Request::builder()
        .uri(format!("/_matrix/federation/v1/openid/userinfo?access_token={}", openid_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["sub"], user_id);

    let invalid_request = Request::builder()
        .uri("/_matrix/federation/v1/openid/userinfo?access_token=invalid_token")
        .body(Body::empty())
        .unwrap();

    let invalid_response = ServiceExt::<Request<Body>>::oneshot(app, invalid_request).await.unwrap();
    assert_eq!(invalid_response.status(), StatusCode::UNAUTHORIZED);
}

// =============================================================================
// P2-16: Federation key query/notary semantic convergence (MSC4242-adjacent)
// =============================================================================
//
// Matrix spec v1.18 server-server-api defines three notary query endpoints:
//   1. GET  /_matrix/key/v2/query/{serverName}           — all keys for a server
//   2. GET  /_matrix/key/v2/query/{serverName}/{keyId}   — specific key (Synapse ext)
//   3. POST /_matrix/key/v2/query                         — batch notary query
//
// All three MUST return the spec-compliant wrapped format:
//   { "server_keys": [ { "server_name": ..., "verify_keys": ..., ... } ] }
//
// The single-object format (returned by /_matrix/key/v2/server) is NOT
// spec-compliant for notary query endpoints and breaks interoperability with
// Synapse/Dendrite, which expect the wrapped array format.

/// Spec compliance: `GET /_matrix/key/v2/query/{serverName}` (without key_id)
/// must be registered and return the spec-compliant wrapped format.
#[tokio::test]
async fn test_p2_16_notary_query_without_key_id_returns_wrapped_format() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    // Query own server via the spec-defined notary path (no key_id).
    let request = Request::builder().uri("/_matrix/key/v2/query/test.example.com").body(Body::empty()).unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app, request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "notary query without key_id must be accepted");

    let body = axum::body::to_bytes(response.into_body(), 8192).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    // Spec: response must be { "server_keys": [Server Keys] } — wrapped in array.
    assert!(json["server_keys"].is_array(), "notary query response must be wrapped in server_keys array; got: {json}");
    let server_keys = json["server_keys"].as_array().unwrap();
    assert!(!server_keys.is_empty(), "server_keys array must not be empty for local server");

    let first = &server_keys[0];
    assert_eq!(first["server_name"], "test.example.com");
    assert!(first["verify_keys"].as_object().is_some_and(|keys| !keys.is_empty()));
    assert!(first["valid_until_ts"].is_i64());
    // signatures may be absent in test env without a valid signing key.
}

/// Spec compliance: `GET /_matrix/key/v2/query/{serverName}/{keyId}` (Synapse
/// extension) must also return the spec-compliant wrapped format for
/// interoperability with Synapse/Dendrite.
#[tokio::test]
async fn test_p2_16_notary_query_with_key_id_returns_wrapped_format() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    // First fetch own server keys to discover the key_id.
    let request = Request::builder().uri("/_matrix/key/v2/server").body(Body::empty()).unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let server_key_json: Value = serde_json::from_slice(&body).unwrap();
    let key_id = server_key_json["verify_keys"].as_object().and_then(|keys| keys.keys().next().cloned()).unwrap();

    // Query own server via the Synapse-extension notary path (with key_id).
    let request = Request::builder()
        .uri(format!("/_matrix/key/v2/query/test.example.com/{}", key_id))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app, request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 8192).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    // Spec: response must be { "server_keys": [Server Keys] } — wrapped in array.
    assert!(json["server_keys"].is_array(), "notary query with key_id must return wrapped format; got: {json}");
    let server_keys = json["server_keys"].as_array().unwrap();
    assert!(!server_keys.is_empty(), "server_keys array must not be empty");

    let first = &server_keys[0];
    assert_eq!(first["server_name"], "test.example.com");
    assert!(first["verify_keys"].get(&key_id).is_some(), "queried key_id must be present in response");
}

/// Spec compliance: `POST /_matrix/key/v2/query` batch notary query must be
/// registered and return the spec-compliant wrapped format.
#[tokio::test]
async fn test_p2_16_batch_notary_query_returns_wrapped_format() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    // Batch query for own server's keys.
    let body = serde_json::json!({
        "server_keys": {
            "test.example.com": {}
        }
    });
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/key/v2/query")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app, request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "POST /_matrix/key/v2/query must be accepted");

    let body = axum::body::to_bytes(response.into_body(), 8192).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    // Spec: response must be { "server_keys": [Server Keys] } — wrapped in array.
    assert!(
        json["server_keys"].is_array(),
        "batch notary query response must be wrapped in server_keys array; got: {json}"
    );
    let server_keys = json["server_keys"].as_array().unwrap();
    assert!(!server_keys.is_empty(), "server_keys array must contain own server's keys");

    let first = &server_keys[0];
    assert_eq!(first["server_name"], "test.example.com");
    assert!(first["verify_keys"].as_object().is_some_and(|keys| !keys.is_empty()));
}

/// Spec compliance: `POST /_matrix/key/v2/query` with empty server_keys request
/// must return an empty server_keys array (per spec: "If no servers are given,
/// the notary server must return an empty server_keys array in the response").
#[tokio::test]
async fn test_p2_16_batch_notary_query_empty_request_returns_empty_array() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let body = serde_json::json!({ "server_keys": {} });
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/key/v2/query")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app, request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert!(json["server_keys"].is_array(), "response must contain server_keys array");
    assert!(json["server_keys"].as_array().unwrap().is_empty(), "empty request must return empty server_keys array");
}

// ── D-110 / D-111：federation 侧 `POST /_matrix/federation/v1/publicRooms` 的 `filter` ──────
//
// 这个端点在 `create_federation_router` 的 **protected** 组里（`ServerSignatures`，见 ruma 的
// `authentication: ServerSignatures`）⇒ 用例必须带 `X-Matrix` 签名，且 app 的
// `federation.allow_ingress` 必须为 true（否则 `federation_auth_middleware` 直接 404）。
// 因此这些用例复用本文件的 `setup_federation_test_app_with_pool` / `signed_federation_request`。

/// 插入一个公开房间 + 它的 `room_summaries` 行（`room_type = None` 即"普通房间"）。
async fn insert_federated_directory_room(pool: &sqlx::PgPool, room_id: &str, name: &str, room_type: Option<&str>) {
    sqlx::query(
        "INSERT INTO rooms (room_id, creator, is_public, room_version, created_ts, name) \
         VALUES ($1, '@fed:localhost', TRUE, '10', 1700000000000, $2)",
    )
    .bind(room_id)
    .bind(name)
    .execute(pool)
    .await
    .expect("insert fixture room");
    sqlx::query(
        "INSERT INTO room_summaries (room_id, room_type, is_space, updated_ts, created_ts) \
         VALUES ($1, $2, $3, 1700000000000, 1700000000000)",
    )
    .bind(room_id)
    .bind(room_type)
    .bind(room_type == Some("m.space"))
    .execute(pool)
    .await
    .expect("insert fixture room summary");
}

/// 签名的 `POST /_matrix/federation/v1/publicRooms`，返回 `(状态码, JSON)`。
async fn post_signed_federated_public_rooms(
    app: &axum::Router,
    key_id: &str,
    signing_key: &ed25519_dalek::SigningKey,
    body: &Value,
) -> (StatusCode, Value) {
    let request = signed_federation_request(
        "POST",
        "/_matrix/federation/v1/publicRooms",
        "localhost",
        key_id,
        signing_key,
        Some(body),
    );
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 256 * 1024).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

/// **D-110**：federation 侧必须真的按 `filter.generic_search_term` 过滤（此前该 handler 只读
/// `limit`，`filter` 被静默忽略）；`total_room_count_estimate` 必须是**过滤后**的匹配数。
#[tokio::test]
async fn test_federation_public_rooms_post_filter_generic_search_term_filters_the_directory() {
    let key_id = "ed25519:test";
    let signing_key_seed = [21u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some((app, pool)) = setup_federation_test_app_with_pool(key_id, &signing_key_b64).await else {
        return;
    };

    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let term = format!("zzfed{suffix}");
    let matching = format!("!fed_match_{suffix}:localhost");
    let other = format!("!fed_other_{suffix}:localhost");
    insert_federated_directory_room(&pool, &matching, &format!("{term} federated directory room"), None).await;
    insert_federated_directory_room(&pool, &other, "unrelated federated public room", None).await;

    let (status, payload) = post_signed_federated_public_rooms(
        &app,
        key_id,
        &signing_key,
        &json!({"limit": 50, "filter": {"generic_search_term": term}}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "federation POST /publicRooms 必须成功：{payload}");
    let chunk = payload["chunk"].as_array().expect("响应的 chunk 必须是数组");
    let ids: Vec<&str> = chunk.iter().filter_map(|room| room["room_id"].as_str()).collect();

    assert!(ids.contains(&matching.as_str()), "federation 搜索必须返回命中的房间：{ids:?}");
    assert!(!ids.contains(&other.as_str()), "federation 搜索不得返回不匹配的房间（filter 未被忽略的证据）：{ids:?}");

    let needle = term.to_lowercase();
    for room in chunk {
        let hit = ["name", "topic", "canonical_alias"]
            .iter()
            .any(|field| room[*field].as_str().is_some_and(|value| value.to_lowercase().contains(&needle)));
        assert!(hit, "federation chunk 里出现了不匹配搜索词的房间：{room}");
    }

    let expected_total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM rooms r WHERE r.is_public = TRUE \
         AND (LOWER(r.name) LIKE $1 OR LOWER(r.topic) LIKE $1 OR LOWER(r.canonical_alias) LIKE $1)",
    )
    .bind(format!("%{needle}%"))
    .fetch_one(&*pool)
    .await
    .expect("count matching rooms");
    assert_eq!(
        payload["total_room_count_estimate"].as_i64(),
        Some(expected_total),
        "federation 的 total_room_count_estimate 必须是**过滤后**的匹配总数：{payload}"
    );
}

/// **D-110**：federation 侧与 C-S 侧共用同一份解析器 ⇒ `filter.room_types` 在 federation 上
/// 同样生效（搜索路径与列表路径都要应用，不允许"只接一半"）。
#[tokio::test]
async fn test_federation_public_rooms_post_filter_room_types_selects_normal_and_space_rooms() {
    let key_id = "ed25519:test";
    let signing_key_seed = [22u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some((app, pool)) = setup_federation_test_app_with_pool(key_id, &signing_key_b64).await else {
        return;
    };

    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let prefix = format!("zzfedrt{suffix}");
    let normal = format!("!fedrt_normal_{suffix}:localhost");
    let space = format!("!fedrt_space_{suffix}:localhost");
    insert_federated_directory_room(&pool, &normal, &format!("{prefix} normal"), None).await;
    insert_federated_directory_room(&pool, &space, &format!("{prefix} space"), Some("m.space")).await;

    let count_space: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM rooms r LEFT JOIN room_summaries rs ON rs.room_id = r.room_id \
         WHERE r.is_public = TRUE AND rs.room_type = ANY(ARRAY['m.space'])",
    )
    .fetch_one(&*pool)
    .await
    .unwrap();

    // 搜索路径（带 generic_search_term）+ 类型过滤：只要空间，普通房间必须被排除
    let (status, payload) = post_signed_federated_public_rooms(
        &app,
        key_id,
        &signing_key,
        &json!({"limit": 50, "filter": {"room_types": ["m.space"], "generic_search_term": prefix}}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "federation POST /publicRooms 必须成功：{payload}");
    let ids: Vec<&str> = payload["chunk"]
        .as_array()
        .expect("chunk 必须是数组")
        .iter()
        .filter_map(|room| room["room_id"].as_str())
        .collect();
    assert!(ids.contains(&space.as_str()), "类型过滤必须保留空间：{ids:?}");
    assert!(!ids.contains(&normal.as_str()), "类型过滤必须排除普通房间：{ids:?}");
    assert_eq!(
        payload["total_room_count_estimate"].as_i64(),
        Some(count_space),
        "带 room_types 的计数必须用同一谓词：{payload}"
    );

    // 列表路径（无搜索词）+ `[null]`：只要普通房间
    let (status, payload) = post_signed_federated_public_rooms(
        &app,
        key_id,
        &signing_key,
        &json!({"limit": 50, "filter": {"room_types": [null]}}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "federation POST /publicRooms 必须成功：{payload}");
    let ids: Vec<&str> = payload["chunk"]
        .as_array()
        .expect("chunk 必须是数组")
        .iter()
        .filter_map(|room| room["room_id"].as_str())
        .collect();
    assert!(ids.contains(&normal.as_str()), "`[null]` 必须保留普通房间：{ids:?}");
    assert!(!ids.contains(&space.as_str()), "`[null]` 必须排除空间：{ids:?}");
}

/// **D-110 + D-111**：形状非法 / 本仓不支持的 filter 在 federation 侧同样 **400 `M_INVALID_PARAM`**；
/// `RoomNetwork` 的两个字段按规范在**请求体顶层**，放进 `filter` 里属形状非法。
#[tokio::test]
async fn test_federation_public_rooms_post_filter_rejects_unsupported_and_malformed_fields() {
    let key_id = "ed25519:test";
    let signing_key_seed = [23u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some((app, _pool)) = setup_federation_test_app_with_pool(key_id, &signing_key_b64).await else {
        return;
    };

    for (label, body) in [
        ("include_all_networks=true（顶层）", json!({"include_all_networks": true})),
        ("third_party_instance_id（顶层）", json!({"third_party_instance_id": "irc"})),
        ("include_all_networks 错放进 filter", json!({"filter": {"include_all_networks": true}})),
        ("third_party_instance_id 错放进 filter", json!({"filter": {"third_party_instance_id": "irc"}})),
        ("filter 不是对象", json!({"filter": "nope"})),
        ("room_types 不是数组", json!({"filter": {"room_types": "m.space"}})),
        ("room_types 条目非法", json!({"filter": {"room_types": [42]}})),
        ("generic_search_term 不是字符串", json!({"filter": {"generic_search_term": 42}})),
    ] {
        let (status, payload) = post_signed_federated_public_rooms(&app, key_id, &signing_key, &body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{label}: federation 侧必须 400：{payload}");
        assert_eq!(payload["errcode"].as_str(), Some("M_INVALID_PARAM"), "{label}: 必须是 M_INVALID_PARAM：{payload}");
    }

    // `include_all_networks: false`（顶层）是规范默认值 ⇒ 接受（与 C-S 侧一致）
    let (status, payload) = post_signed_federated_public_rooms(
        &app,
        key_id,
        &signing_key,
        &json!({"limit": 5, "include_all_networks": false}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "include_all_networks=false 必须被接受：{payload}");
}
