use super::models::*;
use super::storage::CrossSigningStorage;
use crate::error::ApiError;
use chrono::Utc;

#[derive(Clone)]
pub struct CrossSigningService {
    storage: CrossSigningStorage,
}

impl CrossSigningService {
    pub fn new(storage: CrossSigningStorage) -> Self {
        Self { storage }
    }

    pub async fn upload_cross_signing_keys(
        &self,
        upload: CrossSigningUpload,
    ) -> Result<(), ApiError> {
        let user_id = upload.master_key["user_id"].as_str().unwrap();

        let master_key = CrossSigningKey {
            id: uuid::Uuid::new_v4(),
            user_id: user_id.to_string(),
            key_type: "master".to_string(),
            public_key: upload.master_key["keys"]["ed25519:MASTER"]
                .as_str()
                .unwrap()
                .to_string(),
            usage: upload.master_key["usage"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect(),
            signatures: upload.master_key["signatures"].clone(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        self.storage.create_cross_signing_key(&master_key).await?;

        let self_signing_key = CrossSigningKey {
            id: uuid::Uuid::new_v4(),
            user_id: user_id.to_string(),
            key_type: "self_signing".to_string(),
            public_key: upload.self_signing_key["keys"]["ed25519:SELF_SIGNING"]
                .as_str()
                .unwrap()
                .to_string(),
            usage: upload.self_signing_key["usage"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect(),
            signatures: upload.self_signing_key["signatures"].clone(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        self.storage
            .create_cross_signing_key(&self_signing_key)
            .await?;

        let user_signing_key = CrossSigningKey {
            id: uuid::Uuid::new_v4(),
            user_id: user_id.to_string(),
            key_type: "user_signing".to_string(),
            public_key: upload.user_signing_key["keys"]["ed25519:USER_SIGNING"]
                .as_str()
                .unwrap()
                .to_string(),
            usage: upload.user_signing_key["usage"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect(),
            signatures: upload.user_signing_key["signatures"].clone(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
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

        let master_key = keys.iter().find(|k| k.key_type == "master").unwrap();
        let self_signing_key = keys.iter().find(|k| k.key_type == "self_signing").unwrap();
        let user_signing_key = keys.iter().find(|k| k.key_type == "user_signing").unwrap();

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
            let mut signatures = k.signatures.as_object().unwrap().clone();
            signatures.insert(user_id.to_string(), signature.clone());
            k.signatures = serde_json::Value::Object(signatures);
            k.updated_at = Utc::now();
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
        let device_key = DeviceKeyInfo {
            user_id: user_id.to_string(),
            device_id: device_id.to_string(),
            key_type: key["type"].as_str().unwrap().to_string(),
            algorithm: key["algorithm"].as_str().unwrap().to_string(),
            public_key: key["key"].as_str().unwrap().to_string(),
            signatures: key["signatures"].clone(),
            created_at: Utc::now(),
        };
        self.storage.save_device_key(&device_key).await?;
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
                                created_at: Utc::now(),
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

    pub async fn get_user_signatures(
        &self,
        user_id: &str,
    ) -> Result<UserSignatures, ApiError> {
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

        let valid = sig
            .map(|s| s.signature == request.signature)
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

        let master_key_signature = master_key["signatures"]
            .as_object()
            .and_then(|sigs| {
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

    pub async fn delete_cross_signing_keys(
        &self,
        user_id: &str,
    ) -> Result<(), ApiError> {
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
            created_at: Utc::now(),
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
            created_at: Utc::now(),
        };

        self.storage.save_device_signature(&device_sig).await
    }
}
