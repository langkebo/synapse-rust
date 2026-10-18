use super::models::*;
use super::storage::DeviceKeyStoreApi;
use crate::cross_signing::storage::CrossSigningStorage;
use crate::crypto::CryptoError;
use crate::signed_json::verify_device_keys_signature;
use crate::signed_json::verify_one_time_key_signature;
use chrono::Utc;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use synapse_cache::CacheManager;
use synapse_common::current_timestamp_millis;
use synapse_common::map_database;
use synapse_common::ApiError;
use synapse_common::MatrixErrorCode;
use synapse_storage::DehydratedDeviceStorage;

/// Maximum number of one-time keys a single device may hold per algorithm.
///
/// Mirrors Synapse's `MAX_ONE_TIME_KEYS_PER_ALGORITHM_PER_DEVICE` (PR #20162).
const MAX_ONE_TIME_KEYS_PER_ALGORITHM_PER_DEVICE: i64 = 500;

/// Derives the one-time-key algorithm for a key identified by `key_id` with
/// value `key_data`, matching the derivation used when persisting the key.
fn one_time_key_algorithm<'a>(key_id: &'a str, key_data: &'a Value) -> &'a str {
    if key_data.is_string() {
        "curve25519"
    } else {
        key_id.split(':').next().unwrap_or("signed_curve25519")
    }
}

#[derive(Clone)]
/// The `DeviceKeyService` type.
pub struct DeviceKeyService {
    storage: Arc<dyn DeviceKeyStoreApi>,
    cross_signing_storage: Option<Arc<CrossSigningStorage>>,
    dehydrated_device_storage: Option<DehydratedDeviceStorage>,
    cache: Arc<CacheManager>,
}

/// Implementation of [`DeviceKeyService`] methods.
impl DeviceKeyService {
    /// See [`new`].
    pub fn new(storage: Arc<dyn DeviceKeyStoreApi>, cache: Arc<CacheManager>) -> Self {
        Self { storage, cross_signing_storage: None, dehydrated_device_storage: None, cache }
    }

    /// See [`with_cross_signing_storage`].
    pub fn with_cross_signing_storage(mut self, storage: Arc<CrossSigningStorage>) -> Self {
        self.cross_signing_storage = Some(storage);
        self
    }

    /// See [`with_dehydrated_device_storage`].
    pub fn with_dehydrated_device_storage(mut self, storage: DehydratedDeviceStorage) -> Self {
        self.dehydrated_device_storage = Some(storage);
        self
    }

    /// See [`query_keys`].
    pub async fn query_keys(&self, request: KeyQueryRequest) -> Result<KeyQueryResponse, ApiError> {
        self.query_keys_internal(request, None).await
    }

    /// See [`query_keys_for_federation`].
    pub async fn query_keys_for_federation(
        &self,
        request: KeyQueryRequest,
        local_server_name: &str,
    ) -> Result<KeyQueryResponse, ApiError> {
        self.query_keys_internal(request, Some(local_server_name)).await
    }

    async fn query_keys_internal(
        &self,
        request: KeyQueryRequest,
        local_server_name: Option<&str>,
    ) -> Result<KeyQueryResponse, ApiError> {
        let mut device_keys = serde_json::Map::new();
        let failures = serde_json::Map::new();

        if let Some(query_map) = request.device_keys.as_object() {
            for (user_id, device_ids) in query_map {
                if let Some(server_name) = local_server_name {
                    if !is_local_user_id(user_id, server_name) {
                        continue;
                    }
                }

                let device_ids: Vec<String> = if let Some(arr) = device_ids.as_array() {
                    let ids: Vec<String> = arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
                    if ids.is_empty() {
                        vec!["*".to_string()]
                    } else {
                        ids
                    }
                } else {
                    vec!["*".to_string()]
                };

                let cache_key = format!("device_keys_bulk:{user_id}");
                let cached_user_keys: Option<serde_json::Map<String, Value>> =
                    match self.cache.get::<serde_json::Map<String, Value>>(&cache_key).await {
                        Ok(Some(cached)) => {
                            let filtered: serde_json::Map<String, Value> = if device_ids.contains(&"*".to_string()) {
                                cached
                            } else {
                                cached.into_iter().filter(|(k, _)| device_ids.contains(k)).collect()
                            };
                            if filtered.is_empty() {
                                None
                            } else {
                                Some(filtered)
                            }
                        }
                        _ => None,
                    };

                let mut user_keys = if let Some(cached) = cached_user_keys {
                    cached
                } else {
                    let keys = if device_ids.contains(&"*".to_string()) {
                        self.storage.get_all_device_keys(user_id).await?
                    } else {
                        self.storage.get_device_keys(user_id, device_ids.as_slice()).await?
                    };

                    let mut fetched = serde_json::Map::new();
                    Self::populate_user_keys(&mut fetched, keys);

                    if !fetched.is_empty() {
                        if let Err(e) = self.cache.set(&cache_key, &fetched, 300).await {
                            ::tracing::warn!(
                                user_id = %user_id,
                                cache_key = %cache_key,
                                error = %e,
                                "Failed to cache device key query result"
                            );
                        }
                    }
                    fetched
                };

                // MSC3814 — merge the user's dehydrated device into the keys
                // response so that senders can discover it and address
                // to-device messages to it. We never cache this branch since
                // the dehydrated device can be rotated independently of the
                // regular device key set.
                if let Some(dh_storage) = &self.dehydrated_device_storage {
                    if let Ok(Some(record)) = dh_storage.get_by_user(user_id).await {
                        let include = device_ids.is_empty()
                            || device_ids.contains(&"*".to_string())
                            || device_ids.iter().any(|d| d == &record.device_id);
                        if include {
                            if let Some(device_keys) = record
                                .device_data
                                .as_object()
                                .and_then(|obj| obj.get("device_keys"))
                                .filter(|v| v.is_object())
                            {
                                user_keys.insert(record.device_id.clone(), device_keys.clone());
                            }
                        }
                    }
                }

                device_keys.insert(user_id.clone(), serde_json::Value::Object(user_keys));
            }
        }

        let mut master_keys = serde_json::Map::new();
        let mut self_signing_keys = serde_json::Map::new();
        let mut user_signing_keys = serde_json::Map::new();

        if let Some(cs_storage) = &self.cross_signing_storage {
            let user_ids: Vec<String> = device_keys.keys().cloned().collect();
            if !user_ids.is_empty() {
                if let Ok(cs_keys_by_user) = cs_storage.get_cross_signing_keys_batch(&user_ids).await {
                    for (user_id, keys) in cs_keys_by_user {
                        for key in keys {
                            let key_value = key.key_json.clone().unwrap_or_else(|| {
                                serde_json::json!({
                                    "user_id": key.user_id,
                                    "usage": key.usage,
                                    "keys": { format!("ed25519:{}", key.public_key): key.public_key },
                                    "signatures": key.signatures,
                                })
                            });

                            match key.key_type.as_str() {
                                "master" => {
                                    master_keys.insert(user_id.clone(), key_value);
                                }
                                "self_signing" => {
                                    self_signing_keys.insert(user_id.clone(), key_value);
                                }
                                "user_signing" => {
                                    user_signing_keys.insert(user_id.clone(), key_value);
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }

        Ok(KeyQueryResponse {
            device_keys: serde_json::Value::Object(device_keys),
            master_keys: serde_json::Value::Object(master_keys),
            self_signing_keys: serde_json::Value::Object(self_signing_keys),
            user_signing_keys: serde_json::Value::Object(user_signing_keys),
            failures: serde_json::Value::Object(failures),
        })
    }

    /// See [`upload_keys`].
    pub async fn upload_keys(
        &self,
        request: KeyUploadRequest,
        auth_user_id: &str,
        auth_device_id: &str,
    ) -> Result<KeyUploadResponse, ApiError> {
        let mut one_time_key_counts = serde_json::Map::new();
        let mut record_target: Option<(String, String)> = None;

        if let Some(ref device_keys) = request.device_keys {
            let user_id = device_keys.user_id.clone();
            let device_id = device_keys.device_id.clone();
            record_target = Some((user_id.clone(), device_id.clone()));

            let device_keys_value =
                serde_json::to_value(device_keys).map_err(map_database!("Failed to serialize device keys"))?;

            let has_keys = device_keys.keys.as_object().is_some_and(|k| !k.is_empty());
            let has_signatures = device_keys.signatures.as_object().is_some_and(|s| !s.is_empty());

            if has_keys && has_signatures {
                match verify_device_keys_signature(&device_keys_value) {
                    Ok(true) => {}
                    Ok(false) | Err(_) => {
                        // E2EE-05: 验签失败的设备密钥不得存储分发（MITM 风险）
                        ::tracing::warn!(
                            target: "e2ee",
                            user_id = %user_id,
                            device_id = %device_id,
                            "Device key signature verification failed; rejecting upload"
                        );
                        return Err(ApiError::bad_request("Invalid device key signature".to_string()));
                    }
                }
            }

            if let Some(keys) = device_keys.keys.as_object() {
                for (key_id, public_key) in keys {
                    let key = DeviceKey {
                        user_id: user_id.clone(),
                        device_id: device_id.clone(),
                        display_name: None,
                        algorithm: if key_id.contains("curve25519") {
                            "curve25519".to_string()
                        } else {
                            "ed25519".to_string()
                        },
                        key_id: key_id.clone(),
                        public_key: public_key.as_str().unwrap_or_default().to_string(),
                        signatures: serde_json::to_value(&device_keys.signatures).unwrap_or(serde_json::json!({})),
                        created_ts: Utc::now(),
                        updated_ts: Utc::now(),
                    };

                    self.storage.create_device_key(&key).await?;

                    let cache_key = format!("device_keys:{user_id}:{device_id}");
                    if let Err(e) = self.cache.set(&cache_key, &key, 300).await {
                        ::tracing::warn!(
                            user_id = %user_id,
                            device_id = %device_id,
                            cache_key = %cache_key,
                            error = %e,
                            "Failed to cache uploaded device key"
                        );
                    }
                }
            }
        }

        if let Some(ref one_time_keys) = request.one_time_keys {
            let (user_id, device_id) = if let Some(ref dk) = request.device_keys {
                (dk.user_id.clone(), dk.device_id.clone())
            } else {
                (auth_user_id.to_string(), auth_device_id.to_string())
            };

            if record_target.is_none() && !user_id.is_empty() && !device_id.is_empty() {
                record_target = Some((user_id.clone(), device_id.clone()));
            }

            let device_ed25519_key =
                self.storage.get_device_key(&user_id, &device_id, "ed25519").await?.map(|k| k.public_key);

            if let Some(keys) = one_time_keys.as_object() {
                // Enforce a per-device, per-algorithm cap of 500 one-time keys
                // (upstream parity with Synapse PR #20162). Count the incoming
                // keys by algorithm, add what the device already holds, and
                // reject the whole upload — without persisting anything — if any
                // algorithm would exceed the limit.
                let mut new_counts: HashMap<&str, i64> = HashMap::new();
                for (key_id, key_data) in keys {
                    *new_counts.entry(one_time_key_algorithm(key_id, key_data)).or_insert(0) += 1;
                }
                if !user_id.is_empty() && !device_id.is_empty() {
                    let existing = self.storage.get_one_time_keys_count_by_algorithm(&user_id, &device_id).await?;
                    for (algorithm, new_count) in new_counts {
                        let total = existing.get(algorithm).copied().unwrap_or(0) + new_count;
                        if total > MAX_ONE_TIME_KEYS_PER_ALGORITHM_PER_DEVICE {
                            return Err(ApiError::bad_request(format!(
                                "Uploading {new_count} more {algorithm} one-time keys would leave the device holding {total}, over the limit of {MAX_ONE_TIME_KEYS_PER_ALGORITHM_PER_DEVICE}"
                            ))
                            .with_code(MatrixErrorCode::TooLarge));
                        }
                    }
                }

                for (key_id, key_data) in keys {
                    let algorithm = one_time_key_algorithm(key_id, key_data).to_string();
                    let (public_key, signatures) = if key_data.is_string() {
                        (key_data.as_str().unwrap_or_default().to_string(), serde_json::json!({}))
                    } else {
                        let pk = key_data["key"].as_str().unwrap_or_default().to_string();
                        let sigs = key_data.get("signatures").cloned().unwrap_or(serde_json::json!({}));
                        (pk, sigs)
                    };

                    if algorithm == "signed_curve25519" {
                        if let Some(ref ed25519_pk) = device_ed25519_key {
                            match verify_one_time_key_signature(
                                &user_id, &device_id, &algorithm, key_id, key_data, ed25519_pk,
                            ) {
                                Ok(true) => {}
                                // E2EE-05: 签名无效/缺失的 OTK 不得存储
                                Ok(false) => {
                                    tracing::warn!(
                                        "Invalid signature on one-time key {} for user {} device {}; rejecting upload",
                                        key_id,
                                        user_id,
                                        device_id
                                    );
                                    return Err(ApiError::bad_request("Invalid one-time key signature".to_string()));
                                }
                                Err(CryptoError::SignatureVerificationFailed) => {
                                    tracing::warn!(
                                        "Missing or malformed signature on one-time key {} for user {} device {}; rejecting upload",
                                        key_id, user_id, device_id
                                    );
                                    return Err(ApiError::bad_request("Invalid one-time key signature".to_string()));
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        "Signature verification error on one-time key {}: {}; rejecting upload",
                                        key_id,
                                        e
                                    );
                                    return Err(ApiError::bad_request("Invalid one-time key signature".to_string()));
                                }
                            }
                        } else {
                            // E-01: A `signed_curve25519` one-time key MUST be
                            // signature-verified against the device's
                            // `ed25519` key. If the device has no ed25519
                            // key we cannot establish the trust chain, so
                            // refuse the upload rather than persisting an
                            // unverifiable OTK that a MITM could later
                            // substitute.
                            return Err(ApiError::bad_request(format!(
                                "Cannot store signed_curve25519 one-time key for device {user_id}/{device_id} without ed25519 device key"
                            )));
                        }
                    }

                    let key = DeviceKey {
                        user_id: user_id.clone(),
                        device_id: device_id.clone(),
                        display_name: None,
                        algorithm,
                        key_id: key_id.clone(),
                        public_key,
                        signatures,
                        created_ts: Utc::now(),
                        updated_ts: Utc::now(),
                    };

                    self.storage.create_device_key(&key).await?;
                }
            }

            if !user_id.is_empty() {
                let counts = self.storage.get_one_time_keys_count_by_algorithm(&user_id, &device_id).await?;
                for (algo, count) in counts {
                    one_time_key_counts.insert(algo, serde_json::Value::Number(count.into()));
                }
            }
        }

        if let Some(ref fallback_keys) = request.fallback_keys {
            let (user_id, device_id) = if let Some(ref dk) = request.device_keys {
                (dk.user_id.clone(), dk.device_id.clone())
            } else {
                (auth_user_id.to_string(), auth_device_id.to_string())
            };

            if !user_id.is_empty() && !device_id.is_empty() {
                if let Some(keys) = fallback_keys.as_object() {
                    self.storage.delete_fallback_keys(&user_id, &device_id).await?;

                    let device_ed25519_key =
                        self.storage.get_device_key(&user_id, &device_id, "ed25519").await?.map(|k| k.public_key);

                    for (key_id, key_data) in keys {
                        let (algorithm, public_key, signatures) = if key_data.is_string() {
                            (
                                "curve25519".to_string(),
                                key_data.as_str().unwrap_or_default().to_string(),
                                serde_json::json!({}),
                            )
                        } else {
                            let algo = key_id.split(':').next().unwrap_or("signed_curve25519");
                            let pk = key_data["key"].as_str().unwrap_or_default().to_string();
                            let sigs = key_data.get("signatures").cloned().unwrap_or(serde_json::json!({}));
                            (algo.to_string(), pk, sigs)
                        };

                        if algorithm == "signed_curve25519" {
                            if let Some(ref ed25519_pk) = device_ed25519_key {
                                match verify_one_time_key_signature(
                                    &user_id, &device_id, &algorithm, key_id, key_data, ed25519_pk,
                                ) {
                                    Ok(true) => {}
                                    // E2EE-05: 签名无效/缺失的回退密钥不得存储
                                    Ok(false) => {
                                        tracing::warn!(
                                            "Invalid signature on fallback key {} for user {} device {}; rejecting upload",
                                            key_id, user_id, device_id
                                        );
                                        return Err(ApiError::bad_request(
                                            "Invalid fallback key signature".to_string(),
                                        ));
                                    }
                                    Err(CryptoError::SignatureVerificationFailed) => {
                                        tracing::warn!(
                                            "Missing or malformed signature on fallback key {} for user {} device {}; rejecting upload",
                                            key_id, user_id, device_id
                                        );
                                        return Err(ApiError::bad_request(
                                            "Invalid fallback key signature".to_string(),
                                        ));
                                    }
                                    Err(e) => {
                                        tracing::warn!(
                                            "Signature verification error on fallback key {}: {}; rejecting upload",
                                            key_id,
                                            e
                                        );
                                        return Err(ApiError::bad_request(
                                            "Invalid fallback key signature".to_string(),
                                        ));
                                    }
                                }
                            } else {
                                // E-01: same rule as one-time keys — a
                                // `signed_curve25519` fallback key without
                                // an ed25519 device key is unverifiable and
                                // must be refused, not silently stored.
                                return Err(ApiError::bad_request(format!(
                                    "Cannot store signed_curve25519 fallback key for device {user_id}/{device_id} without ed25519 device key"
                                )));
                            }
                        }

                        let key = DeviceKey {
                            user_id: user_id.clone(),
                            device_id: device_id.clone(),
                            display_name: None,
                            algorithm,
                            key_id: key_id.clone(),
                            public_key,
                            signatures,
                            created_ts: Utc::now(),
                            updated_ts: Utc::now(),
                        };

                        self.storage.create_fallback_key(&key).await?;
                    }

                    if record_target.is_none() {
                        record_target = Some((user_id.clone(), device_id.clone()));
                    }
                }
            }
        }

        if let Some((user_id, device_id)) = record_target {
            let cache_key = format!("device_keys_bulk:{user_id}");
            self.cache.delete(&cache_key).await;

            let single_cache_key = format!("device_keys:{user_id}:{device_id}");
            self.cache.delete(&single_cache_key).await;

            // A dropped device-list change means peers never learn about this device (D-07),
            // so fail the upload and let the client retry rather than reporting success.
            self.storage.record_device_list_change(&user_id, Some(&device_id), "changed").await?;
        }

        if one_time_key_counts.is_empty() {
            let counts = self.storage.get_one_time_keys_count_by_algorithm(auth_user_id, auth_device_id).await?;
            for (algo, count) in counts {
                one_time_key_counts.insert(algo, serde_json::Value::Number(count.into()));
            }
        }

        Ok(KeyUploadResponse { one_time_key_counts: serde_json::Value::Object(one_time_key_counts) })
    }

    /// See [`claim_keys`].
    ///
    /// Instrumentation: `e2ee_handshake_duration_seconds` times the client
    /// key-claim round trip — the step that establishes Olm sessions between
    /// devices — around [`claim_keys_internal`].
    pub async fn claim_keys(&self, request: KeyClaimRequest) -> Result<KeyClaimResponse, ApiError> {
        let started = std::time::Instant::now();
        let result = self.claim_keys_internal(request, None).await;
        if let Some(metrics) = synapse_common::server_metrics::global_server_metrics() {
            metrics.record_e2ee_handshake(started.elapsed().as_secs_f64());
        }
        result
    }

    /// See [`claim_keys_for_federation`].
    pub async fn claim_keys_for_federation(
        &self,
        request: KeyClaimRequest,
        local_server_name: &str,
    ) -> Result<KeyClaimResponse, ApiError> {
        self.claim_keys_internal(request, Some(local_server_name)).await
    }

    async fn claim_keys_internal(
        &self,
        request: KeyClaimRequest,
        local_server_name: Option<&str>,
    ) -> Result<KeyClaimResponse, ApiError> {
        let mut one_time_keys = serde_json::Map::new();
        let failures = serde_json::Map::new();

        if let Some(claim_map) = request.one_time_keys.as_object() {
            for (user_id, device_keys) in claim_map {
                if let Some(server_name) = local_server_name {
                    if !is_local_user_id(user_id, server_name) {
                        continue;
                    }
                }

                let mut user_keys = serde_json::Map::new();

                if let Some(keys) = device_keys.as_object() {
                    for (device_id, algorithm) in keys {
                        if let Some(algo_str) = algorithm.as_str() {
                            if let Some(key) = self.storage.claim_one_time_key(user_id, device_id, algo_str).await? {
                                let key_data = serde_json::json!({
                                    "key": key.public_key,
                                    "signatures": key.signatures,
                                });
                                let key_id = if key.key_id.starts_with(&format!("{algo_str}:")) {
                                    key.key_id.clone()
                                } else {
                                    format!("{}:{}", algo_str, key.key_id)
                                };
                                user_keys.insert(
                                    device_id.clone(),
                                    serde_json::json!({
                                        key_id: key_data
                                    }),
                                );

                                if let Ok(remaining) = self.storage.get_one_time_keys_count(user_id, device_id).await {
                                    if remaining < 5 {
                                        tracing::warn!(
                                            "OTK stock low for {}:{} — {} remaining",
                                            user_id,
                                            device_id,
                                            remaining
                                        );
                                    }
                                }
                                continue;
                            }

                            // MSC3814 — fall back to the dehydrated device's
                            // one-time / fallback key pool when the regular
                            // device storage has nothing for this device.
                            if let Some(dh_storage) = &self.dehydrated_device_storage {
                                if let Ok(Some((key_id, payload))) =
                                    dh_storage.claim_one_time_key(user_id, device_id, algo_str).await
                                {
                                    user_keys.insert(device_id.clone(), serde_json::json!({ key_id: payload }));
                                }
                            }
                        }
                    }
                }

                one_time_keys.insert(user_id.clone(), serde_json::Value::Object(user_keys));
            }
        }

        Ok(KeyClaimResponse {
            one_time_keys: serde_json::Value::Object(one_time_keys),
            failures: serde_json::Value::Object(failures),
        })
    }

    /// 删除某设备的全部密钥材料（device key + one-time keys + fallback keys）。
    ///
    /// ⚠️ 缓存必须清两类：
    /// - `device_keys:{user}:{device}`：单设备查询；
    /// - `device_keys_bulk:{user}`：`/keys/query` 的整用户结果（TTL 300s）。
    ///   只清前者时，删完设备后 5 分钟内 `/keys/query` 仍会从 bulk 缓存里把它返回，
    ///   「已删除」的设备在协议层依然可见 —— 这正是 B-4 要消灭的幽灵设备。
    pub async fn delete_keys(&self, user_id: &str, device_id: &str) -> Result<(), ApiError> {
        self.storage.delete_device_keys(user_id, device_id).await?;

        let cache_key = format!("device_keys:{user_id}:{device_id}");
        self.cache.delete(&cache_key).await;
        self.cache.delete(&format!("device_keys_bulk:{user_id}")).await;

        // Same reasoning as the upload path: a missing "deleted" change leaves peers with a
        // device they can still see (D-07). The delete itself is idempotent, so a retry is safe.
        self.storage.record_device_list_change(user_id, Some(device_id), "deleted").await?;

        Ok(())
    }

    /// 删除该用户**所有**设备的密钥材料（登出全部设备 / 管理员踢下线 / 设备批量删除）。
    ///
    /// 逐设备复用 [`Self::delete_keys`]，因此缓存失效与 device-list change 的语义与单设备删除完全一致，
    /// 不会出现「批量路径漏记录变更」这种分叉。
    ///
    /// @returns 被清理的设备数
    pub async fn delete_all_keys(&self, user_id: &str) -> Result<usize, ApiError> {
        let device_ids = self.storage.list_device_ids_with_keys(user_id).await?;

        for device_id in &device_ids {
            self.delete_keys(user_id, device_id).await?;
        }

        // 即使没有任何 device keys，bulk 缓存也可能还挂着上一次查询的空/旧结果
        self.cache.delete(&format!("device_keys_bulk:{user_id}")).await;

        Ok(device_ids.len())
    }

    /// See [`get_key_changes`].
    pub async fn get_key_changes(
        &self,
        from: &str,
        to: &str,
        current_user_id: &str,
    ) -> Result<(Vec<String>, Vec<String>), ApiError> {
        let from_ts = from.parse::<i64>().unwrap_or(0);
        let to_ts = to.parse::<i64>().unwrap_or(current_timestamp_millis());

        self.storage.get_key_changes_with_left(from_ts, to_ts, current_user_id).await
    }

    /// See [`upload_signatures`].
    pub async fn upload_signatures(
        &self,
        user_id: &str,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, ApiError> {
        let mut failures = serde_json::Map::new();

        // Matrix spec：请求体**直接就是**签名映射（`{userId: {keyId: {signingUser: sig}}}`），
        // 规范里没有外层包装。曾经的"兼容 `{"signatures": {...}}`"分支已删除（R-05 #1）——
        // 消费者取证为零：SDK 侧的 body 由 rust-crypto 的 `SignatureUploadRequest` 直接
        // 给出裸映射，后端自己的单测与集成测试也都发裸映射；且对裸映射输入行为**等价**
        // （裸映射不含顶层 `signatures` 键，故原 `unwrap_or(body)` 本就取整个 body）。
        //
        // 显式类型校验（替代此前的静默落空）：请求体必须是 JSON 对象。旧写法用
        // `if let Some(sig_map) = body.as_object() { ... }`，非对象输入（字符串/数组/null）
        // 会**静默跳过**并返回 200 `{"failures":{}}`，把客户端的格式错误伪装成“无失败”，
        // 掩盖调用方 bug。规范要求 body 为对象，故这里按 400 `M_BAD_JSON` 显式拒绝。
        let sig_map = body
            .as_object()
            .ok_or_else(|| ApiError::bad_request("Request body must be a JSON object of signatures"))?;

        for (target_user_id, user_sigs) in sig_map {
            if let Some(user_sig_map) = user_sigs.as_object() {
                for (target_key_id, sig_data) in user_sig_map {
                    if let Some(sig_obj) = sig_data.as_object() {
                        for (signing_user_id, signing_key_sigs) in sig_obj {
                            // E2EE-06: 只允许已认证用户上传以自己名义的签名，
                            // 防止攻击者伪造他人的交叉签名关系
                            if signing_user_id != user_id {
                                tracing::warn!(
                                    target: "e2ee",
                                    auth_user = %user_id,
                                    signing_user = %signing_user_id,
                                    target_user = %target_user_id,
                                    target_key = %target_key_id,
                                    "Rejecting signature uploaded on behalf of another user"
                                );
                                failures
                                    .entry(target_user_id.clone())
                                    .or_insert_with(|| serde_json::json!({}))
                                    .as_object_mut()
                                    .map(|m| {
                                        m.insert(
                                            target_key_id.clone(),
                                            serde_json::json!({
                                                "errcode": "M_FORBIDDEN",
                                                "error": "Cannot upload signatures on behalf of other users"
                                            }),
                                        )
                                    });
                                continue;
                            }
                            if let Some(key_sigs) = signing_key_sigs.as_object() {
                                for (signing_key_id, signature) in key_sigs {
                                    if let Err(e) = self
                                        .storage
                                        .store_signature(
                                            target_user_id,
                                            target_key_id,
                                            signing_user_id,
                                            signing_key_id,
                                            signature.as_str().unwrap_or(""),
                                        )
                                        .await
                                    {
                                        tracing::error!("Failed to store signature for {target_key_id} (signed by {signing_key_id}): {e}");
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(serde_json::json!({
            "failures": failures
        }))
    }

    fn populate_user_keys(target: &mut serde_json::Map<String, Value>, keys: Vec<DeviceKey>) {
        for key in keys {
            // Only device identity keys (ed25519, curve25519) belong in the
            // device_keys `keys` map.  OTK algorithms like signed_curve25519
            // must be excluded — they are returned separately via
            // one_time_key_counts / one_time_keys, not in device_keys.
            if key.algorithm != "ed25519" && key.algorithm != "curve25519" {
                continue;
            }

            let entry = target.entry(key.device_id.clone()).or_insert_with(|| {
                serde_json::json!({
                    "algorithms": ["m.olm.v1.curve25519-aes-sha2", "m.megolm.v1.aes-sha2"],
                    "device_id": key.device_id,
                    "keys": serde_json::Map::<String, Value>::new(),
                    "signatures": key.signatures.clone(),
                    "user_id": key.user_id,
                })
            });

            if let Some(obj) = entry.as_object_mut() {
                if let Some(keys_map) = obj.get_mut("keys").and_then(|v| v.as_object_mut()) {
                    let prefix = &key.algorithm;
                    let key_name = if key.key_id.starts_with(&format!("{prefix}:")) {
                        key.key_id.clone()
                    } else {
                        format!("{}:{}", prefix, key.key_id)
                    };
                    keys_map.insert(key_name, Value::String(key.public_key.clone()));
                }
                obj.insert("signatures".to_string(), key.signatures);
            }
        }
    }
}

fn is_local_user_id(user_id: &str, local_server_name: &str) -> bool {
    user_id
        .strip_prefix('@')
        .and_then(|user| user.rsplit_once(':'))
        .is_some_and(|(_, server_name)| server_name == local_server_name)
}

#[cfg(test)]
mod tests;
