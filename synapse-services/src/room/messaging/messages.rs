//! Room message operations: send, paginate, ephemeral events, typing indicators.

use crate::common::error::{ApiError, ApiResult};
use serde_json::json;
use synapse_common::current_timestamp_millis;
use synapse_common::{generate_event_id, generate_stream_token_from_ts, parse_stream_token};
use synapse_storage::CreateEventParams;

use super::service::MessagingService;

impl MessagingService {
    #[::tracing::instrument(skip(self, content))]
    pub async fn send_message(
        &self,
        room_id: &str,
        user_id: &str,
        event_type: &str,
        content: &serde_json::Value,
    ) -> ApiResult<serde_json::Value> {
        if !self
            .member_storage
            .is_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check membership", &e))?
        {
            return Err(ApiError::forbidden("You are not a member of this room".to_string()));
        }

        let event_id = generate_event_id(&self.server_name);
        let now = current_timestamp_millis();
        let max_ts = self.event_reader.get_max_origin_server_ts_for_room(room_id).await.unwrap_or(0);
        let now = now.max(max_ts + 1);

        #[allow(unused_variables)]
        let beacon_location_params = {
            #[cfg(feature = "beacons")]
            {
                if matches!(event_type, "m.beacon" | "org.matrix.msc3672.beacon" | "org.matrix.msc3489.beacon") {
                    let Some(beacon_service) = self.beacon_service.as_ref() else {
                        return Err(ApiError::internal("Beacon service not configured".to_string()));
                    };

                    let beacon_info_id = content
                        .get("m.relates_to")
                        .and_then(|v| v.get("event_id"))
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| ApiError::bad_request("Missing m.relates_to.event_id for m.beacon".to_string()))?
                        .to_string();

                    let location = content
                        .get("m.location")
                        .or_else(|| content.get("org.matrix.msc3488.location"))
                        .and_then(|v| v.as_object())
                        .ok_or_else(|| ApiError::bad_request("Missing m.location for m.beacon".to_string()))?;

                    let uri = location
                        .get("uri")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| ApiError::bad_request("Missing m.location.uri".to_string()))?
                        .to_string();

                    let description = location.get("description").and_then(|v| v.as_str()).map(|v| v.to_string());

                    let ts = content
                        .get("m.ts")
                        .or_else(|| content.get("org.matrix.msc3488.ts"))
                        .and_then(|v| v.as_i64())
                        .unwrap_or(now);

                    let accuracy = crate::beacon_service::BeaconService::parse_geo_uri(&uri)
                        .and_then(|(_, _, acc)| acc)
                        .map(|v| v.round() as i64);

                    let beacon_info = beacon_service
                        .get_beacon_info(room_id, &beacon_info_id)
                        .await
                        .map_err(|e| ApiError::internal_with_log("Failed to validate beacon", &e))?;
                    let Some(beacon_info) = beacon_info else {
                        return Err(ApiError::bad_request("Referenced beacon_info does not exist".to_string()));
                    };

                    if !beacon_info.is_live {
                        return Err(ApiError::bad_request("Referenced beacon_info is not live".to_string()));
                    }
                    if let Some(expires_at) = beacon_info.expires_at {
                        if expires_at <= now {
                            return Err(ApiError::bad_request("Referenced beacon_info has expired".to_string()));
                        }
                    }

                    if let Some(retry_after_ms) = beacon_service
                        .check_room_backpressure(room_id, now)
                        .await
                        .map_err(|e| ApiError::internal_with_log("Failed to check room backpressure", &e))?
                    {
                        return Err(ApiError::rate_limited_with_retry(retry_after_ms));
                    }

                    if let Some(retry_after_ms) = beacon_service
                        .check_location_quota(room_id, user_id, now)
                        .await
                        .map_err(|e| ApiError::internal_with_log("Failed to check beacon quota", &e))?
                    {
                        return Err(ApiError::rate_limited_with_retry(retry_after_ms));
                    }

                    let latest = beacon_service
                        .get_latest_location(&beacon_info_id)
                        .await
                        .map_err(|e| ApiError::internal_with_log("Failed to check beacon rate limit", &e))?;
                    if let Some(latest) = latest {
                        if ts <= latest.timestamp {
                            return Err(ApiError::bad_request(
                                "Beacon location timestamp must be increasing".to_string(),
                            ));
                        }
                        let delta = ts - latest.timestamp;
                        if delta < 1000 {
                            return Err(ApiError::rate_limited_with_retry((1000 - delta) as u64));
                        }
                    }

                    Some(synapse_storage::beacon::CreateBeaconLocationParams {
                        room_id: room_id.to_string(),
                        event_id: event_id.clone(),
                        beacon_info_id,
                        sender: user_id.to_string(),
                        uri,
                        description,
                        timestamp: ts,
                        accuracy,
                        created_ts: now,
                    })
                } else {
                    None
                }
            }
            #[cfg(not(feature = "beacons"))]
            {
                None::<()>
            }
        };

        self.create_event(
            CreateEventParams {
                event_id: event_id.clone(),
                room_id: room_id.to_string(),
                user_id: user_id.to_string(),
                event_type: event_type.to_string(),
                content: content.clone(),
                state_key: None,
                origin_server_ts: now,
                redacts: None,
            },
            None,
        )
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to send message", &e))?;

        if let Some(relates_to) = content.get("m.relates_to").or_else(|| content.get("relates_to")) {
            if let (Some(rel_type), Some(target_event_id)) = (
                relates_to.get("rel_type").and_then(|v| v.as_str()),
                relates_to.get("event_id").and_then(|v| v.as_str()),
            ) {
                if let Err(e) = self
                    .relations_storage
                    .create_relation(synapse_storage::relations::CreateRelationParams {
                        room_id: room_id.to_string(),
                        event_id: event_id.clone(),
                        relates_to_event_id: target_event_id.to_string(),
                        relation_type: rel_type.to_string(),
                        sender: user_id.to_string(),
                        origin_server_ts: now,
                        content: content.clone(),
                    })
                    .await
                {
                    ::tracing::warn!(
                        target: "relations",
                        event_id = %event_id,
                        error = %e,
                        "Failed to index event relation"
                    );
                }
            }
        }

        #[cfg(feature = "beacons")]
        if let (Some(beacon_service), Some(params)) = (self.beacon_service.as_ref(), beacon_location_params) {
            beacon_service
                .report_location(params)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to index beacon", &e))?;
        }

        Ok(json!({
            "event_id": event_id
        }))
    }

    pub async fn get_room_messages(
        &self,
        room_id: &str,
        user_id: &str,
        from: i64,
        limit: i64,
        direction: &str,
    ) -> ApiResult<serde_json::Value> {
        let is_member = self
            .member_storage
            .is_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check membership", &e))?;
        if !is_member {
            let room = self
                .room_storage
                .get_room(room_id)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to get room", &e))?;
            let is_public = room.as_ref().is_some_and(|r| r.is_public);
            if !is_public {
                return Err(ApiError::forbidden("You are not a member of this room".to_string()));
            }
        }

        let normalized_direction = if direction == "f" { "f" } else { "b" };

        let start_token = if from > 0 {
            generate_stream_token_from_ts(Some(from))
        } else {
            let max_ts = self
                .event_reader
                .get_max_origin_server_ts_for_room(room_id)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to get room stream", &e))?;
            generate_stream_token_from_ts(Some(max_ts))
        };

        let from_ts = if from > 0 { parse_stream_token(&start_token).or(Some(from)) } else { None };

        let events = self
            .event_reader
            .get_room_events_paginated(room_id, from_ts, limit, normalized_direction)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get messages", &e))?;

        let event_list: Vec<serde_json::Value> = events
            .iter()
            .map(|e| {
                json!({
                    "type": e.event_type,
                    "content": e.content,
                    "sender": e.user_id,
                    "origin_server_ts": e.origin_server_ts,
                    "event_id": e.event_id
                })
            })
            .collect();

        let end_token = events
            .last()
            .map_or_else(|| start_token.clone(), |event| generate_stream_token_from_ts(Some(event.origin_server_ts)));

        Ok(json!({
            "chunk": event_list,
            "start": start_token,
            "end": end_token
        }))
    }

    pub async fn get_ephemeral_events_for_client(
        &self,
        room_id: &str,
        limit: i64,
    ) -> ApiResult<Vec<serde_json::Value>> {
        let now = current_timestamp_millis();
        let rows = self
            .event_reader
            .get_ephemeral_events(room_id, now, limit)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get ephemeral events", &e))?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let event_id = format!("$ephemeral_{}", row.stream_id);
                json!({
                    "type": row.event_type,
                    "sender": row.user_id,
                    "content": row.content,
                    "origin_server_ts": row.created_ts,
                    "stream_id": row.stream_id,
                    "event_id": event_id,
                })
            })
            .collect())
    }

    pub async fn set_typing_ephemeral_event(
        &self,
        room_id: &str,
        user_id: &str,
        typing_user_ids: &[String],
        timeout_ms: i64,
    ) -> ApiResult<()> {
        let content = json!({
            "user_ids": typing_user_ids
        });
        let now = current_timestamp_millis();
        self.event_writer
            .upsert_ephemeral_event(room_id, user_id, "m.typing", &content, now, now, Some(now + timeout_ms))
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to store typing ephemeral event", &e))
    }

    pub async fn clear_typing_ephemeral_event(&self, room_id: &str, user_id: &str) -> ApiResult<()> {
        self.event_writer
            .delete_ephemeral_event(room_id, "m.typing", user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to clear typing ephemeral event", &e))
    }
}
