// Key Rotation Service - 密钥轮换服务
// 管理用户设备密钥的轮换策略和历史记录

use crate::common::ApiResult;
use async_trait::async_trait;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct KeyRotationConfig {
    pub enabled: bool,
    pub interval_days: u32,
    pub max_history_records: usize,
}

impl Default for KeyRotationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_days: 7,
            max_history_records: 100,
        }
    }
}

#[derive(Debug, Clone)]
pub struct KeyRotationRecord {
    pub user_id: String,
    pub device_id: String,
    pub key_id: String,
    pub rotated_at: i64,
    pub rotation_reason: Option<String>,
}

#[async_trait]
pub trait KeyRotationService: Send + Sync {
    async fn get_last_rotation(&self, user_id: &str) -> ApiResult<Option<i64>>;
    async fn get_last_rotation_for_device(&self, user_id: &str, device_id: &str) -> ApiResult<Option<i64>>;
    async fn needs_rotation(&self, user_id: &str, device_id: &str) -> ApiResult<bool>;
    async fn record_rotation(&self, user_id: &str, device_id: &str, key_id: &str, timestamp: i64) -> ApiResult<()>;
    async fn get_rotation_history(&self, user_id: &str, device_id: &str) -> ApiResult<Vec<KeyRotationRecord>>;
    async fn get_all_user_rotations(&self, user_id: &str) -> ApiResult<Vec<KeyRotationRecord>>;
    async fn cleanup_old_records(&self, user_id: &str) -> ApiResult<usize>;
}

pub struct KeyRotationServiceImpl {
    #[allow(dead_code)]
    records: Arc<RwLock<HashMap<String, Vec<KeyRotationRecord>>>>,
    config: KeyRotationConfig,
}

impl KeyRotationServiceImpl {
    pub fn new() -> Self {
        Self {
            #[allow(dead_code)]
    records: Arc::new(RwLock::new(HashMap::new())),
            config: KeyRotationConfig::default(),
        }
    }

    pub fn with_config(config: KeyRotationConfig) -> Self {
        Self {
            #[allow(dead_code)]
    records: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    fn make_key(user_id: &str, device_id: &str) -> String {
        format!("{}:{}", user_id, device_id)
    }

    fn make_user_key(user_id: &str) -> String {
        user_id.to_string()
    }
}

impl Default for KeyRotationServiceImpl {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl KeyRotationService for KeyRotationServiceImpl {
    async fn get_last_rotation(&self, user_id: &str) -> ApiResult<Option<i64>> {
        let records = self.records.read().await;
        let user_key = Self::make_user_key(user_id);
        
        if let Some(user_records) = records.get(&user_key) {
            let max_ts = user_records.iter().map(|r| r.rotated_at).max();
            return Ok(max_ts);
        }
        
        Ok(None)
    }

    async fn get_last_rotation_for_device(&self, user_id: &str, device_id: &str) -> ApiResult<Option<i64>> {
        let records = self.records.read().await;
        let key = Self::make_key(user_id, device_id);
        
        if let Some(device_records) = records.get(&key) {
            if let Some(last) = device_records.last() {
                return Ok(Some(last.rotated_at));
            }
        }
        
        Ok(None)
    }

    async fn needs_rotation(&self, user_id: &str, device_id: &str) -> ApiResult<bool> {
        if !self.config.enabled {
            return Ok(false);
        }
        
        let last_rotation = self.get_last_rotation_for_device(user_id, device_id).await?;
        
        match last_rotation {
            None => Ok(true),
            Some(ts) => {
                let now = Utc::now().timestamp_millis();
                let interval_ms = (self.config.interval_days as i64) * 24 * 60 * 60 * 1000;
                Ok(now - ts > interval_ms)
            }
        }
    }

    async fn record_rotation(&self, user_id: &str, device_id: &str, key_id: &str, timestamp: i64) -> ApiResult<()> {
        let mut records = self.records.write().await;
        
        let device_key = Self::make_key(user_id, device_id);
        let user_key = Self::make_user_key(user_id);
        
        let record = KeyRotationRecord {
            user_id: user_id.to_string(),
            device_id: device_id.to_string(),
            key_id: key_id.to_string(),
            rotated_at: timestamp,
            rotation_reason: None,
        };
        
        records.entry(device_key.clone())
            .or_insert_with(Vec::new)
            .push(record.clone());
        
        records.entry(user_key)
            .or_insert_with(Vec::new)
            .push(record);
        
        Ok(())
    }

    async fn get_rotation_history(&self, user_id: &str, device_id: &str) -> ApiResult<Vec<KeyRotationRecord>> {
        let records = self.records.read().await;
        let key = Self::make_key(user_id, device_id);
        
        Ok(records.get(&key).cloned().unwrap_or_default())
    }

    async fn get_all_user_rotations(&self, user_id: &str) -> ApiResult<Vec<KeyRotationRecord>> {
        let records = self.records.read().await;
        let user_key = Self::make_user_key(user_id);
        
        Ok(records.get(&user_key).cloned().unwrap_or_default())
    }

    async fn cleanup_old_records(&self, user_id: &str) -> ApiResult<usize> {
        let mut records = self.records.write().await;
        let user_key = Self::make_user_key(user_id);
        
        let mut removed = 0;
        
        if let Some(user_records) = records.get_mut(&user_key) {
            if user_records.len() > self.config.max_history_records {
                let excess = user_records.len() - self.config.max_history_records;
                user_records.drain(0..excess);
                removed = excess;
            }
        }
        
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_record_and_get_rotation() {
        let service = KeyRotationServiceImpl::new();
        let now = Utc::now().timestamp_millis();
        
        service.record_rotation("@alice:example.com", "DEVICE1", "key_1", now).await.unwrap();
        
        let last = service.get_last_rotation_for_device("@alice:example.com", "DEVICE1").await.unwrap();
        assert_eq!(last, Some(now));
    }

    #[tokio::test]
    async fn test_needs_rotation_no_records() {
        let service = KeyRotationServiceImpl::new();
        
        let needs = service.needs_rotation("@alice:example.com", "DEVICE1").await.unwrap();
        assert!(needs);
    }

    #[tokio::test]
    async fn test_needs_rotation_recent() {
        let service = KeyRotationServiceImpl::new();
        let now = Utc::now().timestamp_millis();
        
        service.record_rotation("@alice:example.com", "DEVICE1", "key_1", now).await.unwrap();
        
        let needs = service.needs_rotation("@alice:example.com", "DEVICE1").await.unwrap();
        assert!(!needs);
    }

    #[tokio::test]
    async fn test_get_rotation_history() {
        let service = KeyRotationServiceImpl::new();
        let now = Utc::now().timestamp_millis();
        
        service.record_rotation("@alice:example.com", "DEVICE1", "key_1", now).await.unwrap();
        service.record_rotation("@alice:example.com", "DEVICE1", "key_2", now + 1000).await.unwrap();
        
        let history = service.get_rotation_history("@alice:example.com", "DEVICE1").await.unwrap();
        assert_eq!(history.len(), 2);
    }

    #[tokio::test]
    async fn test_get_all_user_rotations() {
        let service = KeyRotationServiceImpl::new();
        let now = Utc::now().timestamp_millis();
        
        service.record_rotation("@alice:example.com", "DEVICE1", "key_1", now).await.unwrap();
        service.record_rotation("@alice:example.com", "DEVICE2", "key_2", now + 1000).await.unwrap();
        
        let all = service.get_all_user_rotations("@alice:example.com").await.unwrap();
        assert_eq!(all.len(), 2);
    }
}
