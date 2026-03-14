use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedRoomSummary {
    pub room_id: String,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    pub is_direct: bool,
    pub is_encrypted: bool,
    pub member_count: i64,
    pub joined_members: i64,
    pub unread_notifications: i64,
    pub highlight_count: i64,
    pub last_event_ts: Option<i64>,
    pub cached_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedRoomMember {
    pub user_id: String,
    pub displayname: Option<String>,
    pub avatar_url: Option<String>,
    pub membership: String,
    pub cached_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedPresence {
    pub user_id: String,
    pub presence: String,
    pub status_msg: Option<String>,
    pub last_active_ts: Option<i64>,
    pub cached_at: i64,
}

pub struct RoomSummaryCache {
    summaries: Arc<RwLock<HashMap<String, CachedRoomSummary>>>,
    members: Arc<RwLock<HashMap<String, Vec<CachedRoomMember>>>>,
    presence: Arc<RwLock<HashMap<String, CachedPresence>>>,
    ttl: Duration,
}

impl RoomSummaryCache {
    pub fn new(ttl_seconds: u64) -> Self {
        Self {
            summaries: Arc::new(RwLock::new(HashMap::new())),
            members: Arc::new(RwLock::new(HashMap::new())),
            presence: Arc::new(RwLock::new(HashMap::new())),
            ttl: Duration::from_secs(ttl_seconds),
        }
    }

    pub async fn get_summary(&self, room_id: &str) -> Option<CachedRoomSummary> {
        let summaries = self.summaries.read().await;
        if let Some(summary) = summaries.get(room_id) {
            let now = chrono::Utc::now().timestamp_millis();
            if now - summary.cached_at < self.ttl.as_millis() as i64 {
                return Some(summary.clone());
            }
        }
        None
    }

    pub async fn set_summary(&self, summary: CachedRoomSummary) {
        let mut summaries = self.summaries.write().await;
        let mut summary = summary;
        summary.cached_at = chrono::Utc::now().timestamp_millis();
        summaries.insert(summary.room_id.clone(), summary);
    }

    pub async fn get_summaries_batch(&self, room_ids: &[String]) -> Vec<CachedRoomSummary> {
        let summaries = self.summaries.read().await;
        let now = chrono::Utc::now().timestamp_millis();
        let ttl_ms = self.ttl.as_millis() as i64;
        
        room_ids
            .iter()
            .filter_map(|id| {
                summaries.get(id).and_then(|s| {
                    if now - s.cached_at < ttl_ms {
                        Some(s.clone())
                    } else {
                        None
                    }
                })
            })
            .collect()
    }

    pub async fn set_summaries_batch(&self, new_summaries: Vec<CachedRoomSummary>) {
        let mut summaries = self.summaries.write().await;
        let now = chrono::Utc::now().timestamp_millis();
        
        for mut summary in new_summaries {
            summary.cached_at = now;
            summaries.insert(summary.room_id.clone(), summary);
        }
    }

    pub async fn invalidate_summary(&self, room_id: &str) {
        let mut summaries = self.summaries.write().await;
        summaries.remove(room_id);
    }

    pub async fn get_members(&self, room_id: &str) -> Option<Vec<CachedRoomMember>> {
        let members = self.members.read().await;
        if let Some(member_list) = members.get(room_id) {
            if !member_list.is_empty() {
                let now = chrono::Utc::now().timestamp_millis();
                if now - member_list[0].cached_at < self.ttl.as_millis() as i64 {
                    return Some(member_list.clone());
                }
            }
        }
        None
    }

    pub async fn set_members(&self, room_id: &str, member_list: Vec<CachedRoomMember>) {
        let mut members = self.members.write().await;
        let now = chrono::Utc::now().timestamp_millis();
        let member_list = member_list
            .into_iter()
            .map(|mut m| {
                m.cached_at = now;
                m
            })
            .collect();
        members.insert(room_id.to_string(), member_list);
    }

    pub async fn invalidate_members(&self, room_id: &str) {
        let mut members = self.members.write().await;
        members.remove(room_id);
    }

    pub async fn get_presence(&self, user_id: &str) -> Option<CachedPresence> {
        let presence = self.presence.read().await;
        if let Some(p) = presence.get(user_id) {
            let now = chrono::Utc::now().timestamp_millis();
            if now - p.cached_at < self.ttl.as_millis() as i64 {
                return Some(p.clone());
            }
        }
        None
    }

    pub async fn set_presence(&self, p: CachedPresence) {
        let mut presence = self.presence.write().await;
        let mut p = p;
        p.cached_at = chrono::Utc::now().timestamp_millis();
        presence.insert(p.user_id.clone(), p);
    }

    pub async fn get_presence_batch(&self, user_ids: &[String]) -> Vec<CachedPresence> {
        let presence = self.presence.read().await;
        let now = chrono::Utc::now().timestamp_millis();
        let ttl_ms = self.ttl.as_millis() as i64;
        
        user_ids
            .iter()
            .filter_map(|id| {
                presence.get(id).and_then(|p| {
                    if now - p.cached_at < ttl_ms {
                        Some(p.clone())
                    } else {
                        None
                    }
                })
            })
            .collect()
    }

    pub async fn clear(&self) {
        let mut summaries = self.summaries.write().await;
        let mut members = self.members.write().await;
        let mut presence = self.presence.write().await;
        
        summaries.clear();
        members.clear();
        presence.clear();
    }

    pub async fn stats(&self) -> CacheStats {
        let summaries = self.summaries.read().await;
        let members = self.members.read().await;
        let presence = self.presence.read().await;
        
        CacheStats {
            summary_count: summaries.len(),
            member_cache_count: members.len(),
            presence_count: presence.len(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CacheStats {
    pub summary_count: usize,
    pub member_cache_count: usize,
    pub presence_count: usize,
}

impl Default for RoomSummaryCache {
    fn default() -> Self {
        Self::new(300) // 5 minutes default TTL
    }
}

pub struct SyncOptimizationService {
    room_cache: Arc<RoomSummaryCache>,
}

impl SyncOptimizationService {
    pub fn new(cache_ttl_seconds: u64) -> Self {
        Self {
            room_cache: Arc::new(RoomSummaryCache::new(cache_ttl_seconds)),
        }
    }

    pub fn cache(&self) -> Arc<RoomSummaryCache> {
        self.room_cache.clone()
    }

    pub async fn preload_room_data(&self, room_ids: &[String]) {
        let cached = self.room_cache.get_summaries_batch(room_ids).await;
        tracing::debug!(
            "Preloaded {} room summaries from cache for {} requested",
            cached.len(),
            room_ids.len()
        );
    }

    pub async fn invalidate_room(&self, room_id: &str) {
        self.room_cache.invalidate_summary(room_id).await;
        self.room_cache.invalidate_members(room_id).await;
        tracing::debug!("Invalidated cache for room: {}", room_id);
    }

    pub async fn get_cache_stats(&self) -> CacheStats {
        self.room_cache.stats().await
    }
}

impl Default for SyncOptimizationService {
    fn default() -> Self {
        Self::new(300)
    }
}
