use super::keys::parse_stream_id;
use crate::routes::context::DeviceContext;
use crate::routes::extractors::MatrixPath;
use crate::routes::extractors::MatrixQuery;
use crate::routes::response_helpers::{empty_json, filter_users_with_shared_rooms};
use crate::routes::{AuthenticatedUser, MatrixJson};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{extract::State, Json};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::Deserialize;
use serde_json::{json, Value};
use synapse_common::ApiError;

/// See [`device_list_update`].
#[axum::debug_handler]
pub(crate) async fn device_list_update(
    State(ctx): State<DeviceContext>,
    auth_user: AuthenticatedUser,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    let requested_users = body
        .get("users")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ApiError::bad_request("Missing users array".to_string()))?
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect::<Vec<String>>();

    let users: Vec<String> = filter_users_with_shared_rooms(&ctx.room_service, &auth_user.user_id, &requested_users)
        .await
        .into_iter()
        .collect();

    let since = body.get("since").or_else(|| body.get("from")).and_then(|v| v.as_str()).and_then(parse_stream_id);

    if since.is_none() {
        let snapshot = ctx.account_device_list_service.get_device_list_snapshot(&users).await?;
        let changed: Vec<Value> = snapshot
            .changed
            .into_iter()
            .map(|device| {
                json!({
                    "user_id": device.user_id,
                    "device_id": device.device_id,
                    "device_data": {
                        "display_name": device.display_name,
                        "last_seen_ts": device.last_seen_ts,
                    }
                })
            })
            .collect();

        return Ok(Json(json!({
            "changed": changed,
            "left": snapshot.left
        })));
    }

    let since = since.unwrap_or(0);
    let to = body.get("to").and_then(|v| v.as_str()).and_then(parse_stream_id).unwrap_or(0);
    let delta = ctx
        .account_device_list_service
        .get_device_list_delta(since, if to > 0 { Some(to) } else { None }, &users)
        .await?;
    let changed: Vec<Value> = delta
        .changed
        .into_iter()
        .map(|device| {
            json!({
                "user_id": device.user_id,
                "device_id": device.device_id,
                "device_data": {
                    "display_name": device.display_name,
                    "last_seen_ts": device.last_seen_ts,
                }
            })
        })
        .collect();
    let deleted: Vec<Value> = delta
        .deleted
        .into_iter()
        .map(|device| {
            json!({
                "user_id": device.user_id,
                "device_id": device.device_id
            })
        })
        .collect();

    Ok(Json(json!({
        "changed": changed,
        "deleted": deleted,
        "left": delta.left,
        "stream_id": delta.stream_id
    })))
}

/// Returns a constant 403: room key distribution is server-internal bookkeeping, so a
/// client has no legitimate use for it. Registered as by-design unsupported in
/// `docs/synapse-rust/ROUTE_CONTRACT.md`（“16 条”表）and pinned on both sides by
/// `scripts/contract/check_unsupported_endpoints.py`（handler 恒失败 ⇔ `unsupported=true`）。
/// See [`room_key_distribution`].
#[allow(
    clippy::unused_async,
    reason = "by-design unsupported endpoint: the handler never awaits anything because it \
              always answers 403 (see ROUTE_CONTRACT.md's by-design table)"
)]
pub(crate) async fn room_key_distribution(
    State(_ctx): State<DeviceContext>,
    _auth_user: AuthenticatedUser,
    MatrixPath(_room_id): MatrixPath<String>,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    Err(synapse_common::error::ApiError::forbidden(
        "Room key distribution is a server-internal endpoint and is not available via the client API".to_string(),
    ))
}

/// See [`send_to_device`].
#[axum::debug_handler]
pub(crate) async fn send_to_device(
    State(ctx): State<DeviceContext>,
    auth_user: AuthenticatedUser,
    MatrixPath((event_type, transaction_id)): MatrixPath<(String, String)>,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, synapse_common::error::ApiError> {
    let sender_device_id =
        auth_user.device_id.as_deref().ok_or_else(|| ApiError::bad_request("Device ID required".to_string()))?;
    let messages = body
        .get("messages")
        .ok_or_else(|| synapse_common::error::ApiError::bad_request("Missing 'messages' field".to_string()))?;

    // Enforce to-device message limits to prevent oversized payloads from
    // blocking the to-device queue and federation transaction dispatch.
    // Inspired by Synapse v1.155 (#19617) which limits to-device EDU size.
    // Defaults live in synapse_common::config::ServerConfig.
    let max_recipients = ctx.config.server.to_device_max_recipients;
    let max_payload_bytes = ctx.config.server.to_device_max_payload_bytes;
    let mut recipient_count: usize = 0;
    if let Some(msg_obj) = messages.as_object() {
        for (_user_id, user_devices) in msg_obj {
            if let Some(devices) = user_devices.as_object() {
                recipient_count += devices.len();
                if recipient_count > max_recipients {
                    return Err(ApiError::bad_request(format!(
                        "Too many to-device recipients: {recipient_count} exceeds limit of {max_recipients}"
                    )));
                }
                for (_device_id, device_msg) in devices {
                    // Reject individual messages larger than the configured limit
                    // to protect downstream storage and federation queues.
                    if serde_json::to_string(device_msg).map(|s| s.len()).unwrap_or(0) > max_payload_bytes {
                        return Err(ApiError::bad_request(format!(
                            "Individual to-device message exceeds {max_payload_bytes} byte limit"
                        )));
                    }
                }
            }
        }
    }

    if event_type == "m.room_key" || event_type == "m.forwarded_room_key" {
        if let Some(msg_obj) = messages.as_object() {
            for (_user_id, user_devices) in msg_obj {
                if let Some(devices) = user_devices.as_object() {
                    for (_device_id, device_msg) in devices {
                        if let Some(session_key) = device_msg
                            .get("session_key")
                            .or_else(|| device_msg.get("content").and_then(|c| c.get("session_key")))
                        {
                            if session_key.as_str().is_none_or(|s| s.is_empty()) {
                                return Err(ApiError::bad_request(
                                    "Session key is empty in room key event. Session creation may have failed."
                                        .to_string(),
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    // B-8：to-device 反滥用限流。
    // 这里**只限流、不拒绝通道**：to-device 是 SAS 验证 / 房间密钥分发的必经通道，
    // 直接拒绝会打断正常端到端加密。但此前它完全没有任何速率约束 ——
    // 任意登录用户都能对任意用户任意设备无限投递（探测在线、灌垃圾、放大型骚扰）。
    enforce_to_device_rate_limit(&ctx, &auth_user.user_id, messages).await?;

    ctx.to_device_service
        .send_messages(&auth_user.user_id, sender_device_id, &event_type, Some(&transaction_id), messages)
        .await?;

    // Notify recipients so that long-polling /sync connections wake up
    // immediately instead of waiting for the next polling cycle.
    if let Some(msg_map) = body.get("messages").and_then(|m| m.as_object()) {
        for (user_id, _) in msg_map {
            ctx.event_notifier.notify_user(user_id);
        }
    }

    Ok(Json(json!({ "failures": {} })))
}

/// See [`upload_signatures`].
#[axum::debug_handler]
pub(crate) async fn upload_signatures(
    State(ctx): State<DeviceContext>,
    auth_user: AuthenticatedUser,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    let response = ctx.device_keys_service.upload_signatures(&auth_user.user_id, body).await?;

    Ok(Json(response))
}

/// Wire body of `POST /_matrix/client/{v1,v3}/keys/device_signing/upload`.
///
/// Declaring the names is what makes the body checkable (the sidecar reports
/// `body: typed` instead of `opaque`). Not `deny_unknown_fields`: unknown names
/// are ignored per the Matrix spec, so a client that adds a field keeps working.
/// The three key objects stay raw [`Value`]s — they are signed JSON forwarded
/// verbatim to the cross-signing service.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct DeviceSigningUploadBody {
    #[serde(default)]
    master_key: Option<Value>,
    #[serde(default)]
    self_signing_key: Option<Value>,
    #[serde(default)]
    user_signing_key: Option<Value>,
    /// UIA `auth` dict, passed to the cross-signing UIA gate verbatim.
    #[serde(default)]
    auth: Option<Value>,
}

impl DeviceSigningUploadBody {
    /// The three key objects, in upload order.
    fn signing_keys(&self) -> [Option<&Value>; 3] {
        [self.master_key.as_ref(), self.self_signing_key.as_ref(), self.user_signing_key.as_ref()]
    }

    /// Whether at least one of the three keys is a non-empty object.
    pub(crate) fn has_signing_keys(&self) -> bool {
        self.signing_keys().into_iter().flatten().any(|key| key.as_object().is_some_and(|key_obj| !key_obj.is_empty()))
    }
}

/// See [`upload_device_signing`].
#[axum::debug_handler]
pub(crate) async fn upload_device_signing(
    State(ctx): State<DeviceContext>,
    auth_user: AuthenticatedUser,
    MatrixJson(body): MatrixJson<DeviceSigningUploadBody>,
) -> Result<axum::response::Response, ApiError> {
    // UIA (User-Interactive Authentication) is required for cross-signing key upload
    // per Matrix spec: POST /_matrix/client/v3/keys/device_signing/upload requires UIA
    if let Err(uia_response) = ctx
        .account_identity_service
        .require_cross_signing_uia(
            &ctx.uia_service,
            body.auth.as_ref(),
            &auth_user.user_id,
            &ctx.token_auth,
            &ctx.credential_auth,
        )
        .await
    {
        return Ok((StatusCode::UNAUTHORIZED, Json(uia_response)).into_response());
    }

    // UIA passed, proceed with business logic
    let device_id =
        auth_user.device_id.as_ref().ok_or_else(|| ApiError::bad_request("Device ID required".to_string()))?;
    if !body.has_signing_keys() {
        return Err(ApiError::bad_request(
            "At least one of master_key, self_signing_key, or user_signing_key is required".to_string(),
        ));
    }

    for key in body.signing_keys().into_iter().flatten() {
        if key.as_object().is_some_and(|key_obj| !key_obj.is_empty()) {
            ctx.cross_signing_service.upload_device_signing_key(&auth_user.user_id, device_id, key).await?;
        }
    }

    // Wake long-polling sync/sliding-sync requests so the client sees the
    // updated cross-signing state immediately after a successful upload.
    ctx.event_notifier.notify_user(&auth_user.user_id);

    Ok(Json(json!({})).into_response())
}

/// See [`create_room_key_request`].
#[axum::debug_handler]
pub(crate) async fn create_room_key_request(
    State(ctx): State<DeviceContext>,
    auth_user: AuthenticatedUser,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    let device_id =
        auth_user.device_id.as_deref().ok_or_else(|| ApiError::bad_request("Device ID required".to_string()))?;
    let body: CreateRoomKeyRequestBody =
        serde_json::from_value(body).map_err(|e| ApiError::bad_request(format!("Invalid room key request: {e}")))?;

    let request = ctx
        .key_request_service
        .create_request(
            &auth_user.user_id,
            device_id,
            &body.room_id,
            &body.session_id,
            &body.algorithm,
            body.request_type.as_deref(),
            body.request_id.as_deref(),
        )
        .await?;

    Ok(Json(serde_json::json!({
        "request_id": request.request_id
    })))
}

/// See [`encode_key_request_cursor`].
pub(crate) fn encode_key_request_cursor(ts: i64, id: &str) -> String {
    BASE64.encode(format!("{}:{}", ts, id))
}

/// See [`decode_key_request_cursor`].
pub(crate) fn decode_key_request_cursor(cursor: &str) -> Option<(i64, String)> {
    let decoded = BASE64.decode(cursor).ok()?;
    let s = String::from_utf8(decoded).ok()?;
    let mut parts = s.splitn(2, ':');
    let ts = parts.next()?.parse().ok()?;
    let id = parts.next()?.to_string();
    Some((ts, id))
}

/// See [`get_room_key_requests`].
pub(crate) async fn get_room_key_requests(
    State(ctx): State<DeviceContext>,
    auth_user: AuthenticatedUser,
    MatrixQuery(params): MatrixQuery<GetRoomKeyRequestsQuery>,
) -> Result<Json<Value>, ApiError> {
    let limit = params.limit.unwrap_or(100).clamp(1, 1000);
    let cursor = params.from.as_deref().and_then(decode_key_request_cursor);

    let requests = ctx
        .key_request_service
        .get_requests_paginated(synapse_e2ee::key_request::KeyRequestPagination {
            user_id: &auth_user.user_id,
            limit: limit as i64,
            from_ts: cursor.as_ref().map(|c| c.0),
            from_id: cursor.as_ref().map(|c| c.1.as_str()),
            status: params.status.as_deref(),
            room_id: params.room_id.as_deref(),
            session_id: params.session_id.as_deref(),
        })
        .await?;

    let next_batch = if requests.len() == limit {
        requests.last().map(|r| encode_key_request_cursor(r.created_ts, &r.request_id))
    } else {
        None
    };

    Ok(Json(serde_json::json!({
        "requests": requests
            .into_iter()
            .map(serialize_room_key_request)
            .collect::<Vec<_>>(),
        "next_batch": next_batch
    })))
}

/// See [`delete_room_key_request`].
#[axum::debug_handler]
pub(crate) async fn delete_room_key_request(
    State(ctx): State<DeviceContext>,
    auth_user: AuthenticatedUser,
    MatrixPath(request_id): MatrixPath<String>,
) -> Result<Json<Value>, ApiError> {
    let existing = ctx.key_request_service.get_request(&request_id).await?;

    let request = existing.ok_or_else(|| ApiError::not_found("Room key request not found".to_string()))?;

    if request.user_id != auth_user.user_id {
        return Err(ApiError::forbidden("Cannot delete another user's room key request".to_string()));
    }

    ctx.key_request_service.cancel_request(&request_id).await?;

    Ok(empty_json())
}

#[derive(Debug, Deserialize)]
struct CreateRoomKeyRequestBody {
    algorithm: String,
    room_id: String,
    session_id: String,
    request_type: Option<String>,
    request_id: Option<String>,
}

/// The `GetRoomKeyRequestsQuery` struct.
#[derive(Debug, Deserialize, Default)]
pub(crate) struct GetRoomKeyRequestsQuery {
    status: Option<String>,
    room_id: Option<String>,
    session_id: Option<String>,
    limit: Option<usize>,
    from: Option<String>,
}

/// See [`serialize_room_key_request`].
pub(crate) fn serialize_room_key_request(request: synapse_e2ee::key_request::KeyRequestInfo) -> Value {
    let action = request.action;
    let status = if action == "cancellation" || action == "cancelled" {
        "cancelled"
    } else if request.is_fulfilled {
        "fulfilled"
    } else {
        "pending"
    };

    serde_json::json!({
        "request_id": request.request_id,
        "user_id": request.user_id,
        "device_id": request.device_id,
        "room_id": request.room_id,
        "session_id": request.session_id,
        "algorithm": request.algorithm,
        "action": &action,
        "request_type": action,
        "status": status,
        "created_ts": request.created_ts,
        "is_fulfilled": request.is_fulfilled,
        "fulfilled_by_device": request.fulfilled_by_device,
        "fulfilled_ts": request.fulfilled_ts,
    })
}

// ── B-8：to-device 反滥用限流 ────────────────────────────────────────────────

/// 与接收方**共享房间**时的速率：每个 (发送方, 接收方) 每秒 2 个、突发 40。
///
/// 共享房间意味着双方本来就能互发房间事件，to-device 只是加密协商的辅助通道，
/// 正常验证流程只有个位数消息，40 的突发已经非常宽松。
const TO_DEVICE_TRUSTED_RATE_PER_SEC: u32 = 2;
/// 共享房间时的突发容量。
const TO_DEVICE_TRUSTED_BURST: u32 = 40;

/// **无**共享房间时的速率：每秒 1 个、突发 5。
///
/// 这里刻意压得很紧：没有共享房间的 to-device 只有两种合理来源 ——
/// 好友关系的验证流程，或刚被邀请进房前的密钥协商。两者都不需要高频。
/// 攻击者用它探测在线状态 / 灌垃圾时，这个上限会把成本抬起来。
const TO_DEVICE_UNTRUSTED_RATE_PER_SEC: u32 = 1;
/// 无共享房间时的突发容量。
const TO_DEVICE_UNTRUSTED_BURST: u32 = 5;

/// 每个发送方一枚总桶：防止「换着 recipient 发」绕过上面的单对限流。
const TO_DEVICE_SENDER_RATE_PER_SEC: u32 = 5;
/// 单个发送方的突发容量。
const TO_DEVICE_SENDER_BURST: u32 = 100;

/// 取一个令牌；`Ok(false)` 表示桶已空。
async fn take_to_device_token(
    ctx: &DeviceContext,
    key: &str,
    rate_per_second: u32,
    burst: u32,
) -> Result<bool, ApiError> {
    match ctx.cache.rate_limit_token_bucket_take(key, rate_per_second, burst).await {
        Ok(decision) => Ok(decision.allowed),
        Err(e) => {
            // 限流后端故障时**放行**，但必须留痕：to-device 承载 E2EE 协商，
            // 这里 fail-closed 会让「缓存挂掉」升级成「加密彻底不可用」。
            tracing::warn!(
                target: "security_audit",
                event = "to_device_rate_limit_unavailable",
                error = %e,
                "to-device rate limiter unavailable, allowing the batch"
            );
            Ok(true)
        }
    }
}

/// 对一批 to-device 消息做限流。
///
/// 计费单位是 **(发送方, 接收用户)**，不是设备：给同一用户的 5 台设备群发算一次。
async fn enforce_to_device_rate_limit(
    ctx: &DeviceContext,
    sender_user_id: &str,
    messages: &Value,
) -> Result<(), ApiError> {
    let Some(msg_obj) = messages.as_object() else {
        return Ok(());
    };
    let recipients: Vec<String> = msg_obj.keys().cloned().collect();
    if recipients.is_empty() {
        return Ok(());
    }

    let prefix = ctx.config.redis.key_prefix.as_str();

    // 1) 每发送方的总桶：挡住「换 recipient」的扇出攻击
    let sender_key = format!("{prefix}to_device_rl:sender:{sender_user_id}");
    if !take_to_device_token(ctx, &sender_key, TO_DEVICE_SENDER_RATE_PER_SEC, TO_DEVICE_SENDER_BURST).await? {
        tracing::warn!(
            target: "security_audit",
            event = "to_device_sender_rate_limited",
            sender_user_id = %sender_user_id,
            recipient_count = recipients.len(),
            "to-device batch rejected by the per-sender rate limit"
        );
        return Err(ApiError::rate_limited("Too many to-device messages".to_string()));
    }

    // 2) 关系判定：共享房间 = 可信；其余按低频处理。
    //    `filter_users_with_shared_rooms` 是 fail-closed 的（查询失败只保留自己），
    //    所以后端抖动时会把所有人降级到低频档 —— 是收紧而不是放开。
    let shared = filter_users_with_shared_rooms(&ctx.room_service, sender_user_id, &recipients).await;

    for recipient in &recipients {
        let trusted = shared.contains(recipient);
        let (rate, burst) = if trusted {
            (TO_DEVICE_TRUSTED_RATE_PER_SEC, TO_DEVICE_TRUSTED_BURST)
        } else {
            (TO_DEVICE_UNTRUSTED_RATE_PER_SEC, TO_DEVICE_UNTRUSTED_BURST)
        };

        let key = format!("{prefix}to_device_rl:{sender_user_id}:{recipient}");
        if !take_to_device_token(ctx, &key, rate, burst).await? {
            // 只有低频档被限才记审计：共享房间之间的正常协商不值得每次刷审计日志。
            if !trusted {
                tracing::warn!(
                    target: "security_audit",
                    event = "to_device_untrusted_recipient_rate_limited",
                    sender_user_id = %sender_user_id,
                    recipient_user_id = %recipient,
                    "to-device to a non-shared-room user rejected by rate limit"
                );
            }
            return Err(ApiError::rate_limited("Too many to-device messages".to_string()));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_request_cursor_round_trip() {
        let ts: i64 = 12345;
        let id = "test_token";
        let encoded = encode_key_request_cursor(ts, id);
        let decoded = decode_key_request_cursor(&encoded);
        assert_eq!(decoded, Some((ts, id.to_string())));
    }

    #[test]
    fn test_key_request_cursor_empty_string() {
        let decoded = decode_key_request_cursor("");
        assert_eq!(decoded, None);
    }

    #[test]
    fn test_key_request_cursor_invalid_base64() {
        let decoded = decode_key_request_cursor("!!!not-valid-base64!!!");
        assert_eq!(decoded, None);
    }

    #[test]
    fn test_key_request_cursor_with_special_chars() {
        let ts: i64 = 12345;
        let id = "a/b+c=";
        let encoded = encode_key_request_cursor(ts, id);
        let decoded = decode_key_request_cursor(&encoded);
        assert_eq!(decoded, Some((ts, id.to_string())));
    }

    #[test]
    fn test_key_request_cursor_with_newline() {
        let ts: i64 = 12345;
        let id = "test\ntoken";
        let encoded = encode_key_request_cursor(ts, id);
        let decoded = decode_key_request_cursor(&encoded);
        assert_eq!(decoded, Some((ts, id.to_string())));
    }
}
