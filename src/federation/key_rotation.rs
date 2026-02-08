use base64::Engine;
use chrono::{Duration, Utc};
use ed25519_dalek::Verifier;
use serde_json::json;
use sqlx::{Pool, Postgres};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration as TokioDuration};

const KEY_ROTATION_INTERVAL_DAYS: i64 = 7;
const KEY_GRACE_PERIOD_MINUTES: i64 = 5;

#[derive(Debug, Clone)]
pub struct SigningKey {
    pub key_id: String,
    pub secret_key: String,
    pub public_key: String,
    pub created_at: i64,
    pub expires_at: i64,
}

#[derive(Debug, Clone)]
pub struct KeyRotationManager {
    pool: Arc<Pool<Postgres>>,
    memory_cache: Arc<RwLock<HashMap<String, (String, i64)>>>,
    current_key: Arc<RwLock<Option<SigningKey>>>,
    historical_keys: Arc<RwLock<HashMap<String, SigningKey>>>,
    server_name: String,
    rotation_enabled: Arc<RwLock<bool>>,
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
        }
    }

    pub async fn start_auto_rotation(&self) {
        let mut interval = interval(TokioDuration::from_secs(3600));
        let manager = self.clone();

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

    pub async fn initialize(&self, secret_key: &str, key_id: &str) -> Result<(), anyhow::Error> {
        let created_at = Utc::now().timestamp_millis();
        let expires_at =
            (Utc::now() + Duration::days(KEY_ROTATION_INTERVAL_DAYS)).timestamp_millis();

        let public_key = self.derive_public_key(secret_key).await?;

        let signing_key = SigningKey {
            key_id: key_id.to_string(),
            secret_key: secret_key.to_string(),
            public_key,
            created_at,
            expires_at,
        };

        *self.current_key.write().await = Some(signing_key.clone());

        let key_json = json!({
            "secret_key": signing_key.secret_key,
            "public_key": signing_key.public_key
        });

        sqlx::query(
            r#"
            INSERT INTO federation_signing_keys (server_name, key_id, secret_key, public_key, created_at, expires_at, key_json, ts_added_ms, ts_valid_until_ms)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (server_name, key_id) DO UPDATE SET
                secret_key = EXCLUDED.secret_key,
                public_key = EXCLUDED.public_key,
                created_at = EXCLUDED.created_at,
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
        .bind(signing_key.created_at)
        .bind(signing_key.expires_at)
        .bind(key_json)
        .bind(signing_key.created_at)
        .bind(signing_key.expires_at)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn should_rotate_keys(&self) -> bool {
        if let Some(key) = &*self.current_key.read().await {
            let now = Utc::now().timestamp_millis();
            let days_until_expiry = (key.expires_at - now) / (24 * 60 * 60 * 1000);
            days_until_expiry < 7
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
        let key_record = sqlx::query!(
            r#"
            SELECT public_key, expires_at FROM federation_signing_keys WHERE key_id = $1
            "#,
            key_id
        )
        .fetch_optional(&*self.pool)
        .await?;

        match key_record {
            Some(record) => {
                let expires_at = record.expires_at;
                let now = Utc::now().timestamp_millis();

                if now > expires_at + Duration::minutes(KEY_GRACE_PERIOD_MINUTES).num_milliseconds()
                {
                    return Ok(false);
                }

                return self
                    .verify_signature(&record.public_key, signature, content)
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

    fn generate_valid_test_key() -> String {
        use rand::RngCore;
        let mut rng = rand::thread_rng();
        let mut secret_bytes = [0u8; 32];
        rng.fill_bytes(&mut secret_bytes);
        base64::engine::general_purpose::STANDARD_NO_PAD.encode(&secret_bytes)
    }

    #[tokio::test]
    #[ignore] // Requires running database
    async fn test_key_rotation_initialization() {
        let pool = create_test_pool().await;
        let manager = KeyRotationManager::new(&pool, "test.example.com");

        let valid_key = generate_valid_test_key();
        manager
            .initialize(&valid_key, "ed25519:test")
            .await
            .unwrap();

        let current = manager.get_current_key().await.unwrap().unwrap();
        assert_eq!(current.key_id, "ed25519:test");
        assert!(!current.public_key.is_empty());
    }

    #[tokio::test]
    #[ignore] // Requires running database
    async fn test_should_rotate_keys() {
        let pool = create_test_pool().await;
        let manager = KeyRotationManager::new(&pool, "test.example.com");

        assert!(manager.should_rotate_keys().await);

        let valid_key = generate_valid_test_key();
        manager
            .initialize(&valid_key, "ed25519:test")
            .await
            .unwrap();
        assert!(!manager.should_rotate_keys().await);
    }

    #[tokio::test]
    async fn test_grace_period() {
        let pool = create_test_pool().await;
        let manager = KeyRotationManager::new(&pool, "test.example.com");

        let expired_key = SigningKey {
            key_id: "ed25519:expired".to_string(),
            secret_key: "test".to_string(),
            public_key: "test".to_string(),
            created_at: 0,
            expires_at: Utc::now().timestamp_millis() - 1000,
        };

        assert!(manager.is_within_grace_period(&expired_key).await);
    }
}
