use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use once_cell::sync::Lazy;
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

static TEST_POOL: Lazy<Option<Arc<sqlx::PgPool>>> = Lazy::new(|| {
    let database_url =
        match std::env::var("TEST_DATABASE_URL").or_else(|_| std::env::var("DATABASE_URL")) {
            Ok(url) => url,
            Err(_) => return None,
        };

    let rt = tokio::runtime::Runtime::new().ok()?;
    let pool = rt.block_on(async {
        sqlx::postgres::PgPoolOptions::new()
            .max_connections(3)
            .min_connections(1)
            .connect(&database_url)
            .await
            .ok()
    })?;

    Some(Arc::new(pool))
});

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

fn setup_test_app() -> Option<axum::Router> {
    let pool = TEST_POOL.as_ref()?;
    let cache = Arc::new(CacheManager::new(CacheConfig::default()));
    let config = create_test_config();
    let container = ServiceContainer::new(pool, cache.clone(), config, None);
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

#[tokio::test]
async fn test_trusted_private_chat_transaction() {
    let Some(app) = setup_test_app() else {
        eprintln!("Skipping test: database not available");
        return;
    };

    let Some(alice_token) =
        register_user(&app, &format!("alice_tx_{}", rand::random::<u32>())).await
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
            json!({
                "name": "Trusted Room",
                "preset": "trusted_private_chat"
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request)
        .await
        .unwrap();

    if response.status() != StatusCode::OK {
        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        panic!("Create room failed: {:?}", String::from_utf8_lossy(&body));
    }
    assert_eq!(response.status(), StatusCode::OK);
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

    if response.status() != StatusCode::OK {
        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        panic!(
            "Get room state failed: {:?}",
            String::from_utf8_lossy(&body)
        );
    }
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 10240)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let state_events = json
        .get("state")
        .and_then(|v| v.as_array())
        .expect("Expected state array");

    let history_vis = state_events.iter().find(|e| {
        e["type"] == "m.room.history_visibility" && e["content"]["history_visibility"] == "invited"
    });
    assert!(
        history_vis.is_some(),
        "Should have history_visibility = invited"
    );

    let guest_access = state_events.iter().find(|e| {
        e["type"] == "m.room.guest_access" && e["content"]["guest_access"] == "forbidden"
    });
    assert!(
        guest_access.is_some(),
        "Should have guest_access = forbidden"
    );

    let privacy = state_events
        .iter()
        .find(|e| e["type"] == "com.hula.privacy" && e["content"]["action"] == "block_screenshot");
    assert!(
        privacy.is_some(),
        "Should have com.hula.privacy = block_screenshot"
    );
}
