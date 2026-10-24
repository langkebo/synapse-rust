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
/// The `write_access` module — the pair of room-event write gates.
pub(crate) mod write_access;

pub(crate) use e2ee::*;
pub(crate) use events::*;
pub(crate) use management::*;
pub(crate) use members::*;
pub(crate) use receipts::*;
pub(crate) use state::*;
pub(crate) use write_access::*;

use crate::routes::context::RoomContext;
use crate::routes::sticky_event::MSC4354_STICKY_DURATION_FIELD;
use crate::routes::{ensure_room_member_strict_ctx, AuthenticatedUser};
use serde::{Deserialize, Serialize};
use synapse_common::{parse_pagination_token, parse_stream_token, ApiError};
use synapse_services::delayed_event_service::CreateDelayedEventRequest;
use synapse_services::room::MAX_STICKY_DURATION_MS;

/// 解析后的 /messages 游标：`(origin_server_ts, stream_ordering)`。
type MessagesCursor = (i64, Option<i64>);

/// 解析 /messages 的 `from` 游标（ISSUE-06 / A10）。
///
/// 支持三种形式：复合 `t{ts}_{stream}`、legacy `t{ts}`、裸整数时间戳。
/// 无 `from` 参数（或空串）时返回 `Ok(None)`（从最新/最旧一页开始）；
/// 参数存在但无法解析为上述任一形式时返回 `M_INVALID_PARAM`，不再把非法游标
/// 静默当作「无游标」处理（否则会悄悄从房间最新一页重新开始）。
fn parse_room_messages_from_token(raw: Option<&str>) -> Result<Option<MessagesCursor>, ApiError> {
    let Some(token) = raw else {
        return Ok(None);
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
fn parse_pagination_direction(dir: Option<&str>) -> Result<&'static str, ApiError> {
    let Some(dir) = dir else {
        return Ok("b");
    };
    match dir {
        "f" => Ok("f"),
        "b" => Ok("b"),
        _ => Err(ApiError::invalid_param("The 'dir' parameter must be 'f' or 'b'")),
    }
}

/// The MSC4140/MSC4354 transport hints carried in the query string of every
/// room **write** request (`/send`, `/state`, …).
///
/// Both hints travel as `Option<String>` rather than a parsed integer: the
/// parsing errors are `M_INVALID_PARAM` with helper-specific messages, and the
/// helpers stay the single place that validates them.
///
/// Deliberately **not** `deny_unknown_fields`: these handlers used to take a
/// bare `Query<...>` map and ignored every other parameter, so an unknown
/// parameter must keep being ignored (a 200 rather than a 400).
#[derive(Debug, Default, Deserialize)]
pub(crate) struct RoomWriteQuery {
    #[serde(rename = "org.matrix.msc4140.delay")]
    pub(crate) delay: Option<String>,
    #[serde(rename = "org.matrix.msc4354.sticky_duration_ms")]
    pub(crate) sticky_duration_ms: Option<String>,
    #[serde(rename = "sticky_duration_ms")]
    pub(crate) sticky_duration_ms_short: Option<String>,
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

/// The largest delay this server accepts, in milliseconds (24 hours).
///
/// One constant for **both** scheduling surfaces — the `delay` hint on `/send`
/// and `/state`, and the `delay_ms` field of the dedicated endpoint — so a
/// client cannot discover which door it used by probing the boundary.
pub(crate) const MSC4140_MAX_DELAY_MS: i64 = 86_400_000;

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
/// NEW-13①：从消息内容里取出 `m.thread` 关系的**根事件 id**。
///
/// 只认 `m.relates_to.rel_type == "m.thread"`（规范值，不接受客户端自造的别名）；
/// `event_id` 缺失或不是字符串一律当作"不是线程回复"，不做猜测。
pub(crate) fn thread_root_event_id(body: &serde_json::Value) -> Option<String> {
    let relates_to = body.get("m.relates_to")?;
    if relates_to.get("rel_type").and_then(serde_json::Value::as_str) != Some("m.thread") {
        return None;
    }
    relates_to.get("event_id").and_then(serde_json::Value::as_str).map(str::to_string)
}

pub(crate) fn msc4140_query_delay_ms(query: &RoomWriteQuery) -> Result<Option<i64>, ApiError> {
    let Some(raw) = query.delay.as_deref() else {
        return Ok(None);
    };
    raw.trim()
        .parse::<i64>()
        .map(Some)
        .map_err(|_| ApiError::invalid_input(format!("Query parameter {MSC4140_DELAY_QUERY_PARAM} must be an integer")))
}

/// The short (non-namespaced) spelling of [`MSC4354_STICKY_DURATION_FIELD`].
///
/// Accepted as a query-string alias because this server has always tolerated it
/// on the sticky-event routes (see
/// `routes::sticky_event::sticky_duration_from_event`); the proposal spelling
/// wins when both are present.
pub(crate) const MSC4354_STICKY_DURATION_SHORT_FIELD: &str = "sticky_duration_ms";

/// The `unsigned` field MSC4354 reads back a resolved sticky TTL from.
///
/// Mirrors upstream `EventUnsignedContentFields.STICKY_TTL`
/// (`"msc4354_sticky_duration_ttl_ms"`): the name a spec-conformant client
/// inspects to learn how long an event stays pinned.
pub(crate) const MSC4354_STICKY_TTL_UNSIGNED_FIELD: &str = synapse_services::room::service::STICKY_TTL_UNSIGNED_FIELD;

/// Parse the `org.matrix.msc4354.sticky_duration_ms` hint out of the query
/// string; `None` when it is absent.
///
/// Upstream reads this from the query string on `/send` and `/state`; the
/// dedicated delayed-event endpoint carries the same name as a body field. Same
/// strictness as [`msc4140_query_delay_ms`]: a non-integer is `M_INVALID_PARAM`
/// rather than a silent no-op, and the bound (`0 < n <= MAX_STICKY_DURATION_MS`)
/// is enforced here so an out-of-range duration can never reach the event's
/// `unsigned`.
pub(crate) fn msc4354_query_sticky_duration_ms(query: &RoomWriteQuery) -> Result<Option<i64>, ApiError> {
    let Some(raw) = query.sticky_duration_ms.as_deref().or(query.sticky_duration_ms_short.as_deref()) else {
        return Ok(None);
    };
    let duration_ms = raw.trim().parse::<i64>().map_err(|_| {
        ApiError::invalid_param(format!("Query parameter {MSC4354_STICKY_DURATION_FIELD} must be an integer"))
    })?;
    validate_sticky_bounds(duration_ms)?;
    Ok(Some(duration_ms))
}

/// Reject a sticky duration outside `0 < n <= MAX_STICKY_DURATION_MS`.
///
/// Shared by the query-string form ([`msc4354_query_sticky_duration_ms`]) and the
/// body form ([`msc4354_sticky_duration`]) so the two doors cannot drift apart:
/// the same number must be acceptable (or not) wherever the client puts it.
fn validate_sticky_bounds(duration_ms: i64) -> Result<(), ApiError> {
    if duration_ms <= 0 {
        return Err(ApiError::invalid_param(format!(
            "{MSC4354_STICKY_DURATION_FIELD} must be a positive integer (milliseconds)"
        )));
    }
    if duration_ms > MAX_STICKY_DURATION_MS {
        return Err(ApiError::invalid_param(format!(
            "{MSC4354_STICKY_DURATION_FIELD} must not exceed 1 hour ({MAX_STICKY_DURATION_MS}ms)"
        )));
    }
    Ok(())
}

/// Parse MSC4354's sticky hint from a request **body** and/or the query string.
///
/// The dedicated delayed-event endpoint carries the hint as a body field
/// (`StickyDelayedEventBodyModel` upstream), while `/send` and `/state` carry it
/// in the query string. Upstream tolerates both while the MSC settles, and so does
/// this: two present values must agree, otherwise 400 — silently picking one
/// would let the caller believe a different TTL was stored than the one applied.
///
/// `None` means "not asked for"; a JSON `null` counts as absent (JSON has no
/// `Absent` sentinel, and a serialised optional field conveys the same thing).
pub(crate) fn msc4354_sticky_duration(
    body: Option<&serde_json::Value>,
    query: &RoomWriteQuery,
) -> Result<Option<i64>, ApiError> {
    let from_query = msc4354_query_sticky_duration_ms(query)?;
    let from_body = body.map(msc4354_sticky_duration_from_body).transpose()?.flatten();

    match (from_query, from_body) {
        (Some(query_ms), Some(body_ms)) if query_ms != body_ms => {
            Err(ApiError::invalid_param(format!("Conflicting values given for {MSC4354_STICKY_DURATION_FIELD}")))
        }
        (from_query, from_body) => Ok(from_query.or(from_body)),
    }
}

fn msc4354_sticky_duration_from_body(body: &serde_json::Value) -> Result<Option<i64>, ApiError> {
    let Some(raw) = body.get(MSC4354_STICKY_DURATION_FIELD).or_else(|| body.get(MSC4354_STICKY_DURATION_SHORT_FIELD))
    else {
        return Ok(None);
    };
    if raw.is_null() {
        return Ok(None);
    }
    let duration_ms = raw
        .as_i64()
        .ok_or_else(|| ApiError::invalid_param(format!("'{MSC4354_STICKY_DURATION_FIELD}' must be an integer")))?;
    validate_sticky_bounds(duration_ms)?;
    Ok(Some(duration_ms))
}

/// Record MSC4354's requested sticky TTL on an already-persisted event's client
/// projection (`unsigned`).
///
/// `unsigned` sits outside the signed bytes and the content hash, so writing it
/// after the event is stored cannot invalidate the event's signature (see
/// `synapse_services` `MessagingService::update_event_unsigned`). The write
/// overwrites the column — matching the `invite_room_state` precedent in
/// `routes::federation::membership::invite` — which is correct here because a
/// freshly sent message carries no `unsigned` to merge with.
///
/// Note: this is the **upstream** sticky model (an event property). This server
/// separately tracks sticky events as per-user pins via the private
/// `room_sticky_events` table, and it is that table — not this field — that
/// drives the `sticky_events` array in `/sync`. The two models coexist.
pub(crate) async fn apply_msc4354_sticky_ttl(
    ctx: &RoomContext,
    event_id: &str,
    duration_ms: i64,
) -> Result<(), ApiError> {
    ctx.room_service
        .messaging()
        .update_event_unsigned(event_id, &serde_json::json!({ MSC4354_STICKY_TTL_UNSIGNED_FIELD: duration_ms }))
        .await
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
///
/// Validation and the hint's location live here; storing the row and the
/// idempotency bookkeeping live in [`schedule_delayed_event_core`], which the
/// dedicated endpoint shares.
#[allow(clippy::too_many_arguments)] // 写路径透传参数多，语义直白
pub(crate) async fn schedule_delayed_event_if_requested(
    ctx: &RoomContext,
    user_id: &str,
    device_id: &str,
    room_id: &str,
    event_type: &str,
    state_key: Option<String>,
    body: &serde_json::Value,
    query: &RoomWriteQuery,
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

    // MSC4354: a sticky hint **may** accompany a scheduled send. The TTL cannot be
    // recorded at scheduling time (there is no event yet), so it is stored on the
    // `delayed_events` row and applied by the dispatcher immediately after the
    // send — which is exactly what upstream PR #19354 does
    // (`delayed_events_handler.add(..., sticky_duration_ms=...)` and, at send
    // time, `event_dict[StickyEvent.EVENT_FIELD_NAME] = StickyEventField(...)`).
    //
    // Only the **query** is read here: on `/send` the request body *is* the event
    // content, so a `sticky_duration_ms` key inside it is content, not a hint.
    // Upstream reads this parameter from the query string on `/send` and `/state`
    // for the same reason.
    //
    // A sticky **state** event is the one combination upstream forbids outright
    // ("Sticky state events are not allowed"), and the same refusal belongs here:
    // a state event's `unsigned` TTL has no client meaning.
    let sticky_duration_ms = msc4354_query_sticky_duration_ms(query)?;
    if sticky_duration_ms.is_some() && state_key.is_some() {
        return Err(ApiError::invalid_param(
            "Sticky state events are not allowed: cannot combine `state_key` with a sticky duration".to_string(),
        ));
    }

    // Validate delay is positive and bounded (max 24h, see MSC4140_MAX_DELAY_MS).
    if delay_ms <= 0 {
        return Err(ApiError::bad_request(
            "org.matrix.msc4140.delay must be a positive integer (milliseconds)".to_string(),
        ));
    }
    if delay_ms > MSC4140_MAX_DELAY_MS {
        return Err(ApiError::bad_request(format!(
            "org.matrix.msc4140.delay must not exceed 24 hours ({MSC4140_MAX_DELAY_MS}ms)"
        )));
    }

    // Remove the MSC4140 delay field from the content before storing — it is a
    // transport-level scheduling hint, not part of the event content. (A hint
    // sent as a query parameter is not in the content to begin with; the removal
    // only matters for the legacy body spelling.)
    let mut content = body.clone();
    if let Some(obj) = content.as_object_mut() {
        obj.remove(MSC4140_DELAY_QUERY_PARAM);
    }

    let response = schedule_delayed_event_core(
        ctx,
        user_id,
        device_id,
        room_id,
        event_type,
        state_key,
        content,
        delay_ms,
        sticky_duration_ms,
        txn_id,
        MSC4140_SEND_TXN_NAMESPACE,
    )
    .await?;

    Ok(Some(response))
}

/// The `/send` + `/state` idempotency namespace.
///
/// Namespaced on purpose: this cache holds the `delay_id` of a *scheduled*
/// event, while the same `txn:{user}:{room}:{txn}` key on the immediate path
/// holds the **event_id** of a sent event. Sharing one namespace across the two
/// surfaces would let a client's retry of a plain `/send` be answered with the
/// other call's answer (see [`MSC4140_SCHEDULE_TXN_NAMESPACE`]).
pub(crate) const MSC4140_SEND_TXN_NAMESPACE: &str = "txn";

/// The dedicated endpoint's idempotency namespace.
///
/// Distinct from [`MSC4140_SEND_TXN_NAMESPACE`]: the two surfaces are different
/// operations on the same `(room, txn_id)` coordinate, and a client that retries
/// a plain `/send` and a scheduling call carrying the same `txn_id` must not be
/// handed the other one's answer.
pub(crate) const MSC4140_SCHEDULE_TXN_NAMESPACE: &str = "msc4140-delayed-event";

/// Store a delayed event and build the client-facing `{"delay_id": N}` response.
///
/// The shared tail of both scheduling surfaces: the `delay` hint on `/send` and
/// `/state` ([`schedule_delayed_event_if_requested`]) and the dedicated endpoint
/// (`routes::delayed_events::schedule_delayed_event`). Callers own *validation
/// and authorization*; this owns *storage + idempotency*.
///
/// Deliberately does **not** authorize. Every caller is required to run the
/// matching write gate first, because the dispatcher replays the stored row
/// without any further check.
///
/// The `txn_id` replay lookup lives here rather than in each caller so both
/// surfaces dedupe identically; `txn_cache_namespace` is what keeps their cached
/// values from colliding.
#[allow(clippy::too_many_arguments)] // 写路径透传参数多，语义直白
pub(crate) async fn schedule_delayed_event_core(
    ctx: &RoomContext,
    user_id: &str,
    device_id: &str,
    room_id: &str,
    event_type: &str,
    state_key: Option<String>,
    content: serde_json::Value,
    delay_ms: i64,
    sticky_duration_ms: Option<i64>,
    txn_id: Option<&str>,
    txn_cache_namespace: &str,
) -> Result<axum::extract::Json<serde_json::Value>, ApiError> {
    let txn_id = txn_id.filter(|t| !t.is_empty());
    let cache_key = txn_id.map(|t| format!("{txn_cache_namespace}:{user_id}:{room_id}:{t}"));

    if let Some(cache_key) = cache_key.as_deref() {
        if let Ok(Some(cached)) = ctx.cache.get::<String>(cache_key).await {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&cached) {
                return Ok(axum::extract::Json(value));
            }
        }
    }

    let request = CreateDelayedEventRequest {
        room_id: room_id.to_string(),
        user_id: user_id.to_string(),
        device_id: device_id.to_string(),
        event_type: event_type.to_string(),
        state_key,
        content,
        delay_ms,
        sticky_duration_ms,
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

    let response = serde_json::json!({ "delay_id": delayed.id });
    if let Some(cache_key) = cache_key.as_deref() {
        if let Err(e) = ctx.cache.set(cache_key, &response.to_string(), 3600).await {
            ::tracing::warn!("Failed to cache delayed event txn_id dedup marker: {e}");
        }
    }

    Ok(axum::extract::Json(response))
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
    /// MSC4289: extra creators for the replacement room's `m.room.create`.
    /// Only honoured by room versions that define the rule (v12+).
    #[serde(default)]
    pub(crate) additional_creators: Option<Vec<String>>,
}

/// The `UpgradeRoomResponse` struct.
#[derive(Debug, Serialize)]
pub(crate) struct UpgradeRoomResponse {
    pub(crate) replacement_room: String,
}

#[cfg(test)]
mod tests {
    use super::{
        msc4354_query_sticky_duration_ms, msc4354_sticky_duration, parse_pagination_direction,
        parse_room_messages_from_token, state_event_content_response, RoomWriteQuery, MSC4140_DELAY_QUERY_PARAM,
        MSC4354_STICKY_DURATION_FIELD, MSC4354_STICKY_DURATION_SHORT_FIELD,
    };
    use serde_json::json;
    use synapse_common::error::ApiErrorKind;

    /// Build the query struct the way `serde_urlencoded` would: recognised keys
    /// land in their field, everything else is ignored (no `deny_unknown_fields`).
    fn query(pairs: &[(&str, &str)]) -> RoomWriteQuery {
        let mut query = RoomWriteQuery::default();
        for (key, value) in pairs {
            let value = Some((*value).to_string());
            match *key {
                MSC4140_DELAY_QUERY_PARAM => query.delay = value,
                MSC4354_STICKY_DURATION_FIELD => query.sticky_duration_ms = value,
                MSC4354_STICKY_DURATION_SHORT_FIELD => query.sticky_duration_ms_short = value,
                _ => {}
            }
        }
        query
    }

    /// MSC4354 on the **delayed** surfaces: the hint is accepted from the request
    /// body (the dedicated endpoint's location) and/or the query string (`/send`,
    /// `/state`), and two disagreeing values are a client error rather than a
    /// silent pick.
    #[test]
    fn sticky_duration_is_read_from_body_and_query() {
        let body =
            |value: serde_json::Value| json!({ "delay_ms": 1000, "content": {}, MSC4354_STICKY_DURATION_FIELD: value });

        assert_eq!(msc4354_sticky_duration(Some(&body(json!(60_000))), &query(&[])).unwrap(), Some(60_000));

        let via_query = query(&[(MSC4354_STICKY_DURATION_FIELD, "30000")]);
        assert_eq!(msc4354_sticky_duration(Some(&json!({})), &via_query).unwrap(), Some(30_000));
        assert_eq!(
            msc4354_sticky_duration(Some(&body(json!(30_000))), &via_query).unwrap(),
            Some(30_000),
            "agreeing values are fine"
        );
        assert_eq!(
            msc4354_sticky_duration(Some(&body(json!(60_000))), &via_query).unwrap_err().kind,
            ApiErrorKind::BadRequest,
            "disagreeing body/query values must not be silently resolved"
        );

        // Absent everywhere, and an explicit `null`, both mean "no hint".
        assert_eq!(msc4354_sticky_duration(Some(&json!({})), &query(&[])).unwrap(), None);
        assert_eq!(msc4354_sticky_duration(Some(&body(serde_json::Value::Null)), &query(&[])).unwrap(), None);
    }

    // MSC4354 (UG-10): the sticky duration hint is read from the query string,
    // same strictness as the MSC4140 delay hint (V-19-9).

    #[test]
    fn msc4354_query_sticky_duration_absent_is_none() {
        assert!(msc4354_query_sticky_duration_ms(&query(&[])).unwrap().is_none());
        assert!(msc4354_query_sticky_duration_ms(&query(&[("unrelated", "1")])).unwrap().is_none());
    }

    #[test]
    fn msc4354_query_sticky_duration_accepts_both_spellings() {
        assert_eq!(
            msc4354_query_sticky_duration_ms(&query(&[(MSC4354_STICKY_DURATION_FIELD, "60000")])).unwrap(),
            Some(60_000)
        );
        assert_eq!(
            msc4354_query_sticky_duration_ms(&query(&[(MSC4354_STICKY_DURATION_SHORT_FIELD, "60000")])).unwrap(),
            Some(60_000)
        );
    }

    #[test]
    fn msc4354_query_sticky_duration_proposal_spelling_wins() {
        let q = query(&[(MSC4354_STICKY_DURATION_FIELD, "1000"), (MSC4354_STICKY_DURATION_SHORT_FIELD, "2000")]);
        assert_eq!(msc4354_query_sticky_duration_ms(&q).unwrap(), Some(1_000));
    }

    #[test]
    fn msc4354_query_sticky_duration_trims_whitespace() {
        assert_eq!(
            msc4354_query_sticky_duration_ms(&query(&[(MSC4354_STICKY_DURATION_FIELD, " 5000 ")])).unwrap(),
            Some(5_000)
        );
    }

    #[test]
    fn msc4354_query_sticky_duration_accepts_the_one_hour_bound() {
        assert_eq!(
            msc4354_query_sticky_duration_ms(&query(&[(MSC4354_STICKY_DURATION_FIELD, "3600000")])).unwrap(),
            Some(3_600_000)
        );
    }

    #[test]
    fn msc4354_query_sticky_duration_rejects_bad_values() {
        for (value, why) in [("not-a-number", "non-integer"), ("0", "zero"), ("-1", "negative"), ("3600001", "over 1h")]
        {
            let err = msc4354_query_sticky_duration_ms(&query(&[(MSC4354_STICKY_DURATION_FIELD, value)])).unwrap_err();
            assert!(err.is_bad_request(), "expected M_INVALID_PARAM for {why} ({value})");
        }
    }

    // A10: `/messages` must not silently fall back when `dir`/`from` are invalid.

    #[test]
    fn test_parse_pagination_direction_defaults_to_backward() {
        assert_eq!(parse_pagination_direction(None).unwrap(), "b");
    }

    #[test]
    fn test_parse_pagination_direction_accepts_f_and_b() {
        assert_eq!(parse_pagination_direction(Some("f")).unwrap(), "f");
        assert_eq!(parse_pagination_direction(Some("b")).unwrap(), "b");
    }

    #[test]
    fn test_parse_pagination_direction_rejects_invalid_values() {
        for invalid in [Some("sideways"), Some(""), Some("F")] {
            let err = parse_pagination_direction(invalid).unwrap_err();
            assert!(err.is_bad_request(), "expected M_INVALID_PARAM for {invalid:?}");
        }
    }

    #[test]
    fn test_parse_room_messages_from_token_absent_or_empty_is_none() {
        assert!(parse_room_messages_from_token(None).unwrap().is_none());
        assert!(parse_room_messages_from_token(Some("")).unwrap().is_none());
    }

    #[test]
    fn test_parse_room_messages_from_token_accepts_bare_timestamp() {
        let cursor = parse_room_messages_from_token(Some("12345")).unwrap();
        assert_eq!(cursor, Some((12345, None)));
    }

    #[test]
    fn test_parse_room_messages_from_token_rejects_invalid_values() {
        let err = parse_room_messages_from_token(Some("not-a-token")).unwrap_err();
        assert!(err.is_bad_request(), "expected M_INVALID_PARAM for a non-numeric token");
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
