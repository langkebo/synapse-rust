use super::*;
use crate::directory::{DirectoryStoreApi, RoomDirectoryEntry};
use std::collections::HashMap;
use tokio::sync::RwLock;

/// In-memory directory store mirroring [`crate::directory::DirectoryStorage`].
///
/// Stores public-room directory entries in a `HashMap` behind `RwLock` so that
/// service-layer unit tests can verify persistence across service instances
/// without a real PostgreSQL pool.
#[derive(Clone, Default)]
pub struct InMemoryDirectoryStore {
    entries: Arc<RwLock<HashMap<String, RoomDirectoryEntry>>>,
}

impl InMemoryDirectoryStore {
    /// See [`new`].
    pub fn new() -> Self {
        Self { entries: Arc::new(RwLock::new(HashMap::new())) }
    }
}

#[async_trait::async_trait]
impl DirectoryStoreApi for InMemoryDirectoryStore {
    async fn upsert_directory_entry(&self, entry: &RoomDirectoryEntry) -> Result<(), sqlx::Error> {
        self.entries.write().await.insert(entry.room_id.clone(), entry.clone());
        Ok(())
    }

    async fn remove_from_directory(&self, room_id: &str) -> Result<(), sqlx::Error> {
        self.entries.write().await.remove(room_id);
        Ok(())
    }

    async fn list_public_rooms(&self, limit: i64, offset: i64) -> Result<Vec<RoomDirectoryEntry>, sqlx::Error> {
        let entries = self.entries.read().await;
        let mut result: Vec<RoomDirectoryEntry> = entries.values().cloned().collect();
        // Sort by member_count DESC, room_id ASC (matching the Postgres query)
        result.sort_by(|a, b| b.member_count.cmp(&a.member_count).then_with(|| a.room_id.cmp(&b.room_id)));
        let start = (offset as usize).min(result.len());
        let end = start.saturating_add(limit as usize).min(result.len());
        Ok(result[start..end].to_vec())
    }

    async fn search_public_rooms(&self, filter: &str, limit: i64) -> Result<Vec<RoomDirectoryEntry>, sqlx::Error> {
        let entries = self.entries.read().await;
        let filter_lower = filter.to_lowercase();
        let mut result: Vec<RoomDirectoryEntry> = entries
            .values()
            .filter(|e| {
                let name_match = e.name.as_ref().is_some_and(|n| n.to_lowercase().contains(&filter_lower));
                let topic_match = e.topic.as_ref().is_some_and(|t| t.to_lowercase().contains(&filter_lower));
                name_match || topic_match
            })
            .cloned()
            .collect();
        result.sort_by(|a, b| b.member_count.cmp(&a.member_count).then_with(|| a.room_id.cmp(&b.room_id)));
        result.truncate(limit as usize);
        Ok(result)
    }
}
