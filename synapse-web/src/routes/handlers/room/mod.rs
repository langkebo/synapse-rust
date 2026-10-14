/// The `e2ee` module.
pub(crate) mod e2ee;
/// The `events` module.
pub(crate) mod events;
/// The `management` module.
pub(crate) mod management;
/// The `members` module.
pub(crate) mod members;
/// The `receipts` module.
pub(crate) mod receipts;
/// The `state` module.
pub mod state;

pub(crate) use e2ee::*;
pub(crate) use events::*;
pub(crate) use management::*;
pub(crate) use members::*;
pub(crate) use receipts::*;
pub(crate) use state::*;

use crate::routes::context::RoomContext;
use crate::routes::{ensure_room_member_ctx, ensure_room_member_strict_ctx, AuthenticatedUser};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use synapse_common::{parse_pagination_token, parse_stream_token, ApiError};
use synapse_services::delayed_event_service::CreateDelayedEventRequest;

/// 解析后的 /messages 游标：`(origin_server_ts, stream_ordering)`。
type MessagesCursor = (i64, Option<i64>);

/// 解析 /messages 的 `from` 游标（ISSUE-06 / A10）。
///
/// 支持三种形式：复合 `t{ts}_{stream}`、legacy `t{ts}`、裸整数时间戳。
/// 无 `from` 参数（或空串）时返回 `Ok(None)`（从最新/最旧一页开始）；
/// 参数存在但无法解析为上述任一形式时返回 `M_INVALID_PARAM`，不再把非法游标
/// 静默当作「无游标」处理（否则会悄悄从房间最新一页重新开始）。
fn parse_room_messages_from_token(params: &serde_json::Value) -> Result<Option<MessagesCursor>, ApiError> {
    let Some(raw) = params.get("from") else {
        return Ok(None);
    };
    let Some(token) = raw.as_str() else {
        return Err(ApiError::invalid_param("The 'from' parameter must be a string pagination token"));
    };
    if token.is_empty() {
        return Ok(None);
    }
    parse_pagination_token(token)
        .or_else(|| parse_stream_token(token).map(|ts| (ts, None)))
        .or_else(|| token.parse().ok().map(|ts| (ts, None)))
        .map(Some)
        .ok_or_else(|| ApiError::invalid_param(format!("Invalid 'from' pagination token: {token}")))
}

/// 解析 /messages 的 `dir` 参数（A10）。
///
/// 仅接受 `f`（forward）与 `b`（backward）；缺省时为 `b`。任何其他取值都返回
/// `M_INVALID_PARAM`，避免非法方向被静默回退成 `b` 而返回相反顺序的结果。
fn parse_pagination_direction(params: &serde_json::Value) -> Result<&'static str, ApiError> {
    let Some(value) = params.get("dir") else {
        return Ok("b");
    };
    match value.as_str() {
        Some("f") => Ok("f"),
        Some("b") => Ok("b"),
        _ => Err(ApiError::invalid_param("The 'dir' parameter must be 'f' or 'b'")),
    }
}

/// See [`ensure_room_view_access`].
pub(crate) async fn ensure_room_view_access(
    ctx: &RoomContext,
    auth_user: &AuthenticatedUser,
    room_id: &str,
) -> Result<(), ApiError> {
    ensure_room_member_strict_ctx(ctx, auth_user, room_id, "You must be a member of this room to view events").await?;

    Ok(())
}

/// See [`normalize_room_event_type`].
pub(crate) fn normalize_room_event_type(event_type: &str) -> String {
    if event_type.starts_with("m.room.") || event_type.starts_with("m.") {
        event_type.to_string()
    } else {
        format!("m.room.{event_type}")
    }
}

/// See [`state_event_content_response`].
pub(crate) fn state_event_content_response(content: &serde_json::Value) -> serde_json::Value {
    content.clone()
}

/// See [`ensure_room_state_write_access`].
pub(crate) async fn ensure_room_state_write_access(
    ctx: &RoomContext,
    auth_user: &AuthenticatedUser,
    room_id: &str,
    event_type: &str,
    content: &serde_json::Value,
) -> Result<(), ApiError> {
    ensure_room_member_ctx(ctx, auth_user, room_id, "You must be a member of this room to send state events").await?;

    ctx.room_auth.verify_state_event_write(room_id, &auth_user.user_id, event_type).await?;

    // A power-levels event must also satisfy the power-levels *change* rules
    // (cannot elevate above yourself, cannot touch an equal-or-higher level, and
    // — room v12+, MSC4289 rule 10.4 — must not name a creator). Those rules need
    // the new content, which is why this is not folded into
    // `verify_state_event_write`. The canonical client path for a power-levels
    // update is `PUT /state/m.room.power_levels`, so the check has to live here
    // and not only on the `/send` path.
    if event_type == "m.room.power_levels" {
        ctx.room_auth.verify_power_levels_change(room_id, &auth_user.user_id, content).await?;
    }

    Ok(())
}

/// MSC4140's delay hint parameter name, read from the request **query string**.
///
/// The specification — and upstream Synapse
/// (`_parse_request_for_delayed_event_delay` → `parse_integer(request,
/// "org.matrix.msc4140.delay", negative=True)`) — both put it in the query
/// string. This server originally read a same-named **body** field only, which
/// made a spec-conformant client's `?org.matrix.msc4140.delay=30000` silently
/// meaningless: the event went out immediately while the client still believed it
/// could be cancelled.
pub(crate) const MSC4140_DELAY_QUERY_PARAM: &str = "org.matrix.msc4140.delay";

/// Parse the delay hint out of the query string; `None` when it is absent.
///
/// Deliberately stricter than the legacy body field: upstream answers a
/// non-integer query value with a 400 `M_INVALID_PARAM` rather than ignoring it,
/// so a typo cannot silently degrade a delayed send into an immediate one
/// (`invalid_input` carries the same errcode as upstream's `parse_integer`).
/// Negatives pass through here so that the caller's `<= 0` check owns the single
/// "must be positive" error — the same split upstream uses, which keeps one
/// message for `0`, `-1` and `-99999` instead of two rejections for one mistake.
///
/// `trim` before parsing mirrors Python's `int(str)`, which tolerates
/// surrounding whitespace.
pub(crate) fn msc4140_query_delay_ms(query: &HashMap<String, String>) -> Result<Option<i64>, ApiError> {
    let Some(raw) = query.get(MSC4140_DELAY_QUERY_PARAM) else {
        return Ok(None);
    };
    raw.trim()
        .parse::<i64>()
        .map(Some)
        .map_err(|_| ApiError::invalid_input(format!("Query parameter {MSC4140_DELAY_QUERY_PARAM} must be an integer")))
}

/// MSC4140: if the request carries an `org.matrix.msc4140.delay` hint, store the
/// event as a pending delayed event and return the client-facing `delay_id`;
/// otherwise return `None` so the caller sends the event immediately.
///
/// The hint is read from the **query string** first (the spec/upstream location,
/// see [`MSC4140_DELAY_QUERY_PARAM`]); the request body field of the same name is
/// still accepted as a fallback, because that is the only form this server
/// understood before and dropping it would silently re-time existing clients'
/// requests. The query string wins when both are present.
///
/// Shared by the message (`/send`) and state (`/state`) write paths so that a
/// delayed **state** event carries its real `state_key` into `delayed_events` and
/// is later replayed by the dispatcher through the state-event write path
/// (preserving state semantics) instead of the message path.
///
/// `txn_id` is the message path's idempotency key (used to cache the returned
/// `delay_id`); state endpoints have no transaction id and pass `None`.
#[allow(clippy::too_many_arguments)] // 写路径透传参数多，语义直白
pub(crate) async fn schedule_delayed_event_if_requested(
    ctx: &RoomContext,
    user_id: &str,
    device_id: &str,
    room_id: &str,
    event_type: &str,
    state_key: Option<String>,
    body: &serde_json::Value,
    query: &HashMap<String, String>,
    txn_id: Option<&str>,
) -> Result<Option<axum::extract::Json<serde_json::Value>>, ApiError> {
    let query_delay_ms = msc4140_query_delay_ms(query)?;
    // Legacy body spelling, kept lenient (`as_i64()` ⇒ a non-integer means "no
    // delay") — exactly the behaviour that existed before the query form was
    // recognised. Only the spec location is strict.
    let body_delay_ms = body.get(MSC4140_DELAY_QUERY_PARAM).and_then(|v| v.as_i64());
    let Some(delay_ms) = query_delay_ms.or(body_delay_ms) else {
        return Ok(None);
    };

    // Validate delay is positive and bounded (max 24h = 86_400_000ms).
    if delay_ms <= 0 {
        return Err(ApiError::bad_request(
            "org.matrix.msc4140.delay must be a positive integer (milliseconds)".to_string(),
        ));
    }
    if delay_ms > 86_400_000 {
        return Err(ApiError::bad_request(
            "org.matrix.msc4140.delay must not exceed 24 hours (86_400_000ms)".to_string(),
        ));
    }

    // Remove the MSC4140 delay field from the content before storing — it is a
    // transport-level scheduling hint, not part of the event content. (A hint
    // sent as a query parameter is not in the content to begin with; the removal
    // only matters for the legacy body spelling.)
    let mut content = body.clone();
    if let Some(obj) = content.as_object_mut() {
        obj.remove(MSC4140_DELAY_QUERY_PARAM);
    }

    let request = CreateDelayedEventRequest {
        room_id: room_id.to_string(),
        user_id: user_id.to_string(),
        device_id: device_id.to_string(),
        event_type: event_type.to_string(),
        state_key,
        content,
        delay_ms,
    };

    let delayed = ctx.delayed_event_service.schedule(request).await?;

    ::tracing::info!(
        room_id = %room_id,
        user_id = %user_id,
        delay_id = delayed.id,
        delay_ms,
        event_type = %event_type,
        "MSC4140 delayed event scheduled"
    );

    // Cache the delay_id against the txn_id for idempotency (message path only).
    if let Some(txn_id) = txn_id.filter(|t| !t.is_empty()) {
        let cache_key = format!("txn:{}:{}:{}", user_id, room_id, txn_id);
        let cached = serde_json::json!({ "delay_id": delayed.id });
        if let Err(e) = ctx.cache.set(&cache_key, &cached.to_string(), 3600).await {
            ::tracing::warn!("Failed to cache delayed event txn_id dedup marker: {e}");
        }
    }

    Ok(Some(axum::extract::Json(serde_json::json!({ "delay_id": delayed.id }))))
}

/// See [`get_room_event`].
pub(crate) async fn get_room_event(
    ctx: &RoomContext,
    room_id: &str,
    event_id: &str,
) -> Result<serde_json::Value, ApiError> {
    ctx.room_service.messaging().get_event(room_id, event_id).await
}

/// The `UpgradeRoomRequest` struct.
#[derive(Debug, Deserialize)]
pub(crate) struct UpgradeRoomRequest {
    pub(crate) new_version: String,
}

/// The `UpgradeRoomResponse` struct.
#[derive(Debug, Serialize)]
pub(crate) struct UpgradeRoomResponse {
    pub(crate) replacement_room: String,
}

#[cfg(test)]
mod tests {
    use super::{parse_pagination_direction, parse_room_messages_from_token, state_event_content_response};
    use serde_json::json;

    // A10: `/messages` must not silently fall back when `dir`/`from` are invalid.

    #[test]
    fn test_parse_pagination_direction_defaults_to_backward() {
        let params = json!({});
        assert_eq!(parse_pagination_direction(&params).unwrap(), "b");
    }

    #[test]
    fn test_parse_pagination_direction_accepts_f_and_b() {
        assert_eq!(parse_pagination_direction(&json!({ "dir": "f" })).unwrap(), "f");
        assert_eq!(parse_pagination_direction(&json!({ "dir": "b" })).unwrap(), "b");
    }

    #[test]
    fn test_parse_pagination_direction_rejects_invalid_values() {
        for invalid in [
            json!({ "dir": "sideways" }),
            json!({ "dir": "" }),
            json!({ "dir": "F" }),
            json!({ "dir": 1 }),
            json!({ "dir": null }),
        ] {
            let err = parse_pagination_direction(&invalid).unwrap_err();
            assert!(err.is_bad_request(), "expected M_INVALID_PARAM for {invalid}");
        }
    }

    #[test]
    fn test_parse_room_messages_from_token_absent_or_empty_is_none() {
        assert!(parse_room_messages_from_token(&json!({})).unwrap().is_none());
        assert!(parse_room_messages_from_token(&json!({ "from": "" })).unwrap().is_none());
    }

    #[test]
    fn test_parse_room_messages_from_token_accepts_bare_timestamp() {
        let cursor = parse_room_messages_from_token(&json!({ "from": "12345" })).unwrap();
        assert_eq!(cursor, Some((12345, None)));
    }

    #[test]
    fn test_parse_room_messages_from_token_rejects_invalid_values() {
        for invalid in [json!({ "from": "not-a-token" }), json!({ "from": 42 }), json!({ "from": null })] {
            let err = parse_room_messages_from_token(&invalid).unwrap_err();
            assert!(err.is_bad_request(), "expected M_INVALID_PARAM for {invalid}");
        }
    }

    #[test]
    fn test_state_event_content_response_returns_raw_content_for_empty_state_key() {
        let content = json!({
            "topic": "raw topic payload"
        });

        let response = state_event_content_response(&content);

        assert_eq!(response, content);
        assert!(response.get("event_id").is_none());
        assert!(response.get("type").is_none());
    }

    #[test]
    fn test_state_event_content_response_returns_raw_content_for_keyed_state() {
        let content = json!({
            "enabled": true,
            "label": "alpha"
        });

        let response = state_event_content_response(&content);

        assert_eq!(response, content);
        assert!(response.get("state_key").is_none());
        assert!(response.get("sender").is_none());
    }
}
