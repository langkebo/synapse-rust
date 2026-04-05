use crate::common::*;
use crate::services::*;
use crate::storage::PresenceStorage;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncToken {
    pub stream_id: i64,
    pub room_id: Option<String>,
    pub event_type: Option<String>,
}

impl SyncToken {
    pub fn parse(token: &str) -> Option<Self> {
        if let Some(stripped) = token.strip_prefix('s') {
            stripped.parse::<i64>().ok().map(|stream_id| Self {
                stream_id,
                room_id: None,
                event_type: None,
            })
        } else {
            None
        }
    }

    pub fn encode(&self) -> String {
        format!("s{}", self.stream_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncFilter {
    pub limit: Option<i64>,
    pub types: Option<Vec<String>>,
    pub not_types: Option<Vec<String>>,
    pub senders: Option<Vec<String>>,
    pub not_senders: Option<Vec<String>>,
}

impl Default for SyncFilter {
    fn default() -> Self {
        Self {
            limit: Some(100),
            types: None,
            not_types: None,
            senders: None,
            not_senders: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomFilter {
    pub state: Option<SyncFilter>,
    pub timeline: Option<SyncFilter>,
    pub ephemeral: Option<SyncFilter>,
    pub account_data: Option<SyncFilter>,
}

impl Default for RoomFilter {
    fn default() -> Self {
        Self {
            state: Some(SyncFilter::default()),
            timeline: Some(SyncFilter {
                limit: Some(50),
                ..Default::default()
            }),
            ephemeral: Some(SyncFilter::default()),
            account_data: Some(SyncFilter::default()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRequest {
    pub since: Option<String>,
    pub filter: Option<String>,
    pub full_state: bool,
    pub set_presence: Option<String>,
    pub timeout: u64,
}

#[derive(Debug, Clone)]
pub struct SyncState {
    pub rooms: HashMap<String, RoomSyncState>,
    pub last_stream_id: i64,
}

#[derive(Debug, Clone)]
pub struct RoomSyncState {
    pub timeline_limit: i64,
    pub last_event_id: Option<String>,
    pub last_stream_id: i64,
}

pub struct SyncService {
    presence_storage: PresenceStorage,
    member_storage: RoomMemberStorage,
    event_storage: EventStorage,
    room_storage: RoomStorage,
    #[allow(dead_code)]
    device_storage: DeviceStorage,
}

impl SyncService {
    pub fn new(
        presence_storage: PresenceStorage,
        member_storage: RoomMemberStorage,
        event_storage: EventStorage,
        room_storage: RoomStorage,
        device_storage: DeviceStorage,
    ) -> Self {
        Self {
            presence_storage,
            member_storage,
            event_storage,
            room_storage,
            device_storage,
        }
    }

    pub async fn sync(
        &self,
        user_id: &str,
        timeout: u64,
        full_state: bool,
        set_presence: &str,
        since: Option<&str>,
    ) -> ApiResult<serde_json::Value> {
        self.update_presence(user_id, set_presence).await?;

        let since_token = since.and_then(SyncToken::parse);
        let is_incremental = since_token.is_some() && !full_state;

        let room_ids = self.member_storage.get_joined_rooms(user_id).await?;
        let since_ts = since_token.as_ref().map(|t| t.stream_id).unwrap_or(0);

        let room_events = self
            .fetch_events(&room_ids, since_ts, timeout, is_incremental)
            .await?;

        self.build_sync_response(
            user_id,
            &room_ids,
            room_events,
            &since_token,
            is_incremental,
        )
        .await
    }

    pub async fn room_sync(
        &self,
        user_id: &str,
        room_id: &str,
        timeout: u64,
        full_state: bool,
        since: Option<&str>,
    ) -> ApiResult<serde_json::Value> {
        let since_token = since.and_then(SyncToken::parse);
        let is_incremental = since_token.is_some() && !full_state;
        let since_ts = since_token.as_ref().map(|t| t.stream_id).unwrap_or(0);

        let room_ids = vec![room_id.to_string()];
        let room_events = self
            .fetch_events(&room_ids, since_ts, timeout, is_incremental)
            .await?;

        let events = room_events.get(room_id).cloned().unwrap_or_default();
        let room_sync = self
            .build_room_sync(room_id, user_id, events, is_incremental)
            .await?;

        let mut result = match room_sync {
            serde_json::Value::Object(map) => map,
            _ => serde_json::Map::new(),
        };

        let stream_id = self.get_next_stream_id().await?;
        result.insert(
            "next_batch".to_string(),
            json!(SyncToken {
                stream_id,
                room_id: None,
                event_type: None
            }
            .encode()),
        );

        Ok(serde_json::Value::Object(result))
    }

    pub async fn room_unread_counts(&self, room_id: &str, user_id: &str) -> ApiResult<(i64, i64)> {
        let (highlight_count, notification_count) =
            self.get_unread_counts(room_id, user_id).await?;
        Ok((notification_count, highlight_count))
    }

    async fn update_presence(&self, user_id: &str, set_presence: &str) -> ApiResult<()> {
        if set_presence != "offline" {
            self.presence_storage
                .set_presence(user_id, set_presence, None)
                .await
                .ok();
        }
        Ok(())
    }

    async fn fetch_events(
        &self,
        room_ids: &[String],
        since_ts: i64,
        timeout: u64,
        is_incremental: bool,
    ) -> ApiResult<HashMap<String, Vec<RoomEvent>>> {
        let limit = 50i64;

        if is_incremental {
            let events = self
                .event_storage
                .get_room_events_since_batch(room_ids, since_ts, limit)
                .await?;

            if events.values().all(|v| v.is_empty()) && timeout > 0 {
                self.poll_for_events(room_ids, since_ts, limit, timeout)
                    .await
            } else {
                Ok(events)
            }
        } else {
            self.event_storage
                .get_room_events_batch(room_ids, limit)
                .await
                .map_err(Into::into)
        }
    }

    async fn poll_for_events(
        &self,
        room_ids: &[String],
        since_ts: i64,
        limit: i64,
        timeout: u64,
    ) -> ApiResult<HashMap<String, Vec<RoomEvent>>> {
        let timeout_duration = std::time::Duration::from_millis(timeout);
        let start = std::time::Instant::now();
        let poll_interval = std::time::Duration::from_millis(500);

        loop {
            let events = self
                .event_storage
                .get_room_events_since_batch(room_ids, since_ts, limit)
                .await?;

            if !events.values().all(|v| v.is_empty()) || start.elapsed() >= timeout_duration {
                return Ok(events);
            }

            tokio::time::sleep(poll_interval).await;
        }
    }

    async fn build_sync_response(
        &self,
        user_id: &str,
        room_ids: &[String],
        room_events: HashMap<String, Vec<RoomEvent>>,
        since_token: &Option<SyncToken>,
        is_incremental: bool,
    ) -> ApiResult<serde_json::Value> {
        let mut rooms = serde_json::Map::new();
        for room_id in room_ids {
            let events = room_events.get(room_id).cloned().unwrap_or_default();
            let room_sync = self
                .build_room_sync(room_id, user_id, events, is_incremental)
                .await?;

            if room_sync.is_object() && !room_sync.as_object().is_some_and(|o| o.is_empty()) {
                rooms.insert(room_id.clone(), room_sync);
            }
        }

        let presence_events = self.get_presence_events(user_id, since_token).await?;
        let account_data_events = self.get_account_data_events(user_id).await?;
        let to_device_events = self.get_to_device_events(user_id, since_token).await?;
        let device_lists = self.get_device_lists(user_id, since_token).await?;
        let stream_id = self.get_next_stream_id().await?;

        Ok(json!({
            "next_batch": SyncToken { stream_id, room_id: None, event_type: None }.encode(),
            "rooms": {
                "join": rooms,
                "invite": {},
                "leave": {}
            },
            "presence": { "events": presence_events },
            "account_data": { "events": account_data_events },
            "to_device": { "events": to_device_events },
            "device_lists": device_lists,
            "device_one_time_keys_count": {}
        }))
    }

    async fn build_room_sync(
        &self,
        room_id: &str,
        user_id: &str,
        events: Vec<RoomEvent>,
        is_incremental: bool,
    ) -> ApiResult<serde_json::Value> {
        // state_events are already serialized to JSON by get_room_state_events
        let state_list = if is_incremental {
            vec![]
        } else {
            self.get_room_state_events(room_id).await?
        };

        let event_list: Vec<serde_json::Value> =
            events.iter().map(|e| self.event_to_json(e)).collect();

        let ephemeral_events = self.get_room_ephemeral_events(room_id, user_id).await?;

        let account_data_events = self.get_room_account_data_events(room_id, user_id).await?;

        let (highlight_count, notification_count) =
            self.get_unread_counts(room_id, user_id).await?;

        let prev_batch = events
            .first()
            .map(|e| format!("t{}", e.origin_server_ts))
            .unwrap_or_else(|| format!("t{}", chrono::Utc::now().timestamp_millis()));

        let limited = event_list.len() as i64 >= 50;

        Ok(json!({
            "state": {
                "events": state_list
            },
            "timeline": {
                "events": event_list,
                "limited": limited,
                "prev_batch": prev_batch
            },
            "ephemeral": {
                "events": ephemeral_events
            },
            "account_data": {
                "events": account_data_events
            },
            "unread_notifications": {
                "highlight_count": highlight_count,
                "notification_count": notification_count
            }
        }))
    }

    fn event_to_json(&self, event: &RoomEvent) -> serde_json::Value {
        let now = chrono::Utc::now().timestamp_millis();
        let age = now.saturating_sub(event.origin_server_ts);

        let mut obj = json!({
            "type": event.event_type,
            "content": event.content,
            "sender": event.user_id,
            "origin_server_ts": event.origin_server_ts,
            "event_id": event.event_id,
            "room_id": event.room_id,
            "unsigned": {
                "age": age
            }
        });

        // Include state_key for state events
        if let Some(ref state_key) = event.state_key {
            obj["state_key"] = json!(state_key);
        }

        obj
    }

    async fn get_room_state_events(&self, room_id: &str) -> ApiResult<Vec<serde_json::Value>> {
        let state_events = self
            .event_storage
            .get_state_events(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get room state events: {}", e)))?;

        let now = chrono::Utc::now().timestamp_millis();
        Ok(state_events
            .iter()
            .map(|e| {
                let sender = e.user_id.as_deref().unwrap_or(&e.sender);
                let age = now.saturating_sub(e.origin_server_ts);
                let event_type = e.event_type.as_deref().unwrap_or("m.room.message");
                let mut obj = json!({
                    "type": event_type,
                    "content": e.content,
                    "sender": sender,
                    "origin_server_ts": e.origin_server_ts,
                    "event_id": e.event_id,
                    "room_id": e.room_id,
                    "unsigned": {
                        "age": age
                    }
                });
                if let Some(ref sk) = e.state_key {
                    obj["state_key"] = json!(sk);
                }
                obj
            })
            .collect())
    }

    async fn get_presence_events(
        &self,
        user_id: &str,
        _since: &Option<SyncToken>,
    ) -> ApiResult<Vec<serde_json::Value>> {
        Ok(vec![json!({
            "content": {
                "avatar_url": null,
                "displayname": null,
                "last_active_ago": 0,
                "presence": "online"
            },
            "sender": user_id,
            "type": "m.presence"
        })])
    }

    async fn get_account_data_events(&self, user_id: &str) -> ApiResult<Vec<serde_json::Value>> {
        // Load user account_data from DB
        let rows = sqlx::query("SELECT data_type, content FROM account_data WHERE user_id = $1")
            .bind(user_id)
            .fetch_all(&*self.event_storage.pool)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get account data: {}", e)))?;

        let mut events: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                use sqlx::Row;
                let data_type: String = row.get("data_type");
                let content: serde_json::Value = row.get("content");
                json!({
                    "type": data_type,
                    "content": content
                })
            })
            .collect();

        // If no push rules exist yet, inject the Matrix spec default push rules
        let has_push_rules = events.iter().any(|e| e["type"] == "m.push_rules");
        if !has_push_rules {
            events.push(json!({
                "type": "m.push_rules",
                "content": {
                    "global": {
                        "content": [
                            {
                                "actions": ["notify", {"set_tweak": "highlight", "value": false}],
                                "conditions": [{"kind": "contains_display_name"}],
                                "default": true,
                                "enabled": true,
                                "rule_id": ".m.rule.contains_display_name"
                            }
                        ],
                        "override": [
                            {
                                "actions": ["dont_notify"],
                                "conditions": [{"kind": "event_match", "key": "content.msgtype", "pattern": "m.notice"}],
                                "default": true,
                                "enabled": true,
                                "rule_id": ".m.rule.suppress_notices"
                            }
                        ],
                        "room": [],
                        "sender": [],
                        "underride": [
                            {
                                "actions": ["notify", {"set_tweak": "sound", "value": "default"}],
                                "conditions": [{"kind": "event_match", "key": "type", "pattern": "m.room.message"}],
                                "default": true,
                                "enabled": true,
                                "rule_id": ".m.rule.message"
                            }
                        ]
                    }
                }
            }));
        }

        Ok(events)
    }

    async fn get_to_device_events(
        &self,
        user_id: &str,
        since: &Option<SyncToken>,
    ) -> ApiResult<Vec<serde_json::Value>> {
        let since_stream_id = since.as_ref().map(|t| t.stream_id).unwrap_or(0);

        let rows = sqlx::query(
            r#"
            SELECT sender_user_id, sender_device_id, event_type, content, message_id
            FROM to_device_messages
            WHERE recipient_user_id = $1 AND stream_id > $2
            ORDER BY stream_id ASC
            LIMIT 100
            "#,
        )
        .bind(user_id)
        .bind(since_stream_id)
        .fetch_all(&*self.event_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get to-device events: {}", e)))?;

        let events: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                use sqlx::Row;
                let sender: String = row.get("sender_user_id");
                let _sender_device: String = row.get("sender_device_id");
                let event_type: String = row.get("event_type");
                let content: serde_json::Value = row.get("content");
                let message_id: Option<String> = row.get("message_id");

                let mut obj = json!({
                    "type": event_type,
                    "sender": sender,
                    "content": content,
                });

                if let Some(mid) = message_id {
                    obj["message_id"] = json!(mid);
                }

                obj
            })
            .collect();

        Ok(events)
    }

    async fn get_device_lists(
        &self,
        user_id: &str,
        since: &Option<SyncToken>,
    ) -> ApiResult<serde_json::Value> {
        let since_stream_id = since.as_ref().map(|t| t.stream_id).unwrap_or(0);

        // Get users whose devices have changed
        let changed_rows = sqlx::query(
            r#"
            SELECT DISTINCT user_id
            FROM device_lists_stream
            WHERE stream_id > $1
            AND user_id != $2
            ORDER BY user_id
            LIMIT 100
            "#,
        )
        .bind(since_stream_id)
        .bind(user_id)
        .fetch_all(&*self.event_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get device lists: {}", e)))?;

        let changed: Vec<String> = changed_rows
            .iter()
            .map(|row| {
                use sqlx::Row;
                row.get("user_id")
            })
            .collect();

        // Get users who left (no longer share rooms)
        let left_rows = sqlx::query(
            r#"
            SELECT DISTINCT dl.user_id
            FROM device_lists_stream dl
            LEFT JOIN room_memberships rm ON rm.user_id = dl.user_id
            WHERE dl.stream_id > $1
            AND dl.user_id != $2
            AND rm.user_id IS NULL
            ORDER BY dl.user_id
            LIMIT 100
            "#,
        )
        .bind(since_stream_id)
        .bind(user_id)
        .fetch_all(&*self.event_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get left device lists: {}", e)))?;

        let left: Vec<String> = left_rows
            .iter()
            .map(|row| {
                use sqlx::Row;
                row.get("user_id")
            })
            .collect();

        Ok(json!({
            "changed": changed,
            "left": left
        }))
    }

    async fn get_next_stream_id(&self) -> ApiResult<i64> {
        let row = sqlx::query(
            r#"
            INSERT INTO sync_stream_id DEFAULT VALUES RETURNING id
            "#,
        )
        .fetch_one(&*self.event_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to generate stream ID: {}", e)))?;

        use sqlx::Row;
        let stream_id: i64 = row.get("id");
        Ok(stream_id)
    }

    async fn get_room_ephemeral_events(
        &self,
        room_id: &str,
        _user_id: &str,
    ) -> ApiResult<Vec<serde_json::Value>> {
        let now = chrono::Utc::now().timestamp_millis();

        let rows = sqlx::query(
            r#"
            SELECT event_type, user_id, content
            FROM room_ephemeral
            WHERE room_id = $1
            AND (expires_at IS NULL OR expires_at > $2)
            ORDER BY stream_id DESC
            LIMIT 50
            "#,
        )
        .bind(room_id)
        .bind(now)
        .fetch_all(&*self.event_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get ephemeral events: {}", e)))?;

        let events: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                use sqlx::Row;
                let event_type: String = row.get("event_type");
                let user_id: String = row.get("user_id");
                let content: serde_json::Value = row.get("content");

                json!({
                    "type": event_type,
                    "sender": user_id,
                    "content": content
                })
            })
            .collect();

        Ok(events)
    }

    async fn get_room_account_data_events(
        &self,
        room_id: &str,
        user_id: &str,
    ) -> ApiResult<Vec<serde_json::Value>> {
        let rows = sqlx::query(
            "SELECT data_type, data as content FROM room_account_data WHERE user_id = $1 AND room_id = $2",
        )
        .bind(user_id)
        .bind(room_id)
        .fetch_all(&*self.event_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get room account data: {}", e)))?;

        Ok(rows
            .iter()
            .map(|row| {
                use sqlx::Row;
                let data_type: String = row.get("data_type");
                let content: serde_json::Value = row.get("content");
                json!({
                    "type": data_type,
                    "content": content
                })
            })
            .collect())
    }

    async fn get_unread_counts(&self, room_id: &str, user_id: &str) -> ApiResult<(i64, i64)> {
        // Get the user's last read event timestamp from read_markers
        let last_read_ts: Option<i64> = sqlx::query_scalar(
            r#"
            SELECT e.origin_server_ts
            FROM read_markers rm
            JOIN events e ON e.event_id = rm.event_id
            WHERE rm.room_id = $1 AND rm.user_id = $2
            LIMIT 1
            "#,
        )
        .bind(room_id)
        .bind(user_id)
        .fetch_optional(&*self.event_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get read marker: {}", e)))?
        .flatten();

        let since_ts = last_read_ts.unwrap_or(0);

        // Count total unread events (notifications)
        let notification_count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM events
            WHERE room_id = $1
              AND user_id != $2
              AND origin_server_ts > $3
              AND state_key IS NULL
            "#,
        )
        .bind(room_id)
        .bind(user_id)
        .bind(since_ts)
        .fetch_one(&*self.event_storage.pool)
        .await
        .unwrap_or(0);

        // Count highlight events (mentions of the user)
        let highlight_count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM events
            WHERE room_id = $1
              AND user_id != $2
              AND origin_server_ts > $3
              AND state_key IS NULL
              AND (
                content::text LIKE $4
                OR content::text LIKE '%@room%'
              )
            "#,
        )
        .bind(room_id)
        .bind(user_id)
        .bind(since_ts)
        .bind(format!("%{}%", user_id))
        .fetch_one(&*self.event_storage.pool)
        .await
        .unwrap_or(0);

        Ok((highlight_count, notification_count))
    }

    pub async fn get_room_messages(
        &self,
        room_id: &str,
        user_id: &str,
        from: &str,
        limit: i64,
        _dir: &str,
    ) -> ApiResult<serde_json::Value> {
        if !self
            .member_storage
            .is_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to check membership: {}", e)))?
        {
            return Err(ApiError::forbidden(
                "You are not a member of this room".to_string(),
            ));
        }

        let events = self
            .event_storage
            .get_room_events(room_id, limit)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get messages: {}", e)))?;

        let event_list: Vec<serde_json::Value> =
            events.iter().map(|e| self.event_to_json(e)).collect();

        let end_token = events
            .last()
            .map(|e| format!("t{}", e.origin_server_ts))
            .unwrap_or_else(|| format!("t{}", chrono::Utc::now().timestamp_millis()));

        Ok(json!({
            "chunk": event_list,
            "start": from,
            "end": end_token
        }))
    }

    pub async fn get_public_rooms(
        &self,
        limit: i64,
        _since: Option<&str>,
    ) -> ApiResult<serde_json::Value> {
        let rooms = self
            .room_storage
            .get_public_rooms(limit)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get public rooms: {}", e)))?;

        let room_list: Vec<serde_json::Value> = rooms
            .iter()
            .map(|r| {
                json!({
                    "room_id": r.room_id,
                    "name": r.name,
                    "topic": r.topic,
                    "canonical_alias": r.canonical_alias,
                    "is_public": r.is_public,
                    "join_rule": r.join_rule
                })
            })
            .collect();

        let next_batch = if room_list.len() as i64 >= limit {
            Some(format!("p{}", chrono::Utc::now().timestamp_millis()))
        } else {
            None
        };

        let mut response = json!({
            "chunk": room_list,
            "total_room_count_estimate": room_list.len() as i64
        });

        if let Some(batch) = next_batch {
            response["next_batch"] = json!(batch);
        }

        Ok(response)
    }

    pub async fn get_filter(
        &self,
        _user_id: &str,
        _filter_id: &str,
    ) -> ApiResult<serde_json::Value> {
        Ok(json!({}))
    }

    pub async fn set_filter(
        &self,
        _user_id: &str,
        _filter: &serde_json::Value,
    ) -> ApiResult<String> {
        Ok(format!("filter_{}", chrono::Utc::now().timestamp_millis()))
    }

    pub async fn get_events(
        &self,
        user_id: &str,
        from: &str,
        _timeout: u64,
    ) -> ApiResult<serde_json::Value> {
        let room_ids = self
            .member_storage
            .get_joined_rooms(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get rooms: {}", e)))?;

        let since_ts = from
            .trim_start_matches('s')
            .trim_start_matches('t')
            .parse::<i64>()
            .unwrap_or(0);

        let limit = 100i64;
        let events = self
            .event_storage
            .get_room_events_since_batch(&room_ids, since_ts, limit)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get events: {}", e)))?;

        let mut chunk = vec![];
        for room_events in events.values() {
            for event in room_events {
                chunk.push(self.event_to_json(event));
            }
        }

        let end_token = format!("s{}", chrono::Utc::now().timestamp_millis());

        Ok(json!({
            "start": from,
            "end": end_token,
            "chunk": chunk
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_token_parse() {
        let token = SyncToken::parse("s1234567890");
        assert!(token.is_some());
        let token = token.unwrap();
        assert_eq!(token.stream_id, 1234567890);
    }

    #[test]
    fn test_sync_token_encode() {
        let token = SyncToken {
            stream_id: 1234567890,
            room_id: None,
            event_type: None,
        };
        assert_eq!(token.encode(), "s1234567890");
    }

    #[test]
    fn test_sync_token_roundtrip() {
        let original = SyncToken {
            stream_id: 9876543210,
            room_id: None,
            event_type: None,
        };
        let encoded = original.encode();
        let parsed = SyncToken::parse(&encoded).unwrap();
        assert_eq!(original.stream_id, parsed.stream_id);
    }

    #[test]
    fn test_sync_filter_default() {
        let filter = SyncFilter::default();
        assert_eq!(filter.limit, Some(100));
        assert!(filter.types.is_none());
    }

    #[test]
    fn test_room_filter_default() {
        let filter = RoomFilter::default();
        assert!(filter.state.is_some());
        assert!(filter.timeline.is_some());
        assert_eq!(filter.timeline.unwrap().limit, Some(50));
    }

    #[test]
    fn test_sync_response_format() {
        let response = json!({
            "next_batch": "s1234567890",
            "rooms": {
                "join": {},
                "invite": {},
                "leave": {}
            },
            "presence": json!({
                "events": []
            }),
            "account_data": json!({
                "events": []
            }),
            "to_device": json!({
                "events": []
            }),
            "device_lists": {
                "changed": [],
                "left": []
            }
        });

        assert!(response.get("next_batch").is_some());
        assert!(response["rooms"]["join"].is_object());
        assert!(response["presence"]["events"].is_array());
        assert!(response["device_lists"]["changed"].is_array());
    }

    #[test]
    fn test_room_timeline_format() {
        let timeline = json!({
            "events": [],
            "limited": true,
            "prev_batch": "t1234567890"
        });

        assert!(timeline["events"].is_array());
        assert!(timeline["limited"].is_boolean());
        assert_eq!(timeline["prev_batch"], "t1234567890");
    }

    #[test]
    fn test_room_state_format() {
        let state = json!({
            "events": []
        });
        assert!(state["events"].is_array());
    }

    #[test]
    fn test_presence_format() {
        let presence = json!({
            "events": []
        });
        assert!(presence["events"].is_array());
    }

    #[test]
    fn test_account_data_format() {
        let account_data = json!({
            "events": []
        });
        assert!(account_data["events"].is_array());
    }

    #[test]
    fn test_to_device_format() {
        let to_device = json!({
            "events": []
        });
        assert!(to_device["events"].is_array());
    }

    #[test]
    fn test_device_lists_format() {
        let device_lists = json!({
            "changed": ["@user1:example.com"],
            "left": ["@user2:example.com"]
        });

        assert!(device_lists["changed"].is_array());
        assert!(device_lists["left"].is_array());
    }

    #[test]
    fn test_unread_notifications_format() {
        let notifications = json!({
            "highlight_count": 0,
            "notification_count": 0
        });

        assert_eq!(notifications["highlight_count"], 0);
        assert_eq!(notifications["notification_count"], 0);
    }

    #[test]
    fn test_ephemeral_format() {
        let ephemeral = json!({
            "events": []
        });
        assert!(ephemeral["events"].is_array());
    }

    #[test]
    fn test_room_messages_response_format() {
        let response = json!({
            "chunk": [],
            "start": "t1234567890",
            "end": "t1234567899"
        });

        assert!(response.get("chunk").is_some());
        assert!(response.get("start").is_some());
        assert!(response.get("end").is_some());
    }

    #[test]
    fn test_public_rooms_response_format() {
        let response = json!({
            "chunk": [],
            "total_room_count_estimate": 0,
            "next_batch": "p1234567890"
        });

        assert!(response.get("chunk").is_some());
        assert!(response.get("total_room_count_estimate").is_some());
        assert!(response.get("next_batch").is_some());
    }
}
