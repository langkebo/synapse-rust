use crate::common::ApiError;
use crate::common::key_encryption::{encrypt_key, decrypt_key, is_encrypted};
use crate::federation::signing::sign_json;
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

/// Build a fresh ed25519 key id of form `ed25519:<ms-timestamp>_<rand-hex>`.
///
/// Two rotations within the same wall-clock millisecond would otherwise produce
/// the same id, which silently overwrites the prior entry in `historical_keys`
/// and breaks signature verification of in-flight requests.
fn new_key_id() -> String {
    let ts = Utc::now().timestamp_millis();
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

type CachedKeyEntry = (String, i64);

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
    rotation_config: Arc<RwLock<FederationRotationConfig>>,
}

impl KeyRotationManager {
    pub fn new(pool: &Arc<Pool<Postgres>>, server_name: &str) -> Self {
        Self::with_key_path(pool, server_name, None)
    }

    pub fn with_key_path(
        pool: &Arc<Pool<Postgres>>,
        server_name: &str,
        signing_key_path: Option<String>,
    ) -> Self {
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
            rotation_config: Arc::new(RwLock::new(FederationRotationConfig::default())),
        }
    }

    async fn ensure_signing_keys_table(&self) -> Result<(), ApiError> {
        if self.signing_keys_table_ready.load(Ordering::Relaxed) {
            return Ok(());
        }

        let table_exists: bool = sqlx::query_scalar(
            r"
            SELECT EXISTS (
                SELECT 1
                FROM information_schema.tables
                WHERE table_schema = 'public'
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

        let server_created_index_exists: bool = sqlx::query_scalar(
            r"
            SELECT EXISTS (
                SELECT 1
                FROM pg_indexes
                WHERE schemaname = 'public'
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

        let key_id_index_exists: bool = sqlx::query_scalar(
            r"
            SELECT EXISTS (
                SELECT 1
                FROM pg_indexes
                WHERE schemaname = 'public'
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

        let interval_days: i64 = sqlx::query_scalar(
            r"SELECT value FROM key_rotation_config WHERE key = 'rotation_interval_days'",
        )
        .fetch_optional(&*self.pool)
        .await?
        .flatten()
        .and_then(|v: String| v.parse().ok())
        .unwrap_or(DEFAULT_KEY_ROTATION_INTERVAL_DAYS);

        let threshold_days: i64 = sqlx::query_scalar(
            r"SELECT value FROM key_rotation_config WHERE key = 'rotation_threshold_days'",
        )
        .fetch_optional(&*self.pool)
        .await?
        .flatten()
        .and_then(|v: String| v.parse().ok())
        .unwrap_or(DEFAULT_KEY_ROTATION_THRESHOLD_DAYS);

        let grace_period_minutes: i64 = sqlx::query_scalar(
            r"SELECT value FROM key_rotation_config WHERE key = 'grace_period_minutes'",
        )
        .fetch_optional(&*self.pool)
        .await?
        .flatten()
        .and_then(|v: String| v.parse().ok())
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

        sqlx::query(
            r"
            INSERT INTO key_rotation_config (key, value)
            VALUES ($1, $2)
            ON CONFLICT (key) DO UPDATE SET value = $2
            ",
        )
        .bind(key)
        .bind(value)
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

        let existing_key = sqlx::query_as::<_, SigningKey>(
            r"
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
            ",
        )
        .bind(&self.server_name)
        .bind(Utc::now().timestamp_millis())
        .fetch_optional(&*self.pool)
        .await;

        match existing_key {
            Ok(Some(mut key)) => {
                // Decrypt secret_key if it's encrypted
                if is_encrypted(&key.secret_key) {
                    match &self.master_key {
                        Some(mk) => {
                            key.secret_key = decrypt_key(&key.secret_key, mk)
                                .map_err(|e| ApiError::internal(format!("Failed to decrypt signing key: {e}")))?;
                        }
                        None => {
                            return Err(ApiError::internal(
                                "Signing key is encrypted but no master key is configured"
                            ));
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

    pub async fn export_signing_key_to_file(&self, key: &SigningKey) -> Result<(), ApiError> {
        let path = match &self.signing_key_path {
            Some(p) => p.clone(),
            None => return Ok(()),
        };

        let key_content = format!(
            "ed25519 {} {}",
            key.key_id.strip_prefix("ed25519:").unwrap_or(&key.key_id),
            key.secret_key
        );

        if let Some(parent) = std::path::Path::new(&path).parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        tokio::fs::write(&path, &key_content).await?;
        tracing::debug!("Exported signing key to file: {}", path);
        Ok(())
    }

    pub async fn initialize(&self, secret_key: &str, key_id: &str) -> Result<(), ApiError> {
        self.ensure_signing_keys_table().await?;

        let created_ts = Utc::now().timestamp_millis();
        let interval_days = self.rotation_config.read().await.rotation_interval_days;
        let expires_at =
            (Utc::now() + Duration::days(interval_days)).timestamp_millis();

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

        // Encrypt the secret key for storage if master key is configured
        let stored_secret_key = match &self.master_key {
            Some(mk) => encrypt_key(secret_key, mk)
                .map_err(|e| ApiError::internal(format!("Failed to encrypt signing key: {e}")))?,
            None => {
                tracing::warn!(
                    "Storing federation signing key in plaintext - configure signing_key_master_key for encryption"
                );
                secret_key.to_string()
            }
        };

        let key_json = json!({
            "public_key": signing_key.public_key
        });

        sqlx::query(
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
        )
        .bind(&self.server_name)
        .bind(&signing_key.key_id)
        .bind(&stored_secret_key)
        .bind(&signing_key.public_key)
        .bind(signing_key.created_ts)
        .bind(signing_key.expires_at)
        .bind(key_json)
        .bind(signing_key.created_ts)
        .bind(signing_key.expires_at)
        .execute(&*self.pool)
        .await?;

        if let Err(e) = self.export_signing_key_to_file(&signing_key).await {
            tracing::warn!("Failed to export signing key to file: {}", e);
        }

        Ok(())
    }

    pub async fn should_rotate_keys(&self) -> bool {
        if let Some(key) = &*self.current_key.read().await {
            let now = Utc::now().timestamp_millis();
            let threshold_days = self.rotation_config.read().await.rotation_threshold_days;
            let rotation_threshold = Duration::days(threshold_days).num_milliseconds();
            key.expires_at.saturating_sub(now) <= rotation_threshold
        } else {
            true
        }
    }

    pub async fn rotate_keys(&self, requested_key_id: Option<String>) -> Result<(), ApiError> {
        let current = self.current_key.read().await;
        if let Some(key) = current.as_ref() {
            self.historical_keys
                .write()
                .await
                .insert(key.key_id.clone(), key.clone());
        }
        drop(current);

        let key_id = requested_key_id.unwrap_or_else(new_key_id);
        let (key_id, secret_key) = self.generate_new_key_pair(&key_id);

        self.initialize(&secret_key, &key_id).await?;

        if let Err(e) = self.broadcast_key_change_to_federation().await {
            tracing::warn!("Failed to broadcast key change: {}", e);
        }

        Ok(())
    }

    fn generate_new_key_pair(&self, key_id: &str) -> (String, String) {
        let secret_key =
            base64::engine::general_purpose::STANDARD_NO_PAD.encode(rand::random::<[u8; 32]>());

        (key_id.to_string(), secret_key)
    }

    fn derive_public_key(&self, secret_key: &str) -> Result<String, ApiError> {
        let secret_bytes: [u8; 32] = base64::engine::general_purpose::STANDARD_NO_PAD
            .decode(secret_key)
            .map_err(|_| ApiError::internal("Invalid secret key format"))?
            .try_into()
            .map_err(|_| ApiError::internal("Secret key must be 32 bytes"))?;

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
                if let Ok(()) = self
                    .verify_signature(&current.public_key, signature, content)
                {
                    return Ok(true);
                }
            }
        }

        if let Some(historical) = self.historical_keys.read().await.get(key_id) {
            if self.is_within_grace_period(historical).await {
                if let Ok(()) = self
                    .verify_signature(&historical.public_key, signature, content)
                {
                    return Ok(true);
                }
            }
        }

        self.verify_from_database(key_id, signature, content).await
    }

    async fn is_within_grace_period(&self, key: &SigningKey) -> bool {
        let now = Utc::now().timestamp_millis();
        let grace_minutes = self.rotation_config.read().await.grace_period_minutes;
        let grace_end =
            key.expires_at + Duration::minutes(grace_minutes).num_milliseconds();
        now <= grace_end
    }

    fn verify_signature(
        &self,
        public_key: &str,
        signature: &str,
        content: &[u8],
    ) -> Result<(), ApiError> {
        let pub_key_bytes: [u8; 32] = base64::engine::general_purpose::STANDARD_NO_PAD
            .decode(public_key)
            .map_err(|_| ApiError::internal("Invalid public key format"))?
            .try_into()
            .map_err(|_| ApiError::internal("Public key must be 32 bytes"))?;

        let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&pub_key_bytes)
            .map_err(|_| ApiError::internal("Invalid verifying key"))?;

        let sig_bytes = base64::engine::general_purpose::STANDARD
            .decode(signature)
            .map_err(|_| ApiError::internal("Invalid signature format"))?;

        let dalek_signature = ed25519_dalek::Signature::from_slice(&sig_bytes)
            .map_err(|_| ApiError::internal("Invalid signature length"))?;

        verifying_key
            .verify(content, &dalek_signature)
            .map_err(|_| ApiError::internal("Signature verification failed"))?;

        Ok(())
    }

    async fn verify_from_database(
        &self,
        key_id: &str,
        signature: &str,
        content: &[u8],
    ) -> Result<bool, ApiError> {
        self.ensure_signing_keys_table().await?;

        let key_record: Option<sqlx::postgres::PgRow> = sqlx::query(
            r"
            SELECT public_key, expires_at FROM federation_signing_keys WHERE key_id = $1
            ",
        )
        .bind(key_id)
        .fetch_optional(&*self.pool)
        .await?;

        match key_record {
            Some(record) => {
                let expires_at: i64 = record.get("expires_at");
                let now = Utc::now().timestamp_millis();

                if expires_at > 0 {
                    let grace_minutes = self.rotation_config.read().await.grace_period_minutes;
                    if now
                        > expires_at
                            + Duration::minutes(grace_minutes).num_milliseconds()
                    {
                        return Ok(false);
                    }
                }

                let public_key: String = record.get("public_key");
                self
                    .verify_signature(&public_key, signature, content)
                    .map(|_| true)
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

        sign_json(
            &self.server_name,
            &key_id_for_sign,
            &secret_key,
            &mut response,
        )
        .map_err(|e| ApiError::internal(format!("Failed to sign server keys: {e}")))?;

        Ok(response)
    }

    pub async fn notify_key_change(&self, remote_server: &str) -> Result<(), ApiError> {
        tracing::info!(
            "Notifying {} about key change for server {}",
            remote_server,
            self.server_name
        );
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

    async fn get_known_federation_servers(&self) -> Result<Vec<String>, ApiError> {
        let servers: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT server_name FROM federation_servers WHERE server_name != $1",
        )
        .bind(&self.server_name)
        .fetch_all(&*self.pool)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!("Failed to fetch known federation servers: {e}");
            Vec::new()
        });
        Ok(servers.into_iter().map(|(s,)| s).collect())
    }

    /// Revoke a specific key by marking it as expired in the database
    pub async fn revoke_key(&self, key_id: &str, reason: Option<&str>) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        let now = chrono::Utc::now().timestamp_millis();

        // Build the key_json update with revoked metadata
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

        // Mark the key as expired and add revocation metadata
        let result = sqlx::query(
            r"UPDATE federation_signing_keys
               SET expires_at = $1,
                   key_json = COALESCE(key_json, '{}'::jsonb) || $2::jsonb
               WHERE key_id = $3 AND server_name = $4 AND (expires_at = 0 OR expires_at > $1)",
        )
        .bind(now)
        .bind(serde_json::to_string(&key_json_update).unwrap_or_else(|_| "{}".to_string()))
        .bind(key_id)
        .bind(&self.server_name)
        .execute(&*self.pool)
        .await?;

        let revoked = result.rows_affected();

        // If the revoked key is the current key, clear it from cache and trigger rotation
        {
            let current = self.current_key.read().await;
            if let Some(ref current_key) = *current {
                if current_key.key_id == key_id {
                    drop(current);
                    let _ = self.rotate_keys(None).await;
                }
            }
        }

        // Remove from historical keys cache
        {
            let mut historical = self.historical_keys.write().await;
            historical.remove(key_id);
        }

        Ok(revoked)
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;
    use std::env;

    async fn create_test_pool() -> Option<Arc<PgPool>> {
        let database_url = crate::test_config::test_database_url();
        // Use connect_lazy to allow creating the pool without an immediate connection check
        match PgPool::connect_lazy(&database_url) {
            Ok(pool) => Some(Arc::new(pool)),
            Err(e) => {
                tracing::warn!("Failed to create lazy pool connection: {e}");
                None
            }
        }
    }

    async fn setup_test_database() -> Option<Arc<PgPool>> {
        let database_url = env::var("TEST_DATABASE_URL")
            .or_else(|_| env::var("DATABASE_URL"))
            .unwrap_or_else(|_| crate::test_config::test_database_url());

        let pool = match sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(std::time::Duration::from_secs(10))
            .connect(&database_url)
            .await
        {
            Ok(pool) => Arc::new(pool),
            Err(error) => {
                tracing::warn!(
                    "Skipping key rotation database tests because test database is unavailable: {error}"
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

    /// Generate a proper Ed25519 keypair for testing.
    /// SAFETY: Test-only key, never used in production.
    fn generate_test_signing_key(seed_byte: u8) -> (String, String) {
        let seed = [seed_byte; 32];
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&seed);
        let verifying_key = signing_key.verifying_key();
        let secret_b64 =
            base64::engine::general_purpose::STANDARD_NO_PAD.encode(signing_key.as_bytes());
        let public_b64 =
            base64::engine::general_purpose::STANDARD_NO_PAD.encode(verifying_key.as_bytes());
        (secret_b64, public_b64)
    }

    #[tokio::test]
    async fn test_grace_period() {
        let pool = match create_test_pool().await {
            Some(pool) => pool,
            None => return,
        };
        let manager = KeyRotationManager::new(&pool, "test.example.com");

        let (test_secret, test_public) = generate_test_signing_key(0x01);
        let expired_key = SigningKey {
            server_name: "test.example.com".to_string(),
            key_id: "ed25519:expired".to_string(),
            // SAFETY: Test-only key, never used in production
            secret_key: test_secret,
            public_key: test_public,
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
        assert_eq!(DEFAULT_KEY_ROTATION_INTERVAL_DAYS, 7);
        assert_eq!(DEFAULT_KEY_GRACE_PERIOD_MINUTES, 5);
    }

    #[test]
    fn test_signing_key_creation() {
        let (test_secret, test_public) = generate_test_signing_key(0x02);
        let key = SigningKey {
            server_name: "test.example.com".to_string(),
            key_id: "ed25519:test".to_string(),
            // SAFETY: Test-only key, never used in production
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
            // SAFETY: Test-only key, never used in production
            secret_key: test_secret.clone(),
            public_key: test_public.clone(),
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
        let pool = match create_test_pool().await {
            Some(pool) => pool,
            None => return,
        };
        let manager = KeyRotationManager::new(&pool, "test.example.com");

        let current = manager.get_current_key().await.unwrap();
        assert!(current.is_none());
    }

    #[tokio::test]
    async fn test_should_rotate_keys_no_key() {
        let pool = match create_test_pool().await {
            Some(pool) => pool,
            None => return,
        };
        let manager = KeyRotationManager::new(&pool, "test.example.com");

        let should_rotate = manager.should_rotate_keys().await;
        assert!(should_rotate);
    }

    #[tokio::test]
    async fn test_should_rotate_keys_with_fresh_key() {
        let pool = match create_test_pool().await {
            Some(pool) => pool,
            None => return,
        };
        let manager = KeyRotationManager::new(&pool, "test.example.com");

        let future_expires = (Utc::now() + Duration::days(30)).timestamp_millis();
        let (test_secret, test_public) = generate_test_signing_key(0x04);

        {
            let mut current = manager.current_key.write().await;
            *current = Some(SigningKey {
                server_name: "test.example.com".to_string(),
                key_id: "ed25519:test".to_string(),
                // SAFETY: Test-only key, never used in production
                secret_key: test_secret,
                public_key: test_public,
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
        let pool = match create_test_pool().await {
            Some(pool) => pool,
            None => return,
        };
        let manager = KeyRotationManager::new(&pool, "test.example.com");

        let soon_expires = (Utc::now() + Duration::hours(12)).timestamp_millis();
        let (test_secret, test_public) = generate_test_signing_key(0x05);

        {
            let mut current = manager.current_key.write().await;
            *current = Some(SigningKey {
                server_name: "test.example.com".to_string(),
                key_id: "ed25519:test".to_string(),
                // SAFETY: Test-only key, never used in production
                secret_key: test_secret,
                public_key: test_public,
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
        let (test_secret, test_public) = generate_test_signing_key(0x06);
        let key_json = json!({
            "secret_key": test_secret,
            "public_key": test_public
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
        // SAFETY: Test-only key, never used in production
        .bind(&test_secret)
        .bind(&test_public)
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
        assert_eq!(current.secret_key, test_secret);
        assert_eq!(current.public_key, test_public);
        assert_eq!(current.created_ts, now);
        assert_eq!(current.expires_at, expires_at);
        assert_eq!(current.key_json, key_json);
        assert_eq!(current.ts_added_ms, now);
        assert_eq!(current.ts_valid_until_ms, expires_at);
    }

    #[tokio::test]
    async fn test_cache_historical_key() {
        let pool = match create_test_pool().await {
            Some(pool) => pool,
            None => return,
        };
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
        let pool = match create_test_pool().await {
            Some(pool) => pool,
            None => return,
        };
        let manager = KeyRotationManager::new(&pool, "test.example.com");

        let result = manager.get_server_keys_response().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_server_keys_response_with_key() {
        let pool = match create_test_pool().await {
            Some(pool) => pool,
            None => return,
        };
        let manager = KeyRotationManager::new(&pool, "test.example.com");

        let test_signing_key = ed25519_dalek::SigningKey::from_bytes(&[42u8; 32]);
        let test_verifying_key = test_signing_key.verifying_key();
        let secret_key_b64 =
            base64::engine::general_purpose::STANDARD_NO_PAD.encode(test_signing_key.as_bytes());
        let public_key_b64 =
            base64::engine::general_purpose::STANDARD_NO_PAD.encode(test_verifying_key.as_bytes());

        {
            let mut current = manager.current_key.write().await;
            *current = Some(SigningKey {
                server_name: "test.example.com".to_string(),
                key_id: "ed25519:test".to_string(),
                secret_key: secret_key_b64,
                public_key: public_key_b64,
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
        assert!(result["signatures"].is_object());
    }

    #[tokio::test]
    async fn test_get_server_keys_response_with_historical_keys() {
        let pool = match create_test_pool().await {
            Some(pool) => pool,
            None => return,
        };
        let manager = KeyRotationManager::new(&pool, "test.example.com");

        let test_signing_key = ed25519_dalek::SigningKey::from_bytes(&[42u8; 32]);
        let test_verifying_key = test_signing_key.verifying_key();
        let secret_key_b64 =
            base64::engine::general_purpose::STANDARD_NO_PAD.encode(test_signing_key.as_bytes());
        let public_key_b64 =
            base64::engine::general_purpose::STANDARD_NO_PAD.encode(test_verifying_key.as_bytes());

        let old_signing_key = ed25519_dalek::SigningKey::from_bytes(&[99u8; 32]);
        let old_verifying_key = old_signing_key.verifying_key();
        let old_secret_b64 =
            base64::engine::general_purpose::STANDARD_NO_PAD.encode(old_signing_key.as_bytes());
        let old_public_b64 =
            base64::engine::general_purpose::STANDARD_NO_PAD.encode(old_verifying_key.as_bytes());

        {
            let mut current = manager.current_key.write().await;
            *current = Some(SigningKey {
                server_name: "test.example.com".to_string(),
                key_id: "ed25519:current".to_string(),
                secret_key: secret_key_b64,
                public_key: public_key_b64,
                created_ts: Utc::now().timestamp_millis(),
                expires_at: (Utc::now() + Duration::days(7)).timestamp_millis(),
                key_json: serde_json::json!({}),
                ts_added_ms: Utc::now().timestamp_millis(),
                ts_valid_until_ms: (Utc::now() + Duration::days(7)).timestamp_millis(),
            });
        }

        {
            let mut historical = manager.historical_keys.write().await;
            historical.insert(
                "ed25519:old".to_string(),
                SigningKey {
                    server_name: "test.example.com".to_string(),
                    key_id: "ed25519:old".to_string(),
                    secret_key: old_secret_b64,
                    public_key: old_public_b64,
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
        assert!(result["signatures"].is_object());
    }

    #[tokio::test]
    async fn test_set_rotation_enabled() {
        let pool = match create_test_pool().await {
            Some(pool) => pool,
            None => return,
        };
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
        let pool = match create_test_pool().await {
            Some(pool) => pool,
            None => return,
        };
        let manager = KeyRotationManager::new(&pool, "test.example.com");

        let status = manager.get_rotation_status().await;
        assert_eq!(status["has_current_key"], false);
        assert_eq!(status["should_rotate"], true);
        assert_eq!(status["server_name"], "test.example.com");
    }

    #[tokio::test]
    async fn test_get_rotation_status_with_key() {
        let pool = match create_test_pool().await {
            Some(pool) => pool,
            None => return,
        };
        let manager = KeyRotationManager::new(&pool, "test.example.com");
        let (test_secret, test_public) = generate_test_signing_key(0x07);

        // Set a key
        {
            let mut current = manager.current_key.write().await;
            *current = Some(SigningKey {
                server_name: "test.example.com".to_string(),
                key_id: "ed25519:test".to_string(),
                // SAFETY: Test-only key, never used in production
                secret_key: test_secret,
                public_key: test_public,
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
        let pool = match create_test_pool().await {
            Some(pool) => pool,
            None => return,
        };
        let manager = KeyRotationManager::new(&pool, "test.example.com");

        let result = manager.derive_public_key("not-valid-base64!!!");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_derive_public_key_wrong_length() {
        let pool = match create_test_pool().await {
            Some(pool) => pool,
            None => return,
        };
        let manager = KeyRotationManager::new(&pool, "test.example.com");

        // 16 bytes instead of 32
        let short_key = base64::engine::general_purpose::STANDARD_NO_PAD.encode(b"short_key");
        let result = manager.derive_public_key(&short_key);
        assert!(result.is_err());
    }

    #[allow(clippy::redundant_clone)]
    #[tokio::test]
    async fn test_key_rotation_manager_clone() {
        let pool = match create_test_pool().await {
            Some(pool) => pool,
            None => return,
        };
        let manager = KeyRotationManager::new(&pool, "test.example.com");
        let _cloned = manager.clone();
        // Should compile - Verify clone works
    }
}
