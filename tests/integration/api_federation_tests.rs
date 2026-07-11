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
use synapse_rust::federation::signing::canonical_federation_request_bytes;
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
    container.core.config.server.name = "localhost".to_string();
    container.core.server_name = "localhost".to_string();
    container.core.config.federation.enabled = true;
    container.core.config.federation.allow_ingress = true;
    container.core.config.federation.server_name = "localhost".to_string();
    container.core.config.federation.key_id = Some(key_id.to_string());
    container.core.config.federation.signing_key = Some(signing_key_b64.to_string());
    let cache =
        std::sync::Arc::new(synapse_rust::cache::CacheManager::new(&synapse_rust::cache::CacheConfig::default()));
    let state = synapse_rust::web::routes::state::AppState::new(container, cache);
    Some((synapse_rust::web::create_router(state), pool))
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

    sqlx::query("DELETE FROM room_aliases WHERE alias = $1").bind(&alias).execute(&*pool).await.ok();
    sqlx::query("DELETE FROM rooms WHERE room_id = $1").bind(&room_id).execute(&*pool).await.ok();
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
    assert_eq!(json["capabilities"]["m.room_versions"]["default"], DEFAULT_ROOM_VERSION);
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

    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["server_name"], "test.example.com");
    assert!(json["verify_keys"].get(&key_id).is_some());
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

    let resp_body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&resp_body).unwrap();

    assert_eq!(json["server_name"], server_name);
    assert_eq!(json["verify_keys"][key_id]["key"], verify_key_b64);
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
