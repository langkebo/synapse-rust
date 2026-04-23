use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use std::sync::Arc;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::common::config::{
    AdminRegistrationConfig, Config, CorsConfig, DatabaseConfig, ExperimentalConfig,
    FederationConfig, RateLimitConfig, RedisConfig, SearchConfig, SecurityConfig, ServerConfig,
    SmtpConfig, VoipConfig, WorkerConfig,
};
use synapse_rust::services::ServiceContainer;
use synapse_rust::storage::event_report::EventReportStorage;
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
            key_fetch_max_concurrency: 32,
            key_fetch_timeout_ms: 5000,
            process_inbound_edus: false,
            inbound_edus_max_per_txn: 100,
            inbound_edu_max_concurrency: 8,
            inbound_edu_acquire_timeout_ms: 250,
            inbound_edu_per_origin_max_concurrency: 2,
            process_inbound_presence_edus: false,
            inbound_presence_updates_max_per_txn: 50,
            inbound_presence_backoff_ms: 3000,
            join_max_concurrency: 16,
            join_acquire_timeout_ms: 750,
            admission_mode: false,
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
            admin_mfa_required: false,
            admin_mfa_shared_secret: String::new(),
            admin_mfa_allowed_drift_steps: 1,
            admin_rbac_enabled: true,
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
            allow_external_access: false,
            production_only: true,
            ip_whitelist: Vec::new(),
            require_captcha: false,
            require_manual_approval: false,
            approval_tokens: Vec::new(),
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
        performance: synapse_rust::common::config::PerformanceConfig::default(),
        experimental: ExperimentalConfig::default(),
    }
}

async fn setup_test_app() -> Option<axum::Router> {
    let (app, _pool, _cache) = setup_test_app_with_pool().await?;
    Some(app)
}

async fn setup_test_app_with_pool() -> Option<(axum::Router, Arc<sqlx::PgPool>, Arc<CacheManager>)>
{
    let pool = super::get_test_pool().await?;
    let cache = Arc::new(CacheManager::new(CacheConfig::default()));
    let config = create_test_config();
    let container = ServiceContainer::new(&pool, cache.clone(), config, None);
    let state = AppState::new(container, cache.clone());
    Some((create_router(state), pool, cache))
}

async fn promote_to_admin(pool: &sqlx::PgPool, cache: &CacheManager, user_id: &str) {
    sqlx::query("UPDATE users SET is_admin = TRUE WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .expect("failed to promote user to admin");
    cache.delete(&format!("user:admin:{}", user_id)).await;
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

async fn get_user_id(app: &axum::Router, token: &str) -> Option<String> {
    let request = Request::builder()
        .uri("/_matrix/client/r0/account/whoami")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
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
    json.get("user_id")?.as_str().map(|value| value.to_string())
}

async fn send_message_event(
    app: &axum::Router,
    token: &str,
    room_id: &str,
    txn_id: &str,
    body: Value,
) -> Option<String> {
    let request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/r0/rooms/{}/send/m.room.message/{}",
            room_id, txn_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
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
    json.get("event_id")?
        .as_str()
        .map(|value| value.to_string())
}

#[tokio::test]
async fn test_user_rooms_rejects_admin_access_to_other_users_scope() {
    let Some((app, pool, cache)) = setup_test_app_with_pool().await else {
        return;
    };

    let owner_name = format!("rooms_owner_{}", rand::random::<u32>());
    let admin_name = format!("rooms_admin_{}", rand::random::<u32>());
    let owner_user_id = format!("@{}:localhost", owner_name);
    let admin_user_id = format!("@{}:localhost", admin_name);

    let owner_token = register_user(&app, &owner_name)
        .await
        .expect("owner should register");
    let admin_token = register_user(&app, &admin_name)
        .await
        .expect("admin should register");
    promote_to_admin(&pool, &cache, &admin_user_id).await;

    let room_id = create_room(&app, &owner_token, "owner room")
        .await
        .expect("room should be created");
    assert!(!room_id.is_empty());

    let request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/user/{}/rooms", owner_user_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

async fn delete_room(pool: &sqlx::PgPool, room_id: &str) {
    sqlx::query("DELETE FROM rooms WHERE room_id = $1")
        .bind(room_id)
        .execute(pool)
        .await
        .ok();
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
async fn test_private_room_member_endpoints_reject_invited_user() {
    let Some(app) = setup_test_app().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let Some(alice_token) =
        register_user(&app, &format!("members_owner_{}", rand::random::<u32>())).await
    else {
        eprintln!("Skipping test: failed to register alice");
        return;
    };

    let Some(bob_token) =
        register_user(&app, &format!("members_invited_{}", rand::random::<u32>())).await
    else {
        eprintln!("Skipping test: failed to register bob");
        return;
    };

    let Some(room_id) = create_room(&app, &alice_token, "Private Members Access Room").await else {
        eprintln!("Skipping test: failed to create room");
        return;
    };

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

    let invite_request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/invite", room_id))
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "user_id": bob_user_id }).to_string()))
        .unwrap();
    let invite_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), invite_request)
        .await
        .unwrap();
    assert_eq!(invite_response.status(), StatusCode::OK);

    let members_request = Request::builder()
        .uri(format!("/_matrix/client/r0/rooms/{}/members", room_id))
        .header("Authorization", format!("Bearer {}", bob_token))
        .body(Body::empty())
        .unwrap();
    let members_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), members_request)
        .await
        .unwrap();
    assert_eq!(members_response.status(), StatusCode::FORBIDDEN);

    let joined_members_request = Request::builder()
        .uri(format!(
            "/_matrix/client/r0/rooms/{}/joined_members",
            room_id
        ))
        .header("Authorization", format!("Bearer {}", bob_token))
        .body(Body::empty())
        .unwrap();
    let joined_members_response = ServiceExt::<Request<Body>>::oneshot(app, joined_members_request)
        .await
        .unwrap();
    assert_eq!(joined_members_response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_private_room_aliases_reject_non_member() {
    let Some(app) = setup_test_app().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let Some(owner_token) =
        register_user(&app, &format!("aliases_owner_{}", rand::random::<u32>())).await
    else {
        eprintln!("Skipping test: failed to register owner");
        return;
    };

    let Some(outsider_token) =
        register_user(&app, &format!("aliases_outsider_{}", rand::random::<u32>())).await
    else {
        eprintln!("Skipping test: failed to register outsider");
        return;
    };

    let Some(room_id) = create_room(&app, &owner_token, "Private Alias Guard Room").await else {
        eprintln!("Skipping test: failed to create room");
        return;
    };

    let outsider_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/r0/directory/room/{}/alias",
            room_id
        ))
        .header("Authorization", format!("Bearer {}", outsider_token))
        .body(Body::empty())
        .unwrap();
    let outsider_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), outsider_request)
        .await
        .unwrap();
    assert_eq!(outsider_response.status(), StatusCode::FORBIDDEN);

    let owner_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/r0/directory/room/{}/alias",
            room_id
        ))
        .header("Authorization", format!("Bearer {}", owner_token))
        .body(Body::empty())
        .unwrap();
    let owner_response = ServiceExt::<Request<Body>>::oneshot(app, owner_request)
        .await
        .unwrap();
    assert_eq!(owner_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_room_invite_route_succeeds_for_existing_user() {
    let Some(app) = setup_test_app().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let Some(alice_token) =
        register_user(&app, &format!("invite_alice_{}", rand::random::<u32>())).await
    else {
        eprintln!("Skipping test: failed to register alice");
        return;
    };

    let Some(bob_token) =
        register_user(&app, &format!("invite_bob_{}", rand::random::<u32>())).await
    else {
        eprintln!("Skipping test: failed to register bob");
        return;
    };

    let Some(room_id) = create_room(&app, &alice_token, "Invite Success Room").await else {
        eprintln!("Skipping test: failed to create room");
        return;
    };

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
        .body(Body::from(json!({ "user_id": bob_user_id }).to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_room_invite_route_returns_not_found_for_missing_user() {
    let Some(app) = setup_test_app().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let Some(alice_token) =
        register_user(&app, &format!("invite_missing_{}", rand::random::<u32>())).await
    else {
        eprintln!("Skipping test: failed to register alice");
        return;
    };

    let Some(room_id) = create_room(&app, &alice_token, "Invite Missing User Room").await else {
        eprintln!("Skipping test: failed to create room");
        return;
    };

    let missing_user_id = format!("@missing_user_{}:localhost", rand::random::<u32>());

    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/invite", room_id))
        .header("Authorization", format!("Bearer {}", alice_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({ "user_id": missing_user_id }).to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app, request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_NOT_FOUND");
}

#[tokio::test]
async fn test_room_invite_route_rejects_non_member_inviter() {
    let Some(app) = setup_test_app().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let Some(alice_token) =
        register_user(&app, &format!("invite_owner_{}", rand::random::<u32>())).await
    else {
        eprintln!("Skipping test: failed to register alice");
        return;
    };

    let Some(eve_token) =
        register_user(&app, &format!("invite_intruder_{}", rand::random::<u32>())).await
    else {
        eprintln!("Skipping test: failed to register eve");
        return;
    };

    let Some(bob_token) =
        register_user(&app, &format!("invite_target_{}", rand::random::<u32>())).await
    else {
        eprintln!("Skipping test: failed to register bob");
        return;
    };

    let Some(room_id) = create_room(&app, &alice_token, "Invite Authorization Room").await else {
        eprintln!("Skipping test: failed to create room");
        return;
    };

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
        .header("Authorization", format!("Bearer {}", eve_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "user_id": bob_user_id }).to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app, request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_FORBIDDEN");
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
async fn test_room_write_routes_reject_admin_non_member() {
    let Some((app, pool, cache)) = setup_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let owner_name = format!("room_owner_{}", rand::random::<u32>());
    let member_name = format!("room_member_{}", rand::random::<u32>());
    let admin_name = format!("room_admin_{}", rand::random::<u32>());
    let admin_user_id = format!("@{}:localhost", admin_name);

    let owner_token = register_user(&app, &owner_name)
        .await
        .expect("owner should register");
    let member_token = register_user(&app, &member_name)
        .await
        .expect("member should register");
    let admin_token = register_user(&app, &admin_name)
        .await
        .expect("admin should register");
    promote_to_admin(&pool, &cache, &admin_user_id).await;

    let room_id = create_room(&app, &owner_token, "Admin Non-Member Write Guard Room")
        .await
        .expect("room should be created");
    let member_user_id = get_user_id(&app, &member_token)
        .await
        .expect("member user id should resolve");

    let invite_request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/invite", room_id))
        .header("Authorization", format!("Bearer {}", owner_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "user_id": member_user_id }).to_string()))
        .unwrap();
    let invite_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), invite_request)
        .await
        .unwrap();
    assert_eq!(invite_response.status(), StatusCode::OK);

    let join_request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/join", room_id))
        .header("Authorization", format!("Bearer {}", member_token))
        .body(Body::empty())
        .unwrap();
    let join_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), join_request)
        .await
        .unwrap();
    assert_eq!(join_response.status(), StatusCode::OK);

    let seed_message_request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/r0/rooms/{}/send/m.room.message/owner_seed_txn",
            room_id
        ))
        .header("Authorization", format!("Bearer {}", owner_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({ "msgtype": "m.text", "body": "seed event" }).to_string(),
        ))
        .unwrap();
    let seed_message_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), seed_message_request)
            .await
            .unwrap();
    assert_eq!(seed_message_response.status(), StatusCode::OK);
    let seed_body = axum::body::to_bytes(seed_message_response.into_body(), 1024)
        .await
        .unwrap();
    let seed_json: Value = serde_json::from_slice(&seed_body).unwrap();
    let event_id = seed_json["event_id"]
        .as_str()
        .expect("seed message should return event id");

    let send_request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/r0/rooms/{}/send/m.room.message/admin_write_txn",
            room_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({ "msgtype": "m.text", "body": "admin outsider write" }).to_string(),
        ))
        .unwrap();
    let send_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), send_request)
        .await
        .unwrap();
    assert_eq!(send_response.status(), StatusCode::FORBIDDEN);

    let state_request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/r0/rooms/{}/state/m.room.topic",
            room_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({ "topic": "admin outsider topic" }).to_string(),
        ))
        .unwrap();
    let state_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), state_request)
        .await
        .unwrap();
    assert_eq!(state_response.status(), StatusCode::FORBIDDEN);

    let power_levels_request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/r0/rooms/{}/send/m.room.power_levels/admin_power_levels_txn",
            room_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "users_default": 0,
                "events_default": 0,
                "state_default": 50,
                "ban": 50,
                "kick": 50,
                "redact": 50,
                "invite": 0
            })
            .to_string(),
        ))
        .unwrap();
    let power_levels_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), power_levels_request)
            .await
            .unwrap();
    assert_eq!(power_levels_response.status(), StatusCode::FORBIDDEN);

    let redact_request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/r0/rooms/{}/redact/{}/admin_redact_txn",
            room_id, event_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({ "reason": "admin outsider redact" }).to_string(),
        ))
        .unwrap();
    let redact_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), redact_request)
        .await
        .unwrap();
    assert_eq!(redact_response.status(), StatusCode::FORBIDDEN);

    let kick_request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/kick", room_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({ "user_id": member_user_id, "reason": "admin outsider kick" }).to_string(),
        ))
        .unwrap();
    let kick_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), kick_request)
        .await
        .unwrap();
    assert_eq!(kick_response.status(), StatusCode::FORBIDDEN);

    let ban_request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/ban", room_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({ "user_id": member_user_id, "reason": "admin outsider ban" }).to_string(),
        ))
        .unwrap();
    let ban_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), ban_request)
        .await
        .unwrap();
    assert_eq!(ban_response.status(), StatusCode::FORBIDDEN);

    let unban_request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/unban", room_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "user_id": member_user_id }).to_string()))
        .unwrap();
    let unban_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), unban_request)
        .await
        .unwrap();
    assert_eq!(unban_response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_unban_requires_ban_power_level() {
    let Some(app) = setup_test_app().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let owner_name = format!("unban_owner_{}", rand::random::<u32>());
    let moderator_name = format!("unban_moderator_{}", rand::random::<u32>());
    let target_name = format!("unban_target_{}", rand::random::<u32>());
    let owner_user_id = format!("@{}:localhost", owner_name);
    let moderator_user_id = format!("@{}:localhost", moderator_name);
    let target_user_id = format!("@{}:localhost", target_name);

    let owner_token = register_user(&app, &owner_name)
        .await
        .expect("owner should register");
    let moderator_token = register_user(&app, &moderator_name)
        .await
        .expect("moderator should register");
    let target_token = register_user(&app, &target_name)
        .await
        .expect("target should register");

    let room_id = create_room(&app, &owner_token, "Unban Ban Threshold Guard")
        .await
        .expect("room should be created");

    for invited_user_id in [&moderator_user_id, &target_user_id] {
        let invite_request = Request::builder()
            .method("POST")
            .uri(format!("/_matrix/client/r0/rooms/{}/invite", room_id))
            .header("Authorization", format!("Bearer {}", owner_token))
            .header("Content-Type", "application/json")
            .body(Body::from(
                json!({ "user_id": invited_user_id }).to_string(),
            ))
            .unwrap();
        let invite_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), invite_request)
            .await
            .unwrap();
        assert_eq!(invite_response.status(), StatusCode::OK);
    }

    for token in [&moderator_token, &target_token] {
        let join_request = Request::builder()
            .method("POST")
            .uri(format!("/_matrix/client/r0/rooms/{}/join", room_id))
            .header("Authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();
        let join_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), join_request)
            .await
            .unwrap();
        assert_eq!(join_response.status(), StatusCode::OK);
    }

    let power_levels_request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/r0/rooms/{}/state/m.room.power_levels",
            room_id
        ))
        .header("Authorization", format!("Bearer {}", owner_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "users": {
                    owner_user_id.clone(): 100,
                    moderator_user_id.clone(): 50,
                    target_user_id.clone(): 0
                },
                "users_default": 0,
                "events_default": 0,
                "state_default": 50,
                "ban": 75,
                "kick": 50,
                "redact": 50,
                "invite": 0
            })
            .to_string(),
        ))
        .unwrap();
    let power_levels_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), power_levels_request)
            .await
            .unwrap();
    assert_eq!(power_levels_response.status(), StatusCode::OK);

    let ban_request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/ban", room_id))
        .header("Authorization", format!("Bearer {}", owner_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({ "user_id": target_user_id, "reason": "ban threshold regression guard" })
                .to_string(),
        ))
        .unwrap();
    let ban_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), ban_request)
        .await
        .unwrap();
    assert_eq!(ban_response.status(), StatusCode::OK);

    let unban_request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/unban", room_id))
        .header("Authorization", format!("Bearer {}", moderator_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "user_id": target_user_id }).to_string()))
        .unwrap();
    let unban_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), unban_request)
        .await
        .unwrap();
    assert_eq!(unban_response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_upgrade_room_requires_tombstone_permission() {
    let Some(app) = setup_test_app().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let owner_name = format!("upgrade_owner_{}", rand::random::<u32>());
    let member_name = format!("upgrade_member_{}", rand::random::<u32>());
    let member_user_id = format!("@{}:localhost", member_name);

    let owner_token = register_user(&app, &owner_name)
        .await
        .expect("owner should register");
    let member_token = register_user(&app, &member_name)
        .await
        .expect("member should register");

    let room_id = create_room(&app, &owner_token, "Upgrade Permission Guard Room")
        .await
        .expect("room should be created");

    let invite_request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/invite", room_id))
        .header("Authorization", format!("Bearer {}", owner_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "user_id": member_user_id }).to_string()))
        .unwrap();
    let invite_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), invite_request)
        .await
        .unwrap();
    assert_eq!(invite_response.status(), StatusCode::OK);

    let join_request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/join", room_id))
        .header("Authorization", format!("Bearer {}", member_token))
        .body(Body::empty())
        .unwrap();
    let join_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), join_request)
        .await
        .unwrap();
    assert_eq!(join_response.status(), StatusCode::OK);

    let upgrade_request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/upgrade", room_id))
        .header("Authorization", format!("Bearer {}", member_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "new_version": "11" }).to_string()))
        .unwrap();
    let upgrade_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), upgrade_request)
        .await
        .unwrap();
    assert_eq!(upgrade_response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_unban_rejects_target_with_higher_power_level() {
    let Some(app) = setup_test_app().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let owner_name = format!("unban_owner_power_{}", rand::random::<u32>());
    let moderator_name = format!("unban_moderator_power_{}", rand::random::<u32>());
    let target_name = format!("unban_target_power_{}", rand::random::<u32>());
    let owner_user_id = format!("@{}:localhost", owner_name);
    let moderator_user_id = format!("@{}:localhost", moderator_name);
    let target_user_id = format!("@{}:localhost", target_name);

    let owner_token = register_user(&app, &owner_name)
        .await
        .expect("owner should register");
    let moderator_token = register_user(&app, &moderator_name)
        .await
        .expect("moderator should register");
    let target_token = register_user(&app, &target_name)
        .await
        .expect("target should register");

    let room_id = create_room(&app, &owner_token, "Unban Power Guard Room")
        .await
        .expect("room should be created");

    for invited_user_id in [&moderator_user_id, &target_user_id] {
        let invite_request = Request::builder()
            .method("POST")
            .uri(format!("/_matrix/client/r0/rooms/{}/invite", room_id))
            .header("Authorization", format!("Bearer {}", owner_token))
            .header("Content-Type", "application/json")
            .body(Body::from(
                json!({ "user_id": invited_user_id }).to_string(),
            ))
            .unwrap();
        let invite_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), invite_request)
            .await
            .unwrap();
        assert_eq!(invite_response.status(), StatusCode::OK);
    }

    for token in [&moderator_token, &target_token] {
        let join_request = Request::builder()
            .method("POST")
            .uri(format!("/_matrix/client/r0/rooms/{}/join", room_id))
            .header("Authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();
        let join_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), join_request)
            .await
            .unwrap();
        assert_eq!(join_response.status(), StatusCode::OK);
    }

    let power_levels_request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/r0/rooms/{}/state/m.room.power_levels",
            room_id
        ))
        .header("Authorization", format!("Bearer {}", owner_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "users": {
                    owner_user_id.clone(): 100,
                    moderator_user_id.clone(): 75,
                    target_user_id.clone(): 80
                },
                "users_default": 0,
                "events_default": 0,
                "state_default": 50,
                "ban": 50,
                "kick": 50,
                "redact": 50,
                "invite": 0
            })
            .to_string(),
        ))
        .unwrap();
    let power_levels_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), power_levels_request)
            .await
            .unwrap();
    assert_eq!(power_levels_response.status(), StatusCode::OK);

    let ban_request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/ban", room_id))
        .header("Authorization", format!("Bearer {}", owner_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({ "user_id": target_user_id, "reason": "ban target before unban guard" })
                .to_string(),
        ))
        .unwrap();
    let ban_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), ban_request)
        .await
        .unwrap();
    assert_eq!(ban_response.status(), StatusCode::OK);

    let unban_request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/unban", room_id))
        .header("Authorization", format!("Bearer {}", moderator_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "user_id": target_user_id }).to_string()))
        .unwrap();
    let unban_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), unban_request)
        .await
        .unwrap();
    assert_eq!(unban_response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_redact_requires_current_room_membership_even_for_sender() {
    let Some(app) = setup_test_app().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let user_name = format!("redact_leave_user_{}", rand::random::<u32>());
    let token = register_user(&app, &user_name)
        .await
        .expect("user should register");

    let create_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/createRoom")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({ "name": "Redact Membership Guard Room" }).to_string(),
        ))
        .unwrap();
    let create_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), create_request)
        .await
        .unwrap();
    assert_eq!(create_response.status(), StatusCode::OK);
    let create_body = axum::body::to_bytes(create_response.into_body(), 1024)
        .await
        .unwrap();
    let create_json: Value = serde_json::from_slice(&create_body).unwrap();
    let room_id = create_json["room_id"].as_str().unwrap().to_string();

    let send_request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/r0/rooms/{}/send/m.room.message/redact_membership_txn",
            room_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({ "msgtype": "m.text", "body": "redact after leave" }).to_string(),
        ))
        .unwrap();
    let send_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), send_request)
        .await
        .unwrap();
    assert_eq!(send_response.status(), StatusCode::OK);
    let send_body = axum::body::to_bytes(send_response.into_body(), 1024)
        .await
        .unwrap();
    let send_json: Value = serde_json::from_slice(&send_body).unwrap();
    let event_id = send_json["event_id"].as_str().unwrap().to_string();

    let leave_request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/leave", room_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let leave_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), leave_request)
        .await
        .unwrap();
    assert_eq!(leave_response.status(), StatusCode::OK);

    let redact_request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/r0/rooms/{}/redact/{}/redact_after_leave_txn",
            room_id, event_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({ "reason": "should fail after leaving room" }).to_string(),
        ))
        .unwrap();
    let redact_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), redact_request)
        .await
        .unwrap();
    assert_eq!(redact_response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_room_visibility_route_rejects_admin_non_creator() {
    let Some((app, pool, cache)) = setup_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let owner_name = format!("visibility_owner_{}", rand::random::<u32>());
    let admin_name = format!("visibility_admin_{}", rand::random::<u32>());
    let admin_user_id = format!("@{}:localhost", admin_name);

    let owner_token = register_user(&app, &owner_name)
        .await
        .expect("owner should register");
    let admin_token = register_user(&app, &admin_name)
        .await
        .expect("admin should register");
    promote_to_admin(&pool, &cache, &admin_user_id).await;

    let room_id = create_room(&app, &owner_token, "Visibility Guard Room")
        .await
        .expect("room should be created");

    let admin_request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/r0/directory/list/room/{}",
            room_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "visibility": "public" }).to_string()))
        .unwrap();
    let admin_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), admin_request)
        .await
        .unwrap();
    assert_eq!(admin_response.status(), StatusCode::FORBIDDEN);

    let owner_request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/r0/directory/list/room/{}",
            room_id
        ))
        .header("Authorization", format!("Bearer {}", owner_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "visibility": "public" }).to_string()))
        .unwrap();
    let owner_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), owner_request)
        .await
        .unwrap();
    assert_eq!(owner_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_knock_rejects_invite_only_room() {
    let Some(app) = setup_test_app().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let owner_name = format!("knock_owner_{}", rand::random::<u32>());
    let outsider_name = format!("knock_outsider_{}", rand::random::<u32>());

    let owner_token = register_user(&app, &owner_name)
        .await
        .expect("owner should register");
    let outsider_token = register_user(&app, &outsider_name)
        .await
        .expect("outsider should register");

    let room_id = create_room(&app, &owner_token, "Invite Only Knock Guard")
        .await
        .expect("room should be created");

    let knock_request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/knock/{}", room_id))
        .header("Authorization", format!("Bearer {}", outsider_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "reason": "let me in" }).to_string()))
        .unwrap();
    let knock_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), knock_request)
        .await
        .unwrap();
    assert_eq!(knock_response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_room_visibility_route_rejects_outsider_private_update() {
    let Some(app) = setup_test_app().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let owner_name = format!("visibility_private_owner_{}", rand::random::<u32>());
    let outsider_name = format!("visibility_private_outsider_{}", rand::random::<u32>());

    let owner_token = register_user(&app, &owner_name)
        .await
        .expect("owner should register");
    let outsider_token = register_user(&app, &outsider_name)
        .await
        .expect("outsider should register");

    let room_id = create_room(&app, &owner_token, "Visibility Private Guard Room")
        .await
        .expect("room should be created");

    let outsider_request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/r0/directory/list/room/{}",
            room_id
        ))
        .header("Authorization", format!("Bearer {}", outsider_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "visibility": "private" }).to_string()))
        .unwrap();
    let outsider_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), outsider_request)
        .await
        .unwrap();
    assert_eq!(outsider_response.status(), StatusCode::FORBIDDEN);

    let owner_request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/r0/directory/list/room/{}",
            room_id
        ))
        .header("Authorization", format!("Bearer {}", owner_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "visibility": "private" }).to_string()))
        .unwrap();
    let owner_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), owner_request)
        .await
        .unwrap();
    assert_eq!(owner_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_send_receipt_rejects_non_member() {
    let Some(app) = setup_test_app().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let owner_token = register_user(&app, &format!("receipt_owner_{}", rand::random::<u32>()))
        .await
        .expect("owner should register");
    let outsider_token =
        register_user(&app, &format!("receipt_outsider_{}", rand::random::<u32>()))
            .await
            .expect("outsider should register");

    let room_id = create_room(&app, &owner_token, "Receipt Membership Guard")
        .await
        .expect("room should be created");
    let event_id = send_message_event(
        &app,
        &owner_token,
        &room_id,
        "receipt_seed_txn",
        json!({ "msgtype": "m.text", "body": "seed receipt event" }),
    )
    .await
    .expect("seed message should succeed");

    let request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_matrix/client/r0/rooms/{}/receipt/m.read/{}",
            room_id,
            event_id.replace('$', "%24")
        ))
        .header("Authorization", format!("Bearer {}", outsider_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({}).to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_get_receipts_requires_room_view_access() {
    let Some(app) = setup_test_app().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let owner_token = register_user(&app, &format!("receipts_owner_{}", rand::random::<u32>()))
        .await
        .expect("owner should register");
    let outsider_token = register_user(
        &app,
        &format!("receipts_outsider_{}", rand::random::<u32>()),
    )
    .await
    .expect("outsider should register");

    let room_id = create_room(&app, &owner_token, "Receipts View Guard")
        .await
        .expect("room should be created");
    let event_id = send_message_event(
        &app,
        &owner_token,
        &room_id,
        "receipts_seed_txn",
        json!({ "msgtype": "m.text", "body": "seed receipt lookup event" }),
    )
    .await
    .expect("seed message should succeed");
    let encoded_event_id = event_id.replace('$', "%24");

    let anonymous_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/r0/rooms/{}/receipts/m.read/{}",
            room_id, encoded_event_id
        ))
        .body(Body::empty())
        .unwrap();
    let anonymous_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), anonymous_request)
        .await
        .unwrap();
    assert_eq!(anonymous_response.status(), StatusCode::UNAUTHORIZED);

    let outsider_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/r0/rooms/{}/receipts/m.read/{}",
            room_id,
            event_id.replace('$', "%24")
        ))
        .header("Authorization", format!("Bearer {}", outsider_token))
        .body(Body::empty())
        .unwrap();
    let outsider_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), outsider_request)
        .await
        .unwrap();
    assert_eq!(outsider_response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_send_receipt_rejects_cross_room_event_id() {
    let Some(app) = setup_test_app().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let token = register_user(&app, &format!("receipt_mismatch_{}", rand::random::<u32>()))
        .await
        .expect("user should register");
    let source_room_id = create_room(&app, &token, "Receipt Source Room")
        .await
        .expect("source room should be created");
    let target_room_id = create_room(&app, &token, "Receipt Target Room")
        .await
        .expect("target room should be created");
    let source_event_id = send_message_event(
        &app,
        &token,
        &source_room_id,
        "receipt_mismatch_seed_txn",
        json!({ "msgtype": "m.text", "body": "cross room receipt event" }),
    )
    .await
    .expect("seed message should succeed");

    let request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_matrix/client/r0/rooms/{}/receipt/m.read/{}",
            target_room_id,
            source_event_id.replace('$', "%24")
        ))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({}).to_string()))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_room_visibility_route_rejects_creator_after_leaving_room() {
    let Some(app) = setup_test_app().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let owner_name = format!("visibility_departed_owner_{}", rand::random::<u32>());
    let owner_token = register_user(&app, &owner_name)
        .await
        .expect("owner should register");

    let room_id = create_room(&app, &owner_token, "Visibility Departed Owner Guard")
        .await
        .expect("room should be created");

    let leave_request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/r0/rooms/{}/leave", room_id))
        .header("Authorization", format!("Bearer {}", owner_token))
        .body(Body::empty())
        .unwrap();
    let leave_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), leave_request)
        .await
        .unwrap();
    assert_eq!(leave_response.status(), StatusCode::OK);

    let update_request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/r0/directory/list/room/{}",
            room_id
        ))
        .header("Authorization", format!("Bearer {}", owner_token))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "visibility": "public" }).to_string()))
        .unwrap();
    let update_response = ServiceExt::<Request<Body>>::oneshot(app, update_request)
        .await
        .unwrap();
    assert_eq!(update_response.status(), StatusCode::FORBIDDEN);
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
async fn test_report_room_v3_uses_report_rate_limits_contract_and_returns_expected_payload() {
    let Some((app, pool, _cache)) = setup_test_app_with_pool().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let Some(access_token) =
        register_user(&app, &format!("report_room_v3_{}", rand::random::<u32>())).await
    else {
        eprintln!("Skipping test: failed to register user");
        return;
    };
    let Some(room_id) = create_room(&app, &access_token, "Report Room Contract").await else {
        eprintln!("Skipping test: failed to create room");
        return;
    };

    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/rooms/{}/report", room_id))
        .header("Authorization", format!("Bearer {}", access_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "reason": "spam",
                "description": "integration coverage for blocked_until_at"
            })
            .to_string(),
        ))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 2048)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(json["report_id"].is_number());
    assert_eq!(json["room_id"], room_id);
    assert_eq!(json["status"], "submitted");

    let report_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM event_reports
        WHERE room_id = $1
          AND event_id = $2
        "#,
    )
    .bind(&room_id)
    .bind(format!("room_report:{room_id}"))
    .fetch_one(&*pool)
    .await
    .expect("Failed to verify room report row");
    assert_eq!(report_count, 1);

    let blocked_until_ts_exists: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM information_schema.columns
            WHERE table_schema = current_schema()
              AND table_name = 'report_rate_limits'
              AND column_name = 'blocked_until_ts'
        )
        "#,
    )
    .fetch_one(&*pool)
    .await
    .expect("Failed to inspect blocked_until_ts column");
    assert!(
        !blocked_until_ts_exists,
        "Legacy blocked_until_ts column should not be referenced by the current schema"
    );

    let blocked_until_at_exists: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM information_schema.columns
            WHERE table_schema = current_schema()
              AND table_name = 'report_rate_limits'
              AND column_name = 'blocked_until_at'
        )
        "#,
    )
    .fetch_one(&*pool)
    .await
    .expect("Failed to inspect blocked_until_at column");
    assert!(blocked_until_at_exists);

    let storage = EventReportStorage::new(&pool);
    let blocked_until_at = chrono::Utc::now().timestamp_millis() + 60_000;
    let reporter = sqlx::query_scalar::<_, String>(
        "SELECT reporter_user_id FROM event_reports WHERE room_id = $1 ORDER BY received_ts DESC LIMIT 1",
    )
    .bind(&room_id)
    .fetch_one(&*pool)
    .await
    .expect("Failed to load reporter user id");

    storage
        .block_user_reports(
            &reporter,
            blocked_until_at,
            "rate limited in integration test",
        )
        .await
        .expect("Failed to block reporter");

    let stored_blocked_until_at: Option<i64> =
        sqlx::query_scalar("SELECT blocked_until_at FROM report_rate_limits WHERE user_id = $1")
            .bind(&reporter)
            .fetch_one(&*pool)
            .await
            .expect("Failed to inspect persisted blocked_until_at");
    assert_eq!(stored_blocked_until_at, Some(blocked_until_at));

    let rate_limit = storage
        .check_rate_limit(&reporter)
        .await
        .expect("Failed to evaluate report rate limit");
    assert!(!rate_limit.is_allowed);
    assert_eq!(
        rate_limit.block_reason.as_deref(),
        Some("rate limited in integration test")
    );

    storage
        .unblock_user_reports(&reporter)
        .await
        .expect("Failed to unblock reporter");

    let unblocked_rate_limit = storage
        .check_rate_limit(&reporter)
        .await
        .expect("Failed to evaluate report rate limit after unblock");
    assert!(unblocked_rate_limit.is_allowed);

    delete_room(&pool, &room_id).await;
}

#[tokio::test]
async fn test_report_event_forbidden_for_non_member() {
    let Some(app) = setup_test_app().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let Some(owner_token) = register_user(
        &app,
        &format!("report_event_owner_{}", rand::random::<u32>()),
    )
    .await
    else {
        eprintln!("Skipping test: failed to register owner");
        return;
    };
    let Some(outsider_token) = register_user(
        &app,
        &format!("report_event_outsider_{}", rand::random::<u32>()),
    )
    .await
    else {
        eprintln!("Skipping test: failed to register outsider");
        return;
    };
    let Some(room_id) = create_room(&app, &owner_token, "Report Event Guard").await else {
        eprintln!("Skipping test: failed to create room");
        return;
    };

    let send_request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/r0/rooms/{}/send/m.room.message/report_guard_txn_{}",
            room_id,
            rand::random::<u32>()
        ))
        .header("Authorization", format!("Bearer {}", owner_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({"msgtype": "m.text", "body": "event to report"}).to_string(),
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

    let request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/report/{}",
            room_id, event_id
        ))
        .header("Authorization", format!("Bearer {}", outsider_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({"reason": "outsider should not report", "score": -20}).to_string(),
        ))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app, request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_FORBIDDEN");
}

#[tokio::test]
async fn test_report_room_forbidden_for_non_member() {
    let Some(app) = setup_test_app().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let Some(owner_token) = register_user(
        &app,
        &format!("report_room_owner_{}", rand::random::<u32>()),
    )
    .await
    else {
        eprintln!("Skipping test: failed to register owner");
        return;
    };
    let Some(outsider_token) = register_user(
        &app,
        &format!("report_room_outsider_{}", rand::random::<u32>()),
    )
    .await
    else {
        eprintln!("Skipping test: failed to register outsider");
        return;
    };
    let Some(room_id) = create_room(&app, &owner_token, "Report Room Guard").await else {
        eprintln!("Skipping test: failed to create room");
        return;
    };

    let request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/rooms/{}/report", room_id))
        .header("Authorization", format!("Bearer {}", outsider_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({"reason": "outsider should not report", "description": "not a member"})
                .to_string(),
        ))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app, request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_FORBIDDEN");
}

#[tokio::test]
async fn test_report_event_rejects_event_from_other_room() {
    let Some(app) = setup_test_app().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let Some(access_token) = register_user(
        &app,
        &format!("report_event_mismatch_{}", rand::random::<u32>()),
    )
    .await
    else {
        eprintln!("Skipping test: failed to register user");
        return;
    };
    let Some(source_room_id) = create_room(&app, &access_token, "Report Source Room").await else {
        eprintln!("Skipping test: failed to create source room");
        return;
    };
    let Some(target_room_id) = create_room(&app, &access_token, "Report Target Room").await else {
        eprintln!("Skipping test: failed to create target room");
        return;
    };

    let send_request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/r0/rooms/{}/send/m.room.message/report_mismatch_txn_{}",
            source_room_id,
            rand::random::<u32>()
        ))
        .header("Authorization", format!("Bearer {}", access_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({"msgtype": "m.text", "body": "wrong room event"}).to_string(),
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

    let request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/report/{}",
            target_room_id, event_id
        ))
        .header("Authorization", format!("Bearer {}", access_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({"reason": "room mismatch", "score": -50}).to_string(),
        ))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app, request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_NOT_FOUND");
}

#[tokio::test]
async fn test_scanner_info_rejects_event_from_other_room() {
    let Some(app) = setup_test_app().await else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let Some(access_token) = register_user(
        &app,
        &format!("scanner_info_mismatch_{}", rand::random::<u32>()),
    )
    .await
    else {
        eprintln!("Skipping test: failed to register user");
        return;
    };
    let Some(source_room_id) = create_room(&app, &access_token, "Scanner Source Room").await else {
        eprintln!("Skipping test: failed to create source room");
        return;
    };
    let Some(target_room_id) = create_room(&app, &access_token, "Scanner Target Room").await else {
        eprintln!("Skipping test: failed to create target room");
        return;
    };

    let send_request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/r0/rooms/{}/send/m.room.message/scanner_mismatch_txn_{}",
            source_room_id,
            rand::random::<u32>()
        ))
        .header("Authorization", format!("Bearer {}", access_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({"msgtype": "m.text", "body": "wrong room scanner event"}).to_string(),
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

    let request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v1/rooms/{}/report/{}/scanner_info",
            target_room_id, event_id
        ))
        .header("Authorization", format!("Bearer {}", access_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app, request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["errcode"], "M_NOT_FOUND");
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

#[tokio::test]
async fn test_room_timeline_route_returns_real_messages() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let Some(token) =
        register_user(&app, &format!("room_timeline_{}", rand::random::<u32>())).await
    else {
        return;
    };

    let Some(room_id) = create_room(&app, &token, "Room Timeline Route").await else {
        return;
    };

    let send_request = Request::builder()
        .method("PUT")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/send/m.room.message/timeline_txn_{}",
            room_id,
            rand::random::<u32>()
        ))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({"msgtype": "m.text", "body": "timeline route message"}).to_string(),
        ))
        .unwrap();
    let send_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), send_request)
        .await
        .unwrap();
    assert_eq!(send_response.status(), StatusCode::OK);

    let timeline_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/timeline?limit=10",
            room_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let timeline_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), timeline_request)
        .await
        .unwrap();
    assert_eq!(timeline_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(timeline_response.into_body(), 4096)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let chunk = json["chunk"].as_array().unwrap();
    assert!(!chunk.is_empty());
    assert_eq!(
        chunk[0]["content"]["body"].as_str(),
        Some("timeline route message")
    );
    assert!(json.get("start").is_some());
    assert!(json.get("end").is_some());
}

#[tokio::test]
async fn test_room_messages_route_accepts_sync_prev_batch_token() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let username = format!("room_messages_sync_{}", rand::random::<u32>());
    let Some(token) = register_user(&app, &username).await else {
        return;
    };

    let request_whoami = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/account/whoami")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let response_whoami = ServiceExt::<Request<Body>>::oneshot(app.clone(), request_whoami)
        .await
        .unwrap();
    assert_eq!(response_whoami.status(), StatusCode::OK);
    let body_whoami = axum::body::to_bytes(response_whoami.into_body(), 2048)
        .await
        .unwrap();
    let whoami_json: Value = serde_json::from_slice(&body_whoami).unwrap();
    let user_id = whoami_json["user_id"].as_str().unwrap();

    let Some(room_id) = create_room(&app, &token, "Room Messages Sync Token").await else {
        return;
    };

    for body in ["msg-1", "msg-2", "msg-3"] {
        let send_request = Request::builder()
            .method("PUT")
            .uri(format!(
                "/_matrix/client/v3/rooms/{}/send/m.room.message/{}_{}",
                room_id,
                body,
                rand::random::<u32>()
            ))
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .body(Body::from(
                json!({ "msgtype": "m.text", "body": body }).to_string(),
            ))
            .unwrap();
        let send_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), send_request)
            .await
            .unwrap();
        assert_eq!(send_response.status(), StatusCode::OK);
    }

    let create_filter_request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/user/{}/filter", user_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "room": {
                    "timeline": {
                        "limit": 2,
                        "types": ["m.room.message"]
                    }
                }
            })
            .to_string(),
        ))
        .unwrap();
    let create_filter_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), create_filter_request)
            .await
            .unwrap();
    assert_eq!(create_filter_response.status(), StatusCode::OK);
    let create_filter_body = axum::body::to_bytes(create_filter_response.into_body(), 2048)
        .await
        .unwrap();
    let create_filter_json: Value = serde_json::from_slice(&create_filter_body).unwrap();
    let filter_id = create_filter_json["filter_id"].as_str().unwrap();

    let sync_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/sync?filter={}", filter_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let sync_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), sync_request)
        .await
        .unwrap();
    assert_eq!(sync_response.status(), StatusCode::OK);
    let sync_body = axum::body::to_bytes(sync_response.into_body(), 128 * 1024)
        .await
        .unwrap();
    let sync_json: Value = serde_json::from_slice(&sync_body).unwrap();
    let prev_batch = sync_json["rooms"]["join"][room_id.as_str()]["timeline"]["prev_batch"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(prev_batch.starts_with('t'));

    let messages_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/messages?from={}&dir=b&limit=5",
            room_id, prev_batch
        ))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let messages_response = ServiceExt::<Request<Body>>::oneshot(app, messages_request)
        .await
        .unwrap();
    assert_eq!(messages_response.status(), StatusCode::OK);

    let messages_body = axum::body::to_bytes(messages_response.into_body(), 128 * 1024)
        .await
        .unwrap();
    let messages_json: Value = serde_json::from_slice(&messages_body).unwrap();
    let chunk = messages_json["chunk"].as_array().unwrap();
    assert!(!chunk.is_empty());
    assert_eq!(messages_json["start"], prev_batch);
    assert_eq!(chunk[0]["type"], "m.room.message");
    assert_eq!(chunk[0]["content"]["body"], "msg-1");
    assert!(messages_json["end"].as_str().unwrap().starts_with('t'));
}

#[tokio::test]
async fn test_room_timeline_route_rejects_non_member() {
    let Some((app, pool, cache)) = setup_test_app_with_pool().await else {
        return;
    };

    let Some(alice_token) =
        register_user(&app, &format!("timeline_owner_{}", rand::random::<u32>())).await
    else {
        return;
    };
    let Some(bob_token) =
        register_user(&app, &format!("timeline_guest_{}", rand::random::<u32>())).await
    else {
        return;
    };
    let admin_username = format!("timeline_admin_{}", rand::random::<u32>());
    let Some(admin_token) = register_user(&app, &admin_username).await else {
        return;
    };
    promote_to_admin(&pool, &cache, &format!("@{}:localhost", admin_username)).await;

    let Some(room_id) = create_room(&app, &alice_token, "Timeline Protected Room").await else {
        return;
    };

    let timeline_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/timeline?limit=10",
            room_id
        ))
        .header("Authorization", format!("Bearer {}", bob_token))
        .body(Body::empty())
        .unwrap();
    let timeline_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), timeline_request)
        .await
        .unwrap();
    assert_eq!(timeline_response.status(), StatusCode::FORBIDDEN);

    let admin_timeline_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/timeline?limit=10",
            room_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let admin_timeline_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), admin_timeline_request)
            .await
            .unwrap();
    assert_eq!(admin_timeline_response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_room_messages_route_rejects_non_member() {
    let Some((app, pool, cache)) = setup_test_app_with_pool().await else {
        return;
    };

    let Some(alice_token) =
        register_user(&app, &format!("messages_owner_{}", rand::random::<u32>())).await
    else {
        return;
    };
    let Some(bob_token) =
        register_user(&app, &format!("messages_guest_{}", rand::random::<u32>())).await
    else {
        return;
    };
    let admin_username = format!("messages_admin_{}", rand::random::<u32>());
    let Some(admin_token) = register_user(&app, &admin_username).await else {
        return;
    };
    promote_to_admin(&pool, &cache, &format!("@{}:localhost", admin_username)).await;

    let Some(room_id) = create_room(&app, &alice_token, "Messages Protected Room").await else {
        return;
    };

    let request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/messages?from=0&dir=b&limit=10",
            room_id
        ))
        .header("Authorization", format!("Bearer {}", bob_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let admin_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v3/rooms/{}/messages?from=0&dir=b&limit=10",
            room_id
        ))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let admin_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), admin_request)
        .await
        .unwrap();
    assert_eq!(admin_response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_room_state_route_rejects_non_member() {
    let Some((app, pool, cache)) = setup_test_app_with_pool().await else {
        return;
    };

    let Some(alice_token) =
        register_user(&app, &format!("state_owner_{}", rand::random::<u32>())).await
    else {
        return;
    };
    let Some(bob_token) =
        register_user(&app, &format!("state_guest_{}", rand::random::<u32>())).await
    else {
        return;
    };
    let admin_username = format!("state_admin_{}", rand::random::<u32>());
    let Some(admin_token) = register_user(&app, &admin_username).await else {
        return;
    };
    promote_to_admin(&pool, &cache, &format!("@{}:localhost", admin_username)).await;

    let Some(room_id) = create_room(&app, &alice_token, "State Protected Room").await else {
        return;
    };

    let request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/r0/rooms/{}/state", room_id))
        .header("Authorization", format!("Bearer {}", bob_token))
        .body(Body::empty())
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let admin_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/r0/rooms/{}/state", room_id))
        .header("Authorization", format!("Bearer {}", admin_token))
        .body(Body::empty())
        .unwrap();
    let admin_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), admin_request)
        .await
        .unwrap();
    assert_eq!(admin_response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_room_unread_count_route_returns_counts_from_summary() {
    let Some((app, pool, _cache)) = setup_test_app_with_pool().await else {
        return;
    };

    let Some(token) = register_user(&app, &format!("room_unread_{}", rand::random::<u32>())).await
    else {
        return;
    };

    let Some(room_id) = create_room(&app, &token, "Room Unread Route").await else {
        return;
    };

    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        r#"
        INSERT INTO events (event_id, room_id, sender, user_id, event_type, content, origin_server_ts)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
    )
    .bind(format!("$unread_{}:localhost", rand::random::<u64>()))
    .bind(&room_id)
    .bind("@bob:localhost")
    .bind("@bob:localhost")
    .bind("m.room.message")
    .bind(json!({ "msgtype": "m.text", "body": "hello @room" }))
    .bind(now)
    .execute(&*pool)
    .await
    .unwrap();

    let unread_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/rooms/{}/unread_count", room_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let unread_response = ServiceExt::<Request<Body>>::oneshot(app, unread_request)
        .await
        .unwrap();
    assert_eq!(unread_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(unread_response.into_body(), 2048)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["notification_count"], 1);
    assert_eq!(json["highlight_count"], 1);
}

#[tokio::test]
async fn test_room_unread_count_route_rejects_non_member() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let Some(alice_token) =
        register_user(&app, &format!("unread_owner_{}", rand::random::<u32>())).await
    else {
        return;
    };
    let Some(bob_token) =
        register_user(&app, &format!("unread_guest_{}", rand::random::<u32>())).await
    else {
        return;
    };

    let Some(room_id) = create_room(&app, &alice_token, "Unread Protected Room").await else {
        return;
    };

    let unread_request = Request::builder()
        .method("GET")
        .uri(format!("/_matrix/client/v3/rooms/{}/unread_count", room_id))
        .header("Authorization", format!("Bearer {}", bob_token))
        .body(Body::empty())
        .unwrap();
    let unread_response = ServiceExt::<Request<Body>>::oneshot(app, unread_request)
        .await
        .unwrap();
    assert_eq!(unread_response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_room_spaces_route_returns_child_rooms_and_parent_spaces() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let Some(token) = register_user(&app, &format!("room_spaces_{}", rand::random::<u32>())).await
    else {
        return;
    };

    let Some(space_room_id) = create_room(&app, &token, "Parent Space Room").await else {
        return;
    };
    let Some(child_room_id) = create_room(&app, &token, "Child Room").await else {
        return;
    };

    let create_space_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/spaces")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "room_id": space_room_id,
                "name": "Parent Space",
                "is_public": false
            })
            .to_string(),
        ))
        .unwrap();
    let create_space_response =
        ServiceExt::<Request<Body>>::oneshot(app.clone(), create_space_request)
            .await
            .unwrap();
    assert_eq!(create_space_response.status(), StatusCode::CREATED);
    let create_space_body = axum::body::to_bytes(create_space_response.into_body(), 4096)
        .await
        .unwrap();
    let create_space_json: Value = serde_json::from_slice(&create_space_body).unwrap();
    let space_id = create_space_json["space_id"].as_str().unwrap().to_string();

    let add_child_request = Request::builder()
        .method("POST")
        .uri(format!("/_matrix/client/v3/spaces/{}/children", space_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "room_id": child_room_id,
                "via_servers": ["localhost"],
                "suggested": true
            })
            .to_string(),
        ))
        .unwrap();
    let add_child_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), add_child_request)
        .await
        .unwrap();
    assert_eq!(add_child_response.status(), StatusCode::CREATED);

    let parent_request = Request::builder()
        .uri(format!("/_matrix/client/v3/rooms/{}/spaces", space_room_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let parent_response = ServiceExt::<Request<Body>>::oneshot(app.clone(), parent_request)
        .await
        .unwrap();
    assert_eq!(parent_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(parent_response.into_body(), 4096)
        .await
        .unwrap();
    let parent_json: Value = serde_json::from_slice(&body).unwrap();
    let parent_rooms = parent_json["rooms"].as_array().unwrap();
    assert_eq!(parent_rooms.len(), 1);
    assert_eq!(parent_rooms[0]["room_id"], child_room_id);
    assert_eq!(parent_rooms[0]["suggested"], true);

    let child_request = Request::builder()
        .uri(format!("/_matrix/client/v3/rooms/{}/spaces", child_room_id))
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let child_response = ServiceExt::<Request<Body>>::oneshot(app, child_request)
        .await
        .unwrap();
    assert_eq!(child_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(child_response.into_body(), 4096)
        .await
        .unwrap();
    let child_json: Value = serde_json::from_slice(&body).unwrap();
    let child_spaces = child_json["spaces"].as_array().unwrap();
    assert_eq!(child_spaces.len(), 1);
    assert_eq!(child_spaces[0]["room_id"], space_room_id);
}
