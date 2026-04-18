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
use synapse_rust::e2ee::{DeviceKeys, KeyUploadRequest};
use synapse_rust::federation::signing::canonical_federation_request_bytes;
use synapse_rust::services::ServiceContainer;
use synapse_rust::storage::space::{AddChildRequest, CreateSpaceRequest};
use synapse_rust::storage::{CreateOpenIdTokenRequest, OpenIdTokenStorage};
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

async fn insert_join_membership(state: &AppState, room_id: &str, user_id: &str, joined_ts: i64) {
    sqlx::query(
        r#"
        INSERT INTO room_memberships (room_id, user_id, membership, joined_ts)
        VALUES ($1, $2, 'join', $3)
        "#,
    )
    .bind(room_id)
    .bind(user_id)
    .bind(joined_ts)
    .execute(&*state.services.room_storage.pool)
    .await
    .unwrap();
}

async fn upload_test_device_keys(
    state: &AppState,
    user_id: &str,
    device_id: &str,
    include_one_time_key: bool,
) {
    state
        .services
        .device_storage
        .create_device(device_id, user_id, Some("Federation test device"))
        .await
        .unwrap();

    state
        .services
        .device_keys_service
        .upload_keys(KeyUploadRequest {
            device_keys: Some(DeviceKeys {
                user_id: user_id.to_string(),
                device_id: device_id.to_string(),
                algorithms: vec!["curve25519".to_string(), "ed25519".to_string()],
                keys: json!({
                    format!("curve25519:{}", device_id): format!("curve-key-{}", device_id),
                    format!("ed25519:{}", device_id): format!("ed-key-{}", device_id),
                }),
                signatures: json!({
                    user_id: {
                        format!("ed25519:{}", device_id): "signature"
                    }
                }),
                unsigned: None,
            }),
            one_time_keys: include_one_time_key.then(|| {
                json!({
                    "OTK1": {
                        "key": format!("otk-public-{}", device_id),
                        "signatures": {}
                    }
                })
            }),
        })
        .await
        .unwrap();
}

async fn create_shared_room_for_users(
    state: &AppState,
    room_id: &str,
    creator_user_id: &str,
    other_user_id: &str,
) {
    state
        .services
        .room_storage
        .create_room(room_id, creator_user_id, "public", "10", false)
        .await
        .unwrap();

    let joined_ts = chrono::Utc::now().timestamp_millis();
    insert_join_membership(state, room_id, creator_user_id, joined_ts).await;
    insert_join_membership(state, room_id, other_user_id, joined_ts + 1).await;
}

fn tiny_png() -> Vec<u8> {
    vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90,
        0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63, 0xF8,
        0xCF, 0xC0, 0x00, 0x00, 0x03, 0x01, 0x01, 0x00, 0xC9, 0xFE, 0x92, 0xEF, 0x00, 0x00, 0x00,
        0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ]
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
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_NOT_FOUND");
}

#[tokio::test]
async fn test_federation_exchange_third_party_invite_rejects_sender_domain_mismatch() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [8u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some((app, state)) =
        setup_federation_ingress_app_with(server_name, key_id, &signing_key_b64, |_| {}).await
    else {
        return;
    };

    let room_id = format!("!thirdparty:{}", server_name);
    let creator = "@creator:test.example.com".to_string();
    state
        .services
        .user_storage
        .create_user(&creator, "creator", None, false)
        .await
        .unwrap();
    state
        .services
        .room_storage
        .create_room(&room_id, &creator, "public", "1", true)
        .await
        .unwrap();

    let uri = format!(
        "/_matrix/federation/v1/exchange_third_party_invite/{}",
        room_id
    );
    let body = json!({
        "event_id": format!("$invite:{}", server_name),
        "room_id": room_id,
        "type": "m.room.member",
        "sender": "@mallory:evil.example",
        "state_key": "@alice:target.example",
        "content": {
            "membership": "invite"
        }
    });
    let response = app
        .oneshot(signed_request(
            "PUT",
            &uri,
            server_name,
            key_id,
            &signing_key,
            Some(&body),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_FORBIDDEN");
}

#[tokio::test]
async fn test_federation_thirdparty_invite_rejects_sender_domain_mismatch() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [9u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some((app, state)) =
        setup_federation_ingress_app_with(server_name, key_id, &signing_key_b64, |_| {}).await
    else {
        return;
    };

    let room_id = format!("!thirdpartyinvite:{}", server_name);
    let creator = "@creator:test.example.com".to_string();
    state
        .services
        .user_storage
        .create_user(&creator, "creator", None, false)
        .await
        .unwrap();
    state
        .services
        .room_storage
        .create_room(&room_id, &creator, "public", "1", true)
        .await
        .unwrap();

    let uri = "/_matrix/federation/v1/thirdparty/invite";
    let body = json!({
        "room_id": room_id,
        "invitee": "@alice:target.example",
        "sender": "@mallory:evil.example"
    });
    let response = app
        .oneshot(signed_request(
            "POST",
            uri,
            server_name,
            key_id,
            &signing_key,
            Some(&body),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_FORBIDDEN");
}

#[tokio::test]
async fn test_federation_make_join_rejects_user_domain_mismatch() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [11u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some(app) = setup_federation_ingress_app(server_name, key_id, &signing_key_b64).await
    else {
        return;
    };

    let uri = format!(
        "/_matrix/federation/v1/make_join/!joincheck:{}/@mallory:evil.example",
        server_name
    );
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

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_FORBIDDEN");
}

#[tokio::test]
async fn test_federation_make_leave_rejects_user_domain_mismatch() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [12u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some(app) = setup_federation_ingress_app(server_name, key_id, &signing_key_b64).await
    else {
        return;
    };

    let uri = format!(
        "/_matrix/federation/v1/make_leave/!leavecheck:{}/@mallory:evil.example",
        server_name
    );
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

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_FORBIDDEN");
}

#[tokio::test]
async fn test_federation_knock_rejects_user_domain_mismatch() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [21u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some(app) = setup_federation_ingress_app(server_name, key_id, &signing_key_b64).await
    else {
        return;
    };

    let room_id = format!("!knockcheck:{}", server_name);
    let user_id = "@mallory:evil.example";
    let uri = format!("/_matrix/federation/v1/knock/{}/{}", room_id, user_id);
    let content = json!({
        "room_id": room_id,
        "type": "m.room.member",
        "sender": user_id,
        "state_key": user_id,
        "origin": server_name,
        "content": {
            "membership": "knock"
        }
    });
    let response = app
        .oneshot(signed_request(
            "PUT",
            &uri,
            server_name,
            key_id,
            &signing_key,
            Some(&content),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_FORBIDDEN");
}

#[tokio::test]
async fn test_federation_invite_v2_rejects_sender_domain_mismatch() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [22u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some(app) = setup_federation_ingress_app(server_name, key_id, &signing_key_b64).await
    else {
        return;
    };

    let room_id = format!("!invitecheck:{}", server_name);
    let event_id = "$invitecheck";
    let uri = format!("/_matrix/federation/v2/invite/{}/{}", room_id, event_id);
    let content = json!({
        "event_id": event_id,
        "room_id": room_id,
        "type": "m.room.member",
        "sender": "@mallory:evil.example",
        "state_key": format!("@alice:{}", server_name),
        "origin": server_name,
        "content": {
            "membership": "invite"
        }
    });
    let response = app
        .oneshot(signed_request(
            "PUT",
            &uri,
            server_name,
            key_id,
            &signing_key,
            Some(&content),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_FORBIDDEN");
}

#[tokio::test]
async fn test_federation_get_state_rejects_server_without_joined_member() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [23u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some((app, state)) =
        setup_federation_ingress_app_with(server_name, key_id, &signing_key_b64, |_| {}).await
    else {
        return;
    };

    let room_id = format!("!statecheck:{}", server_name);
    let creator = "@creator:local.example".to_string();

    state
        .services
        .user_storage
        .create_user(&creator, "creator", None, false)
        .await
        .unwrap();
    state
        .services
        .room_storage
        .create_room(&room_id, &creator, "public", "1", true)
        .await
        .unwrap();
    state
        .services
        .member_storage
        .add_member(&room_id, &creator, "join", None, None, None)
        .await
        .unwrap();

    let uri = format!("/_matrix/federation/v1/state/{}", room_id);
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

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_NOT_FOUND");
}

#[tokio::test]
async fn test_federation_state_and_backfill_endpoints_return_spec_shaped_minimal_payloads() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [39u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some((app, state)) =
        setup_federation_ingress_app_with(server_name, key_id, &signing_key_b64, |_| {}).await
    else {
        return;
    };

    let room_id = format!("!state-shape:{}", server_name);
    let creator = "@creator:local.example".to_string();
    let remote_member = format!("@member:{}", server_name);

    state
        .services
        .user_storage
        .create_user(&creator, "creator", None, false)
        .await
        .unwrap();
    state
        .services
        .user_storage
        .create_user(&remote_member, "member", None, false)
        .await
        .unwrap();
    state
        .services
        .room_storage
        .create_room(&room_id, &creator, "invite", "1", false)
        .await
        .unwrap();
    state
        .services
        .member_storage
        .add_member(&room_id, &creator, "join", None, None, None)
        .await
        .unwrap();
    state
        .services
        .member_storage
        .add_member(&room_id, &remote_member, "join", None, None, None)
        .await
        .unwrap();

    for (event_id, event_type, content, ts) in [
        (
            "$power-shape",
            "m.room.power_levels",
            json!({ "users_default": 0 }),
            200_i64,
        ),
        (
            "$a-name-shape",
            "m.room.name",
            json!({ "name": "Alpha" }),
            100_i64,
        ),
        (
            "$z-topic-shape",
            "m.room.topic",
            json!({ "topic": "Zulu" }),
            100_i64,
        ),
        (
            "$avatar-latest-shape",
            "m.room.avatar",
            json!({ "url": "mxc://test.example.com/latest-avatar" }),
            400_i64,
        ),
    ] {
        state
            .services
            .event_storage
            .create_event(
                synapse_rust::storage::event::CreateEventParams {
                    event_id: event_id.to_string(),
                    room_id: room_id.clone(),
                    user_id: creator.clone(),
                    event_type: event_type.to_string(),
                    content,
                    state_key: Some(String::new()),
                    origin_server_ts: ts,
                },
                None,
            )
            .await
            .unwrap();
    }

    for (event_id, ts) in [
        ("$olderer-message-shape", 110_i64),
        ("$older-message-shape", 120_i64),
        ("$seed-shape", 250_i64),
        ("$a-message-shape", 300_i64),
        ("$b-message-shape", 350_i64),
    ] {
        state
            .services
            .event_storage
            .create_event(
                synapse_rust::storage::event::CreateEventParams {
                    event_id: event_id.to_string(),
                    room_id: room_id.clone(),
                    user_id: creator.clone(),
                    event_type: "m.room.message".to_string(),
                    content: json!({ "msgtype": "m.text", "body": event_id }),
                    state_key: None,
                    origin_server_ts: ts,
                },
                None,
            )
            .await
            .unwrap();
    }

    let get_state_response = app
        .clone()
        .oneshot(signed_request(
            "GET",
            &format!("/_matrix/federation/v1/state/{}", room_id),
            server_name,
            key_id,
            &signing_key,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(get_state_response.status(), StatusCode::OK);
    let get_state_body = axum::body::to_bytes(get_state_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let get_state_json: Value = serde_json::from_slice(&get_state_body).unwrap();
    let state_pdus = get_state_json["pdus"].as_array().unwrap();
    let auth_chain = get_state_json["auth_chain"].as_array().unwrap();

    assert_eq!(get_state_json["room_id"], room_id);
    assert_eq!(get_state_json["origin"], server_name);
    assert_eq!(state_pdus.len(), 4);
    assert_eq!(state_pdus[0]["event_id"], "$avatar-latest-shape");
    assert_eq!(state_pdus[1]["event_id"], "$power-shape");
    assert_eq!(state_pdus[2]["event_id"], "$a-name-shape");
    assert_eq!(state_pdus[3]["event_id"], "$z-topic-shape");
    assert_eq!(auth_chain.len(), 4);
    assert!(get_state_json.get("state").is_none());
    assert!(state_pdus[0].get("unsigned").is_none());
    assert!(state_pdus[0].get("depth").is_none());

    let get_state_at_event_response = app
        .clone()
        .oneshot(signed_request(
            "GET",
            &format!(
                "/_matrix/federation/v1/state/{}?event_id=$power-shape",
                room_id
            ),
            server_name,
            key_id,
            &signing_key,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(get_state_at_event_response.status(), StatusCode::OK);
    let get_state_at_event_body =
        axum::body::to_bytes(get_state_at_event_response.into_body(), usize::MAX)
            .await
            .unwrap();
    let get_state_at_event_json: Value = serde_json::from_slice(&get_state_at_event_body).unwrap();
    let state_at_event_pdus = get_state_at_event_json["pdus"].as_array().unwrap();
    assert_eq!(state_at_event_pdus.len(), 3);
    assert_eq!(state_at_event_pdus[0]["event_id"], "$power-shape");
    assert_eq!(state_at_event_pdus[1]["event_id"], "$a-name-shape");
    assert_eq!(state_at_event_pdus[2]["event_id"], "$z-topic-shape");

    let get_state_ids_response = app
        .clone()
        .oneshot(signed_request(
            "GET",
            &format!("/_matrix/federation/v1/state_ids/{}", room_id),
            server_name,
            key_id,
            &signing_key,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(get_state_ids_response.status(), StatusCode::OK);
    let get_state_ids_body = axum::body::to_bytes(get_state_ids_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let get_state_ids_json: Value = serde_json::from_slice(&get_state_ids_body).unwrap();
    let pdu_ids = get_state_ids_json["pdu_ids"].as_array().unwrap();
    let auth_chain_ids = get_state_ids_json["auth_chain_ids"].as_array().unwrap();

    assert_eq!(get_state_ids_json["room_id"], room_id);
    assert_eq!(get_state_ids_json["origin"], server_name);
    assert_eq!(pdu_ids.len(), 4);
    assert_eq!(pdu_ids[0], "$avatar-latest-shape");
    assert_eq!(pdu_ids[1], "$power-shape");
    assert_eq!(pdu_ids[2], "$a-name-shape");
    assert_eq!(pdu_ids[3], "$z-topic-shape");
    assert_eq!(auth_chain_ids.len(), 4);
    assert!(get_state_ids_json.get("state_ids").is_none());

    let get_state_ids_at_event_response = app
        .clone()
        .oneshot(signed_request(
            "GET",
            &format!(
                "/_matrix/federation/v1/state_ids/{}?event_id=$power-shape",
                room_id
            ),
            server_name,
            key_id,
            &signing_key,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(get_state_ids_at_event_response.status(), StatusCode::OK);
    let get_state_ids_at_event_body =
        axum::body::to_bytes(get_state_ids_at_event_response.into_body(), usize::MAX)
            .await
            .unwrap();
    let get_state_ids_at_event_json: Value =
        serde_json::from_slice(&get_state_ids_at_event_body).unwrap();
    let pdu_ids_at_event = get_state_ids_at_event_json["pdu_ids"].as_array().unwrap();
    assert_eq!(pdu_ids_at_event.len(), 3);
    assert_eq!(pdu_ids_at_event[0], "$power-shape");
    assert_eq!(pdu_ids_at_event[1], "$a-name-shape");
    assert_eq!(pdu_ids_at_event[2], "$z-topic-shape");

    let get_event_auth_response = app
        .clone()
        .oneshot(signed_request(
            "GET",
            &format!(
                "/_matrix/federation/v1/get_event_auth/{}/{}",
                room_id, "$power-shape"
            ),
            server_name,
            key_id,
            &signing_key,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(get_event_auth_response.status(), StatusCode::OK);
    let get_event_auth_body = axum::body::to_bytes(get_event_auth_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let get_event_auth_json: Value = serde_json::from_slice(&get_event_auth_body).unwrap();
    let get_event_auth_chain = get_event_auth_json["auth_chain"].as_array().unwrap();
    assert_eq!(get_event_auth_chain.len(), 3);
    assert_eq!(get_event_auth_chain[0]["event_id"], "$power-shape");
    assert_eq!(get_event_auth_chain[1]["event_id"], "$a-name-shape");
    assert_eq!(get_event_auth_chain[2]["event_id"], "$z-topic-shape");

    let backfill_response = app
        .oneshot(signed_request(
            "GET",
            &format!(
                "/_matrix/federation/v1/backfill/{}?v=$seed-shape&limit=2",
                room_id
            ),
            server_name,
            key_id,
            &signing_key,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(backfill_response.status(), StatusCode::OK);
    let backfill_body = axum::body::to_bytes(backfill_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let backfill_json: Value = serde_json::from_slice(&backfill_body).unwrap();
    let backfill_pdus = backfill_json["pdus"].as_array().unwrap();
    let backfill_auth_chain = backfill_json["auth_chain"].as_array().unwrap();

    assert_eq!(backfill_json["origin"], server_name);
    assert!(backfill_json["origin_server_ts"].as_i64().is_some());
    assert_eq!(backfill_pdus.len(), 2);
    assert!(backfill_pdus
        .iter()
        .all(|event| event["origin_server_ts"].as_i64().unwrap_or(i64::MAX) < 250));
    assert!(backfill_pdus
        .iter()
        .all(|event| event["event_id"] != "$seed-shape"));
    assert!(backfill_pdus
        .iter()
        .all(|event| event["event_id"] != "$a-message-shape"));
    assert!(backfill_pdus
        .iter()
        .all(|event| event["event_id"] != "$b-message-shape"));
    assert!(backfill_pdus
        .iter()
        .all(|event| event["event_id"] != "$avatar-latest-shape"));
    assert_eq!(backfill_auth_chain.len(), 3);
    assert!(backfill_json.get("limit").is_none());
    assert!(backfill_pdus[0].get("prev_events").is_none());
}

#[tokio::test]
async fn test_federation_state_endpoints_reject_event_ids_from_other_rooms() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [40u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some((app, state)) =
        setup_federation_ingress_app_with(server_name, key_id, &signing_key_b64, |_| {}).await
    else {
        return;
    };

    let target_room_id = format!("!state-target:{}", server_name);
    let other_room_id = format!("!state-other:{}", server_name);
    let creator = "@creator:local.example".to_string();
    let remote_member = format!("@member:{}", server_name);

    for user_id in [&creator, &remote_member] {
        state
            .services
            .user_storage
            .create_user(user_id, user_id, None, false)
            .await
            .unwrap();
    }

    state
        .services
        .room_storage
        .create_room(&target_room_id, &creator, "invite", "1", false)
        .await
        .unwrap();
    state
        .services
        .member_storage
        .add_member(&target_room_id, &creator, "join", None, None, None)
        .await
        .unwrap();
    state
        .services
        .member_storage
        .add_member(&target_room_id, &remote_member, "join", None, None, None)
        .await
        .unwrap();

    state
        .services
        .room_storage
        .create_room(&other_room_id, &creator, "invite", "1", false)
        .await
        .unwrap();
    state
        .services
        .member_storage
        .add_member(&other_room_id, &creator, "join", None, None, None)
        .await
        .unwrap();

    state
        .services
        .event_storage
        .create_event(
            synapse_rust::storage::event::CreateEventParams {
                event_id: "$other-room-event".to_string(),
                room_id: other_room_id.clone(),
                user_id: creator.clone(),
                event_type: "m.room.message".to_string(),
                content: json!({ "msgtype": "m.text", "body": "wrong room" }),
                state_key: None,
                origin_server_ts: 123_i64,
            },
            None,
        )
        .await
        .unwrap();

    for uri in [
        format!(
            "/_matrix/federation/v1/state/{}?event_id=$other-room-event",
            target_room_id
        ),
        format!(
            "/_matrix/federation/v1/state_ids/{}?event_id=$other-room-event",
            target_room_id
        ),
        format!(
            "/_matrix/federation/v1/backfill/{}?v=$other-room-event&limit=10",
            target_room_id
        ),
    ] {
        let response = app
            .clone()
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

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["errcode"], "M_BAD_JSON");
        assert!(json["error"]
            .as_str()
            .unwrap_or_default()
            .contains("Event does not belong to this room"));
    }
}

#[tokio::test]
async fn test_federation_get_user_devices_omits_sensitive_metadata() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [24u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some((app, state)) =
        setup_federation_ingress_app_with(server_name, key_id, &signing_key_b64, |_| {}).await
    else {
        return;
    };

    let user_id = format!("@alice:{}", server_name);
    let room_id = format!("!device-share:{}", server_name);
    state
        .services
        .user_storage
        .create_user(&user_id, "alice", None, false)
        .await
        .unwrap();
    state
        .services
        .device_storage
        .create_device("ALICEDEVICE", &user_id, Some("Alice device"))
        .await
        .unwrap();
    state
        .services
        .room_storage
        .create_room(&room_id, &user_id, "private", "1", false)
        .await
        .unwrap();
    state
        .services
        .member_storage
        .add_member(&room_id, &user_id, "join", None, None, None)
        .await
        .unwrap();

    let uri = format!("/_matrix/federation/v1/user/devices/{}", user_id);
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

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let devices = json["devices"].as_array().unwrap();
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0]["device_id"], "ALICEDEVICE");
    assert!(devices[0].get("last_seen_ip").is_none());
    assert!(devices[0].get("last_seen_ts").is_none());
}

#[tokio::test]
async fn test_federation_get_user_devices_rejects_server_without_shared_room() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [39u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some((app, state)) =
        setup_federation_ingress_app_with(server_name, key_id, &signing_key_b64, |_| {}).await
    else {
        return;
    };

    let user_id = format!("@device-target:{}", server_name);
    state
        .services
        .user_storage
        .create_user(&user_id, "device-target", None, false)
        .await
        .unwrap();
    state
        .services
        .device_storage
        .create_device("TARGETDEVICE", &user_id, Some("Target device"))
        .await
        .unwrap();

    let uri = format!("/_matrix/federation/v1/user/devices/{}", user_id);
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

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_FORBIDDEN");
}

#[tokio::test]
async fn test_federation_keys_query_filters_local_users_without_shared_rooms() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [40u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some((app, state)) =
        setup_federation_ingress_app_with(server_name, key_id, &signing_key_b64, |_| {}).await
    else {
        return;
    };

    let shared_user_id = format!("@query-shared:{}", server_name);
    let isolated_user_id = format!("@query-isolated:{}", server_name);
    let room_id = format!("!keys-query-share:{}", server_name);

    state
        .services
        .user_storage
        .create_user(&shared_user_id, "query-shared", None, false)
        .await
        .unwrap();
    state
        .services
        .user_storage
        .create_user(&isolated_user_id, "query-isolated", None, false)
        .await
        .unwrap();
    state
        .services
        .room_storage
        .create_room(&room_id, &shared_user_id, "private", "1", false)
        .await
        .unwrap();
    state
        .services
        .member_storage
        .add_member(&room_id, &shared_user_id, "join", None, None, None)
        .await
        .unwrap();

    upload_test_device_keys(&state, &shared_user_id, "SHAREDQUERY", false).await;
    upload_test_device_keys(&state, &isolated_user_id, "ISOLATEDQUERY", false).await;

    let request_body = json!({
        "device_keys": {
            shared_user_id.clone(): [],
            isolated_user_id.clone(): [],
        }
    });
    let response = app
        .oneshot(signed_request(
            "POST",
            "/_matrix/federation/v1/user/keys/query",
            server_name,
            key_id,
            &signing_key,
            Some(&request_body),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert!(json["device_keys"].get(&shared_user_id).is_some());
    assert!(json["device_keys"].get(&isolated_user_id).is_none());
}

#[tokio::test]
async fn test_federation_keys_claim_filters_local_users_without_shared_rooms() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [41u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some((app, state)) =
        setup_federation_ingress_app_with(server_name, key_id, &signing_key_b64, |_| {}).await
    else {
        return;
    };

    let shared_user_id = format!("@claim-shared:{}", server_name);
    let isolated_user_id = format!("@claim-isolated:{}", server_name);
    let room_id = format!("!keys-claim-share:{}", server_name);

    state
        .services
        .user_storage
        .create_user(&shared_user_id, "claim-shared", None, false)
        .await
        .unwrap();
    state
        .services
        .user_storage
        .create_user(&isolated_user_id, "claim-isolated", None, false)
        .await
        .unwrap();
    state
        .services
        .room_storage
        .create_room(&room_id, &shared_user_id, "private", "1", false)
        .await
        .unwrap();
    state
        .services
        .member_storage
        .add_member(&room_id, &shared_user_id, "join", None, None, None)
        .await
        .unwrap();

    upload_test_device_keys(&state, &shared_user_id, "SHAREDCLAIM", true).await;
    upload_test_device_keys(&state, &isolated_user_id, "ISOLATEDCLAIM", true).await;

    let request_body = json!({
        "one_time_keys": {
            shared_user_id.clone(): {
                "SHAREDCLAIM": "signed_curve25519"
            },
            isolated_user_id.clone(): {
                "ISOLATEDCLAIM": "signed_curve25519"
            }
        }
    });
    let response = app
        .oneshot(signed_request(
            "POST",
            "/_matrix/federation/v1/user/keys/claim",
            server_name,
            key_id,
            &signing_key,
            Some(&request_body),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert!(json["one_time_keys"].get(&shared_user_id).is_some());
    assert!(json["one_time_keys"].get(&isolated_user_id).is_none());
}

#[tokio::test]
async fn test_federation_get_room_members_rejects_server_without_joined_member() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [25u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some((app, state)) =
        setup_federation_ingress_app_with(server_name, key_id, &signing_key_b64, |_| {}).await
    else {
        return;
    };

    let room_id = format!("!memberscheck:{}", server_name);
    let creator = "@creator:local.example".to_string();

    state
        .services
        .user_storage
        .create_user(&creator, "creator", None, false)
        .await
        .unwrap();
    state
        .services
        .room_storage
        .create_room(&room_id, &creator, "public", "1", true)
        .await
        .unwrap();
    state
        .services
        .member_storage
        .add_member(&room_id, &creator, "join", None, None, None)
        .await
        .unwrap();

    let uri = format!("/_matrix/federation/v1/members/{}", room_id);
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

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_FORBIDDEN");
}

#[tokio::test]
async fn test_federation_get_room_event_rejects_server_without_joined_member() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [26u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some((app, state)) =
        setup_federation_ingress_app_with(server_name, key_id, &signing_key_b64, |_| {}).await
    else {
        return;
    };

    let room_id = format!("!eventcheck:{}", server_name);
    let creator = "@creator:local.example".to_string();
    let event_id = "$topic-check";

    state
        .services
        .user_storage
        .create_user(&creator, "creator", None, false)
        .await
        .unwrap();
    state
        .services
        .room_storage
        .create_room(&room_id, &creator, "public", "1", true)
        .await
        .unwrap();
    state
        .services
        .member_storage
        .add_member(&room_id, &creator, "join", None, None, None)
        .await
        .unwrap();
    state
        .services
        .event_storage
        .create_event(
            synapse_rust::storage::event::CreateEventParams {
                event_id: event_id.to_string(),
                room_id: room_id.clone(),
                user_id: creator.clone(),
                event_type: "m.room.topic".to_string(),
                content: json!({ "topic": "secret" }),
                state_key: Some(String::new()),
                origin_server_ts: 1_717_171_717_000i64,
            },
            None,
        )
        .await
        .unwrap();

    let uri = format!("/_matrix/federation/v1/room/{}/{}", room_id, event_id);
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

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_FORBIDDEN");
}

#[tokio::test]
async fn test_federation_event_endpoints_return_spec_shaped_pdu_response() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [38u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some((app, state)) =
        setup_federation_ingress_app_with(server_name, key_id, &signing_key_b64, |_| {}).await
    else {
        return;
    };

    let room_id = format!("!event-shape:{}", server_name);
    let event_id = "$event-shape";
    let creator = format!("@owner:{}", server_name);
    let remote_member = format!("@member:{}", server_name);

    state
        .services
        .user_storage
        .create_user(&creator, "owner", None, false)
        .await
        .unwrap();
    state
        .services
        .user_storage
        .create_user(&remote_member, "member", None, false)
        .await
        .unwrap();
    state
        .services
        .room_storage
        .create_room(&room_id, &creator, "invite", "1", false)
        .await
        .unwrap();
    state
        .services
        .member_storage
        .add_member(&room_id, &creator, "join", None, None, None)
        .await
        .unwrap();
    state
        .services
        .member_storage
        .add_member(&room_id, &remote_member, "join", None, None, None)
        .await
        .unwrap();
    state
        .services
        .event_storage
        .create_event(
            synapse_rust::storage::event::CreateEventParams {
                event_id: event_id.to_string(),
                room_id: room_id.clone(),
                user_id: creator.clone(),
                event_type: "m.room.topic".to_string(),
                content: json!({ "topic": "spec shape" }),
                state_key: Some(String::new()),
                origin_server_ts: 1_717_171_718_000i64,
            },
            None,
        )
        .await
        .unwrap();

    let get_event_response = app
        .clone()
        .oneshot(signed_request(
            "GET",
            &format!("/_matrix/federation/v1/event/{}", event_id),
            server_name,
            key_id,
            &signing_key,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(get_event_response.status(), StatusCode::OK);
    let get_event_body = axum::body::to_bytes(get_event_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let get_event_json: Value = serde_json::from_slice(&get_event_body).unwrap();
    let get_event_pdus = get_event_json["pdus"].as_array().unwrap();
    assert_eq!(get_event_json["origin"], server_name);
    assert_eq!(get_event_pdus.len(), 1);
    assert_eq!(get_event_pdus[0]["event_id"], event_id);
    assert_eq!(get_event_pdus[0]["room_id"], room_id);
    assert_eq!(get_event_pdus[0]["content"]["topic"], "spec shape");
    assert!(get_event_json.get("event_id").is_none());

    let get_room_event_response = app
        .oneshot(signed_request(
            "GET",
            &format!("/_matrix/federation/v1/room/{}/{}", room_id, event_id),
            server_name,
            key_id,
            &signing_key,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(get_room_event_response.status(), StatusCode::OK);
    let get_room_event_body = axum::body::to_bytes(get_room_event_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let get_room_event_json: Value = serde_json::from_slice(&get_room_event_body).unwrap();
    let get_room_event_pdus = get_room_event_json["pdus"].as_array().unwrap();
    assert_eq!(get_room_event_json["origin"], server_name);
    assert_eq!(get_room_event_pdus.len(), 1);
    assert_eq!(get_room_event_pdus[0]["event_id"], event_id);
    assert_eq!(get_room_event_pdus[0]["room_id"], room_id);
    assert!(get_room_event_json.get("event_id").is_none());
}

#[tokio::test]
async fn test_federation_get_missing_events_rejects_server_without_joined_member() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [27u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some((app, state)) =
        setup_federation_ingress_app_with(server_name, key_id, &signing_key_b64, |_| {}).await
    else {
        return;
    };

    let room_id = format!("!missingcheck:{}", server_name);
    let creator = "@creator:local.example".to_string();

    state
        .services
        .user_storage
        .create_user(&creator, "creator", None, false)
        .await
        .unwrap();
    state
        .services
        .room_storage
        .create_room(&room_id, &creator, "public", "1", true)
        .await
        .unwrap();
    state
        .services
        .member_storage
        .add_member(&room_id, &creator, "join", None, None, None)
        .await
        .unwrap();

    let uri = format!("/_matrix/federation/v1/get_missing_events/{}", room_id);
    let content = json!({
        "earliest_events": [],
        "latest_events": [],
        "limit": 10
    });
    let response = app
        .oneshot(signed_request(
            "POST",
            &uri,
            server_name,
            key_id,
            &signing_key,
            Some(&content),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_FORBIDDEN");
}

#[tokio::test]
async fn test_federation_timestamp_to_event_rejects_server_without_joined_member() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [28u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some((app, state)) =
        setup_federation_ingress_app_with(server_name, key_id, &signing_key_b64, |_| {}).await
    else {
        return;
    };

    let room_id = format!("!timestampcheck:{}", server_name);
    let creator = "@creator:local.example".to_string();

    state
        .services
        .user_storage
        .create_user(&creator, "creator", None, false)
        .await
        .unwrap();
    state
        .services
        .room_storage
        .create_room(&room_id, &creator, "public", "1", true)
        .await
        .unwrap();
    state
        .services
        .member_storage
        .add_member(&room_id, &creator, "join", None, None, None)
        .await
        .unwrap();

    let uri = format!(
        "/_matrix/federation/v1/timestamp_to_event/{}?ts=1717171717000",
        room_id
    );
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

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_FORBIDDEN");
}

#[tokio::test]
async fn test_federation_get_joining_rules_rejects_unjoined_server_for_invite_room() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [29u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some((app, state)) =
        setup_federation_ingress_app_with(server_name, key_id, &signing_key_b64, |_| {}).await
    else {
        return;
    };

    let room_id = format!("!joinrulesprivate:{}", server_name);
    let creator = "@creator:local.example".to_string();

    state
        .services
        .user_storage
        .create_user(&creator, "creator", None, false)
        .await
        .unwrap();
    state
        .services
        .room_storage
        .create_room(&room_id, &creator, "invite", "1", false)
        .await
        .unwrap();
    state
        .services
        .member_storage
        .add_member(&room_id, &creator, "join", None, None, None)
        .await
        .unwrap();

    let uri = format!("/_matrix/federation/v1/get_joining_rules/{}", room_id);
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

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_FORBIDDEN");
}

#[tokio::test]
async fn test_federation_get_joining_rules_returns_effective_state_event_rule() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [30u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some((app, state)) =
        setup_federation_ingress_app_with(server_name, key_id, &signing_key_b64, |_| {}).await
    else {
        return;
    };

    let room_id = format!("!joinrulesstate:{}", server_name);
    let creator = "@creator:local.example".to_string();
    let remote_member = format!("@member:{}", server_name);

    state
        .services
        .user_storage
        .create_user(&creator, "creator", None, false)
        .await
        .unwrap();
    state
        .services
        .user_storage
        .create_user(&remote_member, "member", None, false)
        .await
        .unwrap();
    state
        .services
        .room_storage
        .create_room(&room_id, &creator, "invite", "1", false)
        .await
        .unwrap();
    state
        .services
        .member_storage
        .add_member(&room_id, &creator, "join", None, None, None)
        .await
        .unwrap();
    state
        .services
        .member_storage
        .add_member(&room_id, &remote_member, "join", None, None, None)
        .await
        .unwrap();
    state
        .services
        .event_storage
        .create_event(
            synapse_rust::storage::event::CreateEventParams {
                event_id: "$join-rules-state".to_string(),
                room_id: room_id.clone(),
                user_id: creator.clone(),
                event_type: "m.room.join_rules".to_string(),
                content: json!({
                    "join_rule": "restricted",
                    "allow": [
                        {
                            "type": "m.room_membership",
                            "room_id": "!space:example.com"
                        }
                    ]
                }),
                state_key: Some(String::new()),
                origin_server_ts: 1_717_171_717_000i64,
            },
            None,
        )
        .await
        .unwrap();

    let uri = format!("/_matrix/federation/v1/get_joining_rules/{}", room_id);
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

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["join_rule"], "restricted");
    assert_eq!(
        json["allow"][0]["room_id"],
        Value::String("!space:example.com".to_string())
    );
}

#[tokio::test]
async fn test_federation_hierarchy_rejects_missing_signature() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [31u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);

    let Some(app) = setup_federation_ingress_app(server_name, key_id, &signing_key_b64).await
    else {
        return;
    };

    let room_id = format!("!hierarchy:{}", server_name);
    let uri = format!("/_matrix/federation/v1/hierarchy/{}", room_id);
    let request = Request::builder()
        .method("GET")
        .uri(&uri)
        .body(Body::empty())
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
async fn test_federation_hierarchy_returns_space_hierarchy_shape_instead_of_placeholder_summary() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [37u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some((app, state)) =
        setup_federation_ingress_app_with(server_name, key_id, &signing_key_b64, |_| {}).await
    else {
        return;
    };

    let creator = format!("@spaceadmin:{}", server_name);
    let root_room_id = format!("!federation-space-root:{}", server_name);
    let child_room_id = format!("!federation-space-child:{}", server_name);

    state
        .services
        .user_storage
        .create_user(&creator, "spaceadmin", None, false)
        .await
        .unwrap();
    state
        .services
        .room_storage
        .create_room(&root_room_id, &creator, "public", "1", true)
        .await
        .unwrap();
    state
        .services
        .room_storage
        .create_room(&child_room_id, &creator, "public", "1", true)
        .await
        .unwrap();
    state
        .services
        .space_storage
        .create_space(CreateSpaceRequest {
            room_id: root_room_id.clone(),
            name: Some("Federation Root Space".to_string()),
            topic: Some("Federation hierarchy".to_string()),
            avatar_url: None,
            creator: creator.clone(),
            join_rule: Some("public".to_string()),
            visibility: Some("public".to_string()),
            is_public: Some(true),
            parent_space_id: None,
        })
        .await
        .unwrap();
    let root_space = state
        .services
        .space_service
        .get_space_by_room(&root_room_id)
        .await
        .unwrap()
        .unwrap();
    state
        .services
        .space_storage
        .add_child(AddChildRequest {
            space_id: root_space.space_id.clone(),
            room_id: child_room_id.clone(),
            sender: creator.clone(),
            is_suggested: true,
            via_servers: vec![server_name.to_string()],
        })
        .await
        .unwrap();

    let uri = format!(
        "/_matrix/federation/v1/hierarchy/{}?max_depth=3&limit=10",
        root_room_id
    );
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

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let rooms = json["rooms"].as_array().unwrap();
    assert_eq!(rooms.len(), 1);
    assert_eq!(rooms[0]["room_id"], child_room_id);
    assert!(json.get("children").is_none());
    assert!(json.get("public").is_none());
}

#[tokio::test]
async fn test_federation_room_directory_query_rejects_unjoined_server_for_private_room() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [32u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some((app, state)) =
        setup_federation_ingress_app_with(server_name, key_id, &signing_key_b64, |_| {}).await
    else {
        return;
    };

    let room_id = format!("!dirprivate:{}", server_name);
    let creator = "@creator:local.example".to_string();

    state
        .services
        .user_storage
        .create_user(&creator, "creator", None, false)
        .await
        .unwrap();
    state
        .services
        .room_storage
        .create_room(&room_id, &creator, "invite", "1", false)
        .await
        .unwrap();
    state
        .services
        .member_storage
        .add_member(&room_id, &creator, "join", None, None, None)
        .await
        .unwrap();

    let uri = format!("/_matrix/federation/v1/query/directory/room/{}", room_id);
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

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_FORBIDDEN");
}

#[tokio::test]
async fn test_federation_state_rejects_unjoined_server_for_private_room() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [36u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some((app, state)) =
        setup_federation_ingress_app_with(server_name, key_id, &signing_key_b64, |_| {}).await
    else {
        return;
    };

    let room_id = format!("!backfillprivate:{}", server_name);
    let creator = "@creator:local.example".to_string();

    state
        .services
        .user_storage
        .create_user(&creator, "creator", None, false)
        .await
        .unwrap();
    state
        .services
        .room_storage
        .create_room(&room_id, &creator, "invite", "1", false)
        .await
        .unwrap();
    state
        .services
        .member_storage
        .add_member(&room_id, &creator, "join", None, None, None)
        .await
        .unwrap();

    let uri = format!("/_matrix/federation/v1/state/{}", room_id);
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

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_NOT_FOUND");
}

#[tokio::test]
async fn test_federation_state_ids_rejects_unjoined_server_for_private_room() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [38u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some((app, state)) =
        setup_federation_ingress_app_with(server_name, key_id, &signing_key_b64, |_| {}).await
    else {
        return;
    };

    let room_id = format!("!stateidsprivate:{}", server_name);
    let creator = "@creator:local.example".to_string();

    state
        .services
        .user_storage
        .create_user(&creator, "creator", None, false)
        .await
        .unwrap();
    state
        .services
        .room_storage
        .create_room(&room_id, &creator, "invite", "1", false)
        .await
        .unwrap();
    state
        .services
        .member_storage
        .add_member(&room_id, &creator, "join", None, None, None)
        .await
        .unwrap();

    let uri = format!("/_matrix/federation/v1/state_ids/{}", room_id);
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

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_NOT_FOUND");
}

#[tokio::test]
async fn test_federation_backfill_rejects_unjoined_server_for_private_room() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [39u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some((app, state)) =
        setup_federation_ingress_app_with(server_name, key_id, &signing_key_b64, |_| {}).await
    else {
        return;
    };

    let room_id = format!("!backfillprivate:{}", server_name);
    let creator = "@creator:local.example".to_string();

    state
        .services
        .user_storage
        .create_user(&creator, "creator", None, false)
        .await
        .unwrap();
    state
        .services
        .room_storage
        .create_room(&room_id, &creator, "invite", "1", false)
        .await
        .unwrap();
    state
        .services
        .member_storage
        .add_member(&room_id, &creator, "join", None, None, None)
        .await
        .unwrap();

    let uri = format!(
        "/_matrix/federation/v1/backfill/{}?v=$seed:test.example.com&limit=10",
        room_id
    );
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

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_NOT_FOUND");
}

#[tokio::test]
async fn test_federation_query_directory_rejects_unjoined_server_for_private_alias() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [37u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some((app, state)) =
        setup_federation_ingress_app_with(server_name, key_id, &signing_key_b64, |_| {}).await
    else {
        return;
    };

    let room_id = format!("!aliasprivate:{}", server_name);
    let creator = "@creator:local.example".to_string();
    let room_alias = format!("#private-alias:{}", server_name);

    state
        .services
        .user_storage
        .create_user(&creator, "creator", None, false)
        .await
        .unwrap();
    state
        .services
        .room_storage
        .create_room(&room_id, &creator, "invite", "1", false)
        .await
        .unwrap();
    state
        .services
        .member_storage
        .add_member(&room_id, &creator, "join", None, None, None)
        .await
        .unwrap();
    state
        .services
        .room_service
        .set_room_alias(&room_id, &room_alias, &creator)
        .await
        .unwrap();

    let uri = format!(
        "/_matrix/federation/v1/query/directory?room_alias={}",
        urlencoding::encode(&room_alias)
    );
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

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_FORBIDDEN");
}

#[tokio::test]
async fn test_federation_query_directory_rejects_non_local_alias() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [38u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some((app, state)) =
        setup_federation_ingress_app_with(server_name, key_id, &signing_key_b64, |_| {}).await
    else {
        return;
    };

    let room_id = format!("!aliasremote:{}", server_name);
    let creator = "@creator:local.example".to_string();
    let remote_alias = "#spoofed:evil.example".to_string();

    state
        .services
        .user_storage
        .create_user(&creator, "creator", None, false)
        .await
        .unwrap();
    state
        .services
        .room_storage
        .create_room(&room_id, &creator, "public", "1", true)
        .await
        .unwrap();
    state
        .services
        .member_storage
        .add_member(&room_id, &creator, "join", None, None, None)
        .await
        .unwrap();
    state
        .services
        .room_service
        .set_room_alias(&room_id, &remote_alias, &creator)
        .await
        .unwrap();

    let uri = format!(
        "/_matrix/federation/v1/query/directory?room_alias={}",
        urlencoding::encode(&remote_alias)
    );
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

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_NOT_FOUND");
}

#[tokio::test]
async fn test_federation_media_download_rejects_non_local_server_name() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [39u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some((app, state)) =
        setup_federation_ingress_app_with(server_name, key_id, &signing_key_b64, |_| {}).await
    else {
        return;
    };

    let upload = state
        .services
        .media_service
        .upload_media("@alice:test.example.com", &tiny_png(), "image/png", None)
        .await
        .unwrap();
    let media_id = upload["media_id"].as_str().unwrap();

    let uri = format!(
        "/_matrix/federation/v1/media/download/{}/{}",
        "evil.example", media_id
    );
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

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_NOT_FOUND");
}

#[tokio::test]
async fn test_federation_media_thumbnail_rejects_oversized_dimensions() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [40u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some(app) = setup_federation_ingress_app(server_name, key_id, &signing_key_b64).await
    else {
        return;
    };

    let uri = format!(
        "/_matrix/federation/v1/media/thumbnail/{}/deadbeef?width=100000&height=100000&method=scale",
        server_name
    );
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

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(
        json["error"]
            .as_str()
            .is_some_and(|message| message.contains("Thumbnail dimensions must be between 1")),
        "Unexpected error payload: {json}"
    );
}

#[tokio::test]
async fn test_federation_profile_query_rejects_non_local_user() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [33u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some((app, state)) =
        setup_federation_ingress_app_with(server_name, key_id, &signing_key_b64, |_| {}).await
    else {
        return;
    };

    let remote_user = "@alice:evil.example".to_string();
    state
        .services
        .user_storage
        .create_user(&remote_user, "alice", None, false)
        .await
        .unwrap();

    let uri = format!("/_matrix/federation/v1/query/profile/{}", remote_user);
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

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_NOT_FOUND");
}

#[tokio::test]
async fn test_federation_profile_query_rejects_missing_local_user() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [36u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some((app, _state)) =
        setup_federation_ingress_app_with(server_name, key_id, &signing_key_b64, |_| {}).await
    else {
        return;
    };

    let missing_user = format!("@ghost:{}", server_name);
    let uri = format!("/_matrix/federation/v1/query/profile/{}", missing_user);
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

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_NOT_FOUND");
}

#[tokio::test]
async fn test_federation_profile_query_honors_field_filter_without_leaking_user_id() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [34u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some((app, state)) =
        setup_federation_ingress_app_with(server_name, key_id, &signing_key_b64, |_| {}).await
    else {
        return;
    };

    let user_id = format!("@alice:{}", server_name);
    state
        .services
        .user_storage
        .create_user(&user_id, "alice", None, false)
        .await
        .unwrap();
    state
        .services
        .registration_service
        .set_displayname(&user_id, "Alice Federation")
        .await
        .unwrap();
    state
        .services
        .registration_service
        .set_avatar_url(&user_id, "mxc://test.example.com/alice")
        .await
        .unwrap();

    let remote_user = "@mallory:evil.example";
    state
        .services
        .user_storage
        .create_user(remote_user, "mallory", None, false)
        .await
        .unwrap();

    let room_id = format!("!profile-share:{}", server_name);
    create_shared_room_for_users(&state, &room_id, &user_id, remote_user).await;

    let uri = format!(
        "/_matrix/federation/v1/query/profile?user_id={}&field=displayname",
        user_id
    );
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

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["displayname"], "Alice Federation");
    assert!(json.get("avatar_url").is_none());
    assert!(json.get("user_id").is_none());
}

#[tokio::test]
async fn test_federation_profile_query_requires_shared_room_with_origin() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [38u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some((app, state)) =
        setup_federation_ingress_app_with(server_name, key_id, &signing_key_b64, |_| {}).await
    else {
        return;
    };

    let user_id = format!("@alice:{}", server_name);
    state
        .services
        .user_storage
        .create_user(&user_id, "alice", None, false)
        .await
        .unwrap();
    state
        .services
        .registration_service
        .set_displayname(&user_id, "Alice Federation")
        .await
        .unwrap();

    let uri = format!(
        "/_matrix/federation/v1/query/profile?user_id={}&field=displayname",
        user_id
    );
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

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_FORBIDDEN");
}

#[tokio::test]
async fn test_federation_profile_query_rejects_invalid_field() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [35u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some((app, state)) =
        setup_federation_ingress_app_with(server_name, key_id, &signing_key_b64, |_| {}).await
    else {
        return;
    };

    let user_id = format!("@alice:{}", server_name);
    state
        .services
        .user_storage
        .create_user(&user_id, "alice", None, false)
        .await
        .unwrap();

    let uri = format!(
        "/_matrix/federation/v1/query/profile/{}?field=blurhash",
        user_id
    );
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

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_BAD_JSON");
}

#[tokio::test]
async fn test_federation_openid_userinfo_rejects_deactivated_user_token() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [37u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);

    let Some((app, state)) =
        setup_federation_ingress_app_with(server_name, key_id, &signing_key_b64, |_| {}).await
    else {
        return;
    };

    let user_id = format!("@openid-deactivated:{}", server_name);
    state
        .services
        .user_storage
        .create_user(&user_id, "openid-deactivated", None, false)
        .await
        .unwrap();

    let openid_storage = OpenIdTokenStorage::new(&state.services.user_storage.pool);
    let token_value = "stale_openid_token".to_string();
    openid_storage
        .create_token(CreateOpenIdTokenRequest {
            token: token_value.clone(),
            user_id: user_id.clone(),
            device_id: None,
            expires_at: chrono::Utc::now().timestamp_millis() + 60_000,
        })
        .await
        .unwrap();

    state
        .services
        .user_storage
        .deactivate_user(&user_id)
        .await
        .unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/_matrix/federation/v1/openid/userinfo?access_token={}",
                    token_value
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_UNAUTHORIZED");
}

#[tokio::test]
async fn test_federation_query_auth_returns_minimal_success_payload() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [36u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some(app) = setup_federation_ingress_app(server_name, key_id, &signing_key_b64).await
    else {
        return;
    };

    let response = app
        .oneshot(signed_request(
            "GET",
            "/_matrix/federation/v1/query/auth",
            server_name,
            key_id,
            &signing_key,
            None,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(json["auth_chain"].is_array());
}

#[tokio::test]
async fn test_federation_event_auth_returns_not_found_instead_of_placeholder_success() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [35u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some(app) = setup_federation_ingress_app(server_name, key_id, &signing_key_b64).await
    else {
        return;
    };

    let response = app
        .oneshot(signed_request(
            "GET",
            "/_matrix/federation/v1/event_auth",
            server_name,
            key_id,
            &signing_key,
            None,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_NOT_FOUND");
}

#[tokio::test]
async fn test_federation_keys_upload_returns_unrecognized_with_migration_hint() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [34u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some(app) = setup_federation_ingress_app(server_name, key_id, &signing_key_b64).await
    else {
        return;
    };

    let response = app
        .oneshot(signed_request(
            "POST",
            "/_matrix/federation/v1/keys/upload",
            server_name,
            key_id,
            &signing_key,
            Some(&json!({})),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_UNRECOGNIZED");
    assert!(json["error"]
        .as_str()
        .unwrap_or_default()
        .contains("user/keys endpoints"));
}

#[tokio::test]
async fn test_federation_legacy_keys_claim_returns_unrecognized_with_supported_path_hint() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [33u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some(app) = setup_federation_ingress_app(server_name, key_id, &signing_key_b64).await
    else {
        return;
    };

    let response = app
        .oneshot(signed_request(
            "POST",
            "/_matrix/federation/v1/keys/claim",
            server_name,
            key_id,
            &signing_key,
            Some(&json!({})),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_UNRECOGNIZED");
    assert!(json["error"]
        .as_str()
        .unwrap_or_default()
        .contains("/_matrix/federation/v1/user/keys/claim"));
}

#[tokio::test]
async fn test_federation_legacy_keys_query_returns_unrecognized_with_supported_path_hint() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [32u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some(app) = setup_federation_ingress_app(server_name, key_id, &signing_key_b64).await
    else {
        return;
    };

    let response = app
        .oneshot(signed_request(
            "POST",
            "/_matrix/federation/v1/keys/query",
            server_name,
            key_id,
            &signing_key,
            Some(&json!({})),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_UNRECOGNIZED");
    assert!(json["error"]
        .as_str()
        .unwrap_or_default()
        .contains("/_matrix/federation/v1/user/keys/query"));
}

#[tokio::test]
async fn test_federation_send_join_v2_rejects_sender_domain_mismatch() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [13u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some(app) = setup_federation_ingress_app(server_name, key_id, &signing_key_b64).await
    else {
        return;
    };

    let room_id = format!("!joincheck:{}", server_name);
    let event_id = "$joincheck";
    let uri = format!("/_matrix/federation/v2/send_join/{}/{}", room_id, event_id);
    let content = json!({
        "event_id": event_id,
        "room_id": room_id,
        "type": "m.room.member",
        "sender": "@mallory:evil.example",
        "state_key": "@mallory:evil.example",
        "origin": server_name,
        "content": {
            "membership": "join"
        }
    });
    let request = signed_request(
        "PUT",
        &uri,
        server_name,
        key_id,
        &signing_key,
        Some(&content),
    );

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_FORBIDDEN");
}

#[tokio::test]
async fn test_federation_send_join_v2_persists_join_event() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [16u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some((app, state)) =
        setup_federation_ingress_app_with(server_name, key_id, &signing_key_b64, |_| {}).await
    else {
        return;
    };

    let room_id = format!("!joinpersist:{}", server_name);
    let creator = format!("@creator:{}", server_name);
    let existing_member = format!("@member:{}", server_name);
    let joiner = format!("@joiner:{}", server_name);
    state
        .services
        .user_storage
        .create_user(&creator, "creator", None, false)
        .await
        .unwrap();
    state
        .services
        .user_storage
        .create_user(&existing_member, "member", None, false)
        .await
        .unwrap();
    state
        .services
        .user_storage
        .create_user(&joiner, "joiner", None, false)
        .await
        .unwrap();
    create_shared_room_for_users(&state, &room_id, &creator, &existing_member).await;

    let event_id = "$joinpersist";
    let origin_server_ts = 1_717_171_717_000_i64;
    let content = json!({
        "event_id": event_id,
        "room_id": room_id,
        "type": "m.room.member",
        "sender": joiner,
        "state_key": joiner,
        "origin": server_name,
        "origin_server_ts": origin_server_ts,
        "content": {
            "membership": "join",
            "displayname": "Federated Joiner"
        }
    });

    let response = app
        .oneshot(signed_request(
            "PUT",
            &format!("/_matrix/federation/v2/send_join/{}/{}", room_id, event_id),
            server_name,
            key_id,
            &signing_key,
            Some(&content),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["event_id"], event_id);
    assert_eq!(json["room_id"], room_id);

    let stored_event = state
        .services
        .event_storage
        .get_event(event_id)
        .await
        .unwrap()
        .expect("join event should be persisted");
    assert_eq!(stored_event.room_id, room_id);
    assert_eq!(stored_event.user_id, joiner);
    assert_eq!(stored_event.origin_server_ts, origin_server_ts);
    assert_eq!(stored_event.content["membership"], "join");

    let stored_member = state
        .services
        .member_storage
        .get_room_member(&room_id, &joiner)
        .await
        .unwrap()
        .expect("joined member should be recorded");
    assert_eq!(stored_member.membership, "join");
    assert_eq!(
        stored_member.display_name.as_deref(),
        Some("Federated Joiner")
    );
}

#[tokio::test]
async fn test_federation_send_join_rejects_uninvited_user_for_invite_only_room() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [15u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some((app, state)) =
        setup_federation_ingress_app_with(server_name, key_id, &signing_key_b64, |_| {}).await
    else {
        return;
    };

    let room_id = format!("!inviteonly:{}", server_name);
    let creator = format!("@creator:{}", server_name);
    let invitee = format!("@remotejoiner:{}", server_name);

    state
        .services
        .user_storage
        .create_user(&creator, "creator", None, false)
        .await
        .unwrap();
    state
        .services
        .user_storage
        .create_user(&invitee, "remotejoiner", None, false)
        .await
        .unwrap();
    state
        .services
        .room_storage
        .create_room(&room_id, &creator, "invite", "1", false)
        .await
        .unwrap();
    state
        .services
        .member_storage
        .add_member(&room_id, &creator, "join", None, None, None)
        .await
        .unwrap();

    let event_id_v1 = "$invite-only-v1";
    let content_v1 = json!({
        "origin": server_name,
        "event": {
            "event_id": event_id_v1,
            "room_id": room_id,
            "type": "m.room.member",
            "sender": invitee,
            "state_key": invitee,
            "origin": server_name,
            "content": {
                "membership": "join"
            }
        }
    });
    let response_v1 = app
        .clone()
        .oneshot(signed_request(
            "PUT",
            &format!(
                "/_matrix/federation/v1/send_join/{}/{}",
                room_id, event_id_v1
            ),
            server_name,
            key_id,
            &signing_key,
            Some(&content_v1),
        ))
        .await
        .unwrap();
    assert_eq!(response_v1.status(), StatusCode::FORBIDDEN);

    let event_id_v2 = "$invite-only-v2";
    let content_v2 = json!({
        "event_id": event_id_v2,
        "room_id": room_id,
        "type": "m.room.member",
        "sender": invitee,
        "state_key": invitee,
        "origin": server_name,
        "content": {
            "membership": "join"
        }
    });
    let response_v2 = app
        .oneshot(signed_request(
            "PUT",
            &format!(
                "/_matrix/federation/v2/send_join/{}/{}",
                room_id, event_id_v2
            ),
            server_name,
            key_id,
            &signing_key,
            Some(&content_v2),
        ))
        .await
        .unwrap();
    assert_eq!(response_v2.status(), StatusCode::FORBIDDEN);

    let stored_v1 = state
        .services
        .event_storage
        .get_event(event_id_v1)
        .await
        .unwrap();
    let stored_v2 = state
        .services
        .event_storage
        .get_event(event_id_v2)
        .await
        .unwrap();
    assert!(stored_v1.is_none());
    assert!(stored_v2.is_none());
}

#[tokio::test]
async fn test_federation_send_leave_rejects_event_path_mismatch() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [14u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some(app) = setup_federation_ingress_app(server_name, key_id, &signing_key_b64).await
    else {
        return;
    };

    let room_id = format!("!leavecheck:{}", server_name);
    let uri = format!("/_matrix/federation/v1/send_leave/{}/$leave_path", room_id);
    let content = json!({
        "origin": server_name,
        "event": {
            "event_id": "$leave_body",
            "room_id": room_id,
            "type": "m.room.member",
            "sender": format!("@alice:{}", server_name),
            "state_key": format!("@alice:{}", server_name),
            "origin": server_name,
            "content": {
                "membership": "leave"
            }
        }
    });
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
    assert_eq!(json["errcode"], "M_BAD_JSON");
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
async fn test_federation_send_transaction_rejects_member_state_event_without_power() {
    let server_name = "test.example.com";
    let key_id = "ed25519:1";
    let signing_key_seed = [12u8; 32];
    let signing_key_b64 = STANDARD_NO_PAD.encode(signing_key_seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&signing_key_seed);

    let Some((app, state)) =
        setup_federation_ingress_app_with(server_name, key_id, &signing_key_b64, |_| {}).await
    else {
        return;
    };

    let room_id = format!("!txnstate:{}", server_name);
    let creator = format!("@creator:{}", server_name);
    let member = format!("@member:{}", server_name);

    state
        .services
        .user_storage
        .create_user(&creator, "creator", None, false)
        .await
        .unwrap();
    state
        .services
        .user_storage
        .create_user(&member, "member", None, false)
        .await
        .unwrap();
    state
        .services
        .room_storage
        .create_room(&room_id, &creator, "public", "1", true)
        .await
        .unwrap();
    state
        .services
        .member_storage
        .add_member(&room_id, &creator, "join", None, None, None)
        .await
        .unwrap();
    state
        .services
        .member_storage
        .add_member(&room_id, &member, "join", None, None, None)
        .await
        .unwrap();

    let uri = "/_matrix/federation/v1/send/txn-state-default";
    let event_id = "$txn-topic";
    let content = json!({
        "origin": server_name,
        "pdus": [
            {
                "event_id": event_id,
                "room_id": room_id,
                "sender": member,
                "type": "m.room.topic",
                "state_key": "",
                "origin": server_name,
                "origin_server_ts": 1_717_171_717_000i64,
                "content": {
                    "topic": "forbidden update"
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

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["results"][0]["event_id"], event_id);
    assert!(json["results"][0]["error"].as_str().is_some());

    let stored = state
        .services
        .event_storage
        .get_event(event_id)
        .await
        .unwrap();
    assert!(stored.is_none());
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
