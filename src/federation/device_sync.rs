use crate::cache::CacheManager;
use crate::common::ApiError;
use chrono::{Duration, TimeZone, Utc};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{Pool, Postgres};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

const DEVICE_SYNC_CACHE_TTL: u64 = 3600;
const DEVICE_KEY_EXPIRY_DAYS: i64 = 365;

type DeviceCacheEntry = (Vec<DeviceInfo>, u128);
type DeviceCache = HashMap<String, DeviceCacheEntry>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub device_id: String,
    pub user_id: String,
    pub keys: Option<Value>,
    pub device_display_name: Option<String>,
    pub last_seen_ts: Option<i64>,
    pub last_seen_ip: Option<String>,
    pub is_blocked: bool,
    pub verified: bool,
}

#[derive(Clone)]
pub struct DeviceSyncManager {
    pool: Arc<Pool<Postgres>>,
    http_client: Client,
    local_cache: Arc<RwLock<DeviceCache>>,
    cache_manager: Option<Arc<CacheManager>>,
}

impl DeviceSyncManager {
    pub fn new(pool: &Arc<Pool<Postgres>>, cache_manager: Option<Arc<CacheManager>>) -> Self {
        let http_client = Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_else(|e| {
                tracing::warn!("Failed to build HTTP client, using default: {}", e);
                Client::new()
            });

        Self {
            pool: pool.clone(),
            http_client,
            local_cache: Arc::new(RwLock::new(HashMap::new())),
            cache_manager,
        }
    }

    async fn get_cached_devices(&self, origin: &str, user_id: &str) -> Option<Vec<DeviceInfo>> {
        let cache_key = format!("remote_devices:{}:{}", origin, user_id);

        if let Some(cache) = &self.cache_manager {
            if let Ok(Some(devices_json)) = cache.get::<String>(&cache_key).await {
                if let Ok(devices) = serde_json::from_str::<Vec<DeviceInfo>>(&devices_json) {
                    tracing::debug!("Redis cache hit for remote devices: {}@{}", user_id, origin);
                    return Some(devices);
                }
            }
        }

        if let Some((devices, expiry)) = self.local_cache.read().await.get(&cache_key) {
            let current_time = std::time::SystemTime::UNIX_EPOCH
                .elapsed()
                .map(|d| d.as_millis())
                .unwrap_or(u128::MAX);

            if *expiry > current_time {
                tracing::debug!("Local cache hit for remote devices: {}@{}", user_id, origin);
                return Some(devices.clone());
            }
        }

        None
    }

    async fn cache_devices(&self, origin: &str, user_id: &str, devices: &[DeviceInfo]) {
        let cache_key = format!("remote_devices:{}:{}", origin, user_id);
        let expiry = std::time::SystemTime::UNIX_EPOCH
            .elapsed()
            .map(|d| d.as_millis())
            .unwrap_or(u128::MAX)
            + DEVICE_SYNC_CACHE_TTL as u128 * 1000;

        if let Some(cache) = &self.cache_manager {
            if let Ok(devices_json) = serde_json::to_string(devices) {
                let _ = cache
                    .set(&cache_key, devices_json, DEVICE_SYNC_CACHE_TTL)
                    .await;
            }
        }

        let mut local = self.local_cache.write().await;
        local.insert(cache_key, (devices.to_vec(), expiry));
    }

    pub async fn sync_devices_from_remote(
        &self,
        origin: &str,
        user_id: &str,
    ) -> Result<Vec<DeviceInfo>, ApiError> {
        if let Some(devices) = self.get_cached_devices(origin, user_id).await {
            return Ok(devices);
        }

        let urls = vec![
            format!(
                "https://{}/_matrix/federation/v1/user/devices/{}",
                origin, user_id
            ),
            format!(
                "http://{}:8448/_matrix/federation/v1/user/devices/{}",
                origin, user_id
            ),
        ];

        for url in urls {
            match self.fetch_devices_from_url(&url).await {
                Ok(devices) => {
                    self.cache_devices(origin, user_id, &devices).await;
                    return Ok(devices);
                }
                Err(e) => {
                    tracing::warn!("Failed to fetch devices from {}: {}", url, e);
                    continue;
                }
            }
        }

        Err(ApiError::not_found(format!(
            "Failed to fetch devices for user {} from {}",
            user_id, origin
        )))
    }

    async fn fetch_devices_from_url(&self, url: &str) -> Result<Vec<DeviceInfo>, ApiError> {
        let response = self
            .http_client
            .get(url)
            .send()
            .await
            .map_err(|e| ApiError::internal(format!("HTTP request failed: {}", e)))?;

        if response.status() == StatusCode::NOT_FOUND {
            return Ok(vec![]);
        }

        if !response.status().is_success() {
            return Err(ApiError::internal(format!(
                "Remote server returned error: {}",
                response.status()
            )));
        }

        let body: Value = response
            .json()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to parse response: {}", e)))?;

        let devices_json = body
            .get("devices")
            .and_then(|v| v.as_array())
            .ok_or_else(|| ApiError::internal("Invalid devices response".to_string()))?;

        let devices: Vec<DeviceInfo> = devices_json
            .iter()
            .map(|d| DeviceInfo {
                device_id: d
                    .get("device_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                user_id: d
                    .get("user_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                keys: d.get("keys").cloned(),
                device_display_name: d
                    .get("device_display_name")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                last_seen_ts: d.get("last_seen_ts").and_then(|v| v.as_i64()),
                last_seen_ip: d
                    .get("last_seen_ip")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                is_blocked: d
                    .get("is_blocked")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                verified: d.get("verified").and_then(|v| v.as_bool()).unwrap_or(false),
            })
            .filter(|d| !d.device_id.is_empty())
            .collect();

        Ok(devices)
    }

    pub async fn notify_device_revocation(
        &self,
        origin: &str,
        user_id: &str,
        device_id: &str,
    ) -> Result<(), ApiError> {
        let payload = json!({
            "type": "m.device_list_update",
            "sender": user_id,
            "content": {
                "device_id": device_id,
                "deleted": true
            }
        });

        let urls = vec![
            format!(
                "https://{}/_matrix/federation/v1/send/{}",
                origin,
                uuid::Uuid::new_v4()
            ),
            format!(
                "http://{}:8448/_matrix/federation/v1/send/{}",
                origin,
                uuid::Uuid::new_v4()
            ),
        ];

        for url in urls {
            match self.http_client.put(&url).json(&payload).send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        tracing::info!("Successfully notified device revocation to {}", origin);
                        return Ok(());
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to notify revocation to {}: {}", url, e);
                    continue;
                }
            }
        }

        Err(ApiError::internal(
            "Failed to notify device revocation to remote server".to_string(),
        ))
    }

    pub async fn get_local_devices(&self, user_id: &str) -> Result<Vec<DeviceInfo>, ApiError> {
        let devices: Vec<DeviceRow> = sqlx::query_as(
            r#"
            SELECT device_id, user_id, display_name as device_display_name, 
                   device_key as keys, last_seen_ts, last_seen_ip,
                   FALSE as is_blocked, FALSE as verified
            FROM devices WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to fetch devices: {}", e)))?;

        Ok(devices
            .into_iter()
            .map(|d| DeviceInfo {
                device_id: d.device_id,
                user_id: d.user_id,
                keys: d.keys,
                device_display_name: d.device_display_name,
                last_seen_ts: d.last_seen_ts,
                last_seen_ip: d.last_seen_ip,
                is_blocked: d.is_blocked,
                verified: d.verified,
            })
            .collect())
    }

    pub async fn verify_device_keys_signature(
        &self,
        origin: &str,
        device: &DeviceInfo,
    ) -> Result<bool, ApiError> {
        if let Some(ref keys) = device.keys {
            if let Some(user_signatures) = keys.get("user_signatures") {
                if let Some(sigs) = user_signatures.as_object() {
                    if sigs.contains_key(origin) {
                        return Ok(true);
                    }
                }
            }
        }

        Ok(false)
    }

    pub fn is_device_key_expired(&self, device: &DeviceInfo) -> bool {
        if let Some(last_seen) = device.last_seen_ts {
            let last_seen_time = Utc
                .timestamp_millis_opt(last_seen)
                .earliest()
                .unwrap_or(Utc::now());
            let expiry_date = last_seen_time + Duration::days(DEVICE_KEY_EXPIRY_DAYS);
            expiry_date < Utc::now()
        } else {
            true
        }
    }

    pub async fn cleanup_expired_devices(&self, user_id: &str) -> Result<u64, ApiError> {
        let expiry_threshold = Utc::now() - Duration::days(DEVICE_KEY_EXPIRY_DAYS);

        let result = sqlx::query(
            r#"
            DELETE FROM devices 
            WHERE user_id = $1 
            AND (last_seen_ts IS NULL OR last_seen_ts < $2)
            "#,
        )
        .bind(user_id)
        .bind(expiry_threshold.timestamp_millis())
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to cleanup expired devices: {}", e)))?;

        let deleted_count = result.rows_affected();
        if deleted_count > 0 {
            tracing::info!(
                "Cleaned up {} expired devices for user {}",
                deleted_count,
                user_id
            );
            self.invalidate_user_devices_cache(user_id).await;
        }

        Ok(deleted_count)
    }

    pub async fn sync_device_keys_with_expiry_check(
        &self,
        origin: &str,
        user_id: &str,
    ) -> Result<Vec<DeviceInfo>, ApiError> {
        let devices = self.sync_devices_from_remote(origin, user_id).await?;
        let original_count = devices.len();

        let valid_devices: Vec<DeviceInfo> = devices
            .into_iter()
            .filter(|device| !self.is_device_key_expired(device))
            .collect();

        if valid_devices.len() != original_count {
            tracing::debug!(
                "Filtered out {} expired devices for user {}@{}",
                original_count - valid_devices.len(),
                user_id,
                origin
            );
        }

        Ok(valid_devices)
    }

    pub async fn revoke_device(&self, device_id: &str, user_id: &str) -> Result<(), ApiError> {
        sqlx::query(
            r#"
            UPDATE devices SET 
                device_key = NULL,
                last_seen_ts = NULL
            WHERE device_id = $1 AND user_id = $2
            "#,
        )
        .bind(device_id)
        .bind(user_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to revoke device: {}", e)))?;

        let cache_pattern = format!("remote_devices:*:{}", user_id);
        let mut local = self.local_cache.write().await;
        local.retain(|key, _| !key.starts_with(&cache_pattern));

        if let Some(cache) = &self.cache_manager {
            let _ = cache.delete(&cache_pattern).await;
        }

        Ok(())
    }

    pub async fn invalidate_user_devices_cache(&self, user_id: &str) {
        let cache_pattern = format!("remote_devices:*:{}", user_id);
        let mut local = self.local_cache.write().await;
        local.retain(|key, _| !key.starts_with(&cache_pattern));

        if let Some(cache) = &self.cache_manager {
            let _ = cache.delete(&cache_pattern).await;
        }

        tracing::info!("Invalidated device cache for user: {}", user_id);
    }
}

#[derive(sqlx::FromRow)]
struct DeviceRow {
    device_id: String,
    user_id: String,
    keys: Option<Value>,
    device_display_name: Option<String>,
    last_seen_ts: Option<i64>,
    last_seen_ip: Option<String>,
    is_blocked: bool,
    verified: bool,
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
        match PgPool::connect(&database_url).await {
            Ok(pool) => Arc::new(pool),
            Err(_) => {
                panic!("Failed to connect to test database");
            }
        }
    }

    #[tokio::test]
    async fn test_device_sync_cache() {
        let pool = create_test_pool().await;
        let manager = DeviceSyncManager::new(&pool, None);

        let devices = manager
            .get_local_devices("@test:example.com")
            .await
            .unwrap();
        assert!(devices.is_empty());
    }

    #[tokio::test]
    async fn test_device_revocation() {
        let pool = create_test_pool().await;
        let manager = DeviceSyncManager::new(&pool, None);

        let result = manager
            .revoke_device("DEVICE123", "@test:example.com")
            .await;
        assert!(result.is_ok());
    }
}
