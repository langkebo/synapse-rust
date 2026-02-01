use config::Config as ConfigBuilder;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub redis: RedisConfig,
    pub logging: LoggingConfig,
    pub federation: FederationConfig,
    pub security: SecurityConfig,
    pub search: SearchConfig,
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchConfig {
    pub elasticsearch_url: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitConfig {
    #[serde(default = "default_rate_limit_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub default: RateLimitRule,
    #[serde(default)]
    pub endpoints: Vec<RateLimitEndpointRule>,
    #[serde(default)]
    pub ip_header_priority: Vec<String>,
    #[serde(default)]
    pub include_headers: bool,
    #[serde(default)]
    pub exempt_paths: Vec<String>,
    #[serde(default)]
    pub exempt_path_prefixes: Vec<String>,
    #[serde(default)]
    pub endpoint_aliases: HashMap<String, String>,
}

fn default_rate_limit_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitRule {
    #[serde(default = "default_rate_limit_per_second")]
    pub per_second: u32,
    #[serde(default = "default_rate_limit_burst_size")]
    pub burst_size: u32,
}

fn default_rate_limit_per_second() -> u32 {
    10
}

fn default_rate_limit_burst_size() -> u32 {
    20
}

impl Default for RateLimitRule {
    fn default() -> Self {
        Self {
            per_second: default_rate_limit_per_second(),
            burst_size: default_rate_limit_burst_size(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitEndpointRule {
    pub path: String,
    #[serde(default)]
    pub match_type: RateLimitMatchType,
    pub rule: RateLimitRule,
}

#[derive(Debug, Clone, Copy, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RateLimitMatchType {
    #[default]
    Exact,
    Prefix,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: default_rate_limit_enabled(),
            default: RateLimitRule::default(),
            endpoints: Vec::new(),
            ip_header_priority: vec![
                "x-forwarded-for".to_string(),
                "x-real-ip".to_string(),
                "forwarded".to_string(),
            ],
            include_headers: true,
            exempt_paths: vec!["/".to_string(), "/_matrix/client/versions".to_string()],
            exempt_path_prefixes: Vec::new(),
            endpoint_aliases: HashMap::new(),
        }
    }
}

pub struct ConfigManager {
    config: Arc<RwLock<Config>>,
}

impl ConfigManager {
    pub fn new(config: Config) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
        }
    }

    pub fn get_server_name(&self) -> String {
        let config = self.config.read().unwrap();
        config.server.name.clone()
    }

    pub fn get_server_host(&self) -> String {
        let config = self.config.read().unwrap();
        config.server.host.clone()
    }

    pub fn get_server_port(&self) -> u16 {
        let config = self.config.read().unwrap();
        config.server.port
    }

    pub fn get_database_url(&self) -> String {
        let config = self.config.read().unwrap();
        format!(
            "postgres://{}:{}@{}:{}/{}",
            config.database.username,
            config.database.password,
            config.database.host,
            config.database.port,
            config.database.name
        )
    }

    pub fn get_redis_url(&self) -> String {
        let config = self.config.read().unwrap();
        format!("redis://{}:{}", config.redis.host, config.redis.port)
    }

    pub fn get_config(&self) -> Config {
        let config = self.config.read().unwrap();
        config.clone()
    }

    pub fn update_config<F>(&self, f: F) -> Result<(), Box<dyn std::error::Error>>
    where
        F: FnOnce(&mut Config),
    {
        let mut config = self.config.write().unwrap();
        f(&mut config);
        Ok(())
    }
}

impl Clone for ConfigManager {
    fn clone(&self) -> Self {
        Self {
            config: Arc::clone(&self.config),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub registration_shared_secret: Option<String>,
    pub admin_contact: Option<String>,
    pub max_upload_size: u64,
    pub max_image_resolution: u32,
    pub enable_registration: bool,
    pub enable_registration_captcha: bool,
    pub background_tasks_interval: u64,
    pub expire_access_token: bool,
    pub expire_access_token_lifetime: i64,
    pub refresh_token_lifetime: i64,
    pub refresh_token_sliding_window_size: i64,
    pub session_duration: i64,
    #[serde(default = "default_warmup_pool")]
    pub warmup_pool: bool,
}

fn default_warmup_pool() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub name: String,
    pub pool_size: u32,
    pub max_size: u32,
    pub min_idle: Option<u32>,
    pub connection_timeout: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RedisConfig {
    pub host: String,
    pub port: u16,
    pub key_prefix: String,
    pub pool_size: u32,
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
    pub format: String,
    pub log_file: Option<String>,
    pub log_dir: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FederationConfig {
    pub enabled: bool,
    pub allow_ingress: bool,
    pub server_name: String,
    pub federation_port: u16,
    pub connection_pool_size: u32,
    pub max_transaction_payload: u64,
    pub ca_file: Option<PathBuf>,
    pub client_ca_file: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SecurityConfig {
    pub secret: String,
    pub expiry_time: i64,
    pub refresh_token_expiry: i64,
    pub bcrypt_rounds: u32,
}

impl Config {
    pub async fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let config_path =
            std::env::var("SYNAPSE_CONFIG_PATH").unwrap_or_else(|_| "homeserver.yaml".to_string());

        let config = ConfigBuilder::builder()
            .add_source(config::File::with_name(&config_path))
            .add_source(config::Environment::with_prefix("SYNAPSE"))
            .build()?;

        let config_values: Config = config.try_deserialize()?;
        Ok(config_values)
    }

    pub fn database_url(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}/{}",
            self.database.username,
            self.database.password,
            self.database.host,
            self.database.port,
            self.database.name
        )
    }

    pub fn redis_url(&self) -> String {
        format!("redis://{}:{}", self.redis.host, self.redis.port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_database_url() {
        let config = Config {
            server: ServerConfig {
                name: "test".to_string(),
                host: "127.0.0.1".to_string(),
                port: 8000,
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
                key_prefix: "test:".to_string(),
                pool_size: 10,
                enabled: true,
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
            },
            security: SecurityConfig {
                secret: "test_secret".to_string(),
                expiry_time: 3600,
                refresh_token_expiry: 604800,
                bcrypt_rounds: 12,
            },
            search: SearchConfig {
                elasticsearch_url: "http://localhost:9200".to_string(),
                enabled: false,
            },
            rate_limit: RateLimitConfig::default(),
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
                key_prefix: "prod:".to_string(),
                pool_size: 20,
                enabled: true,
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
            },
            security: SecurityConfig {
                secret: "test_secret".to_string(),
                expiry_time: 3600,
                refresh_token_expiry: 604800,
                bcrypt_rounds: 12,
            },
            search: SearchConfig {
                elasticsearch_url: "http://localhost:9200".to_string(),
                enabled: false,
            },
            rate_limit: RateLimitConfig::default(),
        };

        let url = config.redis_url();
        assert_eq!(url, "redis://redis.example.com:6380");
    }

    #[test]
    fn test_server_config_defaults() {
        let config = ServerConfig {
            name: "test".to_string(),
            host: "0.0.0.0".to_string(),
            port: 8080,
            registration_shared_secret: Some("secret".to_string()),
            admin_contact: Some("admin@example.com".to_string()),
            max_upload_size: 50000000,
            max_image_resolution: 8000000,
            enable_registration: true,
            enable_registration_captcha: true,
            background_tasks_interval: 30,
            expire_access_token: true,
            expire_access_token_lifetime: 86400,
            refresh_token_lifetime: 2592000,
            refresh_token_sliding_window_size: 5000,
            session_duration: 3600,
            warmup_pool: true,
        };

        assert_eq!(config.name, "test");
        assert_eq!(config.port, 8080);
        assert!(config.enable_registration);
        assert!(config.registration_shared_secret.is_some());
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
            key_prefix: "synapse:".to_string(),
            pool_size: 16,
            enabled: true,
        };

        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 6379);
        assert!(config.enabled);
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
            bcrypt_rounds: 12,
        };

        assert!(config.secret.len() > 16);
        assert_eq!(config.bcrypt_rounds, 12);
    }
}
