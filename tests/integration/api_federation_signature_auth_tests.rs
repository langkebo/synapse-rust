use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use base64::engine::general_purpose::STANDARD_NO_PAD;
use base64::Engine as _;
use ed25519_dalek::Signer;
use serde_json::{json, Value};
use std::sync::Arc;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::federation::signing::canonical_federation_request_bytes;
use synapse_rust::services::ServiceContainer;
use synapse_rust::web::AppState;
use tower::ServiceExt;

async fn setup_federation_ingress_app_with<F>(
    server_name: &str,
    key_id: &str,
    signing_key_b64: &str,
    configure: F,
) -> Option<(axum::Router, AppState)>
where
    F: FnOnce(&mut ServiceContainer),
{
    let pool = crate::get_test_pool().await?;
    let mut container = ServiceContainer::new_test_with_pool(pool);
    container.config.server.name = server_name.to_string();
    container.server_name = server_name.to_string();
    container.config.federation.enabled = true;
    container.config.federation.allow_ingress = true;
    container.config.federation.server_name = server_name.to_string();
    container.config.federation.key_id = Some(key_id.to_string());
    container.config.federation.signing_key = Some(signing_key_b64.to_string());
    configure(&mut container);
    let cache = Arc::new(CacheManager::new(CacheConfig::default()));
    let state = AppState::new(container, cache);
    let app = synapse_rust::web::create_router(state.clone());
    Some((app, state))
}

async fn setup_federation_ingress_app(
    server_name: &str,
    key_id: &str,
    signing_key_b64: &str,
) -> Option<axum::Router> {
    let (app, _) =
        setup_federation_ingress_app_with(server_name, key_id, signing_key_b64, |_| {}).await?;
    Some(app)
}

fn signed_request(
    method: &str,
    uri: &str,
    server_name: &str,
    key_id: &str,
    signing_key: &ed25519_dalek::SigningKey,
    content: Option<&Value>,
) -> Request<Body> {
    let signed_bytes =
        canonical_federation_request_bytes(method, uri, server_name, server_name, content);
    let sig = signing_key.sign(&signed_bytes);
    let sig_b64 = STANDARD_NO_PAD.encode(sig.to_bytes());

    let mut builder = Request::builder().method(method).uri(uri).header(
        "Authorization",
        format!(
            "X-Matrix origin=\"{}\",key=\"{}\",sig=\"{}\"",
            server_name, key_id, sig_b64
        ),
    );

    if content.is_some() {
        builder = builder.header("Content-Type", "application/json");
    }

    builder
        .body(Body::from(
            content.map(Value::to_string).unwrap_or_default(),
        ))
        .unwrap()
}

#[tokio::test]
async fn test_federation_protected_route_rejects_missing_signature() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [7u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);

    let Some(app) = setup_federation_ingress_app(server_name, key_id, &signing_key_b64).await
    else {
        return;
    };

    let uri = format!(
        "/_matrix/federation/v1/exchange_third_party_invite/!testroom:{}",
        server_name
    );
    let body = json!({ "invite": { "display_name": "test" } }).to_string();

    let request = Request::builder()
        .method("PUT")
        .uri(&uri)
        .header("Content-Type", "application/json")
        .body(Body::from(body))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_UNAUTHORIZED");
}

#[tokio::test]
async fn test_federation_protected_route_rejects_invalid_signature() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [7u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);

    let Some(app) = setup_federation_ingress_app(server_name, key_id, &signing_key_b64).await
    else {
        return;
    };

    let uri = format!(
        "/_matrix/federation/v1/exchange_third_party_invite/!testroom:{}",
        server_name
    );
    let body = json!({ "invite": { "display_name": "test" } }).to_string();

    let request = Request::builder()
        .method("PUT")
        .uri(&uri)
        .header(
            "Authorization",
            format!(
                "X-Matrix origin=\"{}\",key=\"{}\",sig=\"{}\"",
                server_name, key_id, "invalid"
            ),
        )
        .header("Content-Type", "application/json")
        .body(Body::from(body))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_UNAUTHORIZED");
}

#[tokio::test]
async fn test_federation_protected_route_allows_valid_signature_and_reaches_handler() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [7u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some(app) = setup_federation_ingress_app(server_name, key_id, &signing_key_b64).await
    else {
        return;
    };

    let uri = format!(
        "/_matrix/federation/v1/exchange_third_party_invite/!testroom:{}",
        server_name
    );
    let content = json!({ "invite": { "display_name": "test" } });
    let request = signed_request(
        "PUT",
        &uri,
        server_name,
        key_id,
        &signing_key,
        Some(&content),
    );

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_UNRECOGNIZED");
}

#[tokio::test]
async fn test_federation_send_transaction_processes_presence_edu_when_enabled() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [11u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some((app, state)) =
        setup_federation_ingress_app_with(server_name, key_id, &signing_key_b64, |container| {
            container.config.federation.process_inbound_edus = true;
            container.config.federation.process_inbound_presence_edus = true;
        })
        .await
    else {
        return;
    };

    let user_id = format!("@presence_{}:{}", rand::random::<u32>(), server_name);
    state
        .services
        .user_storage
        .create_user(
            &user_id,
            &format!("presence_{}", rand::random::<u32>()),
            None,
            false,
        )
        .await
        .unwrap();

    let uri = "/_matrix/federation/v1/send/txn-presence";
    let content = json!({
        "origin": server_name,
        "pdus": [],
        "edus": [
            {
                "edu_type": "m.presence",
                "content": {
                    "push": [
                        {
                            "user_id": user_id,
                            "presence": "online",
                            "status_msg": "ready"
                        }
                    ]
                }
            }
        ]
    });

    let response = app
        .oneshot(signed_request(
            "PUT",
            uri,
            server_name,
            key_id,
            &signing_key,
            Some(&content),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let presence = state
        .services
        .presence_storage
        .get_presence(&user_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(presence.0, "online");
    assert_eq!(presence.1.as_deref(), Some("ready"));

    let processed = state
        .services
        .metrics
        .get_counter("federation_inbound_presence_processed_total")
        .unwrap()
        .get();
    assert_eq!(processed, 1);
}

#[tokio::test]
async fn test_federation_make_join_returns_429_when_join_lane_is_saturated() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [13u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some((app, state)) =
        setup_federation_ingress_app_with(server_name, key_id, &signing_key_b64, |container| {
            container.config.federation.join_max_concurrency = 1;
            container.config.federation.join_acquire_timeout_ms = 1;
        })
        .await
    else {
        return;
    };

    let permit = state
        .federation_join_semaphore
        .clone()
        .acquire_owned()
        .await
        .unwrap();

    let uri = format!(
        "/_matrix/federation/v1/make_join/!room:{} /@alice:{}",
        server_name, server_name
    )
    .replace(" ", "");

    let response = app
        .oneshot(signed_request(
            "GET",
            &uri,
            server_name,
            key_id,
            &signing_key,
            None,
        ))
        .await
        .unwrap();
    drop(permit);

    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    let limited = state
        .services
        .metrics
        .get_counter("federation_join_429_total")
        .unwrap()
        .get();
    assert!(limited >= 1);
}
