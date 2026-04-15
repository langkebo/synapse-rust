use base64::Engine;
use chrono::{Duration, Utc};
use ed25519_dalek::Verifier;
use serde_json::json;
use sqlx::{Pool, Postgres, Row};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration as TokioDuration};

const KEY_ROTATION_INTERVAL_DAYS: i64 = 7;
const KEY_ROTATION_THRESHOLD_DAYS: i64 = 1;
const KEY_GRACE_PERIOD_MINUTES: i64 = 5;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SigningKey {
    pub server_name: String,
    pub key_id: String,
    pub secret_key: String,
    pub public_key: String,
    pub created_ts: i64,
    pub expires_at: i64,
    pub key_json: serde_json::Value,
    pub ts_added_ms: i64,
    pub ts_valid_until_ms: i64,
}

#[derive(Debug, Clone)]
pub struct KeyRotationManager {
    pool: Arc<Pool<Postgres>>,
    memory_cache: Arc<RwLock<HashMap<String, (String, i64)>>>,
    current_key: Arc<RwLock<Option<SigningKey>>>,
    historical_keys: Arc<RwLock<HashMap<String, SigningKey>>>,
    server_name: String,
    rotation_enabled: Arc<RwLock<bool>>,
    signing_keys_table_ready: Arc<AtomicBool>,
}

impl KeyRotationManager {
    pub fn new(pool: &Arc<Pool<Postgres>>, server_name: &str) -> Self {
        Self {
            pool: pool.clone(),
            memory_cache: Arc::new(RwLock::new(HashMap::new())),
            current_key: Arc::new(RwLock::new(None)),
            historical_keys: Arc::new(RwLock::new(HashMap::new())),
            server_name: server_name.to_string(),
            rotation_enabled: Arc::new(RwLock::new(true)),
            signing_keys_table_ready: Arc::new(AtomicBool::new(false)),
        }
    }

    async fn ensure_signing_keys_table(&self) -> Result<(), anyhow::Error> {
        if self.signing_keys_table_ready.load(Ordering::Relaxed) {
            return Ok(());
        }

        let table_exists: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM information_schema.tables
                WHERE table_schema = 'public'
                  AND table_name = 'federation_signing_keys'
            )
            "#,
        )
        .fetch_one(&*self.pool)
        .await?;

        if !table_exists {
            sqlx::query(
                r#"
                CREATE TABLE federation_signing_keys (
                    server_name TEXT NOT NULL,
                    key_id TEXT NOT NULL,
                    secret_key TEXT NOT NULL,
                    public_key TEXT NOT NULL,
                    created_ts BIGINT NOT NULL,
                    expires_at BIGINT NOT NULL,
                    key_json JSONB NOT NULL DEFAULT '{}'::jsonb,
                    ts_added_ms BIGINT NOT NULL,
                    ts_valid_until_ms BIGINT NOT NULL,
                    PRIMARY KEY (server_name, key_id)
                )
                "#,
            )
            .execute(&*self.pool)
            .await?;
        }

        let server_created_index_exists: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM pg_indexes
                WHERE schemaname = 'public'
                  AND indexname = 'idx_federation_signing_keys_server_created'
            )
            "#,
        )
        .fetch_one(&*self.pool)
        .await?;

        if !server_created_index_exists {
            sqlx::query(
                r#"
                CREATE INDEX idx_federation_signing_keys_server_created
                ON federation_signing_keys(server_name, created_ts DESC)
                "#,
            )
            .execute(&*self.pool)
            .await?;
        }

        let key_id_index_exists: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM pg_indexes
                WHERE schemaname = 'public'
                  AND indexname = 'idx_federation_signing_keys_key_id'
            )
            "#,
        )
        .fetch_one(&*self.pool)
        .await?;

        if !key_id_index_exists {
            sqlx::query(
                r#"
                CREATE INDEX idx_federation_signing_keys_key_id
                ON federation_signing_keys(key_id)
                "#,
            )
            .execute(&*self.pool)
            .await?;
        }

        self.signing_keys_table_ready.store(true, Ordering::Relaxed);

        Ok(())
    }

    pub async fn start_auto_rotation(&self) {
        let manager = Arc::new(self.clone());

        let init_result = manager.load_or_create_key().await;
        if let Err(e) = init_result {
            tracing::error!("Failed to initialize key rotation: {}", e);
        }

        let mut interval = interval(TokioDuration::from_secs(3600));

        tokio::spawn(async move {
            loop {
                interval.tick().await;

                if *manager.rotation_enabled.read().await && manager.should_rotate_keys().await {
                    tracing::info!("Auto-rotating federation signing keys");
                    if let Err(e) = manager.rotate_keys(None).await {
                        tracing::error!("Failed to auto-rotate keys: {}", e);
                    }
                }
            }
        });

        tracing::info!("Key rotation scheduler started");
    }

    pub async fn load_or_create_key(&self) -> Result<(), anyhow::Error> {
        self.ensure_signing_keys_table().await?;

        let existing_key = sqlx::query_as::<_, SigningKey>(
            r#"
            SELECT
                server_name,
                key_id,
                secret_key,
                public_key,
                created_ts,
                expires_at,
                key_json,
                ts_added_ms,
                ts_valid_until_ms
            FROM federation_signing_keys
            WHERE server_name = $1 AND (expires_at = 0 OR expires_at > $2)
            ORDER BY created_ts DESC
            LIMIT 1
            "#,
        )
        .bind(&self.server_name)
        .bind(Utc::now().timestamp_millis())
        .fetch_optional(&*self.pool)
        .await;

        match existing_key {
            Ok(Some(key)) => {
                *self.current_key.write().await = Some(key);
                tracing::info!("Loaded existing signing key from database");
                return Ok(());
            }
            Ok(None) => {
                // No existing key, create new one
            }
            Err(e) => {
                return Err(e.into());
            }
        }

        let key_id = format!("ed25519:{}", Utc::now().timestamp());
        let secret_key =
            base64::engine::general_purpose::STANDARD_NO_PAD.encode(rand::random::<[u8; 32]>());

        match self.initialize(&secret_key, &key_id).await {
            Ok(_) => {
                tracing::info!("Created new signing key");
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    pub async fn initialize(&self, secret_key: &str, key_id: &str) -> Result<(), anyhow::Error> {
        self.ensure_signing_keys_table().await?;

        let created_ts = Utc::now().timestamp_millis();
        let expires_at =
            (Utc::now() + Duration::days(KEY_ROTATION_INTERVAL_DAYS)).timestamp_millis();

        let public_key = self.derive_public_key(secret_key).await?;

        let signing_key = SigningKey {
            server_name: self.server_name.clone(),
            key_id: key_id.to_string(),
            secret_key: secret_key.to_string(),
            public_key: public_key.clone(),
            created_ts,
            expires_at,
            key_json: json!({
                "secret_key": secret_key,
                "public_key": public_key
            }),
            ts_added_ms: created_ts,
            ts_valid_until_ms: expires_at,
        };

        *self.current_key.write().await = Some(signing_key.clone());

        let key_json = json!({
            "secret_key": signing_key.secret_key,
            "public_key": signing_key.public_key
        });

        sqlx::query(
            r#"
            INSERT INTO federation_signing_keys (server_name, key_id, secret_key, public_key, created_ts, expires_at, key_json, ts_added_ms, ts_valid_until_ms)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (server_name, key_id) DO UPDATE SET
                secret_key = EXCLUDED.secret_key,
                public_key = EXCLUDED.public_key,
                created_ts = EXCLUDED.created_ts,
                expires_at = EXCLUDED.expires_at,
                key_json = EXCLUDED.key_json,
                ts_added_ms = EXCLUDED.ts_added_ms,
                ts_valid_until_ms = EXCLUDED.ts_valid_until_ms
            "#,
        )
        .bind(&self.server_name)
        .bind(&signing_key.key_id)
        .bind(&signing_key.secret_key)
        .bind(&signing_key.public_key)
        .bind(signing_key.created_ts)
        .bind(signing_key.expires_at)
        .bind(key_json)
        .bind(signing_key.created_ts)
        .bind(signing_key.expires_at)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn should_rotate_keys(&self) -> bool {
        if let Some(key) = &*self.current_key.read().await {
            let now = Utc::now().timestamp_millis();
            let rotation_threshold = Duration::days(KEY_ROTATION_THRESHOLD_DAYS).num_milliseconds();
            key.expires_at.saturating_sub(now) <= rotation_threshold
        } else {
            true
        }
    }

    pub async fn rotate_keys(&self, new_key_id: Option<String>) -> Result<(), anyhow::Error> {
        let current = self.current_key.read().await;
        if let Some(key) = current.as_ref() {
            self.historical_keys
                .write()
                .await
                .insert(key.key_id.clone(), key.clone());
        }
        drop(current);

        let key_id = new_key_id.unwrap_or_else(|| format!("ed25519:{}", Utc::now().timestamp()));
        let (key_id, secret_key) = self.generate_new_key_pair(&key_id).await;

        self.initialize(&secret_key, &key_id).await?;

        if let Err(e) = self.broadcast_key_change_to_federation().await {
            tracing::warn!("Failed to broadcast key change: {}", e);
        }

        Ok(())
    }

    async fn generate_new_key_pair(&self, key_id: &str) -> (String, String) {
        let secret_key =
            base64::engine::general_purpose::STANDARD_NO_PAD.encode(rand::random::<[u8; 32]>());

        (key_id.to_string(), secret_key)
    }

    async fn derive_public_key(&self, secret_key: &str) -> Result<String, anyhow::Error> {
        let secret_bytes: [u8; 32] = base64::engine::general_purpose::STANDARD_NO_PAD
            .decode(secret_key)
            .map_err(|_| anyhow::anyhow!("Invalid secret key format"))?
            .try_into()
            .map_err(|_| anyhow::anyhow!("Secret key must be 32 bytes"))?;

        let signing_key = ed25519_dalek::SigningKey::from_bytes(&secret_bytes);
        let verifying_key = signing_key.verifying_key();

        Ok(base64::engine::general_purpose::STANDARD_NO_PAD.encode(verifying_key.as_bytes()))
    }

    pub async fn get_current_key(&self) -> Result<Option<SigningKey>, anyhow::Error> {
        Ok(self.current_key.read().await.clone())
    }

    pub async fn verify_with_key_rotation(
        &self,
        _origin: &str,
        key_id: &str,
        signature: &str,
        content: &[u8],
    ) -> Result<bool, anyhow::Error> {
        if let Some(current) = &*self.current_key.read().await {
            if current.key_id == key_id {
                if let Ok(()) = self
                    .verify_signature(&current.public_key, signature, content)
                    .await
                {
                    return Ok(true);
                }
            }
        }

        if let Some(historical) = self.historical_keys.read().await.get(key_id) {
            if self.is_within_grace_period(historical).await {
                if let Ok(()) = self
                    .verify_signature(&historical.public_key, signature, content)
                    .await
                {
                    return Ok(true);
                }
            }
        }

        self.verify_from_database(key_id, signature, content).await
    }

    async fn is_within_grace_period(&self, key: &SigningKey) -> bool {
        let now = Utc::now().timestamp_millis();
        let grace_end =
            key.expires_at + Duration::minutes(KEY_GRACE_PERIOD_MINUTES).num_milliseconds();
        now <= grace_end
    }

    async fn verify_signature(
        &self,
        public_key: &str,
        signature: &str,
        content: &[u8],
    ) -> Result<(), anyhow::Error> {
        let pub_key_bytes: [u8; 32] = base64::engine::general_purpose::STANDARD_NO_PAD
            .decode(public_key)
            .map_err(|_| anyhow::anyhow!("Invalid public key format"))?
            .try_into()
            .map_err(|_| anyhow::anyhow!("Public key must be 32 bytes"))?;

        let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&pub_key_bytes)
            .map_err(|_| anyhow::anyhow!("Invalid verifying key"))?;

        let sig_bytes = base64::engine::general_purpose::STANDARD
            .decode(signature)
            .map_err(|_| anyhow::anyhow!("Invalid signature format"))?;

        let dalek_signature = ed25519_dalek::Signature::from_slice(&sig_bytes)
            .map_err(|_| anyhow::anyhow!("Invalid signature length"))?;

        verifying_key
            .verify(content, &dalek_signature)
            .map_err(|_| anyhow::anyhow!("Signature verification failed"))?;

        Ok(())
    }

    async fn verify_from_database(
        &self,
        key_id: &str,
        signature: &str,
        content: &[u8],
    ) -> Result<bool, anyhow::Error> {
        self.ensure_signing_keys_table().await?;

        let key_record: Option<sqlx::postgres::PgRow> = sqlx::query(
            r#"
            SELECT public_key, expires_at FROM federation_signing_keys WHERE key_id = $1
            "#,
        )
        .bind(key_id)
        .fetch_optional(&*self.pool)
        .await?;

        match key_record {
            Some(record) => {
                let expires_at: i64 = record.get("expires_at");
                let now = Utc::now().timestamp_millis();

                if expires_at > 0
                    && now
                        > expires_at
                            + Duration::minutes(KEY_GRACE_PERIOD_MINUTES).num_milliseconds()
                {
                    return Ok(false);
                }

                let public_key: String = record.get("public_key");
                return self
                    .verify_signature(&public_key, signature, content)
                    .await
                    .map(|_| true);
            }
            None => Ok(false),
        }
    }

    pub async fn cache_historical_key(&self, origin: &str, key_id: &str, public_key: String) {
        let expires_at = (Utc::now() + Duration::hours(24)).timestamp_millis();

        let cache_key = format!("federation:historical_key:{}:{}", origin, key_id);

        let mut cache = self.memory_cache.write().await;
        cache.insert(cache_key, (public_key, expires_at));
    }

    pub async fn get_server_keys_response(&self) -> Result<serde_json::Value, anyhow::Error> {
        let current_key = match &*self.current_key.read().await {
            Some(key) => key.clone(),
            None => return Err(anyhow::anyhow!("No signing key available")),
        };

        let mut old_verify_keys = serde_json::Map::new();
        for (key_id, key) in &*self.historical_keys.read().await {
            old_verify_keys.insert(
                key_id.clone(),
                json!({
                    "key": key.public_key,
                    "expired_ts": key.expires_at
                }),
            );
        }

        Ok(json!({
            "server_name": self.server_name,
            "verify_keys": {
                current_key.key_id: {
                    "key": current_key.public_key
                }
            },
            "old_verify_keys": old_verify_keys,
            "valid_until_ts": current_key.expires_at
        }))
    }

    pub async fn notify_key_change(&self, remote_server: &str) -> Result<(), anyhow::Error> {
        tracing::info!(
            "Notifying {} about key change for server {}",
            remote_server,
            self.server_name
        );
        let server_keys = self.get_server_keys_response().await?;
        tracing::debug!("Key notification payload: {:?}", server_keys);
        Ok(())
    }

    pub async fn broadcast_key_change_to_federation(&self) -> Result<(), anyhow::Error> {
        let known_servers = self.get_known_federation_servers().await?;
        if known_servers.is_empty() {
            tracing::debug!("No known federation servers to notify about key change");
            return Ok(());
        }
        tracing::info!(
            "Broadcasting key change to {} federation servers",
            known_servers.len()
        );
        for server in &known_servers {
            if let Err(e) = self.notify_key_change(server).await {
                tracing::warn!("Failed to notify {} about key change: {}", server, e);
            }
        }
        Ok(())
    }

    async fn get_known_federation_servers(&self) -> Result<Vec<String>, anyhow::Error> {
        let servers: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT server_name FROM federation_servers WHERE server_name != $1",
        )
        .bind(&self.server_name)
        .fetch_all(&*self.pool)
        .await
        .unwrap_or_default();
        Ok(servers.into_iter().map(|(s,)| s).collect())
    }

    pub async fn set_rotation_enabled(&self, enabled: bool) {
        *self.rotation_enabled.write().await = enabled;
        tracing::info!(
            "Key rotation {}",
            if enabled { "enabled" } else { "disabled" }
        );
    }

    pub async fn get_rotation_status(&self) -> serde_json::Value {
        let current_key = &*self.current_key.read().await;
        let should_rotate = self.should_rotate_keys().await;

        json!({
            "rotation_enabled": *self.rotation_enabled.read().await,
            "has_current_key": current_key.is_some(),
            "should_rotate": should_rotate,
            "server_name": self.server_name
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;
    use std::env;

    async fn create_test_pool() -> Arc<PgPool> {
        let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://synapse:synapse@localhost:5432/synapse_test".to_string()
        });
        // Use connect_lazy to allow creating the pool without an immediate connection check
        match PgPool::connect_lazy(&database_url) {
            Ok(pool) => Arc::new(pool),
            Err(_) => {
                panic!("Failed to create lazy pool connection");
            }
        }
    }

    async fn setup_test_database() -> Option<Arc<PgPool>> {
        let database_url = env::var("TEST_DATABASE_URL")
            .or_else(|_| env::var("DATABASE_URL"))
            .unwrap_or_else(|_| {
                "postgres://synapse:synapse@localhost:5432/synapse_test".to_string()
            });

        let pool = match sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(std::time::Duration::from_secs(10))
            .connect(&database_url)
            .await
        {
            Ok(pool) => Arc::new(pool),
            Err(error) => {
                eprintln!(
                    "Skipping key rotation database tests because test database is unavailable: {}",
                    error
                );
                return None;
            }
        };

        sqlx::query("DROP TABLE IF EXISTS federation_signing_keys CASCADE")
            .execute(&*pool)
            .await
            .ok();

        sqlx::query(
            r#"
            CREATE TABLE federation_signing_keys (
                server_name TEXT NOT NULL,
                key_id TEXT NOT NULL,
                secret_key TEXT NOT NULL,
                public_key TEXT NOT NULL,
                created_ts BIGINT NOT NULL,
                expires_at BIGINT NOT NULL,
                key_json JSONB NOT NULL DEFAULT '{}'::jsonb,
                ts_added_ms BIGINT NOT NULL,
                ts_valid_until_ms BIGINT NOT NULL,
                PRIMARY KEY (server_name, key_id)
            )
            "#,
        )
        .execute(&*pool)
        .await
        .expect("Failed to create federation_signing_keys table");

        Some(pool)
    }

    // fn generate_valid_test_key() -> String {
    //     use rand::RngCore;
    //     let mut rng = rand::thread_rng();
    //     let mut secret_bytes = [0u8; 32];
    //     rng.fill_bytes(&mut secret_bytes);
    //     base64::engine::general_purpose::STANDARD_NO_PAD.encode(secret_bytes)
    // }

    #[tokio::test]
    async fn test_grace_period() {
        let pool = create_test_pool().await;
        let manager = KeyRotationManager::new(&pool, "test.example.com");

        let expired_key = SigningKey {
            server_name: "test.example.com".to_string(),
            key_id: "ed25519:expired".to_string(),
            secret_key: "test".to_string(),
            public_key: "test".to_string(),
            created_ts: 0,
            expires_at: Utc::now().timestamp_millis() - 1000,
            key_json: serde_json::json!({}),
            ts_added_ms: 0,
            ts_valid_until_ms: 0,
        };

        assert!(manager.is_within_grace_period(&expired_key).await);
    }

    #[test]
    fn test_key_rotation_constants() {
        assert_eq!(KEY_ROTATION_INTERVAL_DAYS, 7);
        assert_eq!(KEY_GRACE_PERIOD_MINUTES, 5);
    }

    #[test]
    fn test_signing_key_creation() {
        let key = SigningKey {
            server_name: "test.example.com".to_string(),
            key_id: "ed25519:test".to_string(),
            secret_key: "secret123".to_string(),
            public_key: "public456".to_string(),
            created_ts: 1000,
            expires_at: 2000,
            key_json: serde_json::json!({}),
            ts_added_ms: 1000,
            ts_valid_until_ms: 2000,
        };

        assert_eq!(key.key_id, "ed25519:test");
        assert_eq!(key.secret_key, "secret123");
        assert_eq!(key.public_key, "public456");
        assert_eq!(key.created_ts, 1000);
        assert_eq!(key.expires_at, 2000);
    }

    #[test]
    fn test_signing_key_clone() {
        let key = SigningKey {
            server_name: "test.example.com".to_string(),
            key_id: "ed25519:test".to_string(),
            secret_key: "secret123".to_string(),
            public_key: "public456".to_string(),
            created_ts: 1000,
            expires_at: 2000,
            key_json: serde_json::json!({}),
            ts_added_ms: 1000,
            ts_valid_until_ms: 2000,
        };

        let cloned = key.clone();
        assert_eq!(key.key_id, cloned.key_id);
        assert_eq!(key.secret_key, cloned.secret_key);
    }

    #[tokio::test]
    async fn test_key_rotation_manager_new() {
        let pool = create_test_pool().await;
        let manager = KeyRotationManager::new(&pool, "test.example.com");

        let current = manager.get_current_key().await.unwrap();
        assert!(current.is_none());
    }

    #[tokio::test]
    async fn test_should_rotate_keys_no_key() {
        let pool = create_test_pool().await;
        let manager = KeyRotationManager::new(&pool, "test.example.com");

        let should_rotate = manager.should_rotate_keys().await;
        assert!(should_rotate);
    }

    #[tokio::test]
    async fn test_should_rotate_keys_with_fresh_key() {
        let pool = create_test_pool().await;
        let manager = KeyRotationManager::new(&pool, "test.example.com");

        let future_expires = (Utc::now() + Duration::days(30)).timestamp_millis();

        {
            let mut current = manager.current_key.write().await;
            *current = Some(SigningKey {
                server_name: "test.example.com".to_string(),
                key_id: "ed25519:test".to_string(),
                secret_key: "test".to_string(),
                public_key: "test".to_string(),
                created_ts: Utc::now().timestamp_millis(),
                expires_at: future_expires,
                key_json: serde_json::json!({}),
                ts_added_ms: Utc::now().timestamp_millis(),
                ts_valid_until_ms: future_expires,
            });
        }

        let should_rotate = manager.should_rotate_keys().await;
        assert!(!should_rotate);
    }

    #[tokio::test]
    async fn test_should_rotate_keys_expiring_soon() {
        let pool = create_test_pool().await;
        let manager = KeyRotationManager::new(&pool, "test.example.com");

        let soon_expires = (Utc::now() + Duration::hours(12)).timestamp_millis();

        {
            let mut current = manager.current_key.write().await;
            *current = Some(SigningKey {
                server_name: "test.example.com".to_string(),
                key_id: "ed25519:test".to_string(),
                secret_key: "test".to_string(),
                public_key: "test".to_string(),
                created_ts: Utc::now().timestamp_millis(),
                expires_at: soon_expires,
                key_json: serde_json::json!({}),
                ts_added_ms: Utc::now().timestamp_millis(),
                ts_valid_until_ms: soon_expires,
            });
        }

        let should_rotate = manager.should_rotate_keys().await;
        assert!(should_rotate);
    }

    #[tokio::test]
    async fn test_load_or_create_key_loads_full_existing_record() {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let manager = KeyRotationManager::new(&pool, "test.example.com");
        let now = Utc::now().timestamp_millis();
        let expires_at = now + Duration::days(7).num_milliseconds();
        let key_json = json!({
            "secret_key": "secret123",
            "public_key": "public456"
        });

        sqlx::query(
            r#"
            INSERT INTO federation_signing_keys (
                server_name,
                key_id,
                secret_key,
                public_key,
                created_ts,
                expires_at,
                key_json,
                ts_added_ms,
                ts_valid_until_ms
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
        )
        .bind("test.example.com")
        .bind("ed25519:test")
        .bind("secret123")
        .bind("public456")
        .bind(now)
        .bind(expires_at)
        .bind(key_json.clone())
        .bind(now)
        .bind(expires_at)
        .execute(&*pool)
        .await
        .expect("Failed to insert test federation signing key");

        manager.load_or_create_key().await.unwrap();

        let current = manager
            .get_current_key()
            .await
            .unwrap()
            .expect("current signing key should exist");

        assert_eq!(current.server_name, "test.example.com");
        assert_eq!(current.key_id, "ed25519:test");
        assert_eq!(current.secret_key, "secret123");
        assert_eq!(current.public_key, "public456");
        assert_eq!(current.created_ts, now);
        assert_eq!(current.expires_at, expires_at);
        assert_eq!(current.key_json, key_json);
        assert_eq!(current.ts_added_ms, now);
        assert_eq!(current.ts_valid_until_ms, expires_at);
    }

    #[tokio::test]
    async fn test_cache_historical_key() {
        let pool = create_test_pool().await;
        let manager = KeyRotationManager::new(&pool, "test.example.com");

        manager
            .cache_historical_key("example.com", "ed25519:old", "public_key_data".to_string())
            .await;

        let cache = manager.memory_cache.read().await;
        let key = "federation:historical_key:example.com:ed25519:old".to_string();
        assert!(cache.contains_key(&key));
    }

    #[tokio::test]
    async fn test_get_server_keys_response_no_key() {
        let pool = create_test_pool().await;
        let manager = KeyRotationManager::new(&pool, "test.example.com");

        let result = manager.get_server_keys_response().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_server_keys_response_with_key() {
        let pool = create_test_pool().await;
        let manager = KeyRotationManager::new(&pool, "test.example.com");

        // Set a current key
        {
            let mut current = manager.current_key.write().await;
            *current = Some(SigningKey {
                server_name: "test.example.com".to_string(),
                key_id: "ed25519:test".to_string(),
                secret_key: "test".to_string(),
                public_key: "test_public_key".to_string(),
                created_ts: Utc::now().timestamp_millis(),
                expires_at: (Utc::now() + Duration::days(7)).timestamp_millis(),
                key_json: serde_json::json!({}),
                ts_added_ms: Utc::now().timestamp_millis(),
                ts_valid_until_ms: (Utc::now() + Duration::days(7)).timestamp_millis(),
            });
        }

        let result = manager.get_server_keys_response().await.unwrap();
        assert_eq!(result["server_name"], "test.example.com");
        assert!(result["verify_keys"].is_object());
        assert!(result["valid_until_ts"].is_number());
    }

    #[tokio::test]
    async fn test_get_server_keys_response_with_historical_keys() {
        let pool = create_test_pool().await;
        let manager = KeyRotationManager::new(&pool, "test.example.com");

        // Set current key
        {
            let mut current = manager.current_key.write().await;
            *current = Some(SigningKey {
                server_name: "test.example.com".to_string(),
                key_id: "ed25519:current".to_string(),
                secret_key: "test".to_string(),
                public_key: "current_public".to_string(),
                created_ts: Utc::now().timestamp_millis(),
                expires_at: (Utc::now() + Duration::days(7)).timestamp_millis(),
                key_json: serde_json::json!({}),
                ts_added_ms: Utc::now().timestamp_millis(),
                ts_valid_until_ms: (Utc::now() + Duration::days(7)).timestamp_millis(),
            });
        }

        // Add historical key
        {
            let mut historical = manager.historical_keys.write().await;
            historical.insert(
                "ed25519:old".to_string(),
                SigningKey {
                    server_name: "test.example.com".to_string(),
                    key_id: "ed25519:old".to_string(),
                    secret_key: "old_secret".to_string(),
                    public_key: "old_public".to_string(),
                    created_ts: 0,
                    expires_at: Utc::now().timestamp_millis() - 1000,
                    key_json: serde_json::json!({}),
                    ts_added_ms: 0,
                    ts_valid_until_ms: 0,
                },
            );
        }

        let result = manager.get_server_keys_response().await.unwrap();
        assert!(result["old_verify_keys"].is_object());
    }

    #[tokio::test]
    async fn test_set_rotation_enabled() {
        let pool = create_test_pool().await;
        let manager = KeyRotationManager::new(&pool, "test.example.com");

        // Disable rotation
        manager.set_rotation_enabled(false).await;

        let status = manager.get_rotation_status().await;
        assert_eq!(status["rotation_enabled"], false);

        // Enable rotation
        manager.set_rotation_enabled(true).await;

        let status = manager.get_rotation_status().await;
        assert_eq!(status["rotation_enabled"], true);
    }

    #[tokio::test]
    async fn test_get_rotation_status_no_key() {
        let pool = create_test_pool().await;
        let manager = KeyRotationManager::new(&pool, "test.example.com");

        let status = manager.get_rotation_status().await;
        assert_eq!(status["has_current_key"], false);
        assert_eq!(status["should_rotate"], true);
        assert_eq!(status["server_name"], "test.example.com");
    }

    #[tokio::test]
    async fn test_get_rotation_status_with_key() {
        let pool = create_test_pool().await;
        let manager = KeyRotationManager::new(&pool, "test.example.com");

        // Set a key
        {
            let mut current = manager.current_key.write().await;
            *current = Some(SigningKey {
                server_name: "test.example.com".to_string(),
                key_id: "ed25519:test".to_string(),
                secret_key: "test".to_string(),
                public_key: "test".to_string(),
                created_ts: Utc::now().timestamp_millis(),
                expires_at: (Utc::now() + Duration::days(30)).timestamp_millis(),
                key_json: serde_json::json!({}),
                ts_added_ms: Utc::now().timestamp_millis(),
                ts_valid_until_ms: (Utc::now() + Duration::days(30)).timestamp_millis(),
            });
        }

        let status = manager.get_rotation_status().await;
        assert_eq!(status["has_current_key"], true);
        assert_eq!(status["should_rotate"], false);
    }

    #[tokio::test]
    async fn test_derive_public_key_invalid_base64() {
        let pool = create_test_pool().await;
        let manager = KeyRotationManager::new(&pool, "test.example.com");

        let result = manager.derive_public_key("not-valid-base64!!!").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_derive_public_key_wrong_length() {
        let pool = create_test_pool().await;
        let manager = KeyRotationManager::new(&pool, "test.example.com");

        // 16 bytes instead of 32
        let short_key = base64::engine::general_purpose::STANDARD_NO_PAD.encode(b"short_key");
        let result = manager.derive_public_key(&short_key).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_key_rotation_manager_clone() {
        let pool = std::sync::Arc::new(
            sqlx::postgres::PgPoolOptions::new()
                .connect_lazy("postgres://synapse:synapse@localhost:5432/synapse_test")
                .unwrap(),
        );
        let manager = KeyRotationManager::new(&pool, "test.example.com");
        let _cloned = manager.clone();
        // Should compile - Verify clone works
    }
}
