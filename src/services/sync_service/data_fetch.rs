use super::types::*;
use super::SyncService;
use crate::common::*;
use crate::map_internal;
use crate::services::*;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

impl SyncService {
    pub(crate) async fn update_presence(&self, user_id: &str, set_presence: &str) -> ApiResult<()> {
        let presence_state = crate::common::PresenceState::from_str_opt(set_presence)
            .unwrap_or(crate::common::PresenceState::Online);
        self.presence_storage.set_presence(user_id, presence_state, None).await.ok();
        Ok(())
    }

    pub(crate) fn aggregate_ephemeral_events(events: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
        let events_len = events.len();
        let mut receipt_content = serde_json::Map::new();
        let mut typing_events: Vec<serde_json::Value> = Vec::with_capacity(8);

        for event in events {
            let event_type = event.get("type").and_then(|v| v.as_str()).unwrap_or("");
            match event_type {
                "m.receipt" => {
                    if let Some(content) = event.get("content").and_then(|v| v.as_object()) {
                        for (event_id, receipt_data) in content {
                            let entry = receipt_content
                                .entry(event_id.clone())
                                .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
                            if let Some(entry_obj) = entry.as_object_mut() {
                                if let Some(data_obj) = receipt_data.as_object() {
                                    for (receipt_type, user_data) in data_obj {
                                        entry_obj.insert(receipt_type.clone(), user_data.clone());
                                    }
                                }
                            }
                        }
                    }
                }
                "m.typing" => {
                    typing_events.push(event);
                }
                _ => {
                    typing_events.push(event);
                }
            }
        }

        let mut result: Vec<serde_json::Value> = Vec::with_capacity(events_len.min(64));

        if !receipt_content.is_empty() {
            result.push(json!({
                "type": "m.receipt",
                "content": serde_json::Value::Object(receipt_content)
            }));
        }

        result.extend(typing_events);
        result
    }

    pub(crate) async fn get_room_state_events_batch(
        &self,
        room_ids: &[String],
        event_format: SyncEventFormat,
    ) -> ApiResult<HashMap<String, Vec<Value>>> {
        let state_events = self
            .event_storage
            .get_state_events_batch(room_ids)
            .await
            .map_err(map_internal!("Failed to get room state events"))?;

        Ok(state_events
            .into_iter()
            .map(|(room_id, events)| {
                let values = events.iter().map(|event| Self::state_event_to_json(event, event_format)).collect();
                (room_id, values)
            })
            .collect())
    }

    pub(crate) async fn get_state_events_for_sync_batch(
        &self,
        room_ids: &[String],
        event_format: SyncEventFormat,
        params: StateEventsBatchParams<'_>,
    ) -> ApiResult<HashMap<String, Vec<Value>>> {
        if room_ids.is_empty() {
            return Ok(HashMap::new());
        }

        if !params.is_incremental {
            return self.get_room_state_events_batch(room_ids, event_format).await;
        }

        let delta_state_by_room = if let Some(stream_ord) = params.since_stream_ordering {
            self.event_storage
                .get_state_events_since_stream_batch(room_ids, stream_ord)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to get room state events", &e))?
        } else {
            self.event_storage
                .get_state_events_since_batch(room_ids, params.since_ts)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to get room state events", &e))?
        };

        let newly_visible_rooms: Vec<String> = delta_state_by_room
            .iter()
            .filter_map(|(room_id, events)| {
                let user_just_joined = events.iter().any(|e| {
                    e.event_type.as_deref() == Some("m.room.member")
                        && e.state_key.as_deref() == Some(params.user_id)
                        && matches!(e.content.get("membership").and_then(|v| v.as_str()), Some("join") | Some("invite"))
                        && e.stream_ordering.is_some()
                });
                if user_just_joined {
                    Some(room_id.clone())
                } else {
                    None
                }
            })
            .collect();

        let full_state_for_newly_visible = if newly_visible_rooms.is_empty() {
            HashMap::new()
        } else {
            self.event_storage
                .get_state_events_batch(&newly_visible_rooms)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to get full state for newly visible rooms", &e))?
        };

        if !params.lazy_load_members {
            let mut result: HashMap<String, Vec<Value>> = HashMap::new();
            for (room_id, events) in delta_state_by_room {
                if let Some(full_state) = full_state_for_newly_visible.get(&room_id) {
                    let values =
                        full_state.iter().map(|event| Self::state_event_to_json(event, event_format)).collect();
                    result.insert(room_id, values);
                } else {
                    let values = events.iter().map(|event| Self::state_event_to_json(event, event_format)).collect();
                    result.insert(room_id, values);
                }
            }
            return Ok(result);
        }

        let current_member_state_by_room = self
            .event_storage
            .get_state_events_by_type_batch(room_ids, "m.room.member")
            .await
            .map_err(map_internal!("Failed to get room state events"))?;

        let mut result = HashMap::new();
        for room_id in room_ids {
            let mut values = Vec::new();

            if let Some(full_state) = full_state_for_newly_visible.get(room_id) {
                for event in full_state {
                    if event.event_type.as_deref() == Some("m.room.member") {
                        continue;
                    }
                    values.push(Self::state_event_to_json(event, event_format));
                }
            } else {
                for event in delta_state_by_room.get(room_id).into_iter().flatten() {
                    if event.event_type.as_deref() == Some("m.room.member") {
                        continue;
                    }
                    values.push(Self::state_event_to_json(event, event_format));
                }
            }

            for event in current_member_state_by_room.get(room_id).into_iter().flatten() {
                values.push(Self::state_event_to_json(event, event_format));
            }

            result.insert(room_id.clone(), values);
        }

        Ok(result)
    }

    pub(crate) async fn get_presence_events(
        &self,
        user_id: &str,
        _since: &Option<SyncToken>,
    ) -> ApiResult<Vec<serde_json::Value>> {
        let presence = self
            .presence_storage
            .get_presence_with_meta(user_id)
            .await
            .map_err(map_internal!("Failed to get presence for sync"))?;

        let Some((presence_state, status_msg, last_active_ts)) = presence else {
            return Ok(Vec::new());
        };

        let now = chrono::Utc::now().timestamp_millis();
        let presence_state = PresenceState::from(presence_state.as_str());
        let (last_active_ago, currently_active) = presence_state.derive_activity(last_active_ts, now);

        Ok(vec![json!({
            "content": {
                "avatar_url": null,
                "displayname": null,
                "last_active_ago": last_active_ago,
                "presence": presence_state.to_string(),
                "status_msg": status_msg,
                "currently_active": currently_active
            },
            "sender": user_id,
            "type": "m.presence"
        })])
    }

    pub(crate) async fn get_account_data_events(&self, user_id: &str) -> ApiResult<Vec<serde_json::Value>> {
        let rows = sqlx::query!(
            r#"SELECT data_type AS "data_type!", content AS "content!" FROM account_data WHERE user_id = $1"#,
            user_id,
        )
        .fetch_all(&*self.event_storage.pool)
        .await
        .map_err(map_internal!("Failed to get account data"))?;

        let mut events: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                json!({
                    "type": row.data_type,
                    "content": row.content
                })
            })
            .collect();

        let joined_room_ids: HashSet<String> = self
            .member_storage
            .get_joined_rooms(user_id)
            .await
            .map_err(map_internal!("Failed to load joined rooms"))?
            .into_iter()
            .collect();
        if let Some(direct) = events.iter_mut().find(|e| e["type"] == "m.direct") {
            if let Some(map) = direct.get_mut("content").and_then(|c| c.as_object_mut()) {
                map.retain(|_, value| {
                    if let Some(rooms) = value.as_array_mut() {
                        rooms.retain(|room| {
                            room.as_str()
                                .is_some_and(|id| !id.is_empty() && id.starts_with('!') && joined_room_ids.contains(id))
                        });
                        !rooms.is_empty()
                    } else {
                        false
                    }
                });
            }
        }

        let username = user_id.trim_start_matches('@').split(':').next().unwrap_or("");
        if let Some(existing) = events.iter_mut().find(|e| e["type"] == "m.push_rules") {
            if let Some(content) = existing.get_mut("content") {
                crate::web::routes::push_rules::merge_default_push_rules(content, user_id, username);
            }
        } else {
            events.push(json!({
                "type": "m.push_rules",
                "content": crate::web::routes::push_rules::default_push_rules_for_user(
                    user_id, username,
                ),
            }));
        }

        Ok(events)
    }

    pub(crate) async fn get_to_device_events(
        &self,
        user_id: &str,
        device_id: Option<&str>,
        since: &Option<SyncToken>,
    ) -> ApiResult<(Vec<serde_json::Value>, i64)> {
        let Some(device_id) = device_id else {
            return Ok((Vec::new(), 0));
        };
        let since_stream_id = Self::to_device_since_stream_id(since);

        let rows = sqlx::query!(
            r#"
            SELECT sender_user_id AS "sender_user_id!", sender_device_id, event_type AS "event_type!", content AS "content!", message_id, stream_id AS "stream_id!"
            FROM to_device_messages
            WHERE recipient_user_id = $1
              AND recipient_device_id = $2
              AND stream_id > $3
            ORDER BY stream_id ASC
            LIMIT $4
            "#,
            user_id,
            device_id,
            since_stream_id,
            self.sync_to_device_limit(),
        )
        .fetch_all(&*self.event_storage.pool)
        .await
        .map_err(map_internal!("Failed to get to-device events"))?;

        let mut max_stream_id = since_stream_id;
        let events: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                if row.stream_id > max_stream_id {
                    max_stream_id = row.stream_id;
                }

                let mut obj = json!({
                    "type": row.event_type,
                    "sender": row.sender_user_id,
                    "content": row.content,
                });

                if let Some(mid) = &row.message_id {
                    obj["message_id"] = json!(mid);
                }

                obj
            })
            .collect();

        Ok((events, max_stream_id))
    }

    pub(crate) async fn get_device_lists(
        &self,
        user_id: &str,
        since: &Option<SyncToken>,
    ) -> ApiResult<(serde_json::Value, i64)> {
        let since_stream_id = Self::device_list_since_stream_id(since);

        let changed_rows = sqlx::query!(
            r#"
            SELECT user_id AS "user_id!", MAX(stream_id) AS "max_id!"
            FROM device_lists_stream
            WHERE stream_id > $1
              AND user_id != $2
            GROUP BY user_id
            ORDER BY MAX(stream_id) ASC
            LIMIT 100
            "#,
            since_stream_id,
            user_id,
        )
        .fetch_all(&*self.event_storage.pool)
        .await
        .map_err(map_internal!("Failed to get device lists"))?;

        let mut max_stream_id = since_stream_id;
        let changed: Vec<String> = changed_rows
            .iter()
            .map(|row| {
                if row.max_id > max_stream_id {
                    max_stream_id = row.max_id;
                }
                row.user_id.clone()
            })
            .collect();

        let left_rows = sqlx::query!(
            r#"
            SELECT DISTINCT dl.user_id AS "user_id!"
            FROM device_lists_stream dl
            LEFT JOIN room_memberships rm ON rm.user_id = dl.user_id
            WHERE dl.stream_id > $1
              AND dl.user_id != $2
              AND rm.user_id IS NULL
            ORDER BY dl.user_id
            LIMIT 100
            "#,
            since_stream_id,
            user_id,
        )
        .fetch_all(&*self.event_storage.pool)
        .await
        .map_err(map_internal!("Failed to get left device lists"))?;

        let left: Vec<String> = left_rows
            .iter()
            .map(|row| row.user_id.clone())
            .collect();

        Ok((
            json!({
                "changed": changed,
                "left": left
            }),
            max_stream_id,
        ))
    }

    pub(crate) fn to_device_since_stream_id(since: &Option<SyncToken>) -> i64 {
        since.as_ref().and_then(|token| token.to_device_stream_id).unwrap_or(0)
    }

    pub(crate) fn device_list_since_stream_id(since: &Option<SyncToken>) -> i64 {
        since.as_ref().and_then(|token| token.device_list_stream_id).unwrap_or(0)
    }

    pub(crate) async fn get_room_ephemeral_events(
        &self,
        room_id: &str,
        _user_id: &str,
    ) -> ApiResult<Vec<serde_json::Value>> {
        let now = chrono::Utc::now().timestamp_millis();
        let limit = self.sync_ephemeral_limit();

        let rows = sqlx::query!(
            r#"
            SELECT event_type AS "event_type!", user_id AS "user_id!", content AS "content!"
            FROM room_ephemeral
            WHERE room_id = $1
            AND (expires_at IS NULL OR expires_at > $2)
            ORDER BY stream_id DESC
            LIMIT $3
            "#,
            room_id,
            now,
            limit,
        )
        .fetch_all(&*self.event_storage.pool)
        .await
        .map_err(map_internal!("Failed to get ephemeral events"))?;

        let events: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                json!({
                    "type": row.event_type,
                    "content": row.content
                })
            })
            .collect();

        let events = Self::aggregate_ephemeral_events(events);

        Ok(events)
    }

    pub(crate) async fn get_room_ephemeral_events_batch(
        &self,
        room_ids: &[String],
    ) -> ApiResult<HashMap<String, Vec<serde_json::Value>>> {
        let limit = self.sync_ephemeral_limit();
        let mut result: HashMap<String, Vec<serde_json::Value>> =
            room_ids.iter().cloned().map(|room_id| (room_id, Vec::new())).collect();
        if room_ids.is_empty() {
            return Ok(result);
        }

        let now = chrono::Utc::now().timestamp_millis();
        let rows = sqlx::query!(
            r#"
            SELECT room_id AS "room_id!", event_type AS "event_type!", user_id AS "user_id!", content AS "content!", stream_id AS "stream_id!"
            FROM (
                SELECT
                    room_id,
                    event_type,
                    user_id,
                    content,
                    stream_id,
                    ROW_NUMBER() OVER (
                        PARTITION BY room_id
                        ORDER BY stream_id DESC
                    ) AS rn
                FROM room_ephemeral
                WHERE room_id = ANY($1)
                  AND (expires_at IS NULL OR expires_at > $2)
            ) ranked
            WHERE rn <= $3
            ORDER BY room_id, stream_id DESC
            "#,
            room_ids,
            now,
            limit,
        )
        .fetch_all(&*self.event_storage.pool)
        .await
        .map_err(map_internal!("Failed to get room ephemeral events"))?;

        for row in &rows {
            if let Some(events) = result.get_mut(&row.room_id) {
                if events.len() >= limit as usize {
                    continue;
                }
                events.push(json!({
                    "type": row.event_type,
                    "content": row.content
                }));
            }
        }

        for (_room_id, events) in result.iter_mut() {
            *events = Self::aggregate_ephemeral_events(std::mem::take(events));
        }

        Ok(result)
    }

    pub(crate) async fn get_room_account_data_events(
        &self,
        room_id: &str,
        user_id: &str,
    ) -> ApiResult<Vec<serde_json::Value>> {
        let rows = sqlx::query!(
            r#"SELECT data_type AS "data_type!", data AS "content!" FROM room_account_data WHERE user_id = $1 AND room_id = $2"#,
            user_id,
            room_id,
        )
        .fetch_all(&*self.event_storage.pool)
        .await
        .map_err(map_internal!("Failed to get room account data"))?;

        Ok(rows
            .iter()
            .map(|row| {
                json!({
                    "type": row.data_type,
                    "content": row.content
                })
            })
            .collect())
    }

    pub(crate) async fn get_room_account_data_events_batch(
        &self,
        user_id: &str,
        room_ids: &[String],
    ) -> ApiResult<HashMap<String, Vec<serde_json::Value>>> {
        let mut result: HashMap<String, Vec<serde_json::Value>> =
            room_ids.iter().cloned().map(|room_id| (room_id, Vec::new())).collect();
        if room_ids.is_empty() {
            return Ok(result);
        }

        let rows = sqlx::query!(
            r#"SELECT room_id AS "room_id!", data_type AS "data_type!", data AS "content!" FROM room_account_data WHERE user_id = $1 AND room_id = ANY($2)"#,
            user_id,
            room_ids,
        )
        .fetch_all(&*self.event_storage.pool)
        .await
        .map_err(map_internal!("Failed to get room account data"))?;

        for row in &rows {
            if let Some(events) = result.get_mut(&row.room_id) {
                events.push(json!({
                    "type": row.data_type,
                    "content": row.content
                }));
            }
        }

        Ok(result)
    }

    pub(crate) async fn get_unread_counts(&self, room_id: &str, user_id: &str) -> ApiResult<(i64, i64)> {
        let last_read_ts: Option<i64> = sqlx::query_scalar!(
            r#"
            SELECT e.origin_server_ts AS "last_read_ts!"
            FROM read_markers rm
            JOIN events e ON e.event_id = rm.event_id
            WHERE rm.room_id = $1 AND rm.user_id = $2
            LIMIT 1
            "#,
            room_id,
            user_id,
        )
        .fetch_optional(&*self.event_storage.pool)
        .await
        .map_err(map_internal!("Failed to get read marker"))?;

        let since_ts = last_read_ts.unwrap_or(0);

        let notification_count = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*) AS "count!"
            FROM events
            WHERE room_id = $1
              AND COALESCE(user_id, sender) != $2
              AND origin_server_ts > $3
              AND state_key IS NULL
            "#,
            room_id,
            user_id,
            since_ts,
        )
        .fetch_one(&*self.event_storage.pool)
        .await
        .unwrap_or(0);

        let mention_pattern = format!("%{user_id}%");
        let highlight_count = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*) AS "count!"
            FROM events
            WHERE room_id = $1
              AND COALESCE(user_id, sender) != $2
              AND origin_server_ts > $3
              AND state_key IS NULL
              AND (
                content::text LIKE $4
                OR content::text LIKE '%@room%'
              )
            "#,
            room_id,
            user_id,
            since_ts,
            mention_pattern.as_str(),
        )
        .fetch_one(&*self.event_storage.pool)
        .await
        .unwrap_or(0);

        Ok((highlight_count, notification_count))
    }

    pub(crate) async fn get_unread_counts_batch(
        &self,
        room_ids: &[String],
        user_id: &str,
    ) -> ApiResult<HashMap<String, (i64, i64)>> {
        let mut result: HashMap<String, (i64, i64)> =
            room_ids.iter().cloned().map(|room_id| (room_id, (0, 0))).collect();
        if room_ids.is_empty() {
            return Ok(result);
        }

        let mention_pattern = format!("%{user_id}%");
        let rows = sqlx::query!(
            r#"
            WITH target_rooms AS (
                SELECT UNNEST($2::text[]) AS room_id
            ),
            last_reads AS (
                SELECT tr.room_id, COALESCE(MAX(e.origin_server_ts), 0) AS last_read_ts
                FROM target_rooms tr
                LEFT JOIN read_markers rm
                  ON rm.room_id = tr.room_id
                 AND rm.user_id = $1
                LEFT JOIN events e
                  ON e.event_id = rm.event_id
                GROUP BY tr.room_id
            )
            SELECT
                tr.room_id AS "room_id!",
                COALESCE(COUNT(ev.event_id) FILTER (
                    WHERE COALESCE(ev.user_id, ev.sender) != $1
                      AND ev.state_key IS NULL
                      AND ev.origin_server_ts > lr.last_read_ts
                ), 0) AS "notification_count!",
                COALESCE(COUNT(ev.event_id) FILTER (
                    WHERE COALESCE(ev.user_id, ev.sender) != $1
                      AND ev.state_key IS NULL
                      AND ev.origin_server_ts > lr.last_read_ts
                      AND (
                        ev.content::text LIKE $3
                        OR ev.content::text LIKE '%@room%'
                      )
                ), 0) AS "highlight_count!"
            FROM target_rooms tr
            LEFT JOIN last_reads lr
              ON lr.room_id = tr.room_id
            LEFT JOIN events ev
              ON ev.room_id = tr.room_id
             AND COALESCE(ev.user_id, ev.sender) != $1
             AND ev.state_key IS NULL
             AND ev.origin_server_ts > lr.last_read_ts
            GROUP BY tr.room_id, lr.last_read_ts
            "#,
            user_id,
            room_ids,
            mention_pattern.as_str(),
        )
        .fetch_all(&*self.event_storage.pool)
        .await
        .map_err(map_internal!("Failed to get unread counts"))?;

        for row in &rows {
            result.insert(row.room_id.clone(), (row.highlight_count, row.notification_count));
        }

        Ok(result)
    }
}
