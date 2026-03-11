use std::sync::Arc;
use crate::common::error::ApiError;
use crate::storage::sliding_sync::{
    SlidingSyncStorage, SlidingSyncRoom,
    SlidingSyncRequest, SlidingSyncResponse, SlidingSyncListRequest,
};
use crate::cache::CacheManager;

#[derive(Clone)]
pub struct SlidingSyncService {
    storage: SlidingSyncStorage,
    cache: Arc<CacheManager>,
}

impl SlidingSyncService {
    pub fn new(storage: SlidingSyncStorage, cache: Arc<CacheManager>) -> Self {
        Self { storage, cache }
    }

    pub async fn sync(
        &self,
        user_id: &str,
        device_id: &str,
        request: SlidingSyncRequest,
    ) -> Result<SlidingSyncResponse, ApiError> {
        let conn_id = request.conn_id.as_deref();

        if let Some(pos_str) = &request.pos {
            if !self.storage.validate_pos(user_id, device_id, conn_id, pos_str).await
                .map_err(|e| ApiError::internal(format!("Failed to validate pos: {}", e)))?
            {
                return Err(ApiError::bad_request("Invalid position token"));
            }
        }

        for list_req in &request.lists {
            self.storage.save_list(
                user_id,
                device_id,
                conn_id,
                &list_req.list_key,
                &list_req.sort,
                list_req.filters.as_ref(),
                list_req.room_subscription.as_ref(),
                &list_req.ranges,
            ).await.map_err(|e| ApiError::internal(format!("Failed to save list: {}", e)))?;
        }

        if let Some(unsubs) = &request.room_unsubscriptions {
            for room_id in unsubs {
                self.storage.delete_room(user_id, device_id, room_id, conn_id).await
                    .map_err(|e| ApiError::internal(format!("Failed to unsubscribe room: {}", e)))?;
            }
        }

        let lists_response = self.build_lists_response(user_id, device_id, conn_id, &request.lists).await
            .map_err(|e| ApiError::internal(format!("Failed to build lists response: {}", e)))?;

        let rooms_response = self.build_rooms_response(user_id, device_id, conn_id, &request).await
            .map_err(|e| ApiError::internal(format!("Failed to build rooms response: {}", e)))?;

        let new_token = self.storage.create_or_update_token(user_id, device_id, conn_id).await
            .map_err(|e| ApiError::internal(format!("Failed to update token: {}", e)))?;

        Ok(SlidingSyncResponse {
            pos: new_token.pos.to_string(),
            conn_id: request.conn_id,
            lists: lists_response,
            rooms: rooms_response,
            extensions: request.extensions,
        })
    }

    async fn build_lists_response(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        lists: &[SlidingSyncListRequest],
    ) -> Result<serde_json::Value, sqlx::Error> {
        let mut lists_json = serde_json::Map::new();

        for list_req in lists {
            let mut rooms = Vec::new();
            
            for (start, end) in &list_req.ranges {
                let range_rooms = self.storage.get_rooms_for_list(
                    user_id,
                    device_id,
                    conn_id,
                    &list_req.list_key,
                    *start,
                    *end,
                ).await?;
                
                for room in range_rooms {
                    rooms.push(self.room_to_json(&room));
                }
            }

            let count = self.count_rooms_for_list(user_id, device_id, conn_id, &list_req.list_key).await?;

            lists_json.insert(
                list_req.list_key.clone(),
                serde_json::json!({
                    "ops": self.build_ops(&rooms),
                    "count": count,
                }),
            );
        }

        Ok(serde_json::Value::Object(lists_json))
    }

    async fn count_rooms_for_list(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        list_key: &str,
    ) -> Result<u32, sqlx::Error> {
        let rooms = self.storage.get_rooms_for_list(user_id, device_id, conn_id, list_key, 0, 10000).await?;
        Ok(rooms.len() as u32)
    }

    fn build_ops(&self, rooms: &[serde_json::Value]) -> Vec<serde_json::Value> {
        if rooms.is_empty() {
            return vec![];
        }

        vec![serde_json::json!({
            "op": "SYNC",
            "range": [0, rooms.len() as u32 - 1],
            "room_ids": rooms.iter().map(|r| r["room_id"].clone()).collect::<Vec<_>>()
        })]
    }

    async fn build_rooms_response(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        request: &SlidingSyncRequest,
    ) -> Result<serde_json::Value, sqlx::Error> {
        let mut rooms_json = serde_json::Map::new();

        if let Some(subscriptions) = &request.room_subscriptions {
            if let Some(subs_obj) = subscriptions.as_object() {
                for room_id in subs_obj.keys() {
                    if let Some(room) = self.storage.get_room(user_id, device_id, room_id, conn_id).await? {
                        rooms_json.insert(room_id.clone(), self.room_to_json(&room));
                    }
                }
            }
        }

        for list_req in &request.lists {
            for (start, end) in &list_req.ranges {
                let rooms = self.storage.get_rooms_for_list(
                    user_id,
                    device_id,
                    conn_id,
                    &list_req.list_key,
                    *start,
                    *end,
                ).await?;

                for room in rooms {
                    if !rooms_json.contains_key(&room.room_id) {
                        rooms_json.insert(room.room_id.clone(), self.room_to_json(&room));
                    }
                }
            }
        }

        Ok(serde_json::Value::Object(rooms_json))
    }

    fn room_to_json(&self, room: &SlidingSyncRoom) -> serde_json::Value {
        serde_json::json!({
            "room_id": room.room_id,
            "name": room.name,
            "avatar": room.avatar,
            "is_dm": room.is_dm,
            "is_encrypted": room.is_encrypted,
            "is_tombstoned": room.is_tombstoned,
            "invited": room.invited,
            "highlight_count": room.highlight_count,
            "notification_count": room.notification_count,
            "timestamp": room.timestamp,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update_room_state(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
        bump_stamp: i64,
        highlight_count: i32,
        notification_count: i32,
        is_dm: bool,
        is_encrypted: bool,
        name: Option<&str>,
        avatar: Option<&str>,
    ) -> Result<(), ApiError> {
        self.storage.upsert_room(
            user_id,
            device_id,
            room_id,
            conn_id,
            None,
            bump_stamp,
            highlight_count,
            notification_count,
            is_dm,
            is_encrypted,
            false,
            false,
            name,
            avatar,
            bump_stamp,
        ).await.map_err(|e| ApiError::internal(format!("Failed to update room state: {}", e)))?;

        self.invalidate_room_cache(user_id, device_id, room_id, conn_id).await;

        Ok(())
    }

    pub async fn bump_room(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
        bump_stamp: i64,
    ) -> Result<(), ApiError> {
        self.storage.bump_room(user_id, device_id, room_id, conn_id, bump_stamp).await
            .map_err(|e| ApiError::internal(format!("Failed to bump room: {}", e)))?;

        self.invalidate_room_cache(user_id, device_id, room_id, conn_id).await;

        Ok(())
    }

    pub async fn update_notification_counts(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
        highlight_count: i32,
        notification_count: i32,
    ) -> Result<(), ApiError> {
        self.storage.update_notification_counts(
            user_id,
            device_id,
            room_id,
            conn_id,
            highlight_count,
            notification_count,
        ).await.map_err(|e| ApiError::internal(format!("Failed to update notifications: {}", e)))?;

        self.invalidate_room_cache(user_id, device_id, room_id, conn_id).await;

        Ok(())
    }

    pub async fn remove_room(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
    ) -> Result<(), ApiError> {
        self.storage.delete_room(user_id, device_id, room_id, conn_id).await
            .map_err(|e| ApiError::internal(format!("Failed to remove room: {}", e)))?;

        self.invalidate_room_cache(user_id, device_id, room_id, conn_id).await;

        Ok(())
    }

    pub async fn cleanup_expired_tokens(&self) -> Result<u64, ApiError> {
        let count = self.storage.cleanup_expired_tokens().await
            .map_err(|e| ApiError::internal(format!("Failed to cleanup tokens: {}", e)))?;

        Ok(count)
    }

    async fn invalidate_room_cache(&self, user_id: &str, device_id: &str, room_id: &str, conn_id: Option<&str>) {
        let cache_key = if let Some(cid) = conn_id {
            format!("sliding_sync:room:{}:{}:{}:{}", user_id, device_id, cid, room_id)
        } else {
            format!("sliding_sync:room:{}:{}::{}", user_id, device_id, room_id)
        };
        let _ = self.cache.delete(&cache_key).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::sliding_sync::SlidingSyncFilters;

    #[tokio::test]
    async fn test_room_to_json() {
        let service = create_test_service();
        let room = SlidingSyncRoom {
            id: 1,
            user_id: "@alice:example.com".to_string(),
            device_id: "DEVICE123".to_string(),
            room_id: "!room:example.com".to_string(),
            conn_id: None,
            list_key: Some("main".to_string()),
            bump_stamp: 1234567890000,
            highlight_count: 5,
            notification_count: 10,
            is_dm: true,
            is_encrypted: true,
            is_tombstoned: false,
            invited: false,
            name: Some("Test Room".to_string()),
            avatar: Some("mxc://example.com/avatar".to_string()),
            timestamp: 1234567890000,
            created_ts: 1234567890000,
            updated_ts: 1234567890000,
        };

        let json = service.room_to_json(&room);

        assert_eq!(json["room_id"], "!room:example.com");
        assert_eq!(json["name"], "Test Room");
        assert_eq!(json["highlight_count"], 5);
        assert!(json["is_dm"].as_bool().unwrap());
    }

    #[tokio::test]
    async fn test_build_ops_empty() {
        let service = create_test_service();
        let ops = service.build_ops(&[]);
        assert!(ops.is_empty());
    }

    #[tokio::test]
    async fn test_build_ops_with_rooms() {
        let service = create_test_service();
        let rooms = vec![
            serde_json::json!({"room_id": "!room1:example.com"}),
            serde_json::json!({"room_id": "!room2:example.com"}),
        ];

        let ops = service.build_ops(&rooms);

        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0]["op"], "SYNC");
    }

    fn create_test_service() -> SlidingSyncService {
        SlidingSyncService {
            storage: SlidingSyncStorage::new(std::sync::Arc::new(
                sqlx::postgres::PgPoolOptions::new().max_connections(1).connect_lazy("postgres://localhost/test").unwrap()
            )),
            cache: Arc::new(CacheManager::new(crate::cache::CacheConfig::default())),
        }
    }

    #[tokio::test]
    async fn test_sliding_sync_filters_serialization() {
        let filters = SlidingSyncFilters {
            is_invite: Some(false),
            is_tombstoned: None,
            room_name_like: Some("test".to_string()),
            ..Default::default()
        };

        let json = serde_json::to_value(&filters).unwrap();

        assert!(json.get("is_invite").is_some());
        assert!(json.get("is_tombstoned").is_none());
        assert_eq!(json.get("room_name_like").unwrap().as_str().unwrap(), "test");
    }
}
