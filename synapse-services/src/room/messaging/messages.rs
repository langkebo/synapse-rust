//! Room message operations: send, paginate, ephemeral events, typing indicators.

use crate::common::error::{ApiError, ApiResult};
use serde_json::json;
use synapse_common::current_timestamp_millis;
use synapse_common::{generate_event_id, generate_pagination_token};
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
            .map_err(|e| ApiError::internal_with_context("Failed to check membership", &e))?
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
                        .map_err(|e| ApiError::internal_with_context("Failed to validate beacon", &e))?;
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
                        .map_err(|e| ApiError::internal_with_context("Failed to check room backpressure", &e))?
                    {
                        return Err(ApiError::rate_limited_with_retry(retry_after_ms));
                    }

                    if let Some(retry_after_ms) = beacon_service
                        .check_location_quota(room_id, user_id, now)
                        .await
                        .map_err(|e| ApiError::internal_with_context("Failed to check beacon quota", &e))?
                    {
                        return Err(ApiError::rate_limited_with_retry(retry_after_ms));
                    }

                    let latest = beacon_service
                        .get_latest_location(&beacon_info_id)
                        .await
                        .map_err(|e| ApiError::internal_with_context("Failed to check beacon rate limit", &e))?;
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

        // DB-03-a: write the event and its relation (if any) in a single
        // transaction. Pre-fix, the two `create_*` calls were auto-committed
        // independently, so a relation write failure left an orphan event
        // (only `tracing::warn`, no rollback). Now: both writes share a
        // transaction so they either both succeed or both roll back.
        //
        // Trade-off: a relation-write failure (e.g. FK violation against a
        // missing target event_id) now makes the entire message send fail
        // rather than silently persisting an orphan event. The previous
        // behavior masked structural problems as warnings.
        let mut tx = self
            .event_writer
            .pool()
            .begin()
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to begin send_message transaction", &e))?;

        let event = self
            .create_event(
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
                Some(&mut tx),
            )
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to send message", &e))?;
        // create_event failed: `tx` drops here → sqlx auto-rollback → pool returns clean.

        if let Some(relates_to) = content.get("m.relates_to").or_else(|| content.get("relates_to")) {
            if let (Some(rel_type), Some(target_event_id)) = (
                relates_to.get("rel_type").and_then(|v| v.as_str()),
                relates_to.get("event_id").and_then(|v| v.as_str()),
            ) {
                if let Err(e) = self
                    .relations_storage
                    .create_relation_in_tx(
                        synapse_storage::relations::CreateRelationParams {
                            room_id: room_id.to_string(),
                            event_id: event_id.clone(),
                            relates_to_event_id: target_event_id.to_string(),
                            relation_type: rel_type.to_string(),
                            sender: user_id.to_string(),
                            origin_server_ts: now,
                            content: content.clone(),
                        },
                        &mut tx,
                    )
                    .await
                {
                    // Log before tx drops (which auto-rolls-back the event).
                    ::tracing::error!(
                        target: "relations",
                        event_id = %event_id,
                        target_event_id = %target_event_id,
                        error = %e,
                        "Relation write failed; event will be rolled back"
                    );
                    return Err(ApiError::internal_with_context(
                        "Failed to send message: relation index write failed",
                        &e,
                    ));
                }
            }
        }

        tx.commit()
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to commit send_message transaction", &e))?;

        // Post-commit fan-out. `create_event` skipped these when called with a
        // transaction (`should_update_summary = tx.is_none()`), but send_message
        // owns the event lifecycle here: room summary refresh, appservice event
        // dispatch and federation broadcast must run *after* the commit so a
        // rollback cannot leak a dispatched event. All three are best-effort.
        if let Err(error) =
            self.room_summary_service.queue_update(room_id, &event.event_id, &event.event_type, None).await
        {
            ::tracing::warn!(error = %error, room_id = %room_id, "Failed to queue room summary update");
        } else if let Err(error) = self.room_summary_service.process_pending_updates(32).await {
            ::tracing::warn!(error = %error, room_id = %room_id, batch_size = 32_u64, "Failed to process room summary updates");
        }

        self.dispatch_appservice_event(
            &event.event_id,
            &event.room_id,
            &event.event_type,
            &event.user_id,
            &event.content,
            None,
        )
        .await;

        if let Err(e) = self.sign_and_broadcast_event(&event).await {
            ::tracing::warn!(
                event_id = %event.event_id,
                room_id = %event.room_id,
                event_type = %event.event_type,
                error = %e,
                "Failed to sign and broadcast event"
            );
        }

        #[cfg(feature = "beacons")]
        if let (Some(beacon_service), Some(params)) = (self.beacon_service.as_ref(), beacon_location_params) {
            beacon_service
                .report_location(params)
                .await
                .map_err(|e| ApiError::internal_with_context("Failed to index beacon", &e))?;
        }

        Ok(json!({
            "event_id": event_id
        }))
    }

    /// ISSUE-03: 带持久化 txn 去重的发送入口。
    ///
    /// 去重语义：`room_event_txn_dedup` 表的 PRIMARY KEY (user_id, room_id,
    /// txn_id) 是唯一事实源，路由层的 1h TTL 缓存只是快路径。缓存丢失、
    /// 过期或并发双 PUT 时，重试仍返回同一 `event_id`，房间内不产生重复事件。
    ///
    /// 竞态处理：先查后建存在窗口，两个并发相同 txn 的请求可能各自创建事件；
    /// `record_event_txn` 的 ON CONFLICT 保证只有一个获胜，落败方删除自己
    /// 刚创建的重复事件并返回获胜方的 event_id。
    pub async fn send_message_with_txn(
        &self,
        room_id: &str,
        user_id: &str,
        event_type: &str,
        content: &serde_json::Value,
        txn_id: &str,
    ) -> ApiResult<serde_json::Value> {
        if txn_id.is_empty() {
            return self.send_message(room_id, user_id, event_type, content).await;
        }

        if let Some(existing) = self
            .event_reader
            .get_event_id_by_txn(user_id, room_id, txn_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to look up txn dedup record", &e))?
        {
            return Ok(json!({ "event_id": existing }));
        }

        let result = self.send_message(room_id, user_id, event_type, content).await?;
        let event_id = result.get("event_id").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        if event_id.is_empty() {
            return Ok(result);
        }

        let inserted = self
            .event_writer
            .record_event_txn(user_id, room_id, txn_id, &event_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to record txn dedup marker", &e))?;

        if !inserted {
            // 并发相同 txn：本地事件落败，返回获胜方的 event_id
            if let Some(winner) = self
                .event_reader
                .get_event_id_by_txn(user_id, room_id, txn_id)
                .await
                .map_err(|e| ApiError::internal_with_context("Failed to resolve txn race winner", &e))?
            {
                if winner != event_id {
                    ::tracing::warn!(
                        room_id = %room_id,
                        user_id = %user_id,
                        txn_id = %txn_id,
                        loser_event_id = %event_id,
                        winner_event_id = %winner,
                        "Concurrent duplicate txn detected; dropping losing event"
                    );
                    if let Err(e) = self.event_writer.delete_event_by_id(&event_id).await {
                        ::tracing::warn!(
                            event_id = %event_id,
                            error = %e,
                            "Failed to delete losing duplicate event after txn race"
                        );
                    }
                    return Ok(json!({ "event_id": winner }));
                }
            }
        }

        Ok(result)
    }

    pub async fn get_room_messages(
        &self,
        room_id: &str,
        user_id: &str,
        from: Option<(i64, Option<i64>)>,
        limit: i64,
        direction: &str,
    ) -> ApiResult<serde_json::Value> {
        let is_member = self
            .member_storage
            .is_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to check membership", &e))?;
        if !is_member {
            let room = self
                .room_storage
                .get_room(room_id)
                .await
                .map_err(|e| ApiError::internal_with_context("Failed to get room", &e))?;
            let is_public = room.as_ref().is_some_and(|r| r.is_public);
            if !is_public {
                return Err(ApiError::forbidden("You are not a member of this room".to_string()));
            }
        }

        let normalized_direction = if direction == "f" { "f" } else { "b" };

        // ISSUE-06：start token 直接回显客户端传来的游标（复合形式优先）；
        // 无游标时用房间最新事件时间戳作为起点。
        let start_token = match from {
            Some((ts, stream)) => generate_pagination_token(ts, stream),
            None => {
                let max_ts = self
                    .event_reader
                    .get_max_origin_server_ts_for_room(room_id)
                    .await
                    .map_err(|e| ApiError::internal_with_context("Failed to get room stream", &e))?;
                generate_pagination_token(max_ts, None)
            }
        };

        let events = self
            .event_reader
            .get_room_events_paginated_cursor(room_id, from, limit, normalized_direction)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to get messages", &e))?;

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

        // 页尾事件带出 stream_ordering，生成复合游标，同毫秒事件不再丢失（ISSUE-06）
        let end_token = events.last().map_or_else(
            || start_token.clone(),
            |event| generate_pagination_token(event.origin_server_ts, event.stream_ordering),
        );

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
            .map_err(|e| ApiError::internal_with_context("Failed to get ephemeral events", &e))?;

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
            .map_err(|e| ApiError::internal_with_context("Failed to store typing ephemeral event", &e))
    }

    pub async fn clear_typing_ephemeral_event(&self, room_id: &str, user_id: &str) -> ApiResult<()> {
        self.event_writer
            .delete_ephemeral_event(room_id, "m.typing", user_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to clear typing ephemeral event", &e))
    }
}
