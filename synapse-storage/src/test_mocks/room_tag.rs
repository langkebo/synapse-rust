use super::*;
use synapse_common::current_timestamp_millis;

pub struct InMemoryRoomTagStore {
    tags: Arc<RwLock<Vec<crate::room_tag::RoomTag>>>,
    next_id: Arc<RwLock<i32>>,
}

impl Default for InMemoryRoomTagStore {
    fn default() -> Self {
        Self { tags: Arc::new(RwLock::new(Vec::new())), next_id: Arc::new(RwLock::new(1)) }
    }
}

impl InMemoryRoomTagStore {
    pub fn new() -> Self {
        Self { tags: Arc::new(RwLock::new(Vec::new())), next_id: Arc::new(RwLock::new(1)) }
    }
}

#[async_trait::async_trait]
impl RoomTagStoreApi for InMemoryRoomTagStore {
    async fn get_all_tags(&self, user_id: &str) -> Result<Vec<crate::room_tag::RoomTag>, sqlx::Error> {
        Ok(self.tags.read().await.iter().filter(|t| t.user_id == user_id).cloned().collect())
    }

    async fn get_tags(&self, user_id: &str, room_id: &str) -> Result<Vec<crate::room_tag::RoomTag>, sqlx::Error> {
        Ok(self.tags.read().await.iter().filter(|t| t.user_id == user_id && t.room_id == room_id).cloned().collect())
    }

    async fn add_tag(&self, user_id: &str, room_id: &str, tag: &str, order: Option<f64>) -> Result<(), sqlx::Error> {
        let mut tags = self.tags.write().await;
        // Remove existing tag with same key before inserting.
        tags.retain(|t| !(t.user_id == user_id && t.room_id == room_id && t.tag == tag));
        let mut next_id = self.next_id.write().await;
        let id = *next_id;
        *next_id += 1;
        tags.push(crate::room_tag::RoomTag {
            id,
            user_id: user_id.to_string(),
            room_id: room_id.to_string(),
            tag: tag.to_string(),
            order,
            created_ts: current_timestamp_millis(),
        });
        Ok(())
    }

    async fn remove_tag(&self, user_id: &str, room_id: &str, tag: &str) -> Result<(), sqlx::Error> {
        self.tags.write().await.retain(|t| !(t.user_id == user_id && t.room_id == room_id && t.tag == tag));
        Ok(())
    }
}
