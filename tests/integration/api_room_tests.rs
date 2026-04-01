use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use std::sync::Arc;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::common::config::{
    AdminRegistrationConfig, Config, CorsConfig, DatabaseConfig, FederationConfig, RateLimitConfig,
    RedisConfig, SearchConfig, SecurityConfig, ServerConfig, SmtpConfig, VoipConfig, WorkerConfig,
};
use synapse_rust::services::ServiceContainer;
use synapse_rust::web::routes::create_router;
use synapse_rust::web::AppState;
use tower::ServiceExt;

fn create_test_config() -> Config {
    Config {
        server: ServerConfig {
            name: "localhost".to_string(),
            host: "0.0.0.0".to_string(),
            port: 8008,
            public_baseurl: None,
            signing_key_path: None,
            macaroon_secret_key: None,
            form_secret: None,
            server_name: None,
            suppress_key_server_warning: false,
            serve_server_wellknown: false,
            soft_file_limit: 0,
            user_agent_suffix: None,
            web_client_location: None,
            registration_shared_secret: None,
            admin_contact: None,
            max_upload_size: 1000000,
            max_image_resolution: 1000000,
            enable_registration: true,
            enable_registration_captcha: false,
            background_tasks_interval: 60,
            expire_access_token: true,
            expire_access_token_lifetime: 3600,
            refresh_token_lifetime: 604800,
            refresh_token_sliding_window_size: 1000,
            session_duration: 86400,
            warmup_pool: true,
        },
        database: DatabaseConfig {
            host: "localhost".to_string(),
            port: 5432,
            username: "synapse".to_string(),
            password: "synapse".to_string(),
            name: "synapse".to_string(),
            pool_size: 5,
            max_size: 10,
            min_idle: Some(2),
            connection_timeout: 30,
        },
        redis: RedisConfig {
            host: "localhost".to_string(),
            port: 6379,
            password: None,
            key_prefix: "test:".to_string(),
            pool_size: 5,
            enabled: false,
            connection_timeout_ms: 500,
            command_timeout_ms: 500,
            circuit_breaker: synapse_rust::common::config::CircuitBreakerConfig::default(),
        },
        logging: synapse_rust::common::config::LoggingConfig {
            level: "warn".to_string(),
            format: "json".to_string(),
            log_file: None,
            log_dir: None,
        },
        federation: FederationConfig {
            enabled: false,
            allow_ingress: false,
            server_name: "test.example.com".to_string(),
            federation_port: 8448,
            connection_pool_size: 5,
            max_transaction_payload: 50000,
            ca_file: None,
            client_ca_file: None,
            signing_key: None,
            key_id: None,
            trusted_key_servers: vec![],
            key_refresh_interval: 86400,
            suppress_key_server_warning: false,
            signature_cache_ttl: 3600,
            key_cache_ttl: 3600,
            key_rotation_grace_period_ms: 600000,
        },
        security: SecurityConfig {
            secret: "test_secret".to_string(),
            expiry_time: 3600,
            refresh_token_expiry: 604800,
            argon2_m_cost: 2048,
            argon2_t_cost: 1,
            argon2_p_cost: 1,
            allow_legacy_hashes: false,
            login_failure_lockout_threshold: 5,
            login_lockout_duration_seconds: 900,
        },
        search: SearchConfig {
            enabled: false,
            elasticsearch_url: "http://localhost:9200".to_string(),
            postgres_fts: synapse_rust::common::config::PostgresFtsConfig::default(),
            provider: "elasticsearch".to_string(),
        },
        rate_limit: RateLimitConfig::default(),
        admin_registration: AdminRegistrationConfig {
            enabled: true,
            shared_secret: "test_shared_secret".to_string(),
            nonce_timeout_seconds: 60,
        },
        worker: WorkerConfig::default(),
        cors: CorsConfig::default(),
        smtp: SmtpConfig::default(),
        voip: VoipConfig::default(),
        push: synapse_rust::common::config::PushConfig::default(),
        url_preview: synapse_rust::common::config::UrlPreviewConfig::default(),
        oidc: synapse_rust::common::config::OidcConfig::default(),
        builtin_oidc: synapse_rust::common::config::BuiltinOidcConfig::default(),
        saml: synapse_rust::common::config::SamlConfig::default(),
        retention: synapse_rust::common::config::RetentionConfig::default(),
        telemetry: synapse_rust::common::telemetry_config::OpenTelemetryConfig::default(),
        prometheus: synapse_rust::common::telemetry_config::PrometheusConfig::default(),
    }
}

async fn setup_test_app() -> Option<axum::Router> {
    let pool = super::get_test_pool().await?;
    let cache = Arc::new(CacheManager::new(CacheConfig::default()));
    let config = create_test_config();
    let container = ServiceContainer::new(&pool, cache.clone(), config, None);
    let state = AppState::new(container, cache);
    Some(create_router(state))
}

async fn register_user(app: &axum::Router, username: &str) -> Option<String> {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "username": username,
                "password": "Password123!",
                "auth": {"type": "m.login.dummy"}
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .ok()?;

    if response.status() != StatusCode::OK {
        return None;
    }

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .ok()?;
    let json: Value = serde_json::from_slice(&body).ok()?;
    json.get("access_token")?.as_str().map(|s| s.to_string())
}

async fn create_room(app: &axum::Router, token: &str, name: &str) -> Option<String> {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "name": name }).to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .ok()?;

    if response.status() != StatusCode::OK {
        return None;
    }

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .ok()?;
    let json: Value = serde_json::from_slice(&body).ok()?;
    json.get("room_id")?.as_str().map(|value| value.to_string())
}

#[tokio::test]
async fn test_room_lifecycle() {
    let Some(app) = setup_test_app().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let Some(alice_token) = register_user(&app, &format!("alice_{}", rand::random::<u32>())).await
    else {
        eprintln!("Skipping test: failed to register alice");
        return;
    };
    let Some(bob_token) = register_user(&app, &format!("bob_{}", rand::random::<u32>())).await
    else {
        eprintln!("Skipping test: failed to register bob");
        return;
    };

    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/createRoom")
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "name": "Alice's Room",
                "topic": "Testing room lifecycle",
                "visibility": "public"
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .expect("Request failed");

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let room_id = json["room_id"].as_str().unwrap().to_string();

    let request_whoami = Request::builder()
        .uri("/_matrix/client/r0/account/whoami")
        .header("Authorization", format!("Bearer {}", bob_token))
        .body(Body::empty())
        .unwrap();
    let response_whoami = ServiceExt::<Request<Body>>::oneshot(app.clone(), request_whoami)
        .await
        .unwrap();
    let body_whoami = axum::body::to_bytes(response_whoami.into_body(), 1024)
        .await
        .unwrap();
    let json_whoami: Value = serde_json::from_slice(&body_whoami).unwrap();
    let bob_user_id = json_whoami["user_id"].as_str().unwrap();

    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/invite", room_id))
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({"user_id": bob_user_id}).to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/join", room_id))
        .header("Authorization", format!("Bearer {}", bob_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/r0/rooms/{}/send/m.room.message/txn1",
            room_id
        ))
        .header("Authorization", format!("Bearer {}", bob_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({"msgtype": "m.text", "body": "Hello Alice!"}).to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let request = Request::builder()
        .uri(format!("/_matrix/client/r0/rooms/{}/members", room_id))
        .header("Authorization", format!("Bearer {}", alice_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 10240)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(json["chunk"].as_array().unwrap().len() >= 2);

    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/leave", room_id))
        .header("Authorization", format!("Bearer {}", bob_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_room_directory_and_public_rooms() {
    let Some(app) = setup_test_app().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let Some(alice_token) = register_user(&app, &format!("alice_{}", rand::random::<u32>())).await
    else {
        eprintln!("Skipping test: failed to register alice");
        return;
    };

    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/createRoom")
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({"name": "Public Room", "visibility": "public"}).to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let room_id = json["room_id"].as_str().unwrap().to_string();

    let request = Request::builder()
        .uri("/_matrix/client/r0/publicRooms")
        .header("Authorization", format!("Bearer {}", alice_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 10240)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(json["chunk"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["room_id"] == room_id));
}

#[tokio::test]
async fn test_directory_routes_work_across_r0_and_v3() {
    let Some(app) = setup_test_app().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let Some(alice_token) = register_user(&app, &format!("alice_{}", rand::random::<u32>())).await
    else {
        eprintln!("Skipping test: failed to register alice");
        return;
    };

    let create_room_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/createRoom")
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({"name": "Directory Room", "visibility": "public"}).to_string(),
        ))
        .unwrap();
    let create_room_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), create_room_request)
            .await
            .unwrap();
    assert_eq!(create_room_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(create_room_response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let room_id = json["room_id"].as_str().unwrap().to_string();

    let r0_public_rooms_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/r0/publicRooms")
        .header("Authorization", format!("Bearer {}", alice_token))
        .body(Body::empty())
        .unwrap();
    let r0_public_rooms_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), r0_public_rooms_request)
            .await
            .unwrap();
    assert_eq!(r0_public_rooms_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(r0_public_rooms_response.into_body(), 10240)
        .await
        .unwrap();
    let r0_public_rooms_json: Value = serde_json::from_slice(&body).unwrap();

    let v3_public_rooms_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/publicRooms")
        .header("Authorization", format!("Bearer {}", alice_token))
        .body(Body::empty())
        .unwrap();
    let v3_public_rooms_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), v3_public_rooms_request)
            .await
            .unwrap();
    assert_eq!(v3_public_rooms_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(v3_public_rooms_response.into_body(), 10240)
        .await
        .unwrap();
    let v3_public_rooms_json: Value = serde_json::from_slice(&body).unwrap();

    assert!(r0_public_rooms_json["chunk"]
        .as_array()
        .unwrap()
        .iter()
        .any(|room| room["room_id"] == room_id));
    assert_eq!(r0_public_rooms_json, v3_public_rooms_json);

    let r0_aliases_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/r0/directory/room/{}/alias",
            room_id
        ))
        .header("Authorization", format!("Bearer {}", alice_token))
        .body(Body::empty())
        .unwrap();
    let r0_aliases_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), r0_aliases_request)
        .await
        .unwrap();
    assert_eq!(r0_aliases_response.status(), StatusCode::OK);

    let v3_aliases_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v3/directory/room/{}/alias",
            room_id
        ))
        .header("Authorization", format!("Bearer {}", alice_token))
        .body(Body::empty())
        .unwrap();
    let v3_aliases_response = ServiceExt::<Request<Body>>::oneshot(app, v3_aliases_request)
        .await
        .unwrap();
    assert_eq!(v3_aliases_response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_room_state_and_redaction() {
    let Some(app) = setup_test_app().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let Some(alice_token) = register_user(&app, &format!("alice_{}", rand::random::<u32>())).await
    else {
        eprintln!("Skipping test: failed to register alice");
        return;
    };

    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/createRoom")
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({"name": "State Room"}).to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let room_id = json["room_id"].as_str().unwrap().to_string();

    let request = Request::builder()
        .uri(format!("/_matrix/client/r0/rooms/{}/state", room_id))
        .header("Authorization", format!("Bearer {}", alice_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/r0/rooms/{}/send/m.room.message/txn1",
            room_id
        ))
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({"msgtype": "m.text", "body": "To be redacted"}).to_string(),
        ))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let event_id = json["event_id"].as_str().unwrap().to_string();

    let request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/r0/rooms/{}/redact/{}/txn_redact_1",
            room_id, event_id
        ))
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({"reason": "Test redaction"}).to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_room_moderation() {
    let Some(app) = setup_test_app().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let Some(alice_token) = register_user(&app, &format!("alice_{}", rand::random::<u32>())).await
    else {
        eprintln!("Skipping test: failed to register alice");
        return;
    };
    let Some(bob_token) = register_user(&app, &format!("bob_{}", rand::random::<u32>())).await
    else {
        eprintln!("Skipping test: failed to register bob");
        return;
    };

    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/createRoom")
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({"name": "Moderation Room"}).to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let room_id = json["room_id"].as_str().unwrap().to_string();

    let request_whoami = Request::builder()
        .uri("/_matrix/client/r0/account/whoami")
        .header("Authorization", format!("Bearer {}", bob_token))
        .body(Body::empty())
        .unwrap();
    let response_whoami = ServiceExt::<Request<Body>>::oneshot(app.clone(), request_whoami)
        .await
        .unwrap();
    let body_whoami = axum::body::to_bytes(response_whoami.into_body(), 1024)
        .await
        .unwrap();
    let json_whoami: Value = serde_json::from_slice(&body_whoami).unwrap();
    let bob_user_id = json_whoami["user_id"].as_str().unwrap();

    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/invite", room_id))
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({"user_id": bob_user_id}).to_string()))
        .unwrap();
    ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();

    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/join", room_id))
        .header("Authorization", format!("Bearer {}", bob_token))
        .body(Body::empty())
        .unwrap();
    ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();

    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/kick", room_id))
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({"user_id": bob_user_id, "reason": "Behave!"}).to_string(),
        ))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/ban", room_id))
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({"user_id": bob_user_id, "reason": "Banned!"}).to_string(),
        ))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/unban", room_id))
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({"user_id": bob_user_id}).to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_room_info_and_event_routes_work_across_r0_and_v3() {
    let Some(app) = setup_test_app().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let Some(alice_token) =
        register_user(&app, &format!("room_compat_{}", rand::random::<u32>())).await
    else {
        eprintln!("Skipping test: failed to register alice");
        return;
    };

    let create_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/createRoom")
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({"name": "Compat Room"}).to_string()))
        .unwrap();
    let create_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), create_request)
        .await
        .unwrap();
    assert_eq!(create_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(create_response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let room_id = json["room_id"].as_str().unwrap().to_string();

    let r0_info_request = Request::builder()
        .uri(format!("/_matrix/client/r0/rooms/{}", room_id))
        .header("Authorization", format!("Bearer {}", alice_token))
        .body(Body::empty())
        .unwrap();
    let r0_info_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), r0_info_request)
        .await
        .unwrap();
    assert_eq!(r0_info_response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(r0_info_response.into_body(), 2048)
        .await
        .unwrap();
    let r0_info_json: Value = serde_json::from_slice(&body).unwrap();

    let v3_info_request = Request::builder()
        .uri(format!("/_matrix/client/v3/rooms/{}", room_id))
        .header("Authorization", format!("Bearer {}", alice_token))
        .body(Body::empty())
        .unwrap();
    let v3_info_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), v3_info_request)
        .await
        .unwrap();
    assert_eq!(v3_info_response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(v3_info_response.into_body(), 2048)
        .await
        .unwrap();
    let v3_info_json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(r0_info_json, v3_info_json);

    let send_request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/send/m.room.message/compat_txn_{}",
            room_id,
            rand::random::<u32>()
        ))
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({"msgtype": "m.text", "body": "cross-version event"}).to_string(),
        ))
        .unwrap();
    let send_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), send_request)
        .await
        .unwrap();
    assert_eq!(send_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(send_response.into_body(), 1024)
        .await
        .unwrap();
    let send_json: Value = serde_json::from_slice(&body).unwrap();
    let event_id = send_json["event_id"].as_str().unwrap();

    let get_event_request = Request::builder()
        .uri(format!(
            "/_matrix/client/r0/rooms/{}/event/{}",
            room_id, event_id
        ))
        .header("Authorization", format!("Bearer {}", alice_token))
        .body(Body::empty())
        .unwrap();
    let get_event_response = ServiceExt::<Request<Body>>::oneshot(app, get_event_request)
        .await
        .unwrap();
    assert_eq!(get_event_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_room_report_route_boundaries_are_preserved() {
    let Some(app) = setup_test_app().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let Some(alice_token) =
        register_user(&app, &format!("room_report_{}", rand::random::<u32>())).await
    else {
        eprintln!("Skipping test: failed to register alice");
        return;
    };

    let create_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/createRoom")
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({"name": "Report Boundaries"}).to_string()))
        .unwrap();
    let create_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), create_request)
        .await
        .unwrap();
    assert_eq!(create_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(create_response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let room_id = json["room_id"].as_str().unwrap().to_string();

    let send_request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/r0/rooms/{}/send/m.room.message/report_txn_{}",
            room_id,
            rand::random::<u32>()
        ))
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({"msgtype": "m.text", "body": "report target"}).to_string(),
        ))
        .unwrap();
    let send_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), send_request)
        .await
        .unwrap();
    assert_eq!(send_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(send_response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let event_id = json["event_id"].as_str().unwrap().to_string();

    for path in [
        format!("/_matrix/client/r0/rooms/{}/report/{}", room_id, event_id),
        format!("/_matrix/client/v1/rooms/{}/report/{}", room_id, event_id),
        format!("/_matrix/client/v3/rooms/{}/report/{}", room_id, event_id),
    ] {
        let request = Request::builder()
            .method("POST")
            .uri(path)
            .header("Authorization", format!("Bearer {}", alice_token))
            .header("Content-Type", "application/json")
            .body(Body::from(
                json!({"reason": "compatibility check", "score": -50}).to_string(),
            ))
            .unwrap();
        let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
            .await
            .unwrap();
        assert_ne!(response.status(), StatusCode::NOT_FOUND);
        assert_ne!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    }

    let r0_report_room_request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/report", room_id))
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({"reason": "should not exist"}).to_string(),
        ))
        .unwrap();
    let r0_report_room_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), r0_report_room_request)
            .await
            .unwrap();
    assert_eq!(r0_report_room_response.status(), StatusCode::NOT_FOUND);

    let scanner_request = Request::builder()
        .uri(format!(
            "/_matrix/client/v1/rooms/{}/report/{}/scanner_info",
            room_id, event_id
        ))
        .header("Authorization", format!("Bearer {}", alice_token))
        .body(Body::empty())
        .unwrap();
    let scanner_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), scanner_request)
        .await
        .unwrap();
    assert_ne!(scanner_response.status(), StatusCode::NOT_FOUND);

    let v3_scanner_request = Request::builder()
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/report/{}/scanner_info",
            room_id, event_id
        ))
        .header("Authorization", format!("Bearer {}", alice_token))
        .body(Body::empty())
        .unwrap();
    let v3_scanner_response = ServiceExt::<Request<Body>>::oneshot(app, v3_scanner_request)
        .await
        .unwrap();
    assert_eq!(v3_scanner_response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_relations_routes_work_across_v1_and_r0() {
    let Some(app) = setup_test_app().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let Some(alice_token) =
        register_user(&app, &format!("relations_{}", rand::random::<u32>())).await
    else {
        eprintln!("Skipping test: failed to register alice");
        return;
    };

    let create_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/createRoom")
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({"name": "Relations Compat"}).to_string()))
        .unwrap();
    let create_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), create_request)
        .await
        .unwrap();
    assert_eq!(create_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(create_response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let room_id = json["room_id"].as_str().unwrap().to_string();

    let send_request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/r0/rooms/{}/send/m.room.message/relations_txn_{}",
            room_id,
            rand::random::<u32>()
        ))
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({"msgtype": "m.text", "body": "relation target"}).to_string(),
        ))
        .unwrap();
    let send_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), send_request)
        .await
        .unwrap();
    assert_eq!(send_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(send_response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let event_id = json["event_id"].as_str().unwrap().to_string();

    let v1_put_request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/v1/rooms/{}/relations/{}/m.annotation/{}",
            room_id, event_id, event_id
        ))
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({"key": "🔥"}).to_string()))
        .unwrap();
    let v1_put_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), v1_put_request)
        .await
        .unwrap();
    assert_eq!(v1_put_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(v1_put_response.into_body(), 1024)
        .await
        .unwrap();
    let v1_put_json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v1_put_json["room_id"], room_id);
    assert_eq!(v1_put_json["relates_to"]["event_id"], event_id);

    let r0_relations_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/r0/rooms/{}/relations/{}/m.annotation",
            room_id, event_id
        ))
        .body(Body::empty())
        .unwrap();
    let r0_relations_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), r0_relations_request)
            .await
            .unwrap();
    assert_eq!(r0_relations_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(r0_relations_response.into_body(), 2048)
        .await
        .unwrap();
    let r0_relations_json: Value = serde_json::from_slice(&body).unwrap();
    assert!(!r0_relations_json["chunk"].as_array().unwrap().is_empty());

    let r0_aggregations_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/r0/rooms/{}/aggregations/{}/m.annotation",
            room_id, event_id
        ))
        .body(Body::empty())
        .unwrap();
    let r0_aggregations_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), r0_aggregations_request)
            .await
            .unwrap();
    assert_eq!(r0_aggregations_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(r0_aggregations_response.into_body(), 2048)
        .await
        .unwrap();
    let r0_aggregations_json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(r0_aggregations_json["chunk"][0]["type"], "m.annotation");
    assert_eq!(r0_aggregations_json["chunk"][0]["key"], "🔥");

    let v3_relations_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/relations/{}/m.annotation",
            room_id, event_id
        ))
        .body(Body::empty())
        .unwrap();
    let v3_relations_response = ServiceExt::<Request<Body>>::oneshot(app, v3_relations_request)
        .await
        .unwrap();
    assert_eq!(v3_relations_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_reaction_send_routes_work_across_r0_and_v3() {
    let Some(app) = setup_test_app().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let Some(alice_token) =
        register_user(&app, &format!("reactions_{}", rand::random::<u32>())).await
    else {
        eprintln!("Skipping test: failed to register alice");
        return;
    };

    let create_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/createRoom")
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({"name": "Reaction Compat"}).to_string()))
        .unwrap();
    let create_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), create_request)
        .await
        .unwrap();
    assert_eq!(create_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(create_response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let room_id = json["room_id"].as_str().unwrap().to_string();

    let send_request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/r0/rooms/{}/send/m.room.message/reaction_target_{}",
            room_id,
            rand::random::<u32>()
        ))
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({"msgtype": "m.text", "body": "reaction target"}).to_string(),
        ))
        .unwrap();
    let send_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), send_request)
        .await
        .unwrap();
    assert_eq!(send_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(send_response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let event_id = json["event_id"].as_str().unwrap().to_string();

    let r0_reaction_request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/r0/rooms/{}/send/m.reaction/r0_reaction_{}",
            room_id,
            rand::random::<u32>()
        ))
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "relates_to": {
                    "event_id": event_id,
                    "rel_type": "m.annotation"
                },
                "body": "👍"
            })
            .to_string(),
        ))
        .unwrap();
    let r0_reaction_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), r0_reaction_request)
            .await
            .unwrap();
    assert_eq!(r0_reaction_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(r0_reaction_response.into_body(), 1024)
        .await
        .unwrap();
    let r0_reaction_json: Value = serde_json::from_slice(&body).unwrap();
    assert!(r0_reaction_json["event_id"].as_str().is_some());

    let v3_reaction_request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/send/m.reaction/v3_reaction_{}",
            room_id,
            rand::random::<u32>()
        ))
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "relates_to": {
                    "event_id": event_id,
                    "rel_type": "m.annotation"
                },
                "body": "🔥"
            })
            .to_string(),
        ))
        .unwrap();
    let v3_reaction_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), v3_reaction_request)
            .await
            .unwrap();
    assert_eq!(v3_reaction_response.status(), StatusCode::OK);

    let v3_relations_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/relations/{}",
            room_id, event_id
        ))
        .body(Body::empty())
        .unwrap();
    let v3_relations_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), v3_relations_request)
            .await
            .unwrap();
    assert_eq!(v3_relations_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(v3_relations_response.into_body(), 2048)
        .await
        .unwrap();
    let v3_relations_json: Value = serde_json::from_slice(&body).unwrap();
    let relation_bodies: Vec<_> = v3_relations_json["chunk"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["content"]["body"].as_str())
        .collect();
    assert!(relation_bodies.contains(&"👍"));
    assert!(relation_bodies.contains(&"🔥"));

    let r0_relations_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/r0/rooms/{}/relations/{}",
            room_id, event_id
        ))
        .body(Body::empty())
        .unwrap();
    let r0_relations_response = ServiceExt::<Request<Body>>::oneshot(app, r0_relations_request)
        .await
        .unwrap();
    assert_eq!(r0_relations_response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_room_version_is_available_after_create_room() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let Some(token) = register_user(&app, &format!("room_version_{}", rand::random::<u32>())).await
    else {
        return;
    };

    let create_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/createRoom")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "name": "Version Test Room",
                "topic": "room version"
            })
            .to_string(),
        ))
        .unwrap();
    let create_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), create_request)
        .await
        .unwrap();
    assert_eq!(create_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(create_response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let room_id = json["room_id"].as_str().unwrap();

    let version_request = Request::builder()
        .uri(format!("/_matrix/client/v3/rooms/{}/version", room_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let version_response = ServiceExt::<Request<Body>>::oneshot(app, version_request)
        .await
        .unwrap();
    assert_eq!(version_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(version_response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["room_version"], "1");
}

#[tokio::test]
async fn test_room_hierarchy_returns_room_summary_for_regular_room() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let Some(token) =
        register_user(&app, &format!("room_hierarchy_{}", rand::random::<u32>())).await
    else {
        return;
    };

    let Some(room_id) = create_room(&app, &token, "Hierarchy Test Room").await else {
        return;
    };

    let request = Request::builder()
        .uri(format!("/_matrix/client/v1/rooms/{}/hierarchy", room_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app, request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 2048)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(json["rooms"].is_array());
    assert_eq!(json["rooms"][0]["room_id"], room_id);
    assert!(json.get("next_batch").is_some());
}
