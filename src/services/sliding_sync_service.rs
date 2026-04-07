use crate::cache::CacheManager;
use crate::common::error::ApiError;
use crate::services::TypingService;
use crate::storage::sliding_sync::{
    AdminRoomTokenSyncEntry, SlidingSyncListData, SlidingSyncRequest, SlidingSyncResponse,
    SlidingSyncRoom, SlidingSyncStorage,
};
use std::sync::Arc;

#[derive(Clone)]
pub struct SlidingSyncService {
    storage: SlidingSyncStorage,
    cache: Arc<CacheManager>,
    typing_service: Arc<crate::services::typing_service::TypingServiceImpl>,
}

impl SlidingSyncService {
    pub fn new(
        storage: SlidingSyncStorage,
        cache: Arc<CacheManager>,
        typing_service: Arc<crate::services::typing_service::TypingServiceImpl>,
    ) -> Self {
        Self {
            storage,
            cache,
            typing_service,
        }
    }

    pub async fn sync(
        &self,
        user_id: &str,
        device_id: &str,
        request: SlidingSyncRequest,
    ) -> Result<SlidingSyncResponse, ApiError> {
        let conn_id = request.conn_id.as_deref();

        if let Some(pos_str) = &request.pos {
            if !self
                .storage
                .validate_pos(user_id, device_id, conn_id, pos_str)
                .await
                .map_err(|e| ApiError::internal(format!("Failed to validate pos: {}", e)))?
            {
                return Err(ApiError::bad_request("Invalid position token"));
            }
        }

        for (list_key, list_data) in &request.lists {
            let ranges: Vec<(u32, u32)> = list_data
                .ranges
                .iter()
                .filter_map(|r| {
                    if r.len() >= 2 {
                        Some((r[0], r[1]))
                    } else {
                        None
                    }
                })
                .collect();

            self.storage
                .save_list(
                    user_id,
                    device_id,
                    conn_id,
                    list_key,
                    &list_data.sort,
                    list_data.filters.as_ref(),
                    None,
                    &ranges,
                )
                .await
                .map_err(|e| ApiError::internal(format!("Failed to save list: {}", e)))?;
        }

        if let Some(unsubs) = &request.unsubscribe_rooms {
            for room_id in unsubs {
                self.storage
                    .delete_room(user_id, device_id, room_id, conn_id)
                    .await
                    .map_err(|e| {
                        ApiError::internal(format!("Failed to unsubscribe room: {}", e))
                    })?;
            }
        }

        let lists_response = self
            .build_lists_response(user_id, device_id, conn_id, &request.lists)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to build lists response: {}", e)))?;

        let rooms_response = self
            .build_rooms_response(user_id, device_id, conn_id, &request)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to build rooms response: {}", e)))?;

        let extensions_response = self
            .build_extensions_response(user_id, &rooms_response, request.extensions.as_ref())
            .await
            .map_err(|e| {
                ApiError::internal(format!("Failed to build extensions response: {}", e))
            })?;

        let new_token = self
            .storage
            .create_or_update_token(user_id, device_id, conn_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update token: {}", e)))?;

        Ok(SlidingSyncResponse {
            pos: new_token.pos.to_string(),
            conn_id: request.conn_id,
            lists: lists_response,
            rooms: rooms_response,
            extensions: extensions_response,
        })
    }

    async fn build_lists_response(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        lists: &std::collections::HashMap<String, SlidingSyncListData>,
    ) -> Result<serde_json::Value, sqlx::Error> {
        let mut lists_json = serde_json::Map::new();

        for (list_key, list_data) in lists {
            let mut rooms = Vec::new();

            for range in &list_data.ranges {
                if range.len() >= 2 {
                    let start = range[0];
                    let end = range[1];
                    let range_rooms = self
                        .storage
                        .get_rooms_for_list(user_id, device_id, conn_id, list_key, start, end)
                        .await?;

                    for room in range_rooms {
                        rooms.push(self.room_to_json(&room));
                    }
                }
            }

            let count = self
                .count_rooms_for_list(user_id, device_id, conn_id, list_key)
                .await?;

            lists_json.insert(
                list_key.clone(),
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
        let rooms = self
            .storage
            .get_rooms_for_list(user_id, device_id, conn_id, list_key, 0, 10000)
            .await?;
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
                    let room = if let Some(room) = self
                        .storage
                        .get_room(user_id, device_id, room_id, conn_id)
                        .await?
                    {
                        Some(room)
                    } else {
                        self.storage
                            .materialize_room_from_activity(user_id, device_id, room_id, conn_id)
                            .await?
                    };

                    if let Some(room) = room {
                        rooms_json.insert(room_id.clone(), self.room_to_json(&room));
                    }
                }
            }
        }

        for (list_key, list_data) in &request.lists {
            for range in &list_data.ranges {
                if range.len() >= 2 {
                    let start = range[0];
                    let end = range[1];
                    let rooms = self
                        .storage
                        .get_rooms_for_list(user_id, device_id, conn_id, list_key, start, end)
                        .await?;

                    for room in rooms {
                        if !rooms_json.contains_key(&room.room_id) {
                            rooms_json.insert(room.room_id.clone(), self.room_to_json(&room));
                        }
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

    async fn build_extensions_response(
        &self,
        user_id: &str,
        rooms_response: &serde_json::Value,
        request_extensions: Option<&serde_json::Value>,
    ) -> Result<Option<serde_json::Value>, sqlx::Error> {
        let Some(request_extensions) = request_extensions else {
            return Ok(None);
        };

        let mut response_extensions = request_extensions.as_object().cloned().unwrap_or_default();

        let account_data_enabled = request_extensions
            .get("account_data")
            .and_then(|v| {
                if v.as_bool() == Some(true) {
                    Some(true)
                } else {
                    v.as_object()
                        .map(|obj| obj.get("enabled").and_then(|e| e.as_bool()).unwrap_or(true))
                }
            })
            .unwrap_or(false);

        if account_data_enabled {
            let room_ids: Vec<String> = rooms_response
                .as_object()
                .map(|obj| obj.keys().cloned().collect())
                .unwrap_or_default();

            let global = self.storage.get_global_account_data(user_id).await?;
            let rooms = self
                .storage
                .get_room_account_data(user_id, &room_ids)
                .await?;

            response_extensions.insert(
                "account_data".to_string(),
                serde_json::json!({
                    "global": global,
                    "rooms": rooms
                }),
            );
        }

        let receipts_enabled = request_extensions
            .get("receipts")
            .and_then(|v| {
                if v.as_bool() == Some(true) {
                    Some(true)
                } else {
                    v.as_object()
                        .map(|obj| obj.get("enabled").and_then(|e| e.as_bool()).unwrap_or(true))
                }
            })
            .unwrap_or(false);

        if receipts_enabled {
            let room_ids: Vec<String> = rooms_response
                .as_object()
                .map(|obj| obj.keys().cloned().collect())
                .unwrap_or_default();
            let receipts = self.storage.get_receipts_for_rooms(&room_ids).await?;
            response_extensions.insert(
                "receipts".to_string(),
                serde_json::json!({
                    "rooms": receipts
                }),
            );
        }

        let typing_enabled = request_extensions
            .get("typing")
            .and_then(|v| {
                if v.as_bool() == Some(true) {
                    Some(true)
                } else {
                    v.as_object()
                        .map(|obj| obj.get("enabled").and_then(|e| e.as_bool()).unwrap_or(true))
                }
            })
            .unwrap_or(false);

        if typing_enabled {
            let room_ids: Vec<String> = rooms_response
                .as_object()
                .map(|obj| obj.keys().cloned().collect())
                .unwrap_or_default();
            let mut typing_rooms = serde_json::Map::new();
            for room_id in room_ids {
                let typing_users = self
                    .typing_service
                    .get_typing_users(&room_id)
                    .await
                    .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
                let user_ids: Vec<String> = typing_users.into_keys().collect();
                typing_rooms.insert(room_id, serde_json::json!({ "user_ids": user_ids }));
            }
            response_extensions.insert(
                "typing".to_string(),
                serde_json::json!({
                    "rooms": typing_rooms
                }),
            );
        }

        if response_extensions.is_empty() {
            Ok(None)
        } else {
            Ok(Some(serde_json::Value::Object(response_extensions)))
        }
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
        self.storage
            .upsert_room(
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
            )
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update room state: {}", e)))?;

        self.invalidate_room_cache(user_id, device_id, room_id, conn_id)
            .await;

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
        self.storage
            .bump_room(user_id, device_id, room_id, conn_id, bump_stamp)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to bump room: {}", e)))?;

        self.invalidate_room_cache(user_id, device_id, room_id, conn_id)
            .await;

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
        self.storage
            .update_notification_counts(
                user_id,
                device_id,
                room_id,
                conn_id,
                highlight_count,
                notification_count,
            )
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update notifications: {}", e)))?;

        self.invalidate_room_cache(user_id, device_id, room_id, conn_id)
            .await;

        Ok(())
    }

    pub async fn remove_room(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
    ) -> Result<(), ApiError> {
        self.storage
            .delete_room(user_id, device_id, room_id, conn_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to remove room: {}", e)))?;

        self.invalidate_room_cache(user_id, device_id, room_id, conn_id)
            .await;

        Ok(())
    }

    pub async fn cleanup_expired_tokens(&self) -> Result<u64, ApiError> {
        let count = self
            .storage
            .cleanup_expired_tokens()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to cleanup tokens: {}", e)))?;

        Ok(count)
    }

    pub async fn get_room_token_sync(
        &self,
        room_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<AdminRoomTokenSyncEntry>, i64), ApiError> {
        let entries = self
            .storage
            .list_room_token_sync(room_id, limit, offset)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to list room token sync: {}", e)))?;

        let total = self
            .storage
            .count_room_token_sync(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to count room token sync: {}", e)))?;

        Ok((entries, total))
    }

    async fn invalidate_room_cache(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
    ) {
        let cache_key = if let Some(cid) = conn_id {
            format!(
                "sliding_sync:room:{}:{}:{}:{}",
                user_id, device_id, cid, room_id
            )
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
                sqlx::postgres::PgPoolOptions::new()
                    .max_connections(1)
                    .connect_lazy("postgres://localhost/test")
                    .unwrap(),
            )),
            cache: Arc::new(CacheManager::new(crate::cache::CacheConfig::default())),
            typing_service: Arc::new(crate::services::typing_service::TypingServiceImpl::new()),
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
        assert_eq!(
            json.get("room_name_like").unwrap().as_str().unwrap(),
            "test"
        );
    }
}
