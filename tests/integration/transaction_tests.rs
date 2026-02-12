use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use std::sync::Arc;
use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::common::config::{
    AdminRegistrationConfig, Config, CorsConfig, DatabaseConfig, FederationConfig,
    RateLimitConfig, RedisConfig, SearchConfig, SecurityConfig, ServerConfig, SmtpConfig,
    VoipConfig, WorkerConfig,
};
use synapse_rust::services::{DatabaseInitService, ServiceContainer};
use synapse_rust::web::routes::create_router;
use synapse_rust::web::AppState;
use tower::ServiceExt;

async fn setup_test_app() -> axum::Router {
    // Reuse the setup logic from api_room_tests.rs
    // In a real project, this should be in a shared helper module
    let database_url = std::env::var("TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .unwrap_or_else(|_| "postgres://synapse:secret@localhost:5432/synapse_test".to_string());
    let pool = match sqlx::PgPool::connect(&database_url).await {
        Ok(p) => Arc::new(p),
        Err(e) => {
            panic!("Failed to connect to test database: {}", e);
        }
    };

    let init_service = DatabaseInitService::new(pool.clone());
    if let Err(e) = init_service.initialize().await {
        panic!("Database initialization failed: {}", e);
    }

    // Ensure columns exist
    let columns = vec![
        "ALTER TABLE users ADD COLUMN IF NOT EXISTS is_guest BOOLEAN DEFAULT FALSE",
        "ALTER TABLE users ADD COLUMN IF NOT EXISTS consent_version TEXT",
        "ALTER TABLE users ADD COLUMN IF NOT EXISTS appservice_id TEXT",
        "ALTER TABLE users ADD COLUMN IF NOT EXISTS user_type TEXT",
        "ALTER TABLE users ADD COLUMN IF NOT EXISTS shadow_banned BOOLEAN DEFAULT FALSE",
        "ALTER TABLE users ADD COLUMN IF NOT EXISTS migration_state TEXT",
        "ALTER TABLE users ADD COLUMN IF NOT EXISTS updated_ts BIGINT",
        "ALTER TABLE users ADD COLUMN IF NOT EXISTS invalid_update_ts BIGINT",
        "ALTER TABLE users ADD COLUMN IF NOT EXISTS generation BIGINT NOT NULL DEFAULT 1",
    ];
    for sql in columns {
        let _ = sqlx::query(sql).execute(&*pool).await;
    }

    // Ensure core tables exist (workaround for potential migration issues in test env)
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS devices (
            device_id VARCHAR(255) NOT NULL,
            user_id VARCHAR(255) NOT NULL,
            display_name VARCHAR(255),
            device_key JSONB,
            last_seen_ts BIGINT,
            last_seen_ip VARCHAR(255),
            created_at BIGINT NOT NULL,
            first_seen_ts BIGINT NOT NULL,
            created_ts BIGINT,
            appservice_id VARCHAR(255),
            ignored_user_list TEXT,
            PRIMARY KEY (device_id, user_id)
        )
        "#
    )
    .execute(&*pool)
    .await
    .expect("Failed to ensure devices table exists");

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS access_tokens (
            id BIGSERIAL PRIMARY KEY,
            token TEXT NOT NULL UNIQUE,
            user_id VARCHAR(255) NOT NULL,
            device_id VARCHAR(255),
            appservice_id VARCHAR(255),
            expires_ts BIGINT NOT NULL,
            created_ts BIGINT NOT NULL,
            last_used_ts BIGINT,
            user_agent TEXT,
            ip VARCHAR(255),
            invalidated_ts BIGINT
        )
        "#
    )
    .execute(&*pool)
    .await
    .expect("Failed to ensure access_tokens table exists");

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS refresh_tokens (
            id BIGSERIAL PRIMARY KEY,
            token TEXT NOT NULL UNIQUE,
            user_id VARCHAR(255) NOT NULL,
            device_id VARCHAR(255) NOT NULL,
            expires_ts BIGINT NOT NULL,
            created_ts BIGINT NOT NULL,
            invalidated BOOLEAN DEFAULT FALSE,
            invalidated_ts BIGINT
        )
        "#
    )
    .execute(&*pool)
    .await
    .expect("Failed to ensure refresh_tokens table exists");

    // Ensure events table has required columns (workaround for potential migration issues in test env)
    sqlx::query("ALTER TABLE events ADD COLUMN IF NOT EXISTS redacted BOOLEAN DEFAULT FALSE")
        .execute(&*pool)
        .await
        .expect("Failed to ensure events table redacted column exists");

    sqlx::query("ALTER TABLE events ADD COLUMN IF NOT EXISTS unsigned JSONB DEFAULT '{}'::jsonb")
        .execute(&*pool)
        .await
        .expect("Failed to ensure events table unsigned column exists");

    let cache = Arc::new(CacheManager::new(CacheConfig::default()));

    let config = Config {
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
            pool_size: 10,
            max_size: 20,
            min_idle: Some(5),
            connection_timeout: 30,
        },
        redis: RedisConfig {
            host: "localhost".to_string(),
            port: 6379,
            key_prefix: "test:".to_string(),
            pool_size: 10,
            enabled: false,
        },
        logging: synapse_rust::common::config::LoggingConfig {
            level: "info".to_string(),
            format: "json".to_string(),
            log_file: None,
            log_dir: None,
        },
        federation: FederationConfig {
            enabled: true,
            allow_ingress: false,
            server_name: "test.example.com".to_string(),
            federation_port: 8448,
            connection_pool_size: 10,
            max_transaction_payload: 50000,
            ca_file: None,
            client_ca_file: None,
            signing_key: None,
            key_id: None,
        },
        security: SecurityConfig {
            secret: "test_secret".to_string(),
            expiry_time: 3600,
            refresh_token_expiry: 604800,
            argon2_m_cost: 2048,
            argon2_t_cost: 1,
            argon2_p_cost: 1,
        },
        search: SearchConfig {
            elasticsearch_url: "http://localhost:9200".to_string(),
            enabled: false,
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
    };

    let container = ServiceContainer::new(&pool, cache.clone(), config, None);
    let state = AppState::new(container, cache);
    create_router(state)
}

async fn register_user(app: &axum::Router, username: &str) -> String {
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
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    if json.get("access_token").is_none() {
        panic!("Registration response missing access_token: {:?}", json);
    }
    json["access_token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn test_trusted_private_chat_transaction() {
    let app = setup_test_app().await;
    let alice_token = register_user(&app, &format!("alice_tx_{}", rand::random::<u32>())).await;

    // 1. Create Trusted Private Chat
    // This triggers the complex transaction logic with multiple event creations
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

    // 2. Verify Room State Events
    // We expect: m.room.history_visibility, m.room.guest_access, com.hula.privacy
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
        panic!("Get room state failed: {:?}", String::from_utf8_lossy(&body));
    }
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 10240)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    // The implementation wraps the array in a "state" field, though spec says it should be array
    let state_events = json.get("state").and_then(|v| v.as_array()).expect("Expected state array");

    // Verify history_visibility = invited
    let history_vis = state_events.iter().find(|e| {
        e["type"] == "m.room.history_visibility" && 
        e["content"]["history_visibility"] == "invited"
    });
    assert!(history_vis.is_some(), "Should have history_visibility = invited");

    // Verify guest_access = forbidden
    let guest_access = state_events.iter().find(|e| {
        e["type"] == "m.room.guest_access" && 
        e["content"]["guest_access"] == "forbidden"
    });
    assert!(guest_access.is_some(), "Should have guest_access = forbidden");

    // Verify com.hula.privacy = block_screenshot
    let privacy = state_events.iter().find(|e| {
        e["type"] == "com.hula.privacy" && 
        e["content"]["action"] == "block_screenshot"
    });
    assert!(privacy.is_some(), "Should have com.hula.privacy = block_screenshot");
}
