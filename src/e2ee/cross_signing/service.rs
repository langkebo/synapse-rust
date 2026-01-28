use super::models::*;
use super::storage::CrossSigningStorage;
use std::sync::Arc;
use crate::error::ApiError;

pub struct CrossSigningService {
    storage: CrossSigningStorage<'static>,
}

impl CrossSigningService {
    pub fn new(storage: CrossSigningStorage<'static>) -> Self {
        Self { storage }
    }
    
    pub async fn upload_cross_signing_keys(&self, upload: CrossSigningUpload) -> Result<(), ApiError> {
        let user_id = upload.master_key["user_id"].as_str().unwrap();
        
        let master_key = CrossSigningKey {
            id: uuid::Uuid::new_v4(),
            user_id: user_id.to_string(),
            key_type: "master".to_string(),
            public_key: upload.master_key["keys"]["ed25519:MASTER"].as_str().unwrap().to_string(),
            usage: upload.master_key["usage"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect(),
            signatures: upload.master_key["signatures"].clone(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        self.storage.create_cross_signing_key(&master_key).await?;
        
        let self_signing_key = CrossSigningKey {
            id: uuid::Uuid::new_v4(),
            user_id: user_id.to_string(),
            key_type: "self_signing".to_string(),
            public_key: upload.self_signing_key["keys"]["ed25519:SELF_SIGNING"].as_str().unwrap().to_string(),
            usage: upload.self_signing_key["usage"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect(),
            signatures: upload.self_signing_key["signatures"].clone(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        self.storage.create_cross_signing_key(&self_signing_key).await?;
        
        let user_signing_key = CrossSigningKey {
            id: uuid::Uuid::new_v4(),
            user_id: user_id.to_string(),
            key_type: "user_signing".to_string(),
            public_key: upload.user_signing_key["keys"]["ed25519:USER_SIGNING"].as_str().unwrap().to_string(),
            usage: upload.user_signing_key["usage"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect(),
            signatures: upload.user_signing_key["signatures"].clone(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        self.storage.create_cross_signing_key(&user_signing_key).await?;
        
        Ok(())
    }
    
    pub async fn get_cross_signing_keys(&self, user_id: &str) -> Result<CrossSigningKeys, ApiError> {
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
}