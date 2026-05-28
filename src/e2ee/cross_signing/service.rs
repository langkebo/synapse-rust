use super::models::*;
use super::storage::CrossSigningStorage;
use crate::e2ee::device_keys::DeviceKeyStorage;
use crate::e2ee::signed_json::verify_signed_json;
use crate::error::ApiError;
use crate::services::dehydrated_device_service::DehydratedDeviceService;
use chrono::Utc;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Clone)]
pub struct CrossSigningService {
    storage: CrossSigningStorage,
    device_keys_storage: Option<Arc<DeviceKeyStorage>>,
    dehydrated_device_service: Option<Arc<DehydratedDeviceService>>,
}

impl CrossSigningService {
    fn extract_ed25519_key(
        key_json: &serde_json::Value,
        field_name: &str,
    ) -> Result<(String, String), ApiError> {
        let keys = key_json
            .get("keys")
            .and_then(|v| v.as_object())
            .ok_or_else(|| ApiError::bad_request(format!("Missing keys in {field_name}")))?;

        keys.iter()
            .find_map(|(key_id, value)| {
                if key_id.starts_with("ed25519:") {
                    value
                        .as_str()
                        .map(|public_key| (key_id.clone(), public_key.to_string()))
                } else {
                    None
                }
            })
            .ok_or_else(|| ApiError::bad_request(format!("Missing ed25519 key in {field_name}")))
    }

    pub fn new(storage: CrossSigningStorage) -> Self {
        Self {
            storage,
            device_keys_storage: None,
            dehydrated_device_service: None,
        }
    }

    pub fn with_device_keys_storage(mut self, storage: Arc<DeviceKeyStorage>) -> Self {
        self.device_keys_storage = Some(storage);
        self
    }

    pub fn with_dehydrated_device_service(mut self, service: Arc<DehydratedDeviceService>) -> Self {
        self.dehydrated_device_service = Some(service);
        self
    }

    pub async fn upload_cross_signing_keys(
        &self,
        upload: CrossSigningUpload,
    ) -> Result<(), ApiError> {
        let user_id = upload.master_key["user_id"]
            .as_str()
            .ok_or_else(|| ApiError::bad_request("Missing user_id in master_key".to_string()))?;

        let (_master_key_id, master_public_key) =
            Self::extract_ed25519_key(&upload.master_key, "master_key")?;

        let master_usage = upload.master_key["usage"]
            .as_array()
            .ok_or_else(|| ApiError::bad_request("Missing usage in master_key".to_string()))?
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect::<Vec<_>>();

        let master_key = CrossSigningKey {
            id: uuid::Uuid::new_v4(),
            user_id: user_id.to_string(),
            key_type: "master".to_string(),
            public_key: master_public_key.to_string(),
            usage: master_usage,
            signatures: upload.master_key["signatures"].clone(),
            key_json: Some(upload.master_key.clone()),
            created_ts: Utc::now(),
            updated_ts: Utc::now(),
        };
        self.storage.create_cross_signing_key(&master_key).await?;

        let (_self_signing_key_id, self_signing_public_key) =
            Self::extract_ed25519_key(&upload.self_signing_key, "self_signing_key")?;

        let self_signing_usage = upload.self_signing_key["usage"]
            .as_array()
            .ok_or_else(|| ApiError::bad_request("Missing usage in self_signing_key".to_string()))?
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect::<Vec<_>>();

        let self_signing_key = CrossSigningKey {
            id: uuid::Uuid::new_v4(),
            user_id: user_id.to_string(),
            key_type: "self_signing".to_string(),
            public_key: self_signing_public_key.to_string(),
            usage: self_signing_usage,
            signatures: upload.self_signing_key["signatures"].clone(),
            key_json: Some(upload.self_signing_key.clone()),
            created_ts: Utc::now(),
            updated_ts: Utc::now(),
        };
        self.storage
            .create_cross_signing_key(&self_signing_key)
            .await?;

        let (_user_signing_key_id, user_signing_public_key) =
            Self::extract_ed25519_key(&upload.user_signing_key, "user_signing_key")?;

        let user_signing_usage = upload.user_signing_key["usage"]
            .as_array()
            .ok_or_else(|| ApiError::bad_request("Missing usage in user_signing_key".to_string()))?
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect::<Vec<_>>();

        let user_signing_key = CrossSigningKey {
            id: uuid::Uuid::new_v4(),
            user_id: user_id.to_string(),
            key_type: "user_signing".to_string(),
            public_key: user_signing_public_key.to_string(),
            usage: user_signing_usage,
            signatures: upload.user_signing_key["signatures"].clone(),
            key_json: Some(upload.user_signing_key.clone()),
            created_ts: Utc::now(),
            updated_ts: Utc::now(),
        };
        self.storage
            .create_cross_signing_key(&user_signing_key)
            .await?;

        Ok(())
    }

    pub async fn get_cross_signing_keys(
        &self,
        user_id: &str,
    ) -> Result<CrossSigningKeys, ApiError> {
        let keys = self.storage.get_cross_signing_keys(user_id).await?;

        let master_key = keys
            .iter()
            .find(|k| k.key_type == "master")
            .ok_or_else(|| ApiError::not_found("Master key not found".to_string()))?;
        let self_signing_key = keys
            .iter()
            .find(|k| k.key_type == "self_signing")
            .ok_or_else(|| ApiError::not_found("Self-signing key not found".to_string()))?;
        let user_signing_key = keys
            .iter()
            .find(|k| k.key_type == "user_signing")
            .ok_or_else(|| ApiError::not_found("User-signing key not found".to_string()))?;

        Ok(CrossSigningKeys {
            user_id: user_id.to_string(),
            master_key: master_key.public_key.clone(),
            self_signing_key: self_signing_key.public_key.clone(),
            user_signing_key: user_signing_key.public_key.clone(),
            self_signing_signature: String::new(),
            user_signing_signature: String::new(),
        })
    }

    pub async fn upload_key_signature(
        &self,
        user_id: &str,
        _key_id: &str,
        signature: &serde_json::Value,
    ) -> Result<(), ApiError> {
        let key = self
            .storage
            .get_cross_signing_key(user_id, "master")
            .await?;
        if let Some(mut k) = key {
            let signatures = k
                .signatures
                .as_object()
                .ok_or_else(|| ApiError::internal("Invalid signatures format".to_string()))?
                .clone();
            let mut sig_map = signatures;
            sig_map.insert(user_id.to_string(), signature.clone());
            k.signatures = serde_json::Value::Object(sig_map);
            k.updated_ts = Utc::now();
            self.storage.update_cross_signing_key(&k).await?;
        }
        Ok(())
    }

    pub async fn upload_device_signing_key(
        &self,
        user_id: &str,
        device_id: &str,
        key: &serde_json::Value,
    ) -> Result<(), ApiError> {
        let key_user_id = key.get("user_id").and_then(|v| v.as_str()).unwrap_or("");
        if !key_user_id.is_empty() && key_user_id != user_id {
            return Err(ApiError::bad_request(format!(
                "user_id in key JSON ({key_user_id}) does not match authenticated user ({user_id})"
            )));
        }

        let usage = key.get("usage").and_then(|v| v.as_array());
        let key_type = if let Some(u) = usage {
            u.first().and_then(|v| v.as_str()).unwrap_or("unknown")
        } else {
            key.get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
        };

        let keys = key.get("keys").and_then(|v| v.as_object());
        let (_algorithm, public_key) = if let Some(k_map) = keys {
            if let Some((k, v)) = k_map.iter().next() {
                let parts: Vec<&str> = k.splitn(2, ':').collect();
                if parts.len() == 2 {
                    (parts[0].to_string(), v.as_str().unwrap_or("").to_string())
                } else {
                    ("ed25519".to_string(), v.as_str().unwrap_or("").to_string())
                }
            } else {
                ("".to_string(), "".to_string())
            }
        } else {
            (
                key.get("algorithm")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                key.get("key")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            )
        };

        if public_key.is_empty() {
            return Err(ApiError::bad_request(
                "Missing algorithm or public key".to_string(),
            ));
        }

        match key_type {
            "master" => {
                if let Some(dk_storage) = &self.device_keys_storage {
                    if let Ok(Some(device_key)) = dk_storage
                        .get_device_key(user_id, device_id, "ed25519")
                        .await
                    {
                        let signing_key_id = format!("ed25519:{device_id}");
                        if !Self::verify_key_signature(
                            user_id,
                            key,
                            &device_key.public_key,
                            &signing_key_id,
                        ) {
                            return Err(ApiError::bad_request(
                                "Invalid signature on master key: not signed by device\'s ed25519 key"
                                    .to_string(),
                            ));
                        }
                    } else {
                        return Err(ApiError::bad_request(
                            "Cannot verify master key: no ed25519 device key found".to_string(),
                        ));
                    }
                }
            }
            "self_signing" | "user_signing" => {
                if let Ok(Some(master_key)) =
                    self.storage.get_cross_signing_key(user_id, "master").await
                {
                    if !Self::verify_cross_key_signature(user_id, key, &master_key) {
                        return Err(ApiError::bad_request(format!(
                            "Invalid signature on {key_type} key: not signed by master key"
                        )));
                    }
                } else {
                    return Err(ApiError::bad_request(
                        "Cannot verify cross-signing key: master key not found".to_string(),
                    ));
                }
            }
            _ => {}
        }

        let signatures = key
            .get("signatures")
            .cloned()
            .unwrap_or(serde_json::json!({}));

        let cross_signing_key = CrossSigningKey {
            id: uuid::Uuid::new_v4(),
            user_id: user_id.to_string(),
            key_type: key_type.to_string(),
            public_key,
            usage: usage
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default(),
            signatures,
            key_json: Some(key.clone()),
            created_ts: Utc::now(),
            updated_ts: Utc::now(),
        };

        self.storage
            .create_cross_signing_key(&cross_signing_key)
            .await?;
        Ok(())
    }

    fn verify_key_signature(
        user_id: &str,
        key_json: &serde_json::Value,
        public_key_base64: &str,
        signing_key_id: &str,
    ) -> bool {
        let signatures = match key_json.get("signatures").and_then(|v| v.as_object()) {
            Some(s) => s,
            None => return false,
        };
        let user_sigs = match signatures.get(user_id).and_then(|v| v.as_object()) {
            Some(s) => s,
            None => return false,
        };
        let signature = match user_sigs.get(signing_key_id).and_then(|v| v.as_str()) {
            Some(s) => s,
            None => return false,
        };
        verify_signed_json(
            user_id,
            signing_key_id,
            public_key_base64,
            signature,
            key_json,
        )
        .unwrap_or(false)
    }

    fn verify_cross_key_signature(
        user_id: &str,
        key_json: &serde_json::Value,
        master_key: &CrossSigningKey,
    ) -> bool {
        let master_key_id = master_key
            .key_json
            .as_ref()
            .and_then(|value| Self::extract_ed25519_key(value, "master_key").ok())
            .map_or_else(
                || format!("ed25519:{}", master_key.public_key),
                |(key_id, _)| key_id,
            );
        let signatures = match key_json.get("signatures").and_then(|v| v.as_object()) {
            Some(s) => s,
            None => return false,
        };
        let user_sigs = match signatures.get(user_id).and_then(|v| v.as_object()) {
            Some(s) => s,
            None => return false,
        };
        let signature = match user_sigs.get(&master_key_id).and_then(|v| v.as_str()) {
            Some(s) => s,
            None => return false,
        };
        verify_signed_json(
            user_id,
            &master_key_id,
            &master_key.public_key,
            signature,
            key_json,
        )
        .unwrap_or(false)
    }

    pub async fn upload_signatures(
        &self,
        user_id: &str,
        signatures: &BulkSignatureUpload,
    ) -> Result<SignatureUploadResponse, ApiError> {
        let mut fail: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();

        for (target_user_id, user_sigs) in &signatures.signatures {
            if let Some(user_sigs_obj) = user_sigs.as_object() {
                for (target_key_id, sig_data) in user_sigs_obj {
                    if let Some(sig_obj) = sig_data.as_object() {
                        for (signing_key_id, signature) in sig_obj {
                            let device_sig = DeviceSignature {
                                user_id: user_id.to_string(),
                                device_id: "".to_string(),
                                signing_key_id: signing_key_id.clone(),
                                target_user_id: target_user_id.clone(),
                                target_device_id: "".to_string(),
                                target_key_id: target_key_id.clone(),
                                signature: signature.as_str().unwrap_or("").to_string(),
                                created_ts: Utc::now(),
                            };
                            if let Err(e) = self.storage.save_device_signature(&device_sig).await {
                                fail.insert(
                                    format!("{target_user_id}:{target_key_id}"),
                                    serde_json::json!({
                                        "error": e.to_string(),
                                        "signing_key_id": signing_key_id,
                                    }),
                                );
                            }
                        }
                    }
                }
            }
        }

        Ok(SignatureUploadResponse { fail })
    }

    pub async fn get_user_signatures(&self, user_id: &str) -> Result<UserSignatures, ApiError> {
        let signatures = self.storage.get_user_signatures(user_id).await?;

        Ok(UserSignatures {
            user_id: user_id.to_string(),
            signatures,
        })
    }

    pub async fn verify_signature(
        &self,
        request: &SignatureVerificationRequest,
    ) -> Result<SignatureVerificationResponse, ApiError> {
        let sig = self
            .storage
            .get_signature(&request.user_id, &request.key_id, &request.signing_key_id)
            .await?;

        let Some(_signature_record) = sig else {
            return Ok(SignatureVerificationResponse {
                valid: false,
                verified_at: Utc::now(),
            });
        };

        let signing_key = self
            .storage
            .get_cross_signing_key(&request.user_id, &request.signing_key_id)
            .await?;

        let Some(key) = signing_key else {
            return Ok(SignatureVerificationResponse {
                valid: false,
                verified_at: Utc::now(),
            });
        };

        let valid = verify_signed_json(
            &request.user_id,
            &request.signing_key_id,
            &key.public_key,
            &request.signature,
            &serde_json::json!({
                "user_id": request.user_id,
                "key_id": request.key_id,
            }),
        )
        .unwrap_or(false);

        Ok(SignatureVerificationResponse {
            valid,
            verified_at: Utc::now(),
        })
    }

    pub async fn setup_cross_signing(
        &self,
        user_id: &str,
        request: &CrossSigningSetupRequest,
    ) -> Result<CrossSigningSetupResponse, ApiError> {
        let master_key = request.master_key.clone().unwrap_or_else(|| {
            serde_json::json!({
                "user_id": user_id,
                "usage": ["master"],
                "keys": {},
            })
        });

        let self_signing_key = request.self_signing_key.clone().unwrap_or_else(|| {
            serde_json::json!({
                "user_id": user_id,
                "usage": ["self_signing"],
                "keys": {},
            })
        });

        let user_signing_key = request.user_signing_key.clone().unwrap_or_else(|| {
            serde_json::json!({
                "user_id": user_id,
                "usage": ["user_signing"],
                "keys": {},
            })
        });

        let upload = CrossSigningUpload {
            master_key: master_key.clone(),
            self_signing_key: self_signing_key.clone(),
            user_signing_key: user_signing_key.clone(),
        };

        self.upload_cross_signing_keys(upload).await?;

        let master_key_signature = master_key["signatures"].as_object().and_then(|sigs| {
            sigs.values().next().and_then(|v| {
                v.as_object()
                    .and_then(|obj| obj.values().next())
                    .and_then(|sig_val| sig_val.as_str())
                    .map(|s| s.to_string())
            })
        });

        Ok(CrossSigningSetupResponse {
            master_key,
            self_signing_key,
            user_signing_key,
            master_key_signature,
        })
    }

    pub async fn get_device_signatures(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<Vec<DeviceSignature>, ApiError> {
        self.storage.get_device_signatures(user_id, device_id).await
    }

    pub async fn delete_cross_signing_keys(&self, user_id: &str) -> Result<(), ApiError> {
        self.storage.delete_cross_signing_keys(user_id).await?;
        if let Some(dehydrated_device_service) = &self.dehydrated_device_service {
            dehydrated_device_service
                .delete_all_for_user(user_id)
                .await?;
        }
        Ok(())
    }

    pub async fn sign_device(
        &self,
        user_id: &str,
        device_id: &str,
        signing_key_id: &str,
        signature: &str,
    ) -> Result<(), ApiError> {
        let device_sig = DeviceSignature {
            user_id: user_id.to_string(),
            device_id: device_id.to_string(),
            signing_key_id: signing_key_id.to_string(),
            target_user_id: user_id.to_string(),
            target_device_id: device_id.to_string(),
            target_key_id: device_id.to_string(),
            signature: signature.to_string(),
            created_ts: Utc::now(),
        };

        self.storage.save_device_signature(&device_sig).await
    }

    pub async fn sign_user(
        &self,
        user_id: &str,
        target_user_id: &str,
        signing_key_id: &str,
        signature: &str,
    ) -> Result<(), ApiError> {
        let device_sig = DeviceSignature {
            user_id: user_id.to_string(),
            device_id: "".to_string(),
            signing_key_id: signing_key_id.to_string(),
            target_user_id: target_user_id.to_string(),
            target_device_id: "".to_string(),
            target_key_id: "".to_string(),
            signature: signature.to_string(),
            created_ts: Utc::now(),
        };

        self.storage.save_device_signature(&device_sig).await
    }

    pub async fn verify_device_signature(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<DeviceVerificationStatus, ApiError> {
        let self_signing_key = self
            .storage
            .get_cross_signing_key(user_id, "self_signing")
            .await?;

        let Some(ssk) = self_signing_key else {
            return Ok(DeviceVerificationStatus {
                device_id: device_id.to_string(),
                is_verified: false,
                verified_by: None,
                verified_at: None,
            });
        };

        let signatures = self
            .storage
            .get_device_signatures(user_id, device_id)
            .await?;

        let ssk_sig = signatures.iter().find(|s| {
            s.signing_key_id.starts_with("ed25519:") || s.signing_key_id == "self_signing"
        });

        if let Some(sig) = ssk_sig {
            let device_key_json = if let Some(dk_storage) = &self.device_keys_storage {
                if let Ok(Some(device_key)) = dk_storage
                    .get_device_key(user_id, device_id, "signed_curve25519")
                    .await
                {
                    let mut json = serde_json::json!({
                        "user_id": user_id,
                        "device_id": device_id,
                        "algorithms": device_key.algorithm,
                        "keys": {
                            format!("{}:{}", device_key.algorithm, device_key.key_id): device_key.public_key
                        },
                    });
                    if let Some(obj) = json.as_object_mut() {
                        obj.remove("signatures");
                        obj.remove("unsigned");
                    }
                    json
                } else {
                    serde_json::json!({
                        "user_id": user_id,
                        "device_id": device_id,
                    })
                }
            } else {
                serde_json::json!({
                    "user_id": user_id,
                    "device_id": device_id,
                })
            };

            let signing_key_id = if sig.signing_key_id.starts_with("ed25519:") {
                sig.signing_key_id.clone()
            } else {
                format!("ed25519:{}", ssk.key_type)
            };

            let valid = verify_signed_json(
                user_id,
                &signing_key_id,
                &ssk.public_key,
                &sig.signature,
                &device_key_json,
            )
            .unwrap_or(false);

            Ok(DeviceVerificationStatus {
                device_id: device_id.to_string(),
                is_verified: valid,
                verified_by: if valid {
                    Some("self_signing".to_string())
                } else {
                    None
                },
                verified_at: if valid { Some(Utc::now()) } else { None },
            })
        } else {
            Ok(DeviceVerificationStatus {
                device_id: device_id.to_string(),
                is_verified: false,
                verified_by: None,
                verified_at: None,
            })
        }
    }

    pub async fn get_user_verification_status(
        &self,
        user_id: &str,
    ) -> Result<UserVerificationStatus, ApiError> {
        let master_key = self
            .storage
            .get_cross_signing_key(user_id, "master")
            .await?;
        let self_signing_key = self
            .storage
            .get_cross_signing_key(user_id, "self_signing")
            .await?;
        let user_signing_key = self
            .storage
            .get_cross_signing_key(user_id, "user_signing")
            .await?;

        let has_master = master_key.is_some();
        let has_self_signing = self_signing_key.is_some();
        let has_user_signing = user_signing_key.is_some();

        let mut is_verified = false;
        if let (Some(mk), Some(ssk)) = (&master_key, &self_signing_key) {
            let ssk_signature_valid = Self::verify_cross_signing_signature(
                &mk.public_key,
                &mk.signatures,
                ssk.key_json.as_ref(),
                "self_signing",
            );
            let mk_signature_valid = Self::verify_cross_signing_signature(
                &ssk.public_key,
                &ssk.signatures,
                mk.key_json.as_ref(),
                "master",
            );
            is_verified = ssk_signature_valid || mk_signature_valid;
        }

        if !is_verified && has_master && has_self_signing && has_user_signing {
            tracing::warn!(
                "Cross-signing keys exist for user {} but signature chain is invalid",
                user_id
            );
        }

        Ok(UserVerificationStatus {
            user_id: user_id.to_string(),
            is_verified,
            has_master_key: has_master,
            has_self_signing_key: has_self_signing,
            has_user_signing_key: has_user_signing,
            verified_at: if is_verified { Some(Utc::now()) } else { None },
        })
    }

    fn verify_cross_signing_signature(
        signing_public_key: &str,
        signatures: &serde_json::Value,
        target_key_json: Option<&serde_json::Value>,
        key_type: &str,
    ) -> bool {
        let sig_obj = match signatures.as_object() {
            Some(obj) => obj,
            None => return false,
        };

        let target_json = if let Some(json) = target_key_json {
            let mut json = json.clone();
            if let Some(obj) = json.as_object_mut() {
                obj.remove("signatures");
                obj.remove("unsigned");
            }
            json
        } else {
            serde_json::json!({
                "key_type": key_type,
            })
        };

        for (signer_id, key_sigs) in sig_obj {
            if let Some(key_sigs_obj) = key_sigs.as_object() {
                for (signing_key_id, signature_value) in key_sigs_obj {
                    if !signing_key_id.starts_with("ed25519:") {
                        continue;
                    }

                    let signature_str = match signature_value.as_str() {
                        Some(s) => s,
                        None => continue,
                    };

                    if verify_signed_json(
                        signer_id,
                        signing_key_id,
                        signing_public_key,
                        signature_str,
                        &target_json,
                    )
                    .unwrap_or(false)
                    {
                        return true;
                    }
                }
            }
        }

        false
    }

    pub async fn verify_device_key(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<DeviceKeyVerificationResult, ApiError> {
        let master_key = self
            .storage
            .get_cross_signing_key(user_id, "master")
            .await?;
        let self_signing_key = self
            .storage
            .get_cross_signing_key(user_id, "self_signing")
            .await?;

        let verified_by_master = if let Some(ref mk) = master_key {
            let signatures = self
                .storage
                .get_device_signatures(user_id, device_id)
                .await?;

            signatures.iter().any(|sig| {
                if sig.signing_key_id.starts_with("ed25519:") {
                    let signing_key_id = Self::extract_ed25519_key(
                        mk.key_json.as_ref().unwrap_or(&serde_json::json!({})),
                        "master_key",
                    )
                    .map_or_else(|_| format!("ed25519:{}", mk.public_key), |(kid, _)| kid);

                    sig.signing_key_id == signing_key_id
                        || sig.signing_key_id == format!("ed25519:{}", mk.public_key)
                } else {
                    false
                }
            })
        } else {
            false
        };

        let verified_by_self_signing = if let Some(ref ssk) = self_signing_key {
            let signatures = self
                .storage
                .get_device_signatures(user_id, device_id)
                .await?;

            let ssk_key_id = Self::extract_ed25519_key(
                ssk.key_json.as_ref().unwrap_or(&serde_json::json!({})),
                "self_signing_key",
            )
            .map_or_else(|_| format!("ed25519:{}", ssk.public_key), |(kid, _)| kid);

            let ssk_sig = signatures.iter().find(|s| {
                s.signing_key_id == ssk_key_id || s.signing_key_id.starts_with("ed25519:")
            });

            if let Some(sig) = ssk_sig {
                let device_key_json = if let Some(dk_storage) = &self.device_keys_storage {
                    if let Ok(Some(device_key)) = dk_storage
                        .get_device_key(user_id, device_id, "signed_curve25519")
                        .await
                    {
                        let mut json = serde_json::json!({
                            "user_id": user_id,
                            "device_id": device_id,
                            "algorithms": device_key.algorithm,
                            "keys": {
                                format!("{}:{}", device_key.algorithm, device_key.key_id): device_key.public_key
                            },
                        });
                        if let Some(obj) = json.as_object_mut() {
                            obj.remove("signatures");
                            obj.remove("unsigned");
                        }
                        json
                    } else {
                        serde_json::json!({
                            "user_id": user_id,
                            "device_id": device_id,
                        })
                    }
                } else {
                    serde_json::json!({
                        "user_id": user_id,
                        "device_id": device_id,
                    })
                };

                let signing_key_id = if sig.signing_key_id.starts_with("ed25519:") {
                    sig.signing_key_id.clone()
                } else {
                    format!("ed25519:{}", ssk.key_type)
                };

                verify_signed_json(
                    user_id,
                    &signing_key_id,
                    &ssk.public_key,
                    &sig.signature,
                    &device_key_json,
                )
                .unwrap_or(false)
            } else {
                false
            }
        } else {
            false
        };

        let is_verified = verified_by_master || verified_by_self_signing;
        let verification_method = if verified_by_master {
            Some("master_key".to_string())
        } else if verified_by_self_signing {
            Some("self_signing_key".to_string())
        } else {
            None
        };

        Ok(DeviceKeyVerificationResult {
            user_id: user_id.to_string(),
            device_id: device_id.to_string(),
            is_verified,
            verified_by_master,
            verified_by_self_signing,
            verification_method,
            verified_at: if is_verified { Some(Utc::now()) } else { None },
        })
    }

    pub async fn get_verified_devices(
        &self,
        user_id: &str,
    ) -> Result<VerifiedDevicesMap, ApiError> {
        let mut verified_devices = Vec::new();

        if let Some(dk_storage) = &self.device_keys_storage {
            let all_keys = dk_storage
                .get_all_device_keys(user_id)
                .await
                .unwrap_or_default();

            let mut seen_device_ids = std::collections::HashSet::new();
            for key in &all_keys {
                seen_device_ids.insert(key.device_id.clone());
            }

            for device_id in &seen_device_ids {
                let result = self.verify_device_key(user_id, device_id).await?;
                verified_devices.push(result);
            }
        }

        Ok(VerifiedDevicesMap {
            user_id: user_id.to_string(),
            verified_devices,
        })
    }

    /// Batch version of `get_verified_devices` that avoids N+1 queries.
    /// Fetches cross-signing keys, device signatures, and device keys for all
    /// users in a fixed number of SQL queries, then performs verification
    /// in memory.
    pub async fn get_verified_devices_batch(
        &self,
        user_ids: &[String],
    ) -> Result<HashMap<String, VerifiedDevicesMap>, ApiError> {
        if user_ids.is_empty() {
            return Ok(HashMap::new());
        }

        // 1. Batch-fetch cross-signing keys for all users (1 query)
        let cs_keys_by_user = self
            .storage
            .get_cross_signing_keys_batch(user_ids)
            .await?;

        // 2. Batch-fetch device signatures for all users (1 query)
        let sigs_by_user = self
            .storage
            .get_device_signatures_batch(user_ids)
            .await?;

        // 3. Batch-fetch device keys for all users (1 query if
        //    device_keys_storage is present; we collect unique device_ids per
        //    user from the signatures we already have as a fallback)
        let mut device_ids_by_user: HashMap<String, HashSet<String>> = HashMap::new();
        if let Some(dk_storage) = &self.device_keys_storage {
            if let Ok(all_keys_by_user) = dk_storage.get_all_device_keys_batch(user_ids).await {
                for (user_id, all_keys) in all_keys_by_user {
                    let ids: HashSet<String> =
                        all_keys.iter().map(|k| k.device_id.clone()).collect();
                    if !ids.is_empty() {
                        device_ids_by_user.insert(user_id, ids);
                    }
                }
            }
        }

        // Fallback: if we didn't get device keys from storage, extract device
        // IDs from the signatures we already fetched.
        for (user_id, sigs) in &sigs_by_user {
            if !device_ids_by_user.contains_key(user_id) {
                let ids: HashSet<String> =
                    sigs.iter().map(|s| s.target_device_id.clone()).collect();
                if !ids.is_empty() {
                    device_ids_by_user.insert(user_id.clone(), ids);
                }
            }
        }

        // 4. Perform verification in memory for each user
        let mut result: HashMap<String, VerifiedDevicesMap> = HashMap::new();
        for user_id in user_ids {
            let cs_keys = cs_keys_by_user.get(user_id).cloned().unwrap_or_default();
            let sigs = sigs_by_user.get(user_id).cloned().unwrap_or_default();
            let device_ids = device_ids_by_user
                .get(user_id)
                .cloned()
                .unwrap_or_default();

            let master_key = cs_keys.iter().find(|k| k.key_type == "master").cloned();
            let self_signing_key = cs_keys
                .iter()
                .find(|k| k.key_type == "self_signing")
                .cloned();

            let mut verified_devices = Vec::new();
            for device_id in &device_ids {
                let verified = self
                    .verify_device_key_with_prefetched(
                        user_id,
                        device_id,
                        &master_key,
                        &self_signing_key,
                        &sigs,
                    )
                    .await;
                verified_devices.push(verified);
            }

            result.insert(
                user_id.clone(),
                VerifiedDevicesMap {
                    user_id: user_id.clone(),
                    verified_devices,
                },
            );
        }

        Ok(result)
    }

    /// Same logic as `verify_device_key` but uses pre-fetched data instead of
    /// issuing individual SQL queries.
    async fn verify_device_key_with_prefetched(
        &self,
        user_id: &str,
        device_id: &str,
        master_key: &Option<CrossSigningKey>,
        self_signing_key: &Option<CrossSigningKey>,
        all_signatures: &[DeviceSignature],
    ) -> DeviceKeyVerificationResult {
        // Filter signatures for this specific device
        let signatures: Vec<&DeviceSignature> = all_signatures
            .iter()
            .filter(|s| s.target_device_id == *device_id)
            .collect();

        let verified_by_master = if let Some(ref mk) = master_key {
            signatures.iter().any(|sig| {
                if sig.signing_key_id.starts_with("ed25519:") {
                    let signing_key_id = Self::extract_ed25519_key(
                        mk.key_json.as_ref().unwrap_or(&serde_json::json!({})),
                        "master_key",
                    )
                    .map_or_else(|_| format!("ed25519:{}", mk.public_key), |(kid, _)| kid);

                    sig.signing_key_id == signing_key_id
                        || sig.signing_key_id == format!("ed25519:{}", mk.public_key)
                } else {
                    false
                }
            })
        } else {
            false
        };

        let verified_by_self_signing = if let Some(ref ssk) = self_signing_key {
            let ssk_key_id = Self::extract_ed25519_key(
                ssk.key_json.as_ref().unwrap_or(&serde_json::json!({})),
                "self_signing_key",
            )
            .map_or_else(|_| format!("ed25519:{}", ssk.public_key), |(kid, _)| kid);

            let ssk_sig = signatures.iter().find(|s| {
                s.signing_key_id == ssk_key_id || s.signing_key_id.starts_with("ed25519:")
            });

            if let Some(sig) = ssk_sig {
                let device_key_json = if let Some(dk_storage) = &self.device_keys_storage {
                    if let Ok(Some(device_key)) = dk_storage
                        .get_device_key(user_id, device_id, "signed_curve25519")
                        .await
                    {
                        let mut json = serde_json::json!({
                            "user_id": user_id,
                            "device_id": device_id,
                            "algorithms": device_key.algorithm,
                            "keys": {
                                format!("{}:{}", device_key.algorithm, device_key.key_id): device_key.public_key
                            },
                        });
                        if let Some(obj) = json.as_object_mut() {
                            obj.remove("signatures");
                            obj.remove("unsigned");
                        }
                        json
                    } else {
                        serde_json::json!({
                            "user_id": user_id,
                            "device_id": device_id,
                        })
                    }
                } else {
                    serde_json::json!({
                        "user_id": user_id,
                        "device_id": device_id,
                    })
                };

                let signing_key_id = if sig.signing_key_id.starts_with("ed25519:") {
                    sig.signing_key_id.clone()
                } else {
                    format!("ed25519:{}", ssk.key_type)
                };

                verify_signed_json(
                    user_id,
                    &signing_key_id,
                    &ssk.public_key,
                    &sig.signature,
                    &device_key_json,
                )
                .unwrap_or(false)
            } else {
                false
            }
        } else {
            false
        };

        let is_verified = verified_by_master || verified_by_self_signing;
        let verification_method = if verified_by_master {
            Some("master_key".to_string())
        } else if verified_by_self_signing {
            Some("self_signing_key".to_string())
        } else {
            None
        };

        DeviceKeyVerificationResult {
            user_id: user_id.to_string(),
            device_id: device_id.to_string(),
            is_verified,
            verified_by_master,
            verified_by_self_signing,
            verification_method,
            verified_at: if is_verified { Some(Utc::now()) } else { None },
        }
    }
}
