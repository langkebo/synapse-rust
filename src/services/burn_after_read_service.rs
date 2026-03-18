// Burn After Read Service - 阅后即焚服务
// 管理消息的阅后即焚功能

use async_trait::async_trait;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct BurnSettings {
    pub enabled: bool,
    pub burn_after_ms: i64,
}

#[derive(Debug, Clone)]
pub struct BurnEvent {
    pub event_id: String,
    pub room_id: String,
    pub user_id: String,
    pub created_at: i64,
    pub delete_at: i64,
}

#[derive(Debug, Clone, Default)]
pub struct BurnStats {
    pub total_burned: i64,
    pub total_pending: i64,
    pub rooms_enabled: i64,
}

#[async_trait]
pub trait BurnAfterReadService: Send + Sync {
    async fn set_burn_enabled(&self, user_id: &str, room_id: &str, enabled: bool, burn_after_ms: i64) -> crate::common::ApiResult<()>;
    async fn get_burn_settings(&self, user_id: &str, room_id: &str) -> crate::common::ApiResult<Option<BurnSettings>>;
    async fn get_pending_burns(&self, user_id: &str, room_id: &str) -> crate::common::ApiResult<Vec<BurnEvent>>;
    async fn cancel_burn(&self, user_id: &str, room_id: &str, event_id: &str) -> crate::common::ApiResult<()>;
    async fn delete_burned_message(&self, user_id: &str, room_id: &str, event_id: &str) -> crate::common::ApiResult<()>;
    async fn set_user_default(&self, user_id: &str, default_burn_ms: i64) -> crate::common::ApiResult<()>;
    async fn get_user_stats(&self, user_id: &str) -> crate::common::ApiResult<BurnStats>;
    async fn schedule_burn(&self, user_id: &str, room_id: &str, event_id: &str, burn_after_ms: i64) -> crate::common::ApiResult<()>;
    async fn process_expired_burns(&self) -> crate::common::ApiResult<Vec<BurnEvent>>;
}

pub struct BurnAfterReadServiceImpl {
    settings: Arc<RwLock<HashMap<String, HashMap<String, BurnSettings>>>>,
    pending_burns: Arc<RwLock<HashMap<String, Vec<BurnEvent>>>>,
    user_defaults: Arc<RwLock<HashMap<String, i64>>>,
    burned_events: Arc<RwLock<HashMap<String, i64>>>,
}

impl BurnAfterReadServiceImpl {
    pub fn new() -> Self {
        Self {
            settings: Arc::new(RwLock::new(HashMap::new())),
            pending_burns: Arc::new(RwLock::new(HashMap::new())),
            user_defaults: Arc::new(RwLock::new(HashMap::new())),
            burned_events: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn make_key(user_id: &str, room_id: &str) -> String {
        format!("{}:{}", user_id, room_id)
    }
}

impl Default for BurnAfterReadServiceImpl {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BurnAfterReadService for BurnAfterReadServiceImpl {
    async fn set_burn_enabled(&self, user_id: &str, room_id: &str, enabled: bool, burn_after_ms: i64) -> crate::common::ApiResult<()> {
        let mut settings = self.settings.write().await;
        
        settings
            .entry(user_id.to_string())
            .or_insert_with(HashMap::new)
            .insert(room_id.to_string(), BurnSettings {
                enabled,
                burn_after_ms,
            });
        
        Ok(())
    }

    async fn get_burn_settings(&self, user_id: &str, room_id: &str) -> crate::common::ApiResult<Option<BurnSettings>> {
        let settings = self.settings.read().await;
        
        Ok(settings
            .get(user_id)
            .and_then(|room_settings| room_settings.get(room_id).cloned()))
    }

    async fn get_pending_burns(&self, user_id: &str, room_id: &str) -> crate::common::ApiResult<Vec<BurnEvent>> {
        let key = Self::make_key(user_id, room_id);
        let burns = self.pending_burns.read().await;
        
        Ok(burns.get(&key).cloned().unwrap_or_default())
    }

    async fn cancel_burn(&self, user_id: &str, room_id: &str, event_id: &str) -> crate::common::ApiResult<()> {
        let key = Self::make_key(user_id, room_id);
        let mut burns = self.pending_burns.write().await;
        
        if let Some(events) = burns.get_mut(&key) {
            events.retain(|e| e.event_id != event_id);
        }
        
        Ok(())
    }

    async fn delete_burned_message(&self, user_id: &str, room_id: &str, event_id: &str) -> crate::common::ApiResult<()> {
        let key = Self::make_key(user_id, room_id);
        
        let mut burns = self.pending_burns.write().await;
        if let Some(events) = burns.get_mut(&key) {
            events.retain(|e| e.event_id != event_id);
        }
        
        let mut burned = self.burned_events.write().await;
        burned.insert(format!("{}:{}:{}", user_id, room_id, event_id), Utc::now().timestamp_millis());
        
        Ok(())
    }

    async fn set_user_default(&self, user_id: &str, default_burn_ms: i64) -> crate::common::ApiResult<()> {
        let mut defaults = self.user_defaults.write().await;
        defaults.insert(user_id.to_string(), default_burn_ms);
        Ok(())
    }

    async fn get_user_stats(&self, user_id: &str) -> crate::common::ApiResult<BurnStats> {
        let settings = self.settings.read().await;
        let pending = self.pending_burns.read().await;
        let burned = self.burned_events.read().await;
        
        let rooms_enabled = settings
            .get(user_id)
            .map(|rs| rs.values().filter(|s| s.enabled).count() as i64)
            .unwrap_or(0);
        
        let total_pending: i64 = pending
            .keys()
            .filter(|k| k.starts_with(&format!("{}:", user_id)))
            .map(|k| pending.get(k).map(|v| v.len() as i64).unwrap_or(0))
            .sum();
        
        let total_burned = burned
            .keys()
            .filter(|k| k.starts_with(&format!("{}:", user_id)))
            .count() as i64;
        
        Ok(BurnStats {
            total_burned,
            total_pending,
            rooms_enabled,
        })
    }

    async fn schedule_burn(&self, user_id: &str, room_id: &str, event_id: &str, burn_after_ms: i64) -> crate::common::ApiResult<()> {
        let key = Self::make_key(user_id, room_id);
        let now = Utc::now().timestamp_millis();
        
        let burn_event = BurnEvent {
            event_id: event_id.to_string(),
            room_id: room_id.to_string(),
            user_id: user_id.to_string(),
            created_at: now,
            delete_at: now + burn_after_ms,
        };
        
        let mut burns = self.pending_burns.write().await;
        burns.entry(key).or_insert_with(Vec::new).push(burn_event);
        
        Ok(())
    }

    async fn process_expired_burns(&self) -> crate::common::ApiResult<Vec<BurnEvent>> {
        let now = Utc::now().timestamp_millis();
        let mut burns = self.pending_burns.write().await;
        let mut burned = self.burned_events.write().await;
        let mut expired = Vec::new();
        
        for (_key, events) in burns.iter_mut() {
            let expired_events: Vec<BurnEvent> = events
                .iter()
                .filter(|e| e.delete_at <= now)
                .cloned()
                .collect();
            
            for e in &expired_events {
                burned.insert(
                    format!("{}:{}:{}", e.user_id, e.room_id, e.event_id),
                    now,
                );
            }
            
            expired.extend(expired_events);
            
            events.retain(|e| e.delete_at > now);
        }
        
        Ok(expired)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_set_and_get_burn_settings() {
        let service = BurnAfterReadServiceImpl::new();
        
        service.set_burn_enabled("@alice:example.com", "!room:example.com", true, 60000).await.unwrap();
        
        let settings = service.get_burn_settings("@alice:example.com", "!room:example.com").await.unwrap();
        assert!(settings.is_some());
        let s = settings.unwrap();
        assert!(s.enabled);
        assert_eq!(s.burn_after_ms, 60000);
    }

    #[tokio::test]
    async fn test_schedule_and_get_pending_burns() {
        let service = BurnAfterReadServiceImpl::new();
        
        service.schedule_burn("@alice:example.com", "!room:example.com", "$event1", 60000).await.unwrap();
        
        let pending = service.get_pending_burns("@alice:example.com", "!room:example.com").await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].event_id, "$event1");
    }

    #[tokio::test]
    async fn test_cancel_burn() {
        let service = BurnAfterReadServiceImpl::new();
        
        service.schedule_burn("@alice:example.com", "!room:example.com", "$event1", 60000).await.unwrap();
        service.cancel_burn("@alice:example.com", "!room:example.com", "$event1").await.unwrap();
        
        let pending = service.get_pending_burns("@alice:example.com", "!room:example.com").await.unwrap();
        assert!(pending.is_empty());
    }

    #[tokio::test]
    async fn test_delete_burned_message() {
        let service = BurnAfterReadServiceImpl::new();
        
        service.schedule_burn("@alice:example.com", "!room:example.com", "$event1", 60000).await.unwrap();
        service.delete_burned_message("@alice:example.com", "!room:example.com", "$event1").await.unwrap();
        
        let pending = service.get_pending_burns("@alice:example.com", "!room:example.com").await.unwrap();
        assert!(pending.is_empty());
        
        let stats = service.get_user_stats("@alice:example.com").await.unwrap();
        assert_eq!(stats.total_burned, 1);
    }

    #[tokio::test]
    async fn test_get_user_stats() {
        let service = BurnAfterReadServiceImpl::new();
        
        service.set_burn_enabled("@alice:example.com", "!room1:example.com", true, 60000).await.unwrap();
        service.set_burn_enabled("@alice:example.com", "!room2:example.com", true, 60000).await.unwrap();
        service.schedule_burn("@alice:example.com", "!room1:example.com", "$event1", 60000).await.unwrap();
        service.schedule_burn("@alice:example.com", "!room1:example.com", "$event2", 60000).await.unwrap();
        
        let stats = service.get_user_stats("@alice:example.com").await.unwrap();
        assert_eq!(stats.rooms_enabled, 2);
        assert_eq!(stats.total_pending, 2);
    }

    #[tokio::test]
    async fn test_process_expired_burns() {
        let service = BurnAfterReadServiceImpl::new();
        
        service.schedule_burn("@alice:example.com", "!room:example.com", "$event1", 0).await.unwrap();
        
        let expired = service.process_expired_burns().await.unwrap();
        assert!(!expired.is_empty());
    }
}
