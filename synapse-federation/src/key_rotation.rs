use crate::signing::sign_json;
use async_trait::async_trait;
use base64::Engine;
use chrono::{Duration, Utc};
use parking_lot::RwLock as ParkingLotRwLock;
use serde_json::json;
use sqlx::{Pool, Postgres};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use synapse_cache::{FederationSignatureCache, KeyRotationEvent};
use synapse_common::current_timestamp_millis;
use synapse_common::key_encryption::{decrypt_key, encrypt_key, is_encrypted};
use synapse_common::ApiError;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration as TokioDuration};

const DEFAULT_KEY_ROTATION_INTERVAL_DAYS: i64 = 7;
const DEFAULT_KEY_ROTATION_THRESHOLD_DAYS: i64 = 1;
const DEFAULT_KEY_GRACE_PERIOD_MINUTES: i64 = 5;

#[derive(Debug, Clone)]
struct FederationRotationConfig {
    rotation_interval_days: i64,
    rotation_threshold_days: i64,
    grace_period_minutes: i64,
}

impl Default for FederationRotationConfig {
    fn default() -> Self {
        Self {
            rotation_interval_days: DEFAULT_KEY_ROTATION_INTERVAL_DAYS,
            rotation_threshold_days: DEFAULT_KEY_ROTATION_THRESHOLD_DAYS,
            grace_period_minutes: DEFAULT_KEY_GRACE_PERIOD_MINUTES,
        }
    }
}

fn new_key_id() -> String {
    let ts = current_timestamp_millis();
    let rand: u32 = rand::random();
    format!("ed25519:{ts:x}_{rand:08x}")
}

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

#[derive(Debug, Clone, sqlx::FromRow)]
struct FederationServerName {
    pub server_name: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct FederationKeyRecord {
    pub public_key: String,
    pub expires_at: i64,
}

type CachedKeyEntry = (String, i64);

#[async_trait]
pub trait KeyRotationManagerApi: Send + Sync {
    async fn get_rotation_status(&self) -> serde_json::Value;
    async fn rotate_keys(&self, requested_key_id: Option<String>) -> Result<(), ApiError>;
    async fn get_current_key(&self) -> Result<Option<SigningKey>, ApiError>;
    async fn revoke_key(
        &self,
        key_id: &str,
        reason: Option<&str>,
    ) -> Result<u64, Box<dyn std::error::Error + Send + Sync>>;
    async fn set_rotation_enabled(&self, enabled: bool);
    async fn set_rotation_config_value(&self, key: &str, value: &str) -> Result<(), ApiError>;
}

#[derive(Debug, Clone)]
pub struct KeyRotationManager {
    pool: Arc<Pool<Postgres>>,
    memory_cache: Arc<RwLock<HashMap<String, CachedKeyEntry>>>,
    current_key: Arc<RwLock<Option<SigningKey>>>,
    historical_keys: Arc<RwLock<HashMap<String, SigningKey>>>,
    server_name: String,
    rotation_enabled: Arc<RwLock<bool>>,
    signing_keys_table_ready: Arc<AtomicBool>,
    signing_key_path: Option<String>,
    master_key: Option<Vec<u8>>,
    allow_plaintext_signing_keys: bool,
    rotation_config: Arc<RwLock<FederationRotationConfig>>,
    signature_cache: Arc<ParkingLotRwLock<Option<Arc<FederationSignatureCache>>>>,
}

impl KeyRotationManager {
    pub fn new(pool: &Arc<Pool<Postgres>>, server_name: &str) -> Self {
        Self::with_key_path(pool, server_name, None)
    }

    pub fn with_key_path(pool: &Arc<Pool<Postgres>>, server_name: &str, signing_key_path: Option<String>) -> Self {
        Self::with_key_path_and_master_key(pool, server_name, signing_key_path, None)
    }

    pub fn with_key_path_and_master_key(
        pool: &Arc<Pool<Postgres>>,
        server_name: &str,
        signing_key_path: Option<String>,
        master_key: Option<Vec<u8>>,
    ) -> Self {
        if master_key.is_none() {
            tracing::warn!(
                "No signing_key_master_key configured - federation signing keys will be stored in plaintext. \
                 Set federation.signing_key_master_key for encrypted key storage."
            );
        }
        Self {
            pool: pool.clone(),
            memory_cache: Arc::new(RwLock::new(HashMap::new())),
            current_key: Arc::new(RwLock::new(None)),
            historical_keys: Arc::new(RwLock::new(HashMap::new())),
            server_name: server_name.to_string(),
            rotation_enabled: Arc::new(RwLock::new(true)),
            signing_keys_table_ready: Arc::new(AtomicBool::new(false)),
            signing_key_path,
            master_key,
            allow_plaintext_signing_keys: false,
            rotation_config: Arc::new(RwLock::new(FederationRotationConfig::default())),
            signature_cache: Arc::new(ParkingLotRwLock::new(None)),
        }
    }

    /// Opt in to persisting federation signing keys in plaintext when no master
    /// key is configured. Defaults to `false` (secure): without a master key or
    /// this opt-in, key persistence is refused.
    pub fn with_allow_plaintext_signing_keys(mut self, allow: bool) -> Self {
        self.allow_plaintext_signing_keys = allow;
        self
    }

    /// Resolve how a signing secret key is stored at rest.
    /// - master key present -> encrypt.
    /// - no master key + explicit opt-in -> plaintext (with a warning).
    /// - no master key + no opt-in -> refuse (security: no plaintext federation signing key at rest).
    fn resolve_stored_secret_key(
        master_key: &Option<Vec<u8>>,
        allow_plaintext: bool,
        secret_key: &str,
    ) -> Result<String, ApiError> {
        match master_key {
            Some(mk) => encrypt_key(secret_key, mk)
                .map_err(|e| ApiError::internal(format!("Failed to encrypt signing key: {e}"))),
            None if allow_plaintext => {
                tracing::warn!(
                    "Storing federation signing key in plaintext (explicitly allowed) - configure signing_key_master_key for encryption at rest"
                );
                Ok(secret_key.to_string())
            }
            None => Err(ApiError::internal(
                "Refusing to persist federation signing key without a master key. Set federation.signing_key_master_key to encrypt at rest, or explicitly allow plaintext.".to_string(),
            )),
        }
    }

    async fn ensure_signing_keys_table(&self) -> Result<(), ApiError> {
        if self.signing_keys_table_ready.load(Ordering::Relaxed) {
            return Ok(());
        }

        let table_exists: bool = sqlx::query_scalar::<_, bool>(
            r"
            SELECT EXISTS (
                SELECT 1
                FROM information_schema.tables
                WHERE table_schema = current_schema()
                  AND table_name = 'federation_signing_keys'
            )
            ",
        )
        .fetch_one(&*self.pool)
        .await?;

        if !table_exists {
            sqlx::query(
                r"
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
                ",
            )
            .execute(&*self.pool)
            .await?;
        }

        let server_created_index_exists: bool = sqlx::query_scalar::<_, bool>(
            r"
            SELECT EXISTS (
                SELECT 1
                FROM pg_indexes
                WHERE schemaname = current_schema()
                  AND indexname = 'idx_federation_signing_keys_server_created'
            )
            ",
        )
        .fetch_one(&*self.pool)
        .await?;

        if !server_created_index_exists {
            sqlx::query(
                r"
                CREATE INDEX idx_federation_signing_keys_server_created
                ON federation_signing_keys(server_name, created_ts DESC)
                ",
            )
            .execute(&*self.pool)
            .await?;
        }

        let key_id_index_exists: bool = sqlx::query_scalar::<_, bool>(
            r"
            SELECT EXISTS (
                SELECT 1
                FROM pg_indexes
                WHERE schemaname = current_schema()
                  AND indexname = 'idx_federation_signing_keys_key_id'
            )
            ",
        )
        .fetch_one(&*self.pool)
        .await?;

        if !key_id_index_exists {
            sqlx::query(
                r"
                CREATE INDEX idx_federation_signing_keys_key_id
                ON federation_signing_keys(key_id)
                ",
            )
            .execute(&*self.pool)
            .await?;
        }

        self.signing_keys_table_ready.store(true, Ordering::Relaxed);

        Ok(())
    }

    async fn ensure_key_rotation_config_table(&self) -> Result<(), ApiError> {
        sqlx::query(
            r"
            CREATE TABLE IF NOT EXISTS key_rotation_config (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )
            ",
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn load_rotation_config(&self) -> Result<(), ApiError> {
        self.ensure_key_rotation_config_table().await?;

        let interval_days: i64 =
            sqlx::query_scalar!(r"SELECT value FROM key_rotation_config WHERE key = 'rotation_interval_days'")
                .fetch_optional(&*self.pool)
                .await?
                .and_then(|v| v.parse().ok())
                .unwrap_or(DEFAULT_KEY_ROTATION_INTERVAL_DAYS);

        let threshold_days: i64 =
            sqlx::query_scalar!(r"SELECT value FROM key_rotation_config WHERE key = 'rotation_threshold_days'")
                .fetch_optional(&*self.pool)
                .await?
                .and_then(|v| v.parse().ok())
                .unwrap_or(DEFAULT_KEY_ROTATION_THRESHOLD_DAYS);

        let grace_period_minutes: i64 =
            sqlx::query_scalar!(r"SELECT value FROM key_rotation_config WHERE key = 'grace_period_minutes'")
                .fetch_optional(&*self.pool)
                .await?
                .and_then(|v| v.parse().ok())
                .unwrap_or(DEFAULT_KEY_GRACE_PERIOD_MINUTES);

        let new_config = FederationRotationConfig {
            rotation_interval_days: interval_days,
            rotation_threshold_days: threshold_days,
            grace_period_minutes,
        };

        let mut config = self.rotation_config.write().await;
        tracing::info!(
            "Loaded federation rotation config: interval={}d, threshold={}d, grace={}m",
            new_config.rotation_interval_days,
            new_config.rotation_threshold_days,
            new_config.grace_period_minutes
        );
        *config = new_config;

        Ok(())
    }

    pub async fn set_rotation_config_value(&self, key: &str, value: &str) -> Result<(), ApiError> {
        self.ensure_key_rotation_config_table().await?;

        sqlx::query!(
            r"
            INSERT INTO key_rotation_config (key, value)
            VALUES ($1, $2)
            ON CONFLICT (key) DO UPDATE SET value = $2
            ",
            key,
            value
        )
        .execute(&*self.pool)
        .await?;

        self.load_rotation_config().await?;

        Ok(())
    }

    pub async fn start_auto_rotation(&self) {
        let manager = Arc::new(self.clone());

        let init_result = manager.load_or_create_key().await;
        if let Err(e) = init_result {
            tracing::error!("Failed to initialize key rotation: {}", e);
        }

        if let Err(e) = manager.load_rotation_config().await {
            tracing::warn!("Failed to load rotation config from database, using defaults: {}", e);
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

    pub async fn load_or_create_key(&self) -> Result<(), ApiError> {
        self.ensure_signing_keys_table().await?;

        let existing_key = sqlx::query_as!(
            SigningKey,
            r#"
            SELECT
                server_name AS "server_name!",
                key_id AS "key_id!",
                secret_key AS "secret_key!",
                public_key AS "public_key!",
                created_ts AS "created_ts!",
                expires_at AS "expires_at!",
                key_json AS "key_json!",
                ts_added_ms AS "ts_added_ms!",
                ts_valid_until_ms AS "ts_valid_until_ms!"
            FROM federation_signing_keys
            WHERE server_name = $1 AND (expires_at = 0 OR expires_at > $2)
            ORDER BY created_ts DESC
            LIMIT 1
            "#,
            &self.server_name,
            current_timestamp_millis()
        )
        .fetch_optional(&*self.pool)
        .await;

        match existing_key {
            Ok(Some(mut key)) => {
                if is_encrypted(&key.secret_key) {
                    match &self.master_key {
                        Some(mk) => {
                            key.secret_key = decrypt_key(&key.secret_key, mk)
                                .map_err(|e| ApiError::internal(format!("Failed to decrypt signing key: {e}")))?;
                        }
                        None => {
                            return Err(ApiError::internal("Signing key is encrypted but no master key is configured"));
                        }
                    }
                } else {
                    tracing::warn!(
                        "Federation signing key for {} is stored in plaintext. \
                         Consider configuring signing_key_master_key to encrypt keys at rest.",
                        key.key_id
                    );
                }
                *self.current_key.write().await = Some(key.clone());
                tracing::info!("Loaded existing signing key from database");
                if let Err(e) = self.export_signing_key_to_file(&key).await {
                    tracing::warn!("Failed to export signing key to file: {}", e);
                }
                return Ok(());
            }
            Ok(None) => {}
            Err(e) => {
                return Err(ApiError::from(e));
            }
        }

        let key_id = new_key_id();
        let secret_key = base64::engine::general_purpose::STANDARD_NO_PAD.encode(rand::random::<[u8; 32]>());

        match self.initialize(&secret_key, &key_id).await {
            Ok(_) => {
                tracing::info!("Created new signing key");
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    pub async fn export_signing_key_to_file(&self, key: &SigningKey) -> Result<(), ApiError> {
        let path = match &self.signing_key_path {
            Some(p) => p.clone(),
            None => return Ok(()),
        };

        let key_content =
            format!("ed25519 {} {}", key.key_id.strip_prefix("ed25519:").unwrap_or(&key.key_id), key.secret_key);

        if let Some(parent) = std::path::Path::new(&path).parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        tokio::fs::write(&path, &key_content).await?;
        tracing::debug!("Exported signing key to file: {}", path);
        Ok(())
    }

    pub async fn initialize(&self, secret_key: &str, key_id: &str) -> Result<(), ApiError> {
        self.ensure_signing_keys_table().await?;

        let created_ts = current_timestamp_millis();
        let interval_days = self.rotation_config.read().await.rotation_interval_days;
        let expires_at = (Utc::now() + Duration::days(interval_days)).timestamp_millis();

        let public_key = self.derive_public_key(secret_key)?;

        let signing_key = SigningKey {
            server_name: self.server_name.clone(),
            key_id: key_id.to_string(),
            secret_key: secret_key.to_string(),
            public_key: public_key.clone(),
            created_ts,
            expires_at,
            key_json: json!({
                "public_key": public_key
            }),
            ts_added_ms: created_ts,
            ts_valid_until_ms: expires_at,
        };

        *self.current_key.write().await = Some(signing_key.clone());

        let stored_secret_key =
            Self::resolve_stored_secret_key(&self.master_key, self.allow_plaintext_signing_keys, secret_key)?;

        let key_json = json!({
            "public_key": signing_key.public_key
        });

        sqlx::query!(
            r"
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
            ",
            &self.server_name,
            &signing_key.key_id,
            &stored_secret_key,
            &signing_key.public_key,
            signing_key.created_ts,
            signing_key.expires_at,
            &key_json,
            signing_key.created_ts,
            signing_key.expires_at
        )
        .execute(&*self.pool)
        .await?;

        if let Err(e) = self.export_signing_key_to_file(&signing_key).await {
            tracing::warn!("Failed to export signing key to file: {}", e);
        }

        Ok(())
    }

    pub async fn should_rotate_keys(&self) -> bool {
        if let Some(key) = &*self.current_key.read().await {
            let now = current_timestamp_millis();
            let threshold_days = self.rotation_config.read().await.rotation_threshold_days;
            let rotation_threshold = Duration::days(threshold_days).num_milliseconds();
            key.expires_at.saturating_sub(now) <= rotation_threshold
        } else {
            true
        }
    }

    pub async fn rotate_keys(&self, requested_key_id: Option<String>) -> Result<(), ApiError> {
        let old_key_id = {
            let current = self.current_key.read().await;
            current.as_ref().map(|key| key.key_id.clone())
        };

        let current = self.current_key.read().await;
        if let Some(key) = current.as_ref() {
            self.historical_keys.write().await.insert(key.key_id.clone(), key.clone());
        }
        drop(current);

        let key_id = requested_key_id.unwrap_or_else(new_key_id);
        let (key_id, secret_key) = self.generate_new_key_pair(&key_id);

        self.initialize(&secret_key, &key_id).await?;

        if let Err(e) = self.broadcast_key_change_to_federation().await {
            tracing::warn!("Failed to broadcast key change: {}", e);
        }

        if let Some(old_key_id) = old_key_id {
            if let Some(cache) = self.signature_cache.read().as_ref() {
                let event = KeyRotationEvent {
                    origin: self.server_name.clone(),
                    old_key_id,
                    new_key_id: key_id,
                    timestamp: Instant::now(),
                };
                cache.notify_key_rotation(&event);
                tracing::info!("Notified signature cache of key rotation");
            }
        }

        Ok(())
    }

    fn generate_new_key_pair(&self, key_id: &str) -> (String, String) {
        let secret_key = base64::engine::general_purpose::STANDARD_NO_PAD.encode(rand::random::<[u8; 32]>());

        (key_id.to_string(), secret_key)
    }

    fn derive_public_key(&self, secret_key: &str) -> Result<String, ApiError> {
        let secret_bytes: [u8; 32] = base64::engine::general_purpose::STANDARD_NO_PAD
            .decode(secret_key)
            .map_err(|e| ApiError::internal_with_log("Invalid secret key format", &e))?
            .try_into()
            .map_err(|bytes: Vec<u8>| {
                ApiError::internal(format!("Secret key must be 32 bytes, got {}", bytes.len()))
            })?;

        let signing_key = ed25519_dalek::SigningKey::from_bytes(&secret_bytes);
        let verifying_key = signing_key.verifying_key();

        Ok(base64::engine::general_purpose::STANDARD_NO_PAD.encode(verifying_key.as_bytes()))
    }

    pub async fn get_current_key(&self) -> Result<Option<SigningKey>, ApiError> {
        Ok(self.current_key.read().await.clone())
    }

    pub async fn verify_with_key_rotation(
        &self,
        _origin: &str,
        key_id: &str,
        signature: &str,
        content: &[u8],
    ) -> Result<bool, ApiError> {
        if let Some(current) = &*self.current_key.read().await {
            if current.key_id == key_id {
                if let Ok(()) = self.verify_signature(&current.public_key, signature, content) {
                    return Ok(true);
                }
            }
        }

        if let Some(historical) = self.historical_keys.read().await.get(key_id) {
            if self.is_within_grace_period(historical).await {
                if let Ok(()) = self.verify_signature(&historical.public_key, signature, content) {
                    return Ok(true);
                }
            }
        }

        self.verify_from_database(key_id, signature, content).await
    }

    async fn is_within_grace_period(&self, key: &SigningKey) -> bool {
        let now = current_timestamp_millis();
        let grace_minutes = self.rotation_config.read().await.grace_period_minutes;
        let grace_end = key.expires_at + Duration::minutes(grace_minutes).num_milliseconds();
        now <= grace_end
    }

    fn verify_signature(&self, public_key: &str, signature: &str, content: &[u8]) -> Result<(), ApiError> {
        let pub_key_bytes: [u8; 32] = base64::engine::general_purpose::STANDARD_NO_PAD
            .decode(public_key)
            .map_err(|e| ApiError::internal_with_log("Invalid public key format", &e))?
            .try_into()
            .map_err(|bytes: Vec<u8>| {
                ApiError::internal(format!("Public key must be 32 bytes, got {}", bytes.len()))
            })?;

        let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&pub_key_bytes)
            .map_err(|e| ApiError::internal_with_log("Invalid verifying key", &e))?;

        let sig_bytes = base64::engine::general_purpose::STANDARD
            .decode(signature)
            .map_err(|e| ApiError::internal_with_log("Invalid signature format", &e))?;

        let dalek_signature = ed25519_dalek::Signature::from_slice(&sig_bytes)
            .map_err(|e| ApiError::internal_with_log("Invalid signature length", &e))?;

        verifying_key
            .verify_strict(content, &dalek_signature)
            .map_err(|e| ApiError::internal_with_log("Signature verification failed", &e))?;

        Ok(())
    }

    async fn verify_from_database(&self, key_id: &str, signature: &str, content: &[u8]) -> Result<bool, ApiError> {
        self.ensure_signing_keys_table().await?;

        let key_record = sqlx::query_as!(
            FederationKeyRecord,
            r#"
            SELECT public_key AS "public_key!",
                   expires_at AS "expires_at!"
            FROM federation_signing_keys WHERE key_id = $1
            "#,
            key_id
        )
        .fetch_optional(&*self.pool)
        .await?;

        match key_record {
            Some(record) => {
                let expires_at = record.expires_at;
                let now = current_timestamp_millis();

                if expires_at > 0 {
                    let grace_minutes = self.rotation_config.read().await.grace_period_minutes;
                    if now > expires_at + Duration::minutes(grace_minutes).num_milliseconds() {
                        return Ok(false);
                    }
                }

                let public_key = record.public_key;
                self.verify_signature(&public_key, signature, content).map(|_| true)
            }
            None => Ok(false),
        }
    }

    pub async fn cache_historical_key(&self, origin: &str, key_id: &str, public_key: String) {
        let expires_at = (Utc::now() + Duration::hours(24)).timestamp_millis();

        let cache_key = format!("federation:historical_key:{origin}:{key_id}");

        let mut cache = self.memory_cache.write().await;
        cache.insert(cache_key, (public_key, expires_at));
    }

    pub async fn get_server_keys_response(&self) -> Result<serde_json::Value, ApiError> {
        let current_key = match &*self.current_key.read().await {
            Some(key) => key.clone(),
            None => return Err(ApiError::internal("No signing key available")),
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

        let key_id_for_sign = current_key.key_id.clone();
        let secret_key = current_key.secret_key.clone();

        let mut response = json!({
            "server_name": self.server_name,
            "verify_keys": {
                current_key.key_id: {
                    "key": current_key.public_key
                }
            },
            "old_verify_keys": old_verify_keys,
            "valid_until_ts": current_key.expires_at
        });

        sign_json(&self.server_name, &key_id_for_sign, &secret_key, &mut response)
            .map_err(|e| ApiError::internal(format!("Failed to sign server keys: {e}")))?;

        Ok(response)
    }

    pub async fn notify_key_change(&self, remote_server: &str) -> Result<(), ApiError> {
        tracing::info!("Notifying {} about key change for server {}", remote_server, self.server_name);
        let server_keys = self.get_server_keys_response().await?;
        tracing::debug!("Key notification payload: {:?}", server_keys);
        Ok(())
    }

    pub async fn broadcast_key_change_to_federation(&self) -> Result<(), ApiError> {
        let known_servers = self.get_known_federation_servers().await?;
        if known_servers.is_empty() {
            tracing::debug!("No known federation servers to notify about key change");
            return Ok(());
        }
        tracing::info!("Broadcasting key change to {} federation servers", known_servers.len());
        for server in &known_servers {
            if let Err(e) = self.notify_key_change(server).await {
                tracing::warn!("Failed to notify {} about key change: {}", server, e);
            }
        }
        Ok(())
    }

    async fn get_known_federation_servers(&self) -> Result<Vec<String>, ApiError> {
        let servers = sqlx::query_as!(
            FederationServerName,
            r#"SELECT server_name AS "server_name!" FROM federation_servers WHERE server_name != $1"#,
            &self.server_name
        )
        .fetch_all(&*self.pool)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!("Failed to fetch known federation servers: {e}");
            Vec::new()
        });
        Ok(servers.into_iter().map(|s| s.server_name).collect())
    }

    pub async fn revoke_key(
        &self,
        key_id: &str,
        reason: Option<&str>,
    ) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        let now = current_timestamp_millis();

        let key_json_update = if let Some(r) = reason {
            serde_json::json!({
                "revoked": true,
                "revoked_at": now,
                "revoke_reason": r
            })
        } else {
            serde_json::json!({
                "revoked": true,
                "revoked_at": now
            })
        };

        let result = sqlx::query!(
            r"UPDATE federation_signing_keys
               SET expires_at = $1,
                   key_json = COALESCE(key_json, '{}'::jsonb) || $2::jsonb
               WHERE key_id = $3 AND server_name = $4 AND (expires_at = 0 OR expires_at > $1)",
            now,
            &key_json_update,
            key_id,
            &self.server_name
        )
        .execute(&*self.pool)
        .await?;

        let revoked = result.rows_affected();

        {
            let current = self.current_key.read().await;
            if let Some(ref current_key) = *current {
                if current_key.key_id == key_id {
                    drop(current);
                    let _ = self.rotate_keys(None).await;
                }
            }
        }

        {
            let mut historical = self.historical_keys.write().await;
            historical.remove(key_id);
        }

        Ok(revoked)
    }

    pub async fn set_rotation_enabled(&self, enabled: bool) {
        *self.rotation_enabled.write().await = enabled;
        tracing::info!("Key rotation {}", if enabled { "enabled" } else { "disabled" });
    }

    pub async fn get_rotation_status(&self) -> serde_json::Value {
        let current_key = &*self.current_key.read().await;
        let should_rotate = self.should_rotate_keys().await;
        let config = self.rotation_config.read().await;

        json!({
            "rotation_enabled": *self.rotation_enabled.read().await,
            "has_current_key": current_key.is_some(),
            "should_rotate": should_rotate,
            "server_name": self.server_name,
            "rotation_interval_days": config.rotation_interval_days,
            "rotation_threshold_days": config.rotation_threshold_days,
            "grace_period_minutes": config.grace_period_minutes
        })
    }

    pub fn set_signature_cache(&self, cache: Arc<FederationSignatureCache>) {
        *self.signature_cache.write() = Some(cache);
    }
}

#[async_trait]
impl KeyRotationManagerApi for KeyRotationManager {
    async fn get_rotation_status(&self) -> serde_json::Value {
        self.get_rotation_status().await
    }

    async fn rotate_keys(&self, requested_key_id: Option<String>) -> Result<(), ApiError> {
        self.rotate_keys(requested_key_id).await
    }

    async fn get_current_key(&self) -> Result<Option<SigningKey>, ApiError> {
        self.get_current_key().await
    }

    async fn revoke_key(
        &self,
        key_id: &str,
        reason: Option<&str>,
    ) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        self.revoke_key(key_id, reason).await
    }

    async fn set_rotation_enabled(&self, enabled: bool) {
        self.set_rotation_enabled(enabled).await
    }

    async fn set_rotation_config_value(&self, key: &str, value: &str) -> Result<(), ApiError> {
        self.set_rotation_config_value(key, value).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generate_test_signing_key(seed_byte: u8) -> (String, String) {
        let seed = [seed_byte; 32];
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&seed);
        let verifying_key = signing_key.verifying_key();
        let secret_b64 = base64::engine::general_purpose::STANDARD_NO_PAD.encode(signing_key.as_bytes());
        let public_b64 = base64::engine::general_purpose::STANDARD_NO_PAD.encode(verifying_key.as_bytes());
        (secret_b64, public_b64)
    }

    #[test]
    fn refuses_plaintext_persistence_without_master_key() {
        // No master key, no opt-in -> refuse.
        let err = KeyRotationManager::resolve_stored_secret_key(&None, false, "deadbeef").unwrap_err();
        assert!(err.to_string().to_lowercase().contains("master key"), "err was: {err}");

        // Explicit opt-in -> plaintext allowed (returns the key verbatim).
        let ok = KeyRotationManager::resolve_stored_secret_key(&None, true, "deadbeef").unwrap();
        assert_eq!(ok, "deadbeef");

        // With a master key -> encrypted (not equal to the plaintext input).
        let mk = vec![0u8; 32];
        let enc = KeyRotationManager::resolve_stored_secret_key(&Some(mk), false, "deadbeef").unwrap();
        assert_ne!(enc, "deadbeef");
    }

    #[test]
    fn test_key_rotation_constants() {
        assert_eq!(DEFAULT_KEY_ROTATION_INTERVAL_DAYS, 7);
        assert_eq!(DEFAULT_KEY_GRACE_PERIOD_MINUTES, 5);
    }

    #[test]
    fn test_signing_key_creation() {
        let (test_secret, test_public) = generate_test_signing_key(0x02);
        let key = SigningKey {
            server_name: "test.example.com".to_string(),
            key_id: "ed25519:test".to_string(),
            secret_key: test_secret.clone(),
            public_key: test_public.clone(),
            created_ts: 1000,
            expires_at: 2000,
            key_json: serde_json::json!({}),
            ts_added_ms: 1000,
            ts_valid_until_ms: 2000,
        };

        assert_eq!(key.key_id, "ed25519:test");
        assert_eq!(key.secret_key, test_secret);
        assert_eq!(key.public_key, test_public);
        assert_eq!(key.created_ts, 1000);
        assert_eq!(key.expires_at, 2000);
    }

    #[test]
    fn test_signing_key_clone() {
        let (test_secret, test_public) = generate_test_signing_key(0x03);
        let key = SigningKey {
            server_name: "test.example.com".to_string(),
            key_id: "ed25519:test".to_string(),
            secret_key: test_secret,
            public_key: test_public,
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
}
