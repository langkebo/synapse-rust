use super::models::*;
use super::storage::CrossSigningStorage;
use crate::e2ee::device_keys::DeviceKeyStorage;
use crate::e2ee::signed_json::verify_signed_json;
use crate::error::ApiError;
use chrono::Utc;
use std::sync::Arc;

#[derive(Clone)]
pub struct CrossSigningService {
    storage: CrossSigningStorage,
    device_keys_storage: Option<Arc<DeviceKeyStorage>>,
}

impl CrossSigningService {
    pub fn new(storage: CrossSigningStorage) -> Self {
        Self {
            storage,
            device_keys_storage: None,
        }
    }

    pub fn with_device_keys_storage(mut self, storage: Arc<DeviceKeyStorage>) -> Self {
        self.device_keys_storage = Some(storage);
        self
    }

    pub async fn upload_cross_signing_keys(
        &self,
        upload: CrossSigningUpload,
    ) -> Result<(), ApiError> {
        let user_id = upload.master_key["user_id"]
            .as_str()
            .ok_or_else(|| ApiError::bad_request("Missing user_id in master_key".to_string()))?;

        let master_public_key = upload.master_key["keys"]["ed25519:MASTER"]
            .as_str()
            .ok_or_else(|| ApiError::bad_request("Missing ed25519:MASTER key".to_string()))?;

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

        let self_signing_public_key = upload.self_signing_key["keys"]["ed25519:SELF_SIGNING"]
            .as_str()
            .ok_or_else(|| ApiError::bad_request("Missing ed25519:SELF_SIGNING key".to_string()))?;

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

        let user_signing_public_key = upload.user_signing_key["keys"]["ed25519:USER_SIGNING"]
            .as_str()
            .ok_or_else(|| ApiError::bad_request("Missing ed25519:USER_SIGNING key".to_string()))?;

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
        _device_id: &str,
        key: &serde_json::Value,
    ) -> Result<(), ApiError> {
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
                                    format!("{}:{}", target_user_id, target_key_id),
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
        self.storage.delete_cross_signing_keys(user_id).await
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
            let ssk_signature_valid = self.verify_cross_signing_signature(
                &mk.public_key,
                &mk.signatures,
                ssk.key_json.as_ref(),
                "self_signing",
            );
            let mk_signature_valid = self.verify_cross_signing_signature(
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
        &self,
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
}
