//! Configuration tests — unit tests for config structs, URL builders,
//! environment variable resolution, and defaults.

#[cfg(test)]
#[allow(clippy::module_inception)]
mod tests {
    use crate::common::config::*;
    use crate::common::config::{
        security::{
            default_admin_mfa_allowed_drift_steps, default_admin_rbac_enabled, default_ui_auth_session_timeout,
        },
        server::default_dehydrated_device_cleanup_interval_secs,
    };
    use std::path::PathBuf;

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
                remote_media_lifetime: 2592000,
                local_media_lifetime: 0,
                enable_registration: true,
                enable_registration_captcha: false,
                background_tasks_interval: 60,
                dehydrated_device_cleanup_interval_secs: 3600,
                expire_access_token: true,
                expire_access_token_lifetime: 3600,
                refresh_token_lifetime: 604800,
                refresh_token_sliding_window_size: 1000,
                session_duration: 86400,
                warmup_pool: true,
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
                pool_size: 10,
                max_size: 20,
                min_idle: Some(5),
                connection_timeout: 30,
            },
            redis: RedisConfig {
                host: "localhost".to_string(),
                port: 6379,
                password: None,
                key_prefix: "test:".to_string(),
                pool_size: 10,
                enabled: true,
                connection_timeout_ms: 500,
                command_timeout_ms: 500,
                circuit_breaker: CircuitBreakerConfig::default(),
            },
            logging: LoggingConfig {
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
                signing_key: Some("test_signing_key".to_string()),
                key_id: Some("ed25519:test_key".to_string()),
                trusted_key_servers: vec![],
                key_refresh_interval: 86400,
                suppress_key_server_warning: false,
                signature_cache_ttl: 3600,
                key_cache_ttl: 3600,
                key_rotation_grace_period_ms: 60_0000,
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
                admin_mfa_required: false,
                admin_mfa_shared_secret: String::new(),
                admin_mfa_allowed_drift_steps: default_admin_mfa_allowed_drift_steps(),
                admin_rbac_enabled: default_admin_rbac_enabled(),
                ui_auth_session_timeout: default_ui_auth_session_timeout(),
                csrf_secret: String::new(),
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
            livekit: LivekitConfig::default(),
            voip: VoipConfig::default(),
            push: PushConfig::default(),
            url_preview: UrlPreviewConfig::default(),
            oidc: OidcConfig::default(),
            builtin_oidc: BuiltinOidcConfig::default(),
            saml: SamlConfig::default(),
            retention: RetentionConfig::default(),
            telemetry: synapse_common::telemetry_config::OpenTelemetryConfig::default(),
            prometheus: synapse_common::telemetry_config::PrometheusConfig::default(),
            performance: PerformanceConfig::default(),
            experimental: ExperimentalConfig::default(),
            identity: IdentityConfig::default(),
            translate: TranslateConfig::default(),
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
                remote_media_lifetime: 2592000,
                local_media_lifetime: 0,
                enable_registration: true,
                enable_registration_captcha: false,
                background_tasks_interval: 60,
                dehydrated_device_cleanup_interval_secs: 3600,
                expire_access_token: true,
                expire_access_token_lifetime: 3600,
                refresh_token_lifetime: 604800,
                refresh_token_sliding_window_size: 1000,
                session_duration: 86400,
                warmup_pool: true,
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
                pool_size: 10,
                max_size: 20,
                min_idle: Some(5),
                connection_timeout: 30,
            },
            redis: RedisConfig {
                host: "redis.example.com".to_string(),
                port: 6380,
                password: Some("secret".to_string()),
                key_prefix: "prod:".to_string(),
                pool_size: 20,
                enabled: true,
                connection_timeout_ms: 500,
                command_timeout_ms: 500,
                circuit_breaker: CircuitBreakerConfig::default(),
            },
            logging: LoggingConfig {
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
                signing_key: Some("test_signing_key".to_string()),
                key_id: Some("ed25519:test_key".to_string()),
                trusted_key_servers: vec![],
                key_refresh_interval: 86400,
                suppress_key_server_warning: false,
                signature_cache_ttl: 3600,
                key_cache_ttl: 3600,
                key_rotation_grace_period_ms: 60_0000,
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
                admin_mfa_required: false,
                admin_mfa_shared_secret: String::new(),
                admin_mfa_allowed_drift_steps: default_admin_mfa_allowed_drift_steps(),
                admin_rbac_enabled: default_admin_rbac_enabled(),
                ui_auth_session_timeout: default_ui_auth_session_timeout(),
                csrf_secret: String::new(),
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
            livekit: LivekitConfig::default(),
            voip: VoipConfig::default(),
            push: PushConfig::default(),
            url_preview: UrlPreviewConfig::default(),
            oidc: OidcConfig::default(),
            saml: SamlConfig::default(),
            retention: RetentionConfig::default(),
            telemetry: synapse_common::telemetry_config::OpenTelemetryConfig::default(),
            prometheus: synapse_common::telemetry_config::PrometheusConfig::default(),
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
            form_secret: None,
            server_name: None,
            suppress_key_server_warning: false,
            serve_server_wellknown: false,
            soft_file_limit: 0,
            user_agent_suffix: None,
            web_client_location: None,
            registration_shared_secret: Some("secret".to_string()),
            admin_contact: Some("admin@example.com".to_string()),
            max_upload_size: 50000000,
            max_image_resolution: 8000000,
            remote_media_lifetime: 2592000,
            local_media_lifetime: 0,
            enable_registration: true,
            enable_registration_captcha: true,
            background_tasks_interval: 30,
            dehydrated_device_cleanup_interval_secs: 3600,
            expire_access_token: true,
            expire_access_token_lifetime: 86400,
            refresh_token_lifetime: 2592000,
            refresh_token_sliding_window_size: 5000,
            session_duration: 3600,
            warmup_pool: true,
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
            pool_size: 10,
            max_size: 20,
            min_idle: None,
            connection_timeout: 60,
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
        unsafe {
            std::env::set_var("TEST_REDIS_PASSWORD", "resolved-secret");
        }

        let mut config = Config {
            server: ServerConfig {
                name: "test".to_string(),
                host: "127.0.0.1".to_string(),
                port: 8000,
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
                remote_media_lifetime: 2592000,
                local_media_lifetime: 0,
                enable_registration: true,
                enable_registration_captcha: false,
                background_tasks_interval: 60,
                dehydrated_device_cleanup_interval_secs: 3600,
                expire_access_token: true,
                expire_access_token_lifetime: 3600,
                refresh_token_lifetime: 604800,
                refresh_token_sliding_window_size: 1000,
                session_duration: 86400,
                warmup_pool: true,
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
                pool_size: 10,
                max_size: 20,
                min_idle: Some(5),
                connection_timeout: 30,
            },
            redis: RedisConfig {
                host: "localhost".to_string(),
                port: 6379,
                password: Some("${TEST_REDIS_PASSWORD:?missing}".to_string()),
                key_prefix: "test:".to_string(),
                pool_size: 10,
                enabled: true,
                connection_timeout_ms: 500,
                command_timeout_ms: 500,
                circuit_breaker: CircuitBreakerConfig::default(),
            },
            logging: LoggingConfig {
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
                signing_key: Some("test_signing_key".to_string()),
                key_id: Some("ed25519:test_key".to_string()),
                trusted_key_servers: vec![],
                key_refresh_interval: 86400,
                suppress_key_server_warning: false,
                signature_cache_ttl: 3600,
                key_cache_ttl: 3600,
                key_rotation_grace_period_ms: 60_0000,
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
                admin_mfa_required: false,
                admin_mfa_shared_secret: String::new(),
                admin_mfa_allowed_drift_steps: default_admin_mfa_allowed_drift_steps(),
                admin_rbac_enabled: default_admin_rbac_enabled(),
                ui_auth_session_timeout: default_ui_auth_session_timeout(),
                csrf_secret: String::new(),
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
            livekit: LivekitConfig::default(),
            voip: VoipConfig::default(),
            push: PushConfig::default(),
            url_preview: UrlPreviewConfig::default(),
            oidc: OidcConfig::default(),
            builtin_oidc: BuiltinOidcConfig::default(),
            saml: SamlConfig::default(),
            retention: RetentionConfig::default(),
            telemetry: synapse_common::telemetry_config::OpenTelemetryConfig::default(),
            prometheus: synapse_common::telemetry_config::PrometheusConfig::default(),
            performance: PerformanceConfig::default(),
            experimental: ExperimentalConfig::default(),
            identity: IdentityConfig::default(),
            translate: TranslateConfig::default(),
        };

        config.resolve_env_variables()?;

        assert_eq!(config.redis.password.as_deref(), Some("resolved-secret"));

        unsafe {
            std::env::remove_var("TEST_REDIS_PASSWORD");
        }

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
            log_dir: Some("/var/log".to_string()),
        };

        assert_eq!(config.level, "debug");
        assert!(config.log_file.is_some());
        assert!(config.log_dir.is_some());
    }

    #[test]
    fn test_federation_config_defaults() {
        let config = FederationConfig {
            enabled: true,
            allow_ingress: true,
            server_name: "federation.example.com".to_string(),
            federation_port: 8448,
            connection_pool_size: 50,
            max_transaction_payload: 100000,
            ca_file: Some(PathBuf::from("/etc/synapse/ca.crt")),
            client_ca_file: None,
            signing_key: None,
            key_id: None,
            trusted_key_servers: vec![],
            key_refresh_interval: 86400,
            suppress_key_server_warning: false,
            signature_cache_ttl: 3600,
            key_cache_ttl: 3600,
            key_rotation_grace_period_ms: 60_0000,
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
            admin_mfa_required: false,
            admin_mfa_shared_secret: String::new(),
            admin_mfa_allowed_drift_steps: default_admin_mfa_allowed_drift_steps(),
            admin_rbac_enabled: default_admin_rbac_enabled(),
            ui_auth_session_timeout: default_ui_auth_session_timeout(),
            csrf_secret: String::new(),
        };

        assert!(config.secret.len() > 16);
        assert_eq!(config.argon2_m_cost, 4096);
    }

    #[test]
    fn test_resolve_env_variables_resolves_worker_replication_config() -> Result<(), String> {
        unsafe {
            std::env::set_var("TEST_WORKER_INSTANCE", "sync_worker");
            std::env::set_var("TEST_WORKER_HOST", "sync-worker");
            std::env::set_var("TEST_REPLICATION_SECRET", "worker_replication_secret_2026");
        }

        let mut config = Config::default();
        config.worker.enabled = true;
        config.worker.instance_name = "${TEST_WORKER_INSTANCE:-master}".to_string();
        config.worker.instance_map.insert(
            "sync_worker".to_string(),
            InstanceLocationConfig { host: "${TEST_WORKER_HOST:?missing}".to_string(), port: 8008, tls: false },
        );
        config.worker.replication.enabled = true;
        config.worker.replication.server_name = "${TEST_WORKER_HOST:-matrix.test}".to_string();
        config.worker.replication.http.enabled = true;
        config.worker.replication.http.host = "${TEST_WORKER_HOST:-0.0.0.0}".to_string();
        config.worker.replication.http.secret = Some("${TEST_REPLICATION_SECRET:?missing}".to_string());
        config.worker.replication.http.secret_path =
            Some("${TEST_WORKER_SECRET_PATH:-/tmp/replication.secret}".to_string());

        config.resolve_env_variables()?;

        assert_eq!(config.worker.instance_name, "sync_worker");
        assert_eq!(
            config.worker.instance_map.get("sync_worker").map(|instance| instance.host.as_str()),
            Some("sync-worker")
        );
        assert_eq!(config.worker.replication.server_name, "sync-worker");
        assert_eq!(config.worker.replication.http.host, "sync-worker");
        assert_eq!(config.worker.replication.http.secret.as_deref(), Some("worker_replication_secret_2026"));
        assert_eq!(config.worker.replication.http.secret_path.as_deref(), Some("/tmp/replication.secret"));

        unsafe {
            std::env::remove_var("TEST_WORKER_INSTANCE");
            std::env::remove_var("TEST_WORKER_HOST");
            std::env::remove_var("TEST_REPLICATION_SECRET");
        }

        Ok(())
    }
}
