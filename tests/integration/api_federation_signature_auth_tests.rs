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

async fn setup_federation_ingress_app(
    server_name: &str,
    key_id: &str,
    signing_key_b64: &str,
) -> Option<axum::Router> {
    let pool = crate::get_test_pool().await?;
    let mut container = ServiceContainer::new_test_with_pool(pool);
    container.config.server.name = server_name.to_string();
    container.server_name = server_name.to_string();
    container.config.federation.enabled = true;
    container.config.federation.allow_ingress = true;
    container.config.federation.server_name = server_name.to_string();
    container.config.federation.key_id = Some(key_id.to_string());
    container.config.federation.signing_key = Some(signing_key_b64.to_string());
    let cache = Arc::new(CacheManager::new(CacheConfig::default()));
    let state = AppState::new(container, cache);
    Some(synapse_rust::web::create_router(state))
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

    let signed_bytes =
        canonical_federation_request_bytes("PUT", &uri, server_name, server_name, Some(&content));
    let sig = signing_key.sign(&signed_bytes);
    let sig_b64 = STANDARD_NO_PAD.encode(sig.to_bytes());

    let request = Request::builder()
        .method("PUT")
        .uri(&uri)
        .header(
            "Authorization",
            format!(
                "X-Matrix origin=\"{}\",key=\"{}\",sig=\"{}\"",
                server_name, key_id, sig_b64
            ),
        )
        .header("Content-Type", "application/json")
        .body(Body::from(content.to_string()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_UNRECOGNIZED");
}
