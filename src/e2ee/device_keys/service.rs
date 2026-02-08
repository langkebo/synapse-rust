use super::models::*;
use super::storage::DeviceKeyStorage;
use crate::cache::CacheManager;
use crate::error::ApiError;
use chrono::Utc;
use std::sync::Arc;

#[derive(Clone)]
pub struct DeviceKeyService {
    storage: DeviceKeyStorage,
    cache: Arc<CacheManager>,
}

impl DeviceKeyService {
    pub fn new(storage: DeviceKeyStorage, cache: Arc<CacheManager>) -> Self {
        Self { storage, cache }
    }

    pub async fn query_keys(&self, request: KeyQueryRequest) -> Result<KeyQueryResponse, ApiError> {
        let mut device_keys = serde_json::Map::new();
        let failures = serde_json::Map::new();

        if let Some(query_map) = request.device_keys.as_object() {
            for (user_id, device_ids) in query_map {
                let device_ids: Vec<String> = if let Some(arr) = device_ids.as_array() {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                } else {
                    vec!["*".to_string()]
                };

                let keys = if device_ids.contains(&"*".to_string()) {
                    self.storage.get_all_device_keys(user_id).await?
                } else {
                    self.storage
                        .get_device_keys(user_id, device_ids.as_slice())
                        .await?
                };

                let mut user_keys = serde_json::Map::new();
                for key in keys {
                    let device_key = serde_json::json!({
                        "algorithms": ["m.olm.v1.curve25519-aes-sha2"],
                        "device_id": key.device_id,
                        "keys": {
                            format!("curve25519:{}", key.key_id): key.public_key,
                            format!("ed25519:{}", key.key_id): key.public_key,
                        },
                        "signatures": key.signatures,
                        "user_id": key.user_id,
                    });
                    user_keys.insert(key.device_id, device_key);
                }

                device_keys.insert(user_id.clone(), serde_json::Value::Object(user_keys));
            }
        }

        Ok(KeyQueryResponse {
            device_keys: serde_json::Value::Object(device_keys),
            failures: serde_json::Value::Object(failures),
        })
    }

    pub async fn upload_keys(
        &self,
        request: KeyUploadRequest,
    ) -> Result<KeyUploadResponse, ApiError> {
        let mut one_time_key_counts = serde_json::Map::new();

        if let Some(ref device_keys) = request.device_keys {
            let user_id = device_keys.user_id.clone();
            let device_id = device_keys.device_id.clone();

            if let Some(keys) = device_keys.keys.as_object() {
                for (key_id, public_key) in keys {
                    let key = DeviceKey {
                        id: 0, // 数据库自动生成
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
                        signatures: serde_json::to_value(&device_keys.signatures)
                            .unwrap_or(serde_json::json!({})),
                        created_at: Utc::now(),
                        updated_at: Utc::now(),
                    };

                    self.storage.create_device_key(&key).await?;

                    let cache_key = format!("device_keys:{}:{}", user_id, device_id);
                    let _ = self.cache.set(&cache_key, &key, 300).await;
                }
            }
        }

        if let Some(ref one_time_keys) = request.one_time_keys {
            let (user_id, device_id) = if let Some(ref dk) = request.device_keys {
                (dk.user_id.clone(), dk.device_id.clone())
            } else {
                (String::new(), String::new())
            };

            if let Some(keys) = one_time_keys.as_object() {
                for (key_id, key_data) in keys {
                    let public_key = if key_data.is_string() {
                        key_data.as_str().unwrap_or_default().to_string()
                    } else {
                        key_data["key"].as_str().unwrap_or_default().to_string()
                    };

                    let signatures = if key_data.is_object() {
                        key_data["signatures"].clone()
                    } else {
                        serde_json::json!({})
                    };

                    let key = DeviceKey {
                        id: 0, // 数据库自动生成
                        user_id: user_id.clone(),
                        device_id: device_id.clone(),
                        display_name: None,
                        algorithm: "signed_curve25519".to_string(),
                        key_id: key_id.clone(),
                        public_key,
                        signatures,
                        created_at: Utc::now(),
                        updated_at: Utc::now(),
                    };

                    self.storage.create_device_key(&key).await?;
                }
            }

            if !user_id.is_empty() {
                let count = self
                    .storage
                    .get_one_time_keys_count(&user_id, &device_id)
                    .await?;
                one_time_key_counts.insert(
                    "signed_curve25519".to_string(),
                    serde_json::Value::Number(count.into()),
                );
            }
        }

        Ok(KeyUploadResponse {
            one_time_key_counts: serde_json::Value::Object(one_time_key_counts),
        })
    }

    pub async fn claim_keys(&self, request: KeyClaimRequest) -> Result<KeyClaimResponse, ApiError> {
        let mut one_time_keys = serde_json::Map::new();
        let failures = serde_json::Map::new();

        if let Some(claim_map) = request.one_time_keys.as_object() {
            for (user_id, device_keys) in claim_map {
                let mut user_keys = serde_json::Map::new();

                if let Some(keys) = device_keys.as_object() {
                    for (device_id, algorithm) in keys {
                        if let Some(algo_str) = algorithm.as_str() {
                            if let Some(key) = self
                                .storage
                                .claim_one_time_key(user_id, device_id, algo_str)
                                .await?
                            {
                                let key_data = serde_json::json!({
                                    "key": key.public_key,
                                    "signatures": key.signatures,
                                });
                                user_keys.insert(
                                    device_id.clone(),
                                    serde_json::json!({
                                        format!("{}:{}", algo_str, key.key_id): key_data
                                    }),
                                );
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

    pub async fn delete_keys(&self, user_id: &str, device_id: &str) -> Result<(), ApiError> {
        self.storage.delete_device_keys(user_id, device_id).await?;

        let cache_key = format!("device_keys:{}:{}", user_id, device_id);
        self.cache.delete(&cache_key).await;

        Ok(())
    }

    pub async fn get_key_changes(
        &self,
        from: &str,
        to: &str,
    ) -> Result<(Vec<String>, Vec<String>), ApiError> {
        let from_ts = from.parse::<i64>().unwrap_or(0);
        let to_ts = to.parse::<i64>().unwrap_or(Utc::now().timestamp_millis());

        let changed = self.storage.get_key_changes(from_ts, to_ts).await?;
        let left: Vec<String> = vec![]; // Simplified for now

        Ok((changed, left))
    }
}
