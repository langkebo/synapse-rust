use std::collections::BTreeSet;

use serde_json::{json, Value};
use synapse_common::error::ApiError;
use synapse_e2ee::device_keys::DeviceKeyStorage;

use super::SlidingSyncService;

impl SlidingSyncService {
    pub(super) async fn build_extensions_response(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        since_pos: Option<&str>,
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
                    v.as_object().map(|obj| obj.get("enabled").and_then(|e| e.as_bool()).unwrap_or(true))
                }
            })
            .unwrap_or(false);

        if account_data_enabled {
            let room_ids: Vec<String> =
                rooms_response.as_object().map(|obj| obj.keys().cloned().collect()).unwrap_or_default();

            let global = self.storage.get_global_account_data(user_id).await?;
            let rooms = self.storage.get_room_account_data(user_id, &room_ids).await?;

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
                    v.as_object().map(|obj| obj.get("enabled").and_then(|e| e.as_bool()).unwrap_or(true))
                }
            })
            .unwrap_or(false);

        if receipts_enabled {
            let room_ids: Vec<String> =
                rooms_response.as_object().map(|obj| obj.keys().cloned().collect()).unwrap_or_default();
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
                    v.as_object().map(|obj| obj.get("enabled").and_then(|e| e.as_bool()).unwrap_or(true))
                }
            })
            .unwrap_or(false);

        if typing_enabled {
            let room_ids: Vec<String> =
                rooms_response.as_object().map(|obj| obj.keys().cloned().collect()).unwrap_or_default();
            let mut typing_rooms = serde_json::Map::new();
            match self.typing_service.get_typing_users_batch(&room_ids).await {
                Ok(batch) => {
                    for (room_id, user_ids) in batch {
                        typing_rooms.insert(room_id, serde_json::json!({ "user_ids": user_ids }));
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, room_count = room_ids.len(), "Failed to get typing users batch");
                }
            }
            response_extensions.insert(
                "typing".to_string(),
                serde_json::json!({
                    "rooms": typing_rooms
                }),
            );
        }

        let to_device_request = request_extensions.get("to_device");
        let to_device_enabled = to_device_request
            .and_then(|v| {
                if v.as_bool() == Some(true) {
                    Some(true)
                } else {
                    v.as_object().map(|obj| obj.get("enabled").and_then(|e| e.as_bool()).unwrap_or(true))
                }
            })
            .unwrap_or(false);

        if to_device_enabled {
            let to_device = self.build_to_device_extension(user_id, device_id, to_device_request).await?;
            response_extensions.insert("to_device".to_string(), to_device);
        }

        let e2ee_enabled = request_extensions
            .get("e2ee")
            .and_then(|v| {
                if v.as_bool() == Some(true) {
                    Some(true)
                } else {
                    v.as_object().map(|obj| obj.get("enabled").and_then(|e| e.as_bool()).unwrap_or(true))
                }
            })
            .unwrap_or(false);

        if e2ee_enabled {
            let e2ee = self.build_e2ee_extension(user_id, device_id, conn_id, since_pos).await?;
            response_extensions.insert("e2ee".to_string(), e2ee);
        }

        let presence_enabled = request_extensions
            .get("presence")
            .and_then(|v| {
                if v.as_bool() == Some(true) {
                    Some(true)
                } else {
                    v.as_object().map(|obj| obj.get("enabled").and_then(|e| e.as_bool()).unwrap_or(true))
                }
            })
            .unwrap_or(false);

        if presence_enabled {
            let room_ids: Vec<String> =
                rooms_response.as_object().map(|obj| obj.keys().cloned().collect()).unwrap_or_default();

            let mut all_members = std::collections::HashSet::new();
            all_members.insert(user_id.to_string());

            if let Ok(batch) = self.member_storage.get_members_batch(&room_ids, "join").await {
                for members in batch.values() {
                    for member in members {
                        all_members.insert(member.user_id.clone());
                    }
                }
            }

            let member_list: Vec<String> = all_members.into_iter().collect();
            let presences = self.presence_storage.get_presences(&member_list).await?;

            let mut presence_events = Vec::with_capacity(presences.len().min(32));
            for (uid, (presence, status_msg)) in presences {
                presence_events.push(serde_json::json!({
                    "sender": uid,
                    "type": "m.presence",
                    "content": {
                        "presence": presence,
                        "status_msg": status_msg,
                        "last_active_ago": 0, // Mocked for now
                    }
                }));
            }

            response_extensions.insert(
                "presence".to_string(),
                serde_json::json!({
                    "events": presence_events
                }),
            );
        }

        if response_extensions.is_empty() {
            Ok(None)
        } else {
            Ok(Some(serde_json::Value::Object(response_extensions)))
        }
    }

    async fn build_e2ee_extension(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        since_pos: Option<&str>,
    ) -> Result<Value, sqlx::Error> {
        let device_key_storage = DeviceKeyStorage::new(&self.event_storage.pool);
        let key_counts = device_key_storage
            .get_one_time_keys_count_by_algorithm(user_id, device_id)
            .await
            .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;

        let stream_cache_key = Self::e2ee_device_list_stream_cache_key(user_id, device_id, conn_id);
        let shared_users_cache_key = Self::e2ee_shared_users_cache_key(user_id, device_id, conn_id);
        let since_stream_id = if since_pos.is_some() {
            self.cache.get_raw(&stream_cache_key).and_then(|raw| raw.parse::<i64>().ok()).unwrap_or(0)
        } else {
            0
        };
        let current_stream_id = self.get_current_device_list_stream_id().await?;
        let changed = self.get_changed_device_lists_since(user_id, since_stream_id).await?;
        let previous_shared_users = if since_pos.is_some() {
            self.load_cached_shared_users(&shared_users_cache_key)
        } else {
            Vec::new()
        };
        let current_shared_users = self.get_current_shared_users(user_id).await?;
        let left = Self::compute_left_shared_users(&previous_shared_users, &current_shared_users);

        self.cache.set_raw(&stream_cache_key, &current_stream_id.to_string(), 3600).await;
        self.cache
            .set_raw(
                &shared_users_cache_key,
                &serde_json::to_string(&current_shared_users).unwrap_or_else(|_| "[]".to_string()),
                3600,
            )
            .await;

        let mut otk_counts = serde_json::Map::new();
        for (algo, count) in key_counts {
            otk_counts.insert(algo, json!(count));
        }

        let unused_fallback_types =
            device_key_storage.get_unused_fallback_key_types(user_id, device_id).await.unwrap_or_else(|_| vec![]);

        Ok(json!({
            "device_lists": {
                "changed": changed,
                "left": left,
            },
            "device_one_time_keys_count": otk_counts,
            "device_unused_fallback_key_types": unused_fallback_types,
        }))
    }

    async fn build_to_device_extension(
        &self,
        user_id: &str,
        device_id: &str,
        request_to_device: Option<&Value>,
    ) -> Result<Value, sqlx::Error> {
        let since_stream_id = request_to_device
            .and_then(|value| value.as_object())
            .and_then(|obj| obj.get("since"))
            .and_then(|value| value.as_str())
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(0);
        let limit = request_to_device
            .and_then(|value| value.as_object())
            .and_then(|obj| obj.get("limit"))
            .and_then(|value| value.as_i64())
            .filter(|value| *value > 0)
            .unwrap_or(100);

        let (events, next_batch) = self
            .get_to_device_extension_payload(user_id, device_id, since_stream_id, limit)
            .await
            .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;

        Ok(json!({
            "events": events,
            "next_batch": next_batch,
        }))
    }

    fn e2ee_device_list_stream_cache_key(user_id: &str, device_id: &str, conn_id: Option<&str>) -> String {
        match conn_id {
            Some(conn_id) => format!("sliding_sync:e2ee:{user_id}:{device_id}:{conn_id}"),
            None => format!("sliding_sync:e2ee:{user_id}:{device_id}:"),
        }
    }

    fn e2ee_shared_users_cache_key(user_id: &str, device_id: &str, conn_id: Option<&str>) -> String {
        match conn_id {
            Some(conn_id) => format!("sliding_sync:e2ee:shared_users:{user_id}:{device_id}:{conn_id}"),
            None => format!("sliding_sync:e2ee:shared_users:{user_id}:{device_id}:"),
        }
    }

    async fn get_current_device_list_stream_id(&self) -> Result<i64, sqlx::Error> {
        self.device_storage.get_max_device_list_stream_id().await
    }

    async fn get_changed_device_lists_since(&self, user_id: &str, since_stream_id: i64) -> Result<Vec<String>, sqlx::Error> {
        let (changed, _) =
            self.device_storage.get_device_lists_since_with_shared_rooms(since_stream_id, user_id).await?;
        Ok(changed)
    }

    async fn get_current_shared_users(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error> {
        let mut users = self.member_storage.get_shared_room_users(user_id).await?;
        users.sort();
        users.dedup();
        Ok(users)
    }

    fn load_cached_shared_users(&self, cache_key: &str) -> Vec<String> {
        self.cache
            .get_raw(cache_key)
            .and_then(|raw| serde_json::from_str::<Vec<String>>(&raw).ok())
            .map(|mut users| {
                users.sort();
                users.dedup();
                users
            })
            .unwrap_or_default()
    }

    fn compute_left_shared_users(previous: &[String], current: &[String]) -> Vec<String> {
        let previous: BTreeSet<&str> = previous.iter().map(String::as_str).collect();
        let current: BTreeSet<&str> = current.iter().map(String::as_str).collect();

        previous.difference(&current).map(|user_id| (*user_id).to_string()).collect()
    }

    async fn get_to_device_extension_payload(
        &self,
        user_id: &str,
        device_id: &str,
        since_stream_id: i64,
        limit: i64,
    ) -> Result<(Vec<Value>, Option<String>), ApiError> {
        let (events, last_stream_id) =
            self.to_device_storage.get_messages_since(user_id, device_id, since_stream_id, limit).await?;

        let next_batch = if events.is_empty() {
            Some(self.to_device_storage.get_current_stream_id(user_id, device_id).await?.to_string())
        } else {
            Some(last_stream_id.to_string())
        };

        Ok((events, next_batch))
    }
}
