use super::*;
use serde_json::json;

#[test]
fn test_config_rejects_unknown_top_level_key() {
    // deny_unknown_fields：未知顶层配置键应在反序列化时报 "unknown field"，
    // 而非被 serde 静默丢弃（审查 #11，防止运维配置了不存在的字段却误以为生效）。
    let json = serde_json::json!({"unknown_key": "x"});
    let err = serde_json::from_value::<Config>(json).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("unknown field"), "应报 unknown field，实际：{msg}");
}

/// RD-11/RD-12/RD-13 删除配置键后的迁移语义：
/// - 顶层 section（如 `livekit`）受 `deny_unknown_fields` 保护 ⇒ 旧配置必须报错，
///   运维不会误以为「配了就生效」；
/// - 嵌套字段没有 `deny_unknown_fields` ⇒ 旧配置里的残留键被 serde 静默忽略，
///   不会导致启动失败（迁移期行为，一并锁定）。
#[test]
fn test_removed_config_keys_are_rejected_or_ignored_as_documented() {
    let with_livekit = json!({"livekit": {"api_key": "k", "api_secret": "s", "host": "h"}});
    let err = serde_json::from_value::<Config>(with_livekit).unwrap_err();
    assert!(err.to_string().contains("unknown field"), "顶层 `livekit` 段应被拒绝，实际：{err}");

    let server: Result<super::server::ServerConfig, _> = serde_json::from_value(json!({
        "name": "example.com",
        "enable_registration": true,
        "enable_registration_captcha": false,
        "background_tasks_interval": 60,
        "expire_access_token": true,
        "expire_access_token_lifetime": 86_400,
        "refresh_token_sliding_window_size": 3_600,
        "form_secret": "legacy",
        "soft_file_limit": 1024,
        "user_agent_suffix": "legacy",
        "max_image_resolution": 8_000_000,
        "warmup_pool": true,
    }));
    assert!(server.is_ok(), "嵌套残留键应被静默忽略，实际：{:?}", server.err());

    let logging: Result<super::logging::LoggingConfig, _> = serde_json::from_value(json!({
        "level": "info",
        "format": "json",
        "log_dir": "/var/log",
    }));
    assert!(logging.is_ok(), "嵌套残留键应被静默忽略，实际：{:?}", logging.err());
}

#[test]
fn test_config_database_url() {
    let config = Config {
        server: ServerConfig {
            name: "test".to_string(),
            host: "127.0.0.1".to_string(),
            port: 8000,
            public_baseurl: None,
            signing_key_path: None,
            macaroon_secret_key: None,
            server_name: None,
            serve_server_wellknown: false,
            web_client_location: None,
            registration_shared_secret: None,
            admin_contact: None,
            max_upload_size: 1000000,
            remote_media_lifetime: 2592000,
            local_media_lifetime: 0,
            enable_registration: true,
            enable_registration_captcha: false,
            background_tasks_interval: 60,
            dehydrated_device_cleanup_interval_secs: 3600,
            expire_access_token: true,
            expire_access_token_lifetime: 3600,
            refresh_token_sliding_window_size: 1000,
            allow_public_rooms_without_auth: false,
            allow_public_rooms_over_federation: true,
            auto_join_rooms: vec![],
            autocreate_auto_join_rooms: true,
            encryption_enabled_by_default_for_room_type: None,
            app_service_config_files: vec![],
            presence_enabled: true,
            ..Default::default()
        },
        database: DatabaseConfig {
            host: "localhost".to_string(),
            port: 5432,
            username: "testuser".to_string(),
            password: "testpass".to_string(),
            name: "testdb".to_string(),
            max_size: 20,
            min_idle: Some(5),
            connection_timeout: 30,
            ..Default::default()
        },
        redis: RedisConfig {
            host: "localhost".to_string(),
            port: 6379,
            username: None,
            password: None,
            key_prefix: "test:".to_string(),
            pool_size: 10,
            enabled: true,
            connection_timeout_ms: 500,
            command_timeout_ms: 500,
            circuit_breaker: CircuitBreakerConfig::default(),
        },
        logging: LoggingConfig { level: "info".to_string(), format: "json".to_string(), log_file: None },
        federation: FederationConfig {
            enabled: true,
            allow_ingress: false,
            server_name: "test.example.com".to_string(),
            federation_port: 8448,
            max_transaction_payload: 50000,
            ca_file: None,
            client_ca_file: None,
            signing_key: Some("test_signing_key".to_string()),
            key_id: Some("ed25519:test_key".to_string()),
            trusted_key_servers: vec![],
            signature_cache_ttl: 3600,
            key_cache_ttl: 3600,
            key_rotation_grace_period_ms: 60_0000,
            key_fetch_max_concurrency: 32,
            key_fetch_timeout_ms: 5000,
            allow_http_key_fetch: false,
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
            signing_key_master_key: None,
            ..Default::default()
        },
        security: SecurityConfig {
            secret: "test_secret".to_string(),
            expiry_time: 3600,
            refresh_token_expiry: 604800,
            argon2_m_cost: 4096,
            argon2_t_cost: 3,
            argon2_p_cost: 1,
            allow_legacy_hashes: false,
            login_failure_lockout_threshold: 5,
            login_lockout_duration_seconds: 900,
            login_lockout_fail_open_on_redis_error: true,
            login_lockout_max_ip_attempts: 30,
            login_lockout_ip_allowlist: Vec::new(),
            admin_mfa_required: false,
            admin_mfa_shared_secret: String::new(),
            admin_mfa_allowed_drift_steps: default_admin_mfa_allowed_drift_steps(),
            admin_rbac_enabled: default_admin_rbac_enabled(),
            ui_auth_session_timeout: default_ui_auth_session_timeout(),
            csrf_secret: String::new(),
            ..Default::default()
        },
        search: SearchConfig {
            elasticsearch_url: "http://localhost:9200".to_string(),
            enabled: false,
            postgres_fts: PostgresFtsConfig::default(),
            provider: "elasticsearch".to_string(),
            ..Default::default()
        },
        rate_limit: RateLimitConfig::default(),
        admin_registration: AdminRegistrationConfig::default(),
        worker: WorkerConfig::default(),
        cors: CorsConfig {
            allowed_origins: vec!["*".to_string()],
            allow_credentials: false,
            allowed_methods: default_allowed_methods(),
            allowed_headers: default_allowed_headers(),
            max_age_seconds: default_cors_max_age(),
        },
        smtp: SmtpConfig::default(),
        sms: SmsConfig::default(),
        voip: VoipConfig::default(),
        push: PushConfig::default(),
        url_preview: UrlPreviewConfig::default(),
        oidc: OidcConfig::default(),
        builtin_oidc: BuiltinOidcConfig::default(),
        saml: SamlConfig::default(),
        retention: RetentionConfig::default(),
        telemetry: crate::telemetry_config::OpenTelemetryConfig::default(),
        prometheus: crate::telemetry_config::PrometheusConfig::default(),
        performance: PerformanceConfig::default(),
        experimental: ExperimentalConfig::default(),
        identity: IdentityConfig::default(),
        translate: TranslateConfig::default(),
        sso_redirect_allowlist: vec![],
        ..Config::default()
    };

    let url = config.database_url();
    assert_eq!(url, "postgres://testuser:testpass@localhost:5432/testdb");
}

#[test]
fn test_config_redis_url() {
    let config = Config {
        server: ServerConfig {
            name: "test".to_string(),
            host: "127.0.0.1".to_string(),
            port: 8000,
            public_baseurl: None,
            signing_key_path: None,
            macaroon_secret_key: None,
            server_name: None,
            serve_server_wellknown: false,
            web_client_location: None,
            registration_shared_secret: None,
            admin_contact: None,
            max_upload_size: 1000000,
            remote_media_lifetime: 2592000,
            local_media_lifetime: 0,
            enable_registration: true,
            enable_registration_captcha: false,
            background_tasks_interval: 60,
            dehydrated_device_cleanup_interval_secs: 3600,
            expire_access_token: true,
            expire_access_token_lifetime: 3600,
            refresh_token_sliding_window_size: 1000,
            allow_public_rooms_without_auth: false,
            allow_public_rooms_over_federation: true,
            auto_join_rooms: vec![],
            autocreate_auto_join_rooms: true,
            encryption_enabled_by_default_for_room_type: None,
            app_service_config_files: vec![],
            presence_enabled: true,
            ..Default::default()
        },
        database: DatabaseConfig {
            host: "localhost".to_string(),
            port: 5432,
            username: "testuser".to_string(),
            password: "testpass".to_string(),
            name: "testdb".to_string(),
            max_size: 20,
            min_idle: Some(5),
            connection_timeout: 30,
            ..Default::default()
        },
        redis: RedisConfig {
            host: "redis.example.com".to_string(),
            port: 6380,
            username: None,
            password: Some("secret".to_string()),
            key_prefix: "prod:".to_string(),
            pool_size: 20,
            enabled: true,
            connection_timeout_ms: 500,
            command_timeout_ms: 500,
            circuit_breaker: CircuitBreakerConfig::default(),
        },
        logging: LoggingConfig { level: "info".to_string(), format: "json".to_string(), log_file: None },
        federation: FederationConfig {
            enabled: true,
            allow_ingress: false,
            server_name: "test.example.com".to_string(),
            federation_port: 8448,
            max_transaction_payload: 50000,
            ca_file: None,
            client_ca_file: None,
            signing_key: Some("test_signing_key".to_string()),
            key_id: Some("ed25519:test_key".to_string()),
            trusted_key_servers: vec![],
            signature_cache_ttl: 3600,
            key_cache_ttl: 3600,
            key_rotation_grace_period_ms: 60_0000,
            key_fetch_max_concurrency: 32,
            key_fetch_timeout_ms: 5000,
            allow_http_key_fetch: false,
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
            signing_key_master_key: None,
            ..Default::default()
        },
        security: SecurityConfig {
            secret: "test_secret".to_string(),
            expiry_time: 3600,
            refresh_token_expiry: 604800,
            argon2_m_cost: 4096,
            argon2_t_cost: 3,
            argon2_p_cost: 1,
            allow_legacy_hashes: false,
            login_failure_lockout_threshold: 5,
            login_lockout_duration_seconds: 900,
            login_lockout_fail_open_on_redis_error: true,
            login_lockout_max_ip_attempts: 30,
            login_lockout_ip_allowlist: Vec::new(),
            admin_mfa_required: false,
            admin_mfa_shared_secret: String::new(),
            admin_mfa_allowed_drift_steps: default_admin_mfa_allowed_drift_steps(),
            admin_rbac_enabled: default_admin_rbac_enabled(),
            ui_auth_session_timeout: default_ui_auth_session_timeout(),
            csrf_secret: String::new(),
            ..Default::default()
        },
        search: SearchConfig {
            elasticsearch_url: "http://localhost:9200".to_string(),
            enabled: false,
            postgres_fts: PostgresFtsConfig::default(),
            provider: "elasticsearch".to_string(),
            ..Default::default()
        },
        rate_limit: RateLimitConfig::default(),
        admin_registration: AdminRegistrationConfig::default(),
        worker: WorkerConfig::default(),
        cors: CorsConfig {
            allowed_origins: vec!["*".to_string()],
            allow_credentials: false,
            allowed_methods: default_allowed_methods(),
            allowed_headers: default_allowed_headers(),
            max_age_seconds: default_cors_max_age(),
        },
        smtp: SmtpConfig::default(),
        sms: SmsConfig::default(),
        voip: VoipConfig::default(),
        push: PushConfig::default(),
        url_preview: UrlPreviewConfig::default(),
        oidc: OidcConfig::default(),
        saml: SamlConfig::default(),
        retention: RetentionConfig::default(),
        telemetry: crate::telemetry_config::OpenTelemetryConfig::default(),
        prometheus: crate::telemetry_config::PrometheusConfig::default(),
        performance: PerformanceConfig::default(),
        experimental: ExperimentalConfig::default(),
        identity: IdentityConfig::default(),
        ..Config::default()
    };

    let url = config.redis_url();
    assert_eq!(url, "redis://:secret@redis.example.com:6380/");
}

#[test]
fn test_server_config_defaults() {
    let config = ServerConfig {
        name: "test".to_string(),
        host: "0.0.0.0".to_string(),
        port: 8080,
        public_baseurl: None,
        signing_key_path: None,
        macaroon_secret_key: None,
        server_name: None,
        serve_server_wellknown: false,
        web_client_location: None,
        registration_shared_secret: Some("secret".to_string()),
        admin_contact: Some("admin@example.com".to_string()),
        max_upload_size: 50000000,
        remote_media_lifetime: 2592000,
        local_media_lifetime: 0,
        enable_registration: true,
        enable_registration_captcha: true,
        background_tasks_interval: 30,
        dehydrated_device_cleanup_interval_secs: 3600,
        expire_access_token: true,
        expire_access_token_lifetime: 86400,
        refresh_token_sliding_window_size: 5000,
        allow_public_rooms_without_auth: false,
        allow_public_rooms_over_federation: true,
        auto_join_rooms: vec![],
        autocreate_auto_join_rooms: true,
        encryption_enabled_by_default_for_room_type: None,
        app_service_config_files: vec![],
        presence_enabled: true,
        ..Default::default()
    };

    assert_eq!(config.name, "test");
    assert_eq!(config.port, 8080);
    assert!(config.enable_registration);
    assert!(config.registration_shared_secret.is_some());
}

#[test]
fn test_dehydrated_device_cleanup_interval_default() {
    assert_eq!(default_dehydrated_device_cleanup_interval_secs(), 3600);
}

#[test]
fn test_database_config_defaults() {
    let config = DatabaseConfig {
        host: "db.example.com".to_string(),
        port: 5432,
        username: "synapse".to_string(),
        password: "secure_password".to_string(),
        name: "synapse".to_string(),
        max_size: 20,
        min_idle: None,
        connection_timeout: 60,
        ..Default::default()
    };

    assert_eq!(config.host, "db.example.com");
    assert_eq!(config.port, 5432);
    assert!(config.min_idle.is_none());
}

#[test]
fn test_redis_config_defaults() {
    let config = RedisConfig {
        host: "127.0.0.1".to_string(),
        port: 6379,
        username: None,
        password: None,
        key_prefix: "synapse:".to_string(),
        pool_size: 16,
        enabled: true,
        connection_timeout_ms: 500,
        command_timeout_ms: 500,
        circuit_breaker: CircuitBreakerConfig::default(),
    };

    assert_eq!(config.host, "127.0.0.1");
    assert_eq!(config.port, 6379);
    assert!(config.enabled);
    assert_eq!(config.connection_timeout_ms, 500);
    assert_eq!(config.command_timeout_ms, 500);
    assert!(config.circuit_breaker.enabled);
    assert_eq!(config.connection_url(), "redis://127.0.0.1:6379/");
}

#[test]
fn test_redis_config_connection_url_with_password() {
    let config = RedisConfig {
        host: "redis".to_string(),
        port: 6379,
        username: None,
        password: Some("secret".to_string()),
        key_prefix: "synapse:".to_string(),
        pool_size: 16,
        enabled: true,
        connection_timeout_ms: 500,
        command_timeout_ms: 500,
        circuit_breaker: CircuitBreakerConfig::default(),
    };

    assert_eq!(config.connection_url(), "redis://:secret@redis:6379/");
}

#[test]
fn test_resolve_env_variables_resolves_redis_password() -> Result<(), String> {
    test_env::set("TEST_REDIS_PASSWORD", "resolved-secret");
    test_env::set("TEST_REDIS_USERNAME", "resolved-acl-user");

    let mut config = Config {
        server: ServerConfig {
            name: "test".to_string(),
            host: "127.0.0.1".to_string(),
            port: 8000,
            public_baseurl: None,
            signing_key_path: None,
            macaroon_secret_key: None,
            server_name: None,
            serve_server_wellknown: false,
            web_client_location: None,
            registration_shared_secret: None,
            admin_contact: None,
            max_upload_size: 1000000,
            remote_media_lifetime: 2592000,
            local_media_lifetime: 0,
            enable_registration: true,
            enable_registration_captcha: false,
            background_tasks_interval: 60,
            dehydrated_device_cleanup_interval_secs: 3600,
            expire_access_token: true,
            expire_access_token_lifetime: 3600,
            refresh_token_sliding_window_size: 1000,
            allow_public_rooms_without_auth: false,
            allow_public_rooms_over_federation: true,
            auto_join_rooms: vec![],
            autocreate_auto_join_rooms: true,
            encryption_enabled_by_default_for_room_type: None,
            app_service_config_files: vec![],
            presence_enabled: true,
            ..Default::default()
        },
        database: DatabaseConfig {
            host: "localhost".to_string(),
            port: 5432,
            username: "testuser".to_string(),
            password: "testpass".to_string(),
            name: "testdb".to_string(),
            max_size: 20,
            min_idle: Some(5),
            connection_timeout: 30,
            ..Default::default()
        },
        redis: RedisConfig {
            host: "localhost".to_string(),
            port: 6379,
            // 用户名与密码走**同一**解析路径：上游 Redis 6+ ACL 要求
            // username 必须配 password（见 `Config::validate`），因此
            // `${VAR}` 插值也必须覆盖它，否则 `redis.username` 只能写死。
            username: Some("${TEST_REDIS_USERNAME:?missing}".to_string()),
            password: Some("${TEST_REDIS_PASSWORD:?missing}".to_string()),
            key_prefix: "test:".to_string(),
            pool_size: 10,
            enabled: true,
            connection_timeout_ms: 500,
            command_timeout_ms: 500,
            circuit_breaker: CircuitBreakerConfig::default(),
        },
        logging: LoggingConfig { level: "info".to_string(), format: "json".to_string(), log_file: None },
        federation: FederationConfig {
            enabled: true,
            allow_ingress: false,
            server_name: "test.example.com".to_string(),
            federation_port: 8448,
            max_transaction_payload: 50000,
            ca_file: None,
            client_ca_file: None,
            signing_key: Some("test_signing_key".to_string()),
            key_id: Some("ed25519:test_key".to_string()),
            trusted_key_servers: vec![],
            signature_cache_ttl: 3600,
            key_cache_ttl: 3600,
            key_rotation_grace_period_ms: 60_0000,
            key_fetch_max_concurrency: 32,
            key_fetch_timeout_ms: 5000,
            allow_http_key_fetch: false,
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
            signing_key_master_key: None,
            ..Default::default()
        },
        security: SecurityConfig {
            secret: "test_secret".to_string(),
            expiry_time: 3600,
            refresh_token_expiry: 604800,
            argon2_m_cost: 4096,
            argon2_t_cost: 3,
            argon2_p_cost: 1,
            allow_legacy_hashes: false,
            login_failure_lockout_threshold: 5,
            login_lockout_duration_seconds: 900,
            login_lockout_fail_open_on_redis_error: true,
            login_lockout_max_ip_attempts: 30,
            login_lockout_ip_allowlist: Vec::new(),
            admin_mfa_required: false,
            admin_mfa_shared_secret: String::new(),
            admin_mfa_allowed_drift_steps: default_admin_mfa_allowed_drift_steps(),
            admin_rbac_enabled: default_admin_rbac_enabled(),
            ui_auth_session_timeout: default_ui_auth_session_timeout(),
            csrf_secret: String::new(),
            ..Default::default()
        },
        search: SearchConfig {
            elasticsearch_url: "http://localhost:9200".to_string(),
            enabled: false,
            postgres_fts: PostgresFtsConfig::default(),
            provider: "elasticsearch".to_string(),
            ..Default::default()
        },
        rate_limit: RateLimitConfig::default(),
        admin_registration: AdminRegistrationConfig::default(),
        worker: WorkerConfig::default(),
        cors: CorsConfig {
            allowed_origins: vec!["*".to_string()],
            allow_credentials: false,
            allowed_methods: default_allowed_methods(),
            allowed_headers: default_allowed_headers(),
            max_age_seconds: default_cors_max_age(),
        },
        smtp: SmtpConfig::default(),
        sms: SmsConfig::default(),
        voip: VoipConfig::default(),
        push: PushConfig::default(),
        url_preview: UrlPreviewConfig::default(),
        oidc: OidcConfig::default(),
        builtin_oidc: BuiltinOidcConfig::default(),
        saml: SamlConfig::default(),
        retention: RetentionConfig::default(),
        telemetry: crate::telemetry_config::OpenTelemetryConfig::default(),
        prometheus: crate::telemetry_config::PrometheusConfig::default(),
        performance: PerformanceConfig::default(),
        experimental: ExperimentalConfig::default(),
        identity: IdentityConfig::default(),
        translate: TranslateConfig::default(),
        sso_redirect_allowlist: vec![],
        ..Config::default()
    };

    config.resolve_env_variables()?;

    assert_eq!(config.redis.password.as_deref(), Some("resolved-secret"));
    assert_eq!(config.redis.username.as_deref(), Some("resolved-acl-user"));

    test_env::remove("TEST_REDIS_PASSWORD");
    test_env::remove("TEST_REDIS_USERNAME");

    Ok(())
}

// 部署实测（2026）：`homeserver.yaml` 里
// `signing_key_master_key: "${FEDERATION_MASTER_KEY:-}"` 在变量未设置时
// 解析为**空字符串**，而不是"未配置"。`Some("")` 会让密钥管理器走加密分支，
// 用空的 HKDF 输入派生 AES 密钥（HKDF 接受空 IKM），于是联邦签名私钥以 `enc:`
// 前缀入库 —— 看似加密、实则任何拿到库导出的人都能解开，同时 fail-closed
// 分支（`None` → 拒绝持久化）永远走不到。归一化必须发生在配置边界这一处。
#[test]
fn test_resolve_env_variables_normalizes_blank_signing_key_master_key() -> Result<(), String> {
    for blank in ["", "   ", "\n"] {
        let mut config = Config::default();
        config.federation.signing_key_master_key = Some(blank.to_string());

        config.resolve_env_variables()?;

        assert_eq!(
            config.federation.signing_key_master_key, None,
            "blank master key {blank:?} must normalize to None, not stay as an empty string"
        );
    }
    Ok(())
}

#[test]
fn test_resolve_env_variables_keeps_a_real_signing_key_master_key() -> Result<(), String> {
    let mut config = Config::default();
    let key = "k".repeat(crate::key_encryption::MIN_MASTER_KEY_LEN);
    config.federation.signing_key_master_key = Some(key.clone());

    config.resolve_env_variables()?;

    assert_eq!(config.federation.signing_key_master_key.as_deref(), Some(key.as_str()));
    Ok(())
}

#[test]
fn test_circuit_breaker_config_defaults() {
    let config = CircuitBreakerConfig::default();

    assert!(config.enabled);
    assert_eq!(config.failure_threshold, 10);
    assert_eq!(config.success_threshold, 3);
    assert_eq!(config.timeout_ms, 60_000);
    assert_eq!(config.window_size_seconds, 120);
}

#[test]
fn test_logging_config_with_file() {
    let config = LoggingConfig {
        level: "debug".to_string(),
        format: "text".to_string(),
        log_file: Some("/var/log/synapse.log".to_string()),
    };

    assert_eq!(config.level, "debug");
    assert!(config.log_file.is_some());
}

#[test]
fn test_federation_config_defaults() {
    let config = FederationConfig {
        enabled: true,
        allow_ingress: true,
        server_name: "federation.example.com".to_string(),
        federation_port: 8448,
        max_transaction_payload: 100000,
        ca_file: Some(PathBuf::from("/etc/synapse/ca.crt")),
        client_ca_file: None,
        signing_key: None,
        key_id: None,
        trusted_key_servers: vec![],
        signature_cache_ttl: 3600,
        key_cache_ttl: 3600,
        key_rotation_grace_period_ms: 60_0000,
        key_fetch_max_concurrency: 32,
        key_fetch_timeout_ms: 5000,
        allow_http_key_fetch: false,
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
        signing_key_master_key: None,
        ..Default::default()
    };

    assert!(config.enabled);
    assert!(config.allow_ingress);
    assert!(config.ca_file.is_some());
}

#[test]
fn test_security_config_defaults() {
    let config = SecurityConfig {
        secret: "very_secure_secret_key".to_string(),
        expiry_time: 3600,
        refresh_token_expiry: 604800,
        argon2_m_cost: 4096,
        argon2_t_cost: 3,
        argon2_p_cost: 1,
        allow_legacy_hashes: false,
        login_failure_lockout_threshold: 5,
        login_lockout_duration_seconds: 900,
        login_lockout_fail_open_on_redis_error: true,
        login_lockout_max_ip_attempts: 30,
        login_lockout_ip_allowlist: Vec::new(),
        admin_mfa_required: false,
        admin_mfa_shared_secret: String::new(),
        admin_mfa_allowed_drift_steps: default_admin_mfa_allowed_drift_steps(),
        admin_rbac_enabled: default_admin_rbac_enabled(),
        ui_auth_session_timeout: default_ui_auth_session_timeout(),
        csrf_secret: String::new(),
        ..Default::default()
    };

    assert!(config.secret.len() > 16);
    assert_eq!(config.argon2_m_cost, 4096);
}

#[test]
fn test_refresh_token_lifetime_seconds_uses_security_expiry() {
    let config = Config {
        security: SecurityConfig { refresh_token_expiry: 2_592_000, ..Default::default() },
        ..Default::default()
    };
    assert_eq!(config.refresh_token_lifetime_seconds(), 2_592_000);
}

#[test]
fn test_refresh_token_lifetime_seconds_falls_back_to_default_when_zero() {
    // SecurityConfig::default() 的 refresh_token_expiry 为 0，应回退到
    // DEFAULT_REFRESH_TOKEN_EXPIRY_SECS（7 天）。
    let config = Config::default();
    assert_eq!(config.refresh_token_lifetime_seconds(), crate::DEFAULT_REFRESH_TOKEN_EXPIRY_SECS);
}

fn config_with_token_lifetimes(server_lifetime: i64, security_lifetime: i64) -> Config {
    Config {
        server: ServerConfig { expire_access_token_lifetime: server_lifetime, ..Default::default() },
        security: SecurityConfig { expiry_time: security_lifetime, ..Default::default() },
        ..Default::default()
    }
}

#[test]
fn test_access_token_lifetime_prefers_the_server_level_field() {
    // `server.expire_access_token_lifetime` 是 Synapse 风格的显式配置；只要它不是
    // 历史默认值 3600，就必须压过 `security.expiry_time`。
    assert_eq!(config_with_token_lifetimes(86_400, 3_600).access_token_lifetime_seconds(), 86_400);
}

#[test]
fn test_access_token_lifetime_keeps_the_server_value_when_both_are_set_and_differ() {
    // 两个字段都被显式改过且互相矛盾时，仍以 server 级为准（只打 WARN，不阻塞启动）。
    // 这条分支此前零覆盖：矛盾配置会静默地走另一支。
    assert_eq!(config_with_token_lifetimes(86_400, 7_200).access_token_lifetime_seconds(), 86_400);
}

#[test]
fn test_access_token_lifetime_falls_back_to_security_then_to_an_hour() {
    // server 级仍是历史默认 3600 ⇒ 让 security 级生效。
    assert_eq!(config_with_token_lifetimes(3_600, 7_200).access_token_lifetime_seconds(), 7_200);
    // 两者都未配置 ⇒ 一小时的最终兜底。
    assert_eq!(config_with_token_lifetimes(0, 0).access_token_lifetime_seconds(), 3_600);
}

/// `Config` 的必填字段（`server`/`database`/`redis`/…）无默认值，无法用最小 JSON
/// 直接反序列化整个 `Config`，因此这里用一个镜像了 `Config::redaction_allowed_period`
/// 完全相同 serde 属性的探针结构体来验证该字段的解析行为。
#[derive(serde::Deserialize)]
struct RedactionPeriodProbe {
    #[serde(default, deserialize_with = "super::deserialize_optional_duration_ms")]
    redaction_allowed_period: Option<i64>,
}

fn parse_redaction_period(value: serde_json::Value) -> Result<Option<i64>, serde_json::Error> {
    serde_json::from_value::<RedactionPeriodProbe>(json!({ "redaction_allowed_period": value }))
        .map(|probe| probe.redaction_allowed_period)
}

#[test]
fn test_redaction_allowed_period_accepts_duration_strings() {
    // Synapse 的 redaction_allowed_period 用 duration 字符串（如 "7d"），本项目
    // 内部统一存毫秒；此处锁定 s/m/h/d/w 各单位的换算。
    for (raw, expected) in [
        (json!("7d"), 604_800_000_i64),
        (json!("1h"), 3_600_000),
        (json!("30m"), 1_800_000),
        (json!("90s"), 90_000),
        (json!("2w"), 1_209_600_000),
    ] {
        let parsed = parse_redaction_period(raw.clone()).unwrap();
        assert_eq!(parsed, Some(expected), "raw={raw}");
    }
}

#[test]
fn test_redaction_allowed_period_accepts_integer_millis() {
    let parsed = parse_redaction_period(json!(3_600_000)).unwrap();
    assert_eq!(parsed, Some(3_600_000));
}

#[test]
fn test_redaction_allowed_period_defaults_to_none() {
    // 缺省与显式 null 都应关闭时限。
    let absent: RedactionPeriodProbe = serde_json::from_value(json!({})).unwrap();
    assert_eq!(absent.redaction_allowed_period, None);
    let explicit = parse_redaction_period(serde_json::Value::Null).unwrap();
    assert_eq!(explicit, None);
}

#[test]
fn test_redaction_allowed_period_rejects_invalid_values() {
    // 非法 duration 字符串与浮点数都应硬失败（静默降级会掩盖写错的配置）。
    let err = parse_redaction_period(json!("never")).unwrap_err();
    assert!(err.to_string().contains("redaction_allowed_period"), "实际：{err}");
    let err = parse_redaction_period(json!(1.5)).unwrap_err();
    assert!(err.to_string().contains("redaction_allowed_period"), "实际：{err}");
}

#[test]
fn test_redaction_allowed_period_rejects_non_scalar_shapes() {
    // 布尔/数组/对象既不是数字也不是 duration 字符串。此前只测了浮点，这一支的
    // 报错文本从未被断言过：配置写成 `redaction_allowed_period: true` 会不会被
    // 当成 0 静默放行，取决于这条分支。
    for raw in [json!(true), json!([1]), json!({"ms": 1})] {
        let err = parse_redaction_period(raw.clone()).unwrap_err();
        assert!(
            err.to_string().contains("duration string or an integer number of milliseconds"),
            "raw={raw} 实际：{err}"
        );
    }
}

#[test]
fn test_parse_duration_ms_units() {
    assert_eq!(parse_duration_ms("500"), Some(500));
    assert_eq!(parse_duration_ms("2s"), Some(2_000));
    assert_eq!(parse_duration_ms("2m"), Some(120_000));
    assert_eq!(parse_duration_ms("2h"), Some(7_200_000));
    assert_eq!(parse_duration_ms("2d"), Some(172_800_000));
    assert_eq!(parse_duration_ms("2w"), Some(1_209_600_000));
    assert_eq!(parse_duration_ms(""), None);
    assert_eq!(parse_duration_ms("abc"), None);
}

#[test]
fn test_third_party_rules_parses_rules_and_their_blocked_types() {
    let config: ThirdPartyRulesConfig = serde_json::from_value(json!({
        "rules": [
            { "name": "block_redactions", "blocked_event_types": ["m.room.redaction"] },
        ],
    }))
    .unwrap();

    assert_eq!(config.rules.len(), 1);
    assert_eq!(config.rules[0].name, "block_redactions");
    assert_eq!(config.rules[0].blocked_event_types, vec!["m.room.redaction".to_string()]);
}

/// `server.request_timeout_secs` / `long_poll_request_timeout_secs` 是**可选**配置：
/// 缺省即交给消费方（`synapse_rust::server::build_router`）用
/// `DEFAULT_REQUEST_TIMEOUT_SECS` / `DEFAULT_LONG_POLL_REQUEST_TIMEOUT_SECS` 兜底。
///
/// 这两条取值必须能区分「没写」与「写了 0」，因此类型是 `Option<u64>` 而不是
/// 裸 `u64`：`ServerConfig` 同时 derive `Default`，裸类型会让
/// `Default::default()` 得到 0、serde 得到 30，同一份配置两个答案。
#[test]
fn test_request_timeouts_are_optional_and_round_trip() {
    use super::server::{DEFAULT_LONG_POLL_REQUEST_TIMEOUT_SECS, DEFAULT_REQUEST_TIMEOUT_SECS};

    // `ServerConfig` 有几个字段没有 serde 缺省（name /
    // enable_registration* / background_tasks_interval / expire_access_token* /
    // refresh_token_sliding_window_size），最小可用 JSON 必须把它们都写上。
    let minimal = json!({
        "name": "example.com",
        "enable_registration": true,
        "enable_registration_captcha": false,
        "background_tasks_interval": 300,
        "expire_access_token": true,
        "expire_access_token_lifetime": 86_400,
        "refresh_token_sliding_window_size": 3_600,
    });
    let absent: ServerConfig = serde_json::from_value(minimal.clone()).unwrap();
    assert_eq!(absent.request_timeout_secs, None, "缺省必须是 None（由装配层兜底），不是 0");
    assert_eq!(absent.long_poll_request_timeout_secs, None);
    assert_eq!(ServerConfig::default().request_timeout_secs, None, "`Default` 与 serde 必须同义");

    let mut with_timeouts = minimal;
    with_timeouts["request_timeout_secs"] = json!(5);
    with_timeouts["long_poll_request_timeout_secs"] = json!(120);
    let explicit: ServerConfig = serde_json::from_value(with_timeouts).unwrap();
    assert_eq!(explicit.request_timeout_secs, Some(5));
    assert_eq!(explicit.long_poll_request_timeout_secs, Some(120));

    // 文档里的缺省值就是装配层的兜底值。
    assert_eq!(DEFAULT_REQUEST_TIMEOUT_SECS, 30);
    assert_eq!(DEFAULT_LONG_POLL_REQUEST_TIMEOUT_SECS, 90);
}

#[test]
fn test_third_party_rules_defaults_to_disabled() {
    // 缺省应关闭准入钩子——空规则集，且不因缺字段而报错。
    let absent: ThirdPartyRulesConfig = serde_json::from_value(json!({})).unwrap();
    assert!(absent.rules.is_empty());

    let default = ThirdPartyRulesConfig::default();
    assert!(default.rules.is_empty());
}

#[test]
fn test_third_party_rules_rule_without_blocked_types_is_allowed() {
    // `blocked_event_types` 缺省为空——该规则不阻断任何事件类型。
    let config: ThirdPartyRulesConfig = serde_json::from_value(json!({ "rules": [{ "name": "noop" }] })).unwrap();
    assert_eq!(config.rules.len(), 1);
    assert!(config.rules[0].blocked_event_types.is_empty());
    // 未声明改写——该规则只做拒绝判定。
    assert!(config.rules[0].modification.is_none());
}

#[test]
fn test_third_party_rules_parses_modification() {
    // 规则可携带改写指令，对齐上游 `check_event_allowed` 返回 `(True, dict)`。
    let config: ThirdPartyRulesConfig = serde_json::from_value(json!({
        "rules": [
            {
                "name": "annotate_messages",
                "modification": {
                    "event_types": ["m.room.message"],
                    "content": { "msgtype": "m.notice", "body": "[rewritten by policy]" },
                },
            },
        ],
    }))
    .unwrap();

    let modification = config.rules[0].modification.as_ref().expect("modification should be present");
    assert_eq!(modification.event_types, vec!["m.room.message".to_string()]);
    assert_eq!(modification.content, json!({ "msgtype": "m.notice", "body": "[rewritten by policy]" }));
}
