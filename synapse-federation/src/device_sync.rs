use chrono::{Duration, TimeZone, Utc};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{Pool, Postgres};
use std::collections::HashMap;
use std::sync::Arc;
use synapse_cache::CacheManager;
use synapse_common::background_job::BackgroundJob;
use synapse_common::http_client::pinned_client_for_url;
use synapse_common::security;
use synapse_common::task_queue::RedisTaskQueue;
use synapse_common::ApiError;
use tokio::sync::RwLock;

const DEVICE_SYNC_CACHE_TTL: u64 = 3600;
const DEVICE_KEY_EXPIRY_DAYS: i64 = 365;

type DeviceCacheEntry = (Vec<DeviceInfo>, u128);
type DeviceCache = HashMap<String, DeviceCacheEntry>;

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `DeviceInfo` type.
pub struct DeviceInfo {
    /// The `device_id` field.
    /// The `user_id` field.
    /// The `keys` field.
    /// The `device_display_name` field.
    /// The `last_seen_ts` field.
    pub device_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `is_blocked` field.
    /// The `verified` field.
    /// The `device_display_name` field.
    /// The `last_seen_ts` field.
    pub keys: Option<Value>,
    /// The `device_display_name` field.
    pub device_display_name: Option<String>,
    /// The `is_blocked` field.
    /// The `verified` field.
    pub last_seen_ts: Option<i64>,
    #[serde(skip)]
    /// The `is_blocked` field.
    /// The `verified` field.
    pub last_seen_ip: Option<String>,
    /// The `is_blocked` field.
    /// The `verified` field.
    pub is_blocked: bool,
    /// The `verified` field.
    pub verified: bool,
}

#[derive(Clone)]
/// The `DeviceSyncManager` type.
pub struct DeviceSyncManager {
    pool: Arc<Pool<Postgres>>,
    http_client: Client,
    local_cache: Arc<RwLock<DeviceCache>>,
    cache_manager: Option<Arc<CacheManager>>,
    task_queue: Option<Arc<RedisTaskQueue>>,
}

/// Implementation of [`DeviceSyncManager`] methods.
impl DeviceSyncManager {
    /// See [`new`.
    pub fn new(
        pool: &Arc<Pool<Postgres>>,
        cache_manager: Option<Arc<CacheManager>>,
        task_queue: Option<Arc<RedisTaskQueue>>,
    ) -> Self {
        let http_client = Client::builder().timeout(std::time::Duration::from_secs(10)).build().unwrap_or_else(|e| {
            tracing::warn!("Failed to build HTTP client, using default: {}", e);
            // F-1: 回退到共享默认 client（带超时），不再退化为无超时 Client::new()
            synapse_common::http_client::default_client()
        });

        Self {
            pool: pool.clone(),
            http_client,
            local_cache: Arc::new(RwLock::new(HashMap::new())),
            cache_manager,
            task_queue,
        }
    }

    async fn get_cached_devices(&self, origin: &str, user_id: &str) -> Option<Vec<DeviceInfo>> {
        let cache_key = format!("remote_devices:{origin}:{user_id}");

        if let Some(cache) = &self.cache_manager {
            if let Ok(Some(devices_json)) = cache.get::<String>(&cache_key).await {
                if let Ok(devices) = serde_json::from_str::<Vec<DeviceInfo>>(&devices_json) {
                    tracing::debug!("Redis cache hit for remote devices: {}@{}", user_id, origin);
                    return Some(devices);
                }
            }
        }

        if let Some((devices, expiry)) = self.local_cache.read().await.get(&cache_key) {
            let current_time = std::time::SystemTime::UNIX_EPOCH.elapsed().map(|d| d.as_millis()).unwrap_or(u128::MAX);

            if *expiry > current_time {
                tracing::debug!("Local cache hit for remote devices: {}@{}", user_id, origin);
                return Some(devices.clone());
            }
        }

        None
    }

    async fn cache_devices(&self, origin: &str, user_id: &str, devices: &[DeviceInfo]) {
        let cache_key = format!("remote_devices:{origin}:{user_id}");
        let expiry = std::time::SystemTime::UNIX_EPOCH.elapsed().map(|d| d.as_millis()).unwrap_or(u128::MAX)
            + DEVICE_SYNC_CACHE_TTL as u128 * 1000;

        if let Some(cache) = &self.cache_manager {
            if let Ok(devices_json) = serde_json::to_string(devices) {
                if let Err(e) = cache.set(&cache_key, devices_json, DEVICE_SYNC_CACHE_TTL).await {
                    ::tracing::warn!(
                        origin = %origin,
                        user_id = %user_id,
                        cache_key = %cache_key,
                        error = %e,
                        "Failed to cache remote device sync payload"
                    );
                }
            }
        }

        let mut local = self.local_cache.write().await;
        local.insert(cache_key, (devices.to_vec(), expiry));
    }

    /// See [`sync_devices_from_remote`.
    pub async fn sync_devices_from_remote(&self, origin: &str, user_id: &str) -> Result<Vec<DeviceInfo>, ApiError> {
        if let Some(devices) = self.get_cached_devices(origin, user_id).await {
            return Ok(devices);
        }

        let urls = vec![format!("https://{}/_matrix/federation/v1/user/devices/{}", origin, user_id)];

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

        Err(ApiError::not_found(format!("Failed to fetch devices for user {user_id} from {origin}")))
    }

    async fn fetch_devices_from_url(&self, url: &str) -> Result<Vec<DeviceInfo>, ApiError> {
        // SSRF 防护：解析主机并校验所有解析结果不在私有/链路本地网段内，
        // 然后用已验证的 IP 集合构造钉扎客户端（杜绝 DNS 重绑定）。
        let (host, ips) = security::check_url_and_resolve(url, &security::ssrf_blacklist())
            .map_err(|e| ApiError::bad_request(format!("SSRF check failed: {e}")))?;

        let pinned = pinned_client_for_url(
            url,
            &ips,
            std::time::Duration::from_secs(15),
            true, // no_redirect — federation key fetch never follows redirects
        )
        .map_err(|e| ApiError::internal_with_context("Failed to build pinned client", &e))?;

        tracing::debug!(%host, ips = ?ips.len(), "Fetching remote devices with SSRF-pinned client");

        let response =
            pinned.get(url).send().await.map_err(|e| ApiError::internal_with_cause("HTTP request failed", e))?;

        if response.status() == StatusCode::NOT_FOUND {
            return Ok(vec![]);
        }

        if !response.status().is_success() {
            return Err(ApiError::internal_with_context("Remote server returned error", &response.status()));
        }

        let body: Value =
            response.json().await.map_err(|e| ApiError::internal_with_cause("Failed to parse response", e))?;

        let devices_json = body
            .get("devices")
            .and_then(|v| v.as_array())
            .ok_or_else(|| ApiError::internal("Invalid devices response".to_string()))?;

        let devices: Vec<DeviceInfo> = devices_json
            .iter()
            .map(|d| DeviceInfo {
                device_id: d.get("device_id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                user_id: d.get("user_id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                keys: d.get("keys").cloned(),
                device_display_name: None,
                last_seen_ts: None,
                last_seen_ip: None,
                is_blocked: false,
                verified: false,
            })
            .filter(|d| !d.device_id.is_empty())
            .collect();

        Ok(devices)
    }

    /// See [`notify_device_revocation`.
    pub async fn notify_device_revocation(&self, origin: &str, user_id: &str, device_id: &str) -> Result<(), ApiError> {
        if let Some(queue) = &self.task_queue {
            let payload = json!({
                "type": "m.device_list_update",
                "sender": user_id,
                "content": {
                    "device_id": device_id,
                    "deleted": true
                },
                "destination": origin
            });

            let job = BackgroundJob::Generic { name: "notify_device_revocation".to_string(), payload };

            if let Err(e) = queue.submit(job).await {
                tracing::warn!("Failed to submit revocation task: {}", e);
            } else {
                tracing::info!("Submitted device revocation task for {} to {}", device_id, origin);
                return Ok(());
            }
        }

        let payload = json!({
            "type": "m.device_list_update",
            "sender": user_id,
            "content": {
                "device_id": device_id,
                "deleted": true
            }
        });

        let urls = vec![format!("https://{}/_matrix/federation/v1/send/{}", origin, uuid::Uuid::new_v4())];

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

        Err(ApiError::internal("Failed to notify device revocation to remote server".to_string()))
    }

    /// See [`get_local_devices`.
    pub async fn get_local_devices(&self, user_id: &str) -> Result<Vec<DeviceInfo>, ApiError> {
        // R4 ①：`FALSE as is_blocked` / `FALSE as verified` 是**字面量**（无关系来源）⇒ sqlx 推成
        // 可空，而 `DeviceRow` 的这两个字段是 `bool` ⇒ 需要 `AS "col!"` 断言。谁保证非空：两个值
        // 是常量 `FALSE`，sqlx 的 `bool` 解码对 NULL 会失败，这里的来源根本不可能是 NULL。
        // R6 ⑤：`query_as!` 按**列名**构造结构体 ⇒ 别名必须等于字段名（`device_display_name`/`keys`）。
        let devices: Vec<DeviceRow> = sqlx::query_as!(
            DeviceRow,
            r#"
            SELECT device_id, user_id, display_name as device_display_name,
                   device_key as keys, last_seen_ts, last_seen_ip,
                   FALSE as "is_blocked!", FALSE as "verified!"
            FROM devices WHERE user_id = $1
            "#,
            user_id,
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to fetch devices", e))?;

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

    /// See [`verify_device_keys_signature`.
    pub fn verify_device_keys_signature(&self, origin: &str, device: &DeviceInfo) -> Result<bool, ApiError> {
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

    /// See [`is_device_key_expired`.
    pub fn is_device_key_expired(&self, device: &DeviceInfo) -> bool {
        if let Some(last_seen) = device.last_seen_ts {
            let last_seen_time = Utc.timestamp_millis_opt(last_seen).earliest().unwrap_or(Utc::now());
            let expiry_date = last_seen_time + Duration::days(DEVICE_KEY_EXPIRY_DAYS);
            expiry_date < Utc::now()
        } else {
            device.keys.is_none()
        }
    }

    /// See [`cleanup_expired_devices`.
    pub async fn cleanup_expired_devices(&self, user_id: &str) -> Result<u64, ApiError> {
        let expiry_threshold = Utc::now() - Duration::days(DEVICE_KEY_EXPIRY_DAYS);

        // Exclude device_ids that are also registered as dehydrated devices.
        // Dehydrated devices are offline by design and would otherwise be
        // purged by the last_seen_ts expiry check, breaking rehydration.
        let result = sqlx::query!(
            r#"
            DELETE FROM devices
            WHERE user_id = $1
            AND (last_seen_ts IS NULL OR last_seen_ts < $2)
            AND device_id NOT IN (
                SELECT device_id FROM dehydrated_devices WHERE user_id = $1
            )
            "#,
            user_id,
            expiry_threshold.timestamp_millis(),
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to cleanup expired devices", e))?;

        let deleted_count = result.rows_affected();
        if deleted_count > 0 {
            tracing::info!("Cleaned up {} expired devices for user {}", deleted_count, user_id);
            self.invalidate_user_devices_cache(user_id).await;
        }

        Ok(deleted_count)
    }

    /// See [`sync_device_keys_with_expiry_check`.
    pub async fn sync_device_keys_with_expiry_check(
        &self,
        origin: &str,
        user_id: &str,
    ) -> Result<Vec<DeviceInfo>, ApiError> {
        let devices = self.sync_devices_from_remote(origin, user_id).await?;
        let original_count = devices.len();

        let valid_devices: Vec<DeviceInfo> =
            devices.into_iter().filter(|device| !self.is_device_key_expired(device)).collect();

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

    /// See [`revoke_device`.
    pub async fn revoke_device(&self, device_id: &str, user_id: &str) -> Result<(), ApiError> {
        sqlx::query!(
            r#"
            UPDATE devices SET
                device_key = NULL,
                last_seen_ts = NULL
            WHERE device_id = $1 AND user_id = $2
            "#,
            device_id,
            user_id,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal_with_cause("Failed to revoke device", e))?;

        let cache_pattern = format!("remote_devices:*:{user_id}");
        let mut local = self.local_cache.write().await;
        local.retain(|key, _| !key.starts_with(&cache_pattern));

        if let Some(cache) = &self.cache_manager {
            cache.delete(&cache_pattern).await;
        }

        Ok(())
    }

    /// See [`invalidate_user_devices_cache`.
    pub async fn invalidate_user_devices_cache(&self, user_id: &str) {
        let cache_pattern = format!("remote_devices:*:{user_id}");
        let mut local = self.local_cache.write().await;
        local.retain(|key, _| !key.starts_with(&cache_pattern));

        if let Some(cache) = &self.cache_manager {
            cache.delete(&cache_pattern).await;
        }

        tracing::info!("Invalidated device cache for user: {}", user_id);
    }
}

// C59：本结构体只被 `get_local_devices` 使用，而宏化后的 `query_as!` **不走 `FromRow`**
// ⇒ 原先的 `#[derive(sqlx::FromRow)]` 成为死 derive（同 C31/C34 的清理），一并删除。
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

    fn create_test_device_info() -> DeviceInfo {
        DeviceInfo {
            device_id: "DEVICE123".to_string(),
            user_id: "@alice:example.com".to_string(),
            keys: Some(json!({"ed25519:DEVICE123": "key_base64"})),
            device_display_name: Some("My Phone".to_string()),
            last_seen_ts: Some(1234567890),
            last_seen_ip: Some("192.168.1.1".to_string()),
            is_blocked: false,
            verified: true,
        }
    }

    #[test]
    fn test_device_info_creation() {
        let device = create_test_device_info();
        assert_eq!(device.device_id, "DEVICE123");
        assert_eq!(device.user_id, "@alice:example.com");
        assert!(device.verified);
        assert!(!device.is_blocked);
    }

    #[test]
    fn test_device_info_user_id_format() {
        let device = create_test_device_info();
        assert!(device.user_id.starts_with('@'));
        assert!(device.user_id.contains(':'));
    }

    #[test]
    fn test_device_info_optional_fields() {
        let device = DeviceInfo {
            device_id: "DEVICE456".to_string(),
            user_id: "@bob:example.com".to_string(),
            keys: None,
            device_display_name: None,
            last_seen_ts: None,
            last_seen_ip: None,
            is_blocked: false,
            verified: false,
        };
        assert!(device.keys.is_none());
        assert!(device.device_display_name.is_none());
        assert!(device.last_seen_ts.is_none());
    }

    #[test]
    fn test_device_info_blocked_status() {
        let blocked_device = DeviceInfo {
            device_id: "BLOCKED".to_string(),
            user_id: "@user:example.com".to_string(),
            keys: None,
            device_display_name: None,
            last_seen_ts: None,
            last_seen_ip: None,
            is_blocked: true,
            verified: false,
        };
        assert!(blocked_device.is_blocked);
    }

    #[test]
    fn test_device_info_verified_status() {
        let verified_device = DeviceInfo {
            device_id: "VERIFIED".to_string(),
            user_id: "@user:example.com".to_string(),
            keys: None,
            device_display_name: None,
            last_seen_ts: None,
            last_seen_ip: None,
            is_blocked: false,
            verified: true,
        };
        assert!(verified_device.verified);
    }

    #[test]
    fn test_device_info_keys_format() {
        let device = create_test_device_info();
        assert!(device.keys.is_some());
        let keys = device.keys.unwrap();
        assert!(keys.get("ed25519:DEVICE123").is_some());
    }

    #[test]
    fn test_device_info_serialization() {
        let device = create_test_device_info();
        let json = serde_json::to_string(&device).unwrap();
        assert!(json.contains("DEVICE123"));
        assert!(json.contains("@alice:example.com"));
    }

    #[test]
    fn test_device_info_deserialization() {
        let json = r#"{
            "device_id": "TEST_DEVICE",
            "user_id": "@test:example.com",
            "keys": null,
            "device_display_name": "Test Device",
            "last_seen_ts": 1234567890,
            "last_seen_ip": "10.0.0.1",
            "is_blocked": false,
            "verified": true
        }"#;

        let device: DeviceInfo = serde_json::from_str(json).unwrap();
        assert_eq!(device.device_id, "TEST_DEVICE");
        assert_eq!(device.user_id, "@test:example.com");
    }

    #[test]
    fn test_remote_device_fields_are_safely_ignored() {
        let device_json = json!({
            "device_id": "REMOTE_DEVICE",
            "user_id": "@remote:example.com",
            "keys": {
                "ed25519:REMOTE_DEVICE": "key"
            },
            "device_display_name": "Remote Phone",
            "last_seen_ts": 1234567890,
            "last_seen_ip": "10.0.0.1",
            "is_blocked": true,
            "verified": true
        });

        let device = DeviceInfo {
            device_id: device_json["device_id"].as_str().unwrap().to_string(),
            user_id: device_json["user_id"].as_str().unwrap().to_string(),
            keys: device_json.get("keys").cloned(),
            device_display_name: None,
            last_seen_ts: None,
            last_seen_ip: None,
            is_blocked: false,
            verified: false,
        };

        assert_eq!(device.device_id, "REMOTE_DEVICE");
        assert!(device.device_display_name.is_none());
        assert!(device.last_seen_ts.is_none());
        assert!(device.last_seen_ip.is_none());
        assert!(!device.is_blocked);
        assert!(!device.verified);
    }

    #[test]
    fn test_device_cache_ttl_constant() {
        assert_eq!(DEVICE_SYNC_CACHE_TTL, 3600);
    }

    #[test]
    fn test_device_key_expiry_days_constant() {
        assert_eq!(DEVICE_KEY_EXPIRY_DAYS, 365);
    }

    #[test]
    fn test_device_info_clone() {
        let device = create_test_device_info();
        let cloned = device.clone();
        assert_eq!(device.device_id, cloned.device_id);
        assert_eq!(device.user_id, cloned.user_id);
    }
}

#[cfg(test)]
mod db_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use std::sync::Arc;

    /// 隔离 schema 上的 pool（D-79 的本 crate 适配器）。
    ///
    /// C59-0 之前 `get_local_devices` / `cleanup_expired_devices` / `revoke_device`
    /// **没有任何模块级 DB 往返**：`tests` mod 全是纯结构体用例，集成侧只有
    /// `test_device_sync_cache`（空表）与 `test_device_revocation`（对不存在的设备），
    /// 于是"列名/别名/常量列可空性/清理语义"这几处**没有被真基线钉住**。
    async fn test_pool() -> (crate::test_isolation::IsolatedTestPool, Arc<sqlx::PgPool>) {
        let isolated = crate::test_isolation::isolated_test_pool().await.expect("isolated pool");
        let pool = isolated.pool();
        (isolated, pool)
    }

    async fn ensure_test_user(pool: &sqlx::PgPool, user_id: &str) {
        let username = user_id.strip_prefix('@').and_then(|u| u.split(':').next()).unwrap_or("testuser");
        sqlx::query(
            "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, EXTRACT(EPOCH FROM NOW()) * 1000) ON CONFLICT (user_id) DO NOTHING",
        )
        .bind(user_id)
        .bind(username)
        .execute(pool)
        .await
        .expect("insert test user");
    }

    async fn insert_device(
        pool: &sqlx::PgPool,
        user_id: &str,
        device_id: &str,
        display_name: Option<&str>,
        device_key: Option<serde_json::Value>,
        last_seen_ts: Option<i64>,
    ) {
        sqlx::query(
            r"INSERT INTO devices (device_id, user_id, display_name, device_key, last_seen_ts, created_ts, first_seen_ts)
               VALUES ($1, $2, $3, $4, $5, 1, 1)",
        )
        .bind(device_id)
        .bind(user_id)
        .bind(display_name)
        .bind(device_key)
        .bind(last_seen_ts)
        .execute(pool)
        .await
        .expect("insert device");
    }

    #[tokio::test]
    async fn get_local_devices_maps_columns_and_constant_flags() {
        let (_isolated, pool) = test_pool().await;
        let manager = DeviceSyncManager::new(&pool, None, None);
        let user_id = format!("@ds_{}:example.com", uuid::Uuid::new_v4().simple());

        ensure_test_user(&pool, &user_id).await;
        insert_device(
            &pool,
            &user_id,
            "DEV_FULL",
            Some("My Phone"),
            Some(json!({ "k": "v" })),
            Some(1_700_000_000_000),
        )
        .await;
        // 可空列全 NULL 的设备：`keys`/`device_display_name`/`last_seen_ip`/`last_seen_ts` 都要能往返
        insert_device(&pool, &user_id, "DEV_BARE", None, None, None).await;

        let mut devices = manager.get_local_devices(&user_id).await.expect("get_local_devices");
        devices.sort_by(|a, b| a.device_id.cmp(&b.device_id));
        assert_eq!(devices.len(), 2);

        assert_eq!(devices[0].device_id, "DEV_BARE");
        assert_eq!(devices[0].user_id, user_id);
        assert!(devices[0].keys.is_none());
        assert!(devices[0].device_display_name.is_none());
        assert!(devices[0].last_seen_ts.is_none());
        assert!(devices[0].last_seen_ip.is_none());
        assert!(!devices[0].is_blocked, "常量 FALSE 列必须解成 false");
        assert!(!devices[0].verified);

        assert_eq!(devices[1].device_id, "DEV_FULL");
        // `display_name AS device_display_name` 与 `device_key AS keys` 两个别名必须真的映射到字段
        assert_eq!(devices[1].device_display_name.as_deref(), Some("My Phone"));
        assert_eq!(devices[1].keys, Some(json!({ "k": "v" })));
        assert_eq!(devices[1].last_seen_ts, Some(1_700_000_000_000));

        // 其他用户的行不得被带出
        let other = format!("@ds_other_{}:example.com", uuid::Uuid::new_v4().simple());
        ensure_test_user(&pool, &other).await;
        assert!(manager.get_local_devices(&other).await.expect("other user").is_empty());
    }

    #[tokio::test]
    async fn cleanup_expired_devices_deletes_stale_only_and_spares_dehydrated() {
        let (_isolated, pool) = test_pool().await;
        let manager = DeviceSyncManager::new(&pool, None, None);
        let user_id = format!("@ds_clean_{}:example.com", uuid::Uuid::new_v4().simple());
        ensure_test_user(&pool, &user_id).await;

        let now = Utc::now().timestamp_millis();
        let stale = now - (DEVICE_KEY_EXPIRY_DAYS + 1) * 24 * 60 * 60 * 1000;

        insert_device(&pool, &user_id, "DEV_STALE", None, None, Some(stale)).await;
        insert_device(&pool, &user_id, "DEV_FRESH", None, None, Some(now)).await;
        insert_device(&pool, &user_id, "DEV_NEVER_SEEN", None, None, None).await; // last_seen_ts IS NULL ⇒ 过期
                                                                                  // 脱水设备即使过期也必须保留（离线是设计意图，删掉会破坏 rehydration）
        insert_device(&pool, &user_id, "DEV_DEHYDRATED", None, None, Some(stale)).await;
        sqlx::query(
            r"INSERT INTO dehydrated_devices (user_id, device_id, device_data, algorithm, created_ts, updated_ts)
               VALUES ($1, 'DEV_DEHYDRATED', '{}'::jsonb, 'm.dehydrated_device', 1, 1)",
        )
        .bind(&user_id)
        .execute(&*pool)
        .await
        .expect("insert dehydrated device");

        // 恰好删掉 2 条（stale + never-seen）；fresh 与 dehydrated 保留
        let deleted = manager.cleanup_expired_devices(&user_id).await.expect("cleanup_expired_devices");
        assert_eq!(deleted, 2, "只应删除 last_seen_ts 过期/为 NULL 且非脱水设备的行");

        let remaining: Vec<String> =
            sqlx::query_scalar("SELECT device_id FROM devices WHERE user_id = $1 ORDER BY device_id")
                .bind(&user_id)
                .fetch_all(&*pool)
                .await
                .expect("read remaining");
        assert_eq!(remaining, vec!["DEV_DEHYDRATED".to_string(), "DEV_FRESH".to_string()]);

        // 再跑一次是幂等的（没有新的过期行）
        assert_eq!(manager.cleanup_expired_devices(&user_id).await.expect("second cleanup"), 0);
    }

    #[tokio::test]
    async fn revoke_device_clears_key_and_last_seen_for_that_user_only() {
        let (_isolated, pool) = test_pool().await;
        let manager = DeviceSyncManager::new(&pool, None, None);
        let user_id = format!("@ds_revoke_{}:example.com", uuid::Uuid::new_v4().simple());
        let other = format!("@ds_revoke_other_{}:example.com", uuid::Uuid::new_v4().simple());
        ensure_test_user(&pool, &user_id).await;
        ensure_test_user(&pool, &other).await;

        insert_device(&pool, &user_id, "DEV_TARGET", Some("T"), Some(json!({ "k": "v" })), Some(1_700_000_000_000))
            .await;
        insert_device(&pool, &user_id, "DEV_KEEP", Some("K"), Some(json!({ "k": "v" })), Some(1_700_000_000_000)).await;
        // `devices.device_id` 是**全局主键**（`pk_devices PRIMARY KEY (device_id)`），
        // 所以"别人的设备"必须是另一个 device_id；用它验证 `WHERE … AND user_id = $2` 的 user 维度。
        insert_device(&pool, &other, "DEV_OTHER", Some("O"), Some(json!({ "k": "v" })), Some(1_700_000_000_000)).await;

        manager.revoke_device("DEV_TARGET", &user_id).await.expect("revoke_device");

        let revoked: (Option<serde_json::Value>, Option<i64>) = sqlx::query_as(
            "SELECT device_key, last_seen_ts FROM devices WHERE user_id = $1 AND device_id = 'DEV_TARGET'",
        )
        .bind(&user_id)
        .fetch_one(&*pool)
        .await
        .expect("read revoked");
        assert!(revoked.0.is_none(), "device_key 必须被清空");
        assert!(revoked.1.is_none(), "last_seen_ts 必须被清空");

        // 同一用户的其它设备不受影响（`WHERE device_id = $1 AND user_id = $2` 的 user 维度）
        let kept: (Option<serde_json::Value>, Option<i64>) = sqlx::query_as(
            "SELECT device_key, last_seen_ts FROM devices WHERE user_id = $1 AND device_id = 'DEV_KEEP'",
        )
        .bind(&user_id)
        .fetch_one(&*pool)
        .await
        .expect("read kept");
        assert!(kept.0.is_some() && kept.1.is_some());

        // 用**错误的 user_id** 去 revoke 别人的设备 ⇒ 匹配 0 行（不报错），别人的行不受影响
        manager.revoke_device("DEV_OTHER", &user_id).await.expect("revoke with wrong user must not error");
        let other_row: (Option<serde_json::Value>, Option<i64>) = sqlx::query_as(
            "SELECT device_key, last_seen_ts FROM devices WHERE user_id = $1 AND device_id = 'DEV_OTHER'",
        )
        .bind(&other)
        .fetch_one(&*pool)
        .await
        .expect("read other user");
        assert!(other_row.0.is_some() && other_row.1.is_some(), "user_id 不匹配时不得清空别人的设备");
    }
}
