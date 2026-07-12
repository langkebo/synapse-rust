#[allow(clippy::type_complexity)]
#[derive(Clone, Default)]
pub struct InMemorySlidingSyncStore {
    tokens: std::sync::Arc<
        tokio::sync::RwLock<
            std::collections::HashMap<(String, String, Option<String>), crate::sliding_sync::SlidingSyncToken>,
        >,
    >,
    lists: std::sync::Arc<
        tokio::sync::RwLock<
            std::collections::HashMap<(String, String, Option<String>, String), crate::sliding_sync::SlidingSyncList>,
        >,
    >,
    rooms: std::sync::Arc<
        tokio::sync::RwLock<
            std::collections::HashMap<(String, String, Option<String>, String), crate::sliding_sync::SlidingSyncRoom>,
        >,
    >,
    global_account_data: std::sync::Arc<tokio::sync::RwLock<std::collections::HashMap<String, serde_json::Value>>>,
    room_account_data:
        std::sync::Arc<tokio::sync::RwLock<std::collections::HashMap<(String, String), serde_json::Value>>>,
    receipts: std::sync::Arc<tokio::sync::RwLock<std::collections::HashMap<String, serde_json::Value>>>,
    next_id: std::sync::Arc<std::sync::atomic::AtomicI64>,
}

impl InMemorySlidingSyncStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn key(user_id: &str, device_id: &str, conn_id: Option<&str>) -> (String, String, Option<String>) {
        (user_id.to_string(), device_id.to_string(), conn_id.map(|s| s.to_string()))
    }
}

#[async_trait::async_trait]
impl crate::sliding_sync::SlidingSyncStoreApi for InMemorySlidingSyncStore {
    async fn create_or_update_token(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
    ) -> Result<crate::sliding_sync::SlidingSyncToken, sqlx::Error> {
        let key = Self::key(user_id, device_id, conn_id);
        let now = chrono::Utc::now().timestamp_millis();
        let token_id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let token = crate::sliding_sync::SlidingSyncToken {
            id: token_id,
            user_id: user_id.to_string(),
            device_id: device_id.to_string(),
            conn_id: conn_id.map(|s| s.to_string()),
            token: format!("sst_{}", token_id),
            pos: token_id,
            created_ts: now,
            expires_at: Some(now + 1_800_000),
        };
        self.tokens.write().await.insert(key, token.clone());
        Ok(token)
    }

    async fn get_token(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
    ) -> Result<Option<crate::sliding_sync::SlidingSyncToken>, sqlx::Error> {
        Ok(self.tokens.read().await.get(&Self::key(user_id, device_id, conn_id)).cloned())
    }

    async fn validate_pos(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        pos: &str,
    ) -> Result<bool, sqlx::Error> {
        let Ok(pos_i64) = pos.parse::<i64>() else {
            return Ok(false);
        };
        Ok(self.tokens.read().await.get(&Self::key(user_id, device_id, conn_id)).is_some_and(|t| t.pos == pos_i64))
    }

    async fn save_list(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        list_key: &str,
        sort: &[String],
        filters: Option<&crate::sliding_sync::SlidingSyncFilters>,
        room_subscription: Option<&serde_json::Value>,
        ranges: &[(u32, u32)],
    ) -> Result<crate::sliding_sync::SlidingSyncList, sqlx::Error> {
        let list_key_owned =
            (user_id.to_string(), device_id.to_string(), conn_id.map(|s| s.to_string()), list_key.to_string());
        let now = chrono::Utc::now().timestamp_millis();
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let list = crate::sliding_sync::SlidingSyncList {
            id,
            user_id: user_id.to_string(),
            device_id: device_id.to_string(),
            conn_id: conn_id.map(|s| s.to_string()),
            list_key: list_key.to_string(),
            sort: serde_json::to_value(sort).unwrap_or_default(),
            filters: filters.map(|f| serde_json::to_value(f).unwrap_or_default()),
            room_subscription: room_subscription.cloned(),
            ranges: Some(serde_json::to_value(ranges).unwrap_or_default()),
            created_ts: now,
            updated_ts: now,
        };
        self.lists.write().await.insert(list_key_owned, list.clone());
        Ok(list)
    }

    async fn get_lists(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
    ) -> Result<Vec<crate::sliding_sync::SlidingSyncList>, sqlx::Error> {
        let uid = user_id.to_string();
        let did = device_id.to_string();
        let cid = conn_id.map(|s| s.to_string());
        Ok(self
            .lists
            .read()
            .await
            .iter()
            .filter(|((u, d, c, _), _)| *u == uid && *d == did && *c == cid)
            .map(|(_, v)| v.clone())
            .collect())
    }

    async fn delete_list(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        list_key: &str,
    ) -> Result<(), sqlx::Error> {
        let key = (user_id.to_string(), device_id.to_string(), conn_id.map(|s| s.to_string()), list_key.to_string());
        self.lists.write().await.remove(&key);
        Ok(())
    }

    async fn upsert_room(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
        list_key: Option<&str>,
        bump_stamp: i64,
        highlight_count: i32,
        notification_count: i32,
        is_dm: bool,
        is_encrypted: bool,
        is_tombstoned: bool,
        is_invited: bool,
        name: Option<&str>,
        avatar: Option<&str>,
        timestamp: i64,
    ) -> Result<crate::sliding_sync::SlidingSyncRoom, sqlx::Error> {
        let key = (user_id.to_string(), device_id.to_string(), conn_id.map(|s| s.to_string()), room_id.to_string());
        let now = chrono::Utc::now().timestamp_millis();
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let room = crate::sliding_sync::SlidingSyncRoom {
            id,
            user_id: user_id.to_string(),
            device_id: device_id.to_string(),
            room_id: room_id.to_string(),
            conn_id: conn_id.map(|s| s.to_string()),
            list_key: list_key.map(|s| s.to_string()),
            bump_stamp,
            highlight_count,
            notification_count,
            is_dm,
            is_encrypted,
            is_tombstoned,
            is_invited,
            name: name.map(|s| s.to_string()),
            avatar: avatar.map(|s| s.to_string()),
            timestamp,
            created_ts: now,
            updated_ts: now,
        };
        self.rooms.write().await.insert(key, room.clone());
        Ok(room)
    }

    async fn get_rooms_for_list(
        &self,
        query_params: crate::sliding_sync::SlidingSyncListQuery<'_>,
    ) -> Result<Vec<crate::sliding_sync::SlidingSyncRoom>, sqlx::Error> {
        let uid = query_params.user_id.to_string();
        let did = query_params.device_id.to_string();
        let cid = query_params.conn_id.map(|s| s.to_string());
        let lk = query_params.list_key.to_string();
        let mut rooms: Vec<_> = self
            .rooms
            .read()
            .await
            .iter()
            .filter(|((u, d, c, _), r)| *u == uid && *d == did && *c == cid && r.list_key.as_deref() == Some(&lk))
            .map(|(_, v)| v.clone())
            .collect();
        rooms.sort_by_key(|r| -r.bump_stamp);
        let start = query_params.start as usize;
        let end = query_params.end as usize;
        if start >= rooms.len() {
            return Ok(Vec::new());
        }
        let end = end.min(rooms.len());
        Ok(rooms[start..end].to_vec())
    }

    async fn count_rooms_for_list(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        list_key: &str,
        _filters: Option<&crate::sliding_sync::SlidingSyncFilters>,
    ) -> Result<i64, sqlx::Error> {
        let uid = user_id.to_string();
        let did = device_id.to_string();
        let cid = conn_id.map(|s| s.to_string());
        let lk = list_key.to_string();
        Ok(self
            .rooms
            .read()
            .await
            .iter()
            .filter(|((u, d, c, _), r)| *u == uid && *d == did && *c == cid && r.list_key.as_deref() == Some(&lk))
            .count() as i64)
    }

    async fn get_room(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
    ) -> Result<Option<crate::sliding_sync::SlidingSyncRoom>, sqlx::Error> {
        Ok(self
            .rooms
            .read()
            .await
            .get(&(user_id.to_string(), device_id.to_string(), conn_id.map(|s| s.to_string()), room_id.to_string()))
            .cloned())
    }

    async fn materialize_room_from_activity(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
    ) -> Result<Option<crate::sliding_sync::SlidingSyncRoom>, sqlx::Error> {
        // For the mock, simply delegate to get_room — real impl queries
        // activity tables, but tests seed data via upsert_room.
        self.get_room(user_id, device_id, room_id, conn_id).await
    }

    async fn delete_room(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        self.rooms.write().await.remove(&(
            user_id.to_string(),
            device_id.to_string(),
            conn_id.map(|s| s.to_string()),
            room_id.to_string(),
        ));
        Ok(())
    }

    async fn update_notification_counts(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
        highlight_count: i32,
        notification_count: i32,
    ) -> Result<(), sqlx::Error> {
        let key = (user_id.to_string(), device_id.to_string(), conn_id.map(|s| s.to_string()), room_id.to_string());
        if let Some(room) = self.rooms.write().await.get_mut(&key) {
            room.highlight_count = highlight_count;
            room.notification_count = notification_count;
        }
        Ok(())
    }

    async fn bump_room(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
        bump_stamp: i64,
    ) -> Result<(), sqlx::Error> {
        let key = (user_id.to_string(), device_id.to_string(), conn_id.map(|s| s.to_string()), room_id.to_string());
        if let Some(room) = self.rooms.write().await.get_mut(&key) {
            room.bump_stamp = bump_stamp;
        }
        Ok(())
    }

    async fn cleanup_expired_tokens(&self) -> Result<u64, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut tokens = self.tokens.write().await;
        let before = tokens.len() as u64;
        tokens.retain(|_, t| t.expires_at.is_none_or(|e| e > now));
        Ok(before - tokens.len() as u64)
    }

    async fn list_room_token_sync(
        &self,
        room_id: &str,
        limit: i64,
        _from: Option<&crate::sliding_sync::RoomTokenSyncCursor>,
    ) -> Result<Vec<crate::sliding_sync::AdminRoomTokenSyncEntry>, sqlx::Error> {
        let _rid = room_id.to_string();
        let mut entries: Vec<_> = self
            .tokens
            .read()
            .await
            .iter()
            .filter(|((_, _, _), _t)| true)
            .take(limit as usize)
            .map(|((uid, did, cid), t)| crate::sliding_sync::AdminRoomTokenSyncEntry {
                user_id: uid.clone(),
                device_id: did.clone(),
                conn_id: cid.clone(),
                list_key: None,
                pos: Some(t.pos),
                token_created_ts: Some(t.created_ts),
                token_expires_at: t.expires_at,
                room_timestamp: t.created_ts,
                room_updated_ts: t.created_ts,
                bump_stamp: t.pos,
                highlight_count: 0,
                notification_count: 0,
                is_dm: false,
                is_encrypted: false,
                is_tombstoned: false,
                is_invited: false,
                name: None,
                avatar: None,
                is_expired: t.expires_at.is_some_and(|e| e <= chrono::Utc::now().timestamp_millis()),
            })
            .collect();
        entries.sort_by_key(|e| e.room_updated_ts);
        Ok(entries)
    }

    async fn count_room_token_sync(&self, _room_id: &str) -> Result<i64, sqlx::Error> {
        Ok(self.tokens.read().await.len() as i64)
    }

    async fn get_global_account_data(&self, user_id: &str) -> Result<serde_json::Value, sqlx::Error> {
        Ok(self
            .global_account_data
            .read()
            .await
            .get(user_id)
            .cloned()
            .unwrap_or(serde_json::Value::Object(Default::default())))
    }

    async fn get_room_account_data(
        &self,
        user_id: &str,
        room_ids: &[String],
    ) -> Result<serde_json::Value, sqlx::Error> {
        let data = self.room_account_data.read().await;
        let mut result = serde_json::Map::new();
        for room_id in room_ids {
            if let Some(v) = data.get(&(user_id.to_string(), room_id.clone())) {
                result.insert(room_id.clone(), v.clone());
            }
        }
        Ok(serde_json::Value::Object(result))
    }

    async fn get_receipts_for_rooms(&self, room_ids: &[String]) -> Result<serde_json::Value, sqlx::Error> {
        let data = self.receipts.read().await;
        let mut result = serde_json::Map::new();
        for room_id in room_ids {
            if let Some(v) = data.get(room_id) {
                result.insert(room_id.clone(), v.clone());
            }
        }
        Ok(serde_json::Value::Object(result))
    }

    async fn delete_connection_data(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        let uid = user_id.to_string();
        let did = device_id.to_string();
        let cid = conn_id.map(|s| s.to_string());
        // Remove tokens
        self.tokens.write().await.remove(&Self::key(user_id, device_id, conn_id));
        // Remove lists
        self.lists.write().await.retain(|(u, d, c, _), _| !(*u == uid && *d == did && *c == cid));
        // Remove rooms
        self.rooms.write().await.retain(|(u, d, c, _), _| !(*u == uid && *d == did && *c == cid));
        Ok(())
    }
}
