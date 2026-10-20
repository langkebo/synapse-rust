//! MSC4140 — Cancellable delayed events route handlers.
//!
//! Implements both scheduling surfaces:
//!
//! * the **delay hint** on the ordinary send paths (`/send` with
//!   `?org.matrix.msc4140.delay=`, and the `/state` variants) — see
//!   `handlers::room::schedule_delayed_event_if_requested`;
//! * the **dedicated endpoint** (`PUT .../rooms/{roomId}/delayed_event/…`),
//!   which upstream added in the same PR and now recommends.
//!
//! Whichever door the client used, the server returns a `delay_id` and this
//! module provides the management endpoints to cancel, restart (heartbeat), or
//! force-send the pending event.
//!
//! Endpoints:
//!   PUT  /_matrix/client/unstable/org.matrix.msc4140/rooms/{room_id}/delayed_event/{event_type}/{txn_id}
//!   GET  /_matrix/client/unstable/org.matrix.msc4140/delayed_events/{delay_id}
//!   POST /_matrix/client/unstable/org.matrix.msc4140/delayed_events/{delay_id}
//!   { "action": "send" | "cancel" | "restart" }
//!
//! See: <https://github.com/matrix-org/matrix-spec-proposals/pull/4140>

use crate::routes::context::{AdminContext, RoomContext};
use crate::routes::extractors::{AuthenticatedUser, RoomId};
use crate::routes::handlers::room::{
    ensure_message_write_access, ensure_room_state_write_access, msc4354_sticky_duration, normalize_room_event_type,
    schedule_delayed_event_core, RoomWriteQuery, MSC4140_MAX_DELAY_MS, MSC4140_SCHEDULE_TXN_NAMESPACE,
};
use crate::routes::validate_room_id;
use crate::routes::AppState;
use crate::routes::MatrixJson;
use axum::{
    extract::{Json, Path, Query, State},
    http::HeaderMap,
    routing::{get, post, put},
    Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use synapse_common::ApiError;
use synapse_services::delayed_event_service::{DelayedEvent, DelayedEventAction};

/// MSC4140 — Schedule a delayed event through the dedicated endpoint.
///
/// `PUT /_matrix/client/unstable/org.matrix.msc4140/rooms/{room_id}/delayed_event/{event_type}/{txn_id}`
///
/// Upstream introduced this endpoint and **deprecated without removing** the
/// `delay` query parameter on `/send` and `/state`; this handler is the mirror
/// of those two paths — same room check, same write authorization, same
/// `delay_ms` bounds — only the hint's *location* differs.
///
/// **The authorization here is not decoration.** The background dispatcher that
/// later replays a due row (`src/server/mod.rs`) writes through `room_service`
/// and re-checks nothing, so an under-authorized schedule is an under-authorized
/// write that happens minutes later, on a background task. Whether `state_key`
/// is present selects which gate runs — and it also selects which write path
/// the dispatcher will use, so the gate always matches the eventual write.
///
/// `delay_id` in the response is a **number**, deliberately unlike upstream's
/// string form: this server's `delay_id` is a `BIGSERIAL` and the management
/// endpoints parse it with `Path<i64>`, so a string here would hand out an id
/// its own siblings cannot read back.
pub(crate) async fn schedule_delayed_event(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_type, txn_id)): Path<(RoomId, String, String)>,
    Query(query): Query<RoomWriteQuery>,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    if !ctx.room_service.state().room_exists(&room_id).await? {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let parsed = parse_schedule_request(&body, &query)?;

    let event_type = normalize_room_event_type(&event_type);

    if parsed.state_key.is_some() {
        ensure_room_state_write_access(&ctx, &auth_user, &room_id, &event_type, &parsed.content).await?;

        return schedule_delayed_event_core(
            &ctx,
            &auth_user.user_id,
            auth_user.device_id.as_deref().unwrap_or(""),
            &room_id,
            &event_type,
            parsed.state_key,
            parsed.content,
            parsed.delay_ms,
            parsed.sticky_duration_ms,
            Some(txn_id.as_str()),
            MSC4140_SCHEDULE_TXN_NAMESPACE,
        )
        .await;
    }

    // Message path: the gate rewrites `content` in place (HTML sanitizer,
    // content scan), so it must be handed ownership *before* the core stores it.
    let mut content = parsed.content;
    ensure_message_write_access(&ctx, &auth_user, &room_id, &event_type, &mut content).await?;

    schedule_delayed_event_core(
        &ctx,
        &auth_user.user_id,
        auth_user.device_id.as_deref().unwrap_or(""),
        &room_id,
        &event_type,
        None,
        content,
        parsed.delay_ms,
        parsed.sticky_duration_ms,
        Some(txn_id.as_str()),
        MSC4140_SCHEDULE_TXN_NAMESPACE,
    )
    .await
}

/// The validated body of a dedicated-endpoint scheduling request.
#[derive(Debug)]
struct ScheduleRequest {
    /// `delay_ms`, already checked to be positive and within [`MSC4140_MAX_DELAY_MS`].
    delay_ms: i64,
    /// `content` — the event body proper.
    content: Value,
    /// `state_key` — `None` for a message event, and for an explicit `null`.
    state_key: Option<String>,
    /// MSC4354 sticky duration to apply when the event is finally sent.
    sticky_duration_ms: Option<i64>,
}

/// Validate the body of the dedicated endpoint.
///
/// Split out from the handler so the whole rejection matrix is testable without
/// a database: every branch here is a decision about *what the client asked
/// for*, taken before any state is touched.
///
/// Unevaluated JSON rather than a `Deserialize` struct, so a missing or mistyped
/// field yields 400 `M_BAD_JSON` (this module's style — see
/// `manage_delayed_event`) instead of axum's 422 deserialization rejection.
fn parse_schedule_request(body: &Value, query: &RoomWriteQuery) -> Result<ScheduleRequest, ApiError> {
    // MSC4354: a sticky duration **is** accepted here (upstream PR #19354 carries
    // the same field as a body key). It is stored on the row and applied by the
    // dispatcher right after the send — see
    // `synapse_services::room::scheduled_dispatch::dispatch_due_event`. Both the
    // body field and the query parameter are read (upstream tolerates both while
    // the MSC settles); disagreeing values are 400, never a silent pick.
    let sticky_duration_ms = msc4354_sticky_duration(Some(body), query)?;

    let delay_ms = body
        .get("delay_ms")
        .and_then(Value::as_i64)
        .ok_or_else(|| ApiError::bad_request("Missing or invalid 'delay_ms' field (must be an integer)".to_string()))?;
    if delay_ms <= 0 {
        return Err(ApiError::bad_request("'delay_ms' must be a positive integer (milliseconds)".to_string()));
    }
    if delay_ms > MSC4140_MAX_DELAY_MS {
        return Err(ApiError::bad_request(format!("'delay_ms' must not exceed 24 hours ({MSC4140_MAX_DELAY_MS}ms)")));
    }

    // `content` is the event body itself (unlike `/send`, where the request body
    // *is* the content and the hint sits beside it in the query string), so it
    // is taken as a sub-object and never needs the hint stripped out of it.
    let content =
        body.get("content").filter(|value| value.is_object()).cloned().ok_or_else(|| {
            ApiError::bad_request("Missing or invalid 'content' field (must be an object)".to_string())
        })?;

    // `null` means "no state key" — the same as upstream's absent optional, and
    // unambiguous against `""` (a real, empty state key).
    let state_key = match body.get("state_key") {
        None | Some(Value::Null) => None,
        Some(Value::String(state_key)) => Some(state_key.clone()),
        Some(_) => return Err(ApiError::bad_request("Invalid 'state_key' field (must be a string)".to_string())),
    };

    // Upstream rejects this combination outright ("Sticky state events are not
    // allowed"): a state event's `unsigned` TTL has no client meaning, and the
    // dispatcher would have nowhere to apply it.
    if state_key.is_some() && sticky_duration_ms.is_some() {
        return Err(ApiError::invalid_param(
            "Sticky state events are not allowed: cannot combine `state_key` with a sticky duration".to_string(),
        ));
    }

    Ok(ScheduleRequest { delay_ms, content, state_key, sticky_duration_ms })
}

/// MSC4140 — Get a single pending delayed event.
///
/// `GET /_matrix/client/unstable/org.matrix.msc4140/delayed_events/{delay_id}`
///
/// Only the user who scheduled the event may read it, and only while it is
/// still pending; a missing, foreign, or already-settled event all return
/// `M_NOT_FOUND` so the endpoint never leaks another user's delayed events.
///
/// The response shape mirrors upstream's `DelayedEventResponse.asdict()`
/// exactly: `delay_id`, `room_id`, `type`, optional `state_key`, `delay_ms`,
/// `delayed_since_ts` (the scheduling time), and `content`. `state_key` is
/// omitted when absent (upstream filters out `None` values).
pub(crate) async fn get_delayed_event(
    State(ctx): State<AdminContext>,
    headers: HeaderMap,
    auth_user: AuthenticatedUser,
    Path(delay_id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    let request_id = crate::utils::auth::resolve_request_id(&headers);
    let event = ctx.delayed_event_service.get(delay_id, &auth_user.user_id, &request_id).await?;

    Ok(Json(delayed_event_response(&event)))
}

/// Serialise one delayed event the way MSC4140 reports it.
///
/// Shared by the single-event and the list endpoint so the two can never drift:
/// upstream serves both from the same `DelayedEventResponse.asdict()`.
///
/// Shape: `delay_id`, `room_id`, `type`, optional `state_key`, `delay_ms`,
/// `delayed_since_ts` (the scheduling time), `content`. `state_key` is omitted
/// when absent — upstream filters out `None` values rather than emitting `null`.
fn delayed_event_response(event: &DelayedEvent) -> Value {
    let mut body = serde_json::Map::new();
    body.insert("delay_id".to_string(), json!(event.id));
    body.insert("room_id".to_string(), json!(event.room_id));
    body.insert("type".to_string(), json!(event.event_type));
    if let Some(state_key) = event.state_key.as_ref() {
        body.insert("state_key".to_string(), json!(state_key));
    }
    body.insert("delay_ms".to_string(), json!(event.delay_ms));
    body.insert("delayed_since_ts".to_string(), json!(event.created_ts));
    body.insert("content".to_string(), event.content.clone());
    Value::Object(body)
}

/// MSC4140 — List the caller's pending delayed events.
///
/// `GET /_matrix/client/unstable/org.matrix.msc4140/delayed_events`
///
/// Response is `{"delayed_events": [ … ]}` with the same per-item shape as the
/// single-event GET. The caller's own events only — the filter is part of the
/// storage query, not a post-filter (see `DelayedEventService::list_for_user`).
///
/// Upstream serves `from`/`limit` pagination from this endpoint but has not
/// implemented it (`# TODO: Support Pagination stream API`), and this server
/// matches that: no pagination parameters are read, and none are honoured.
pub(crate) async fn list_delayed_events(
    State(ctx): State<AdminContext>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let events = ctx.delayed_event_service.list_for_user(&auth_user.user_id).await?;
    let listed: Vec<Value> = events.iter().map(delayed_event_response).collect();
    Ok(Json(json!({ "delayed_events": listed })))
}

/// Body of `POST …/delayed_events/{delay_id}`.
#[derive(Debug, Deserialize)]
pub(crate) struct ManageDelayedEventBody {
    pub action: String,
}

/// MSC4140 Gen 1 — Manage a delayed event.
///
/// `POST /_matrix/client/unstable/org.matrix.msc4140/delayed_events/{delay_id}`
///
/// The `delay_id` is the server-generated numeric id returned when the delayed
/// event was scheduled. Only the user who scheduled the event may manage it;
/// this is enforced fail-closed by comparing `event.user_id` to the caller.
pub(crate) async fn manage_delayed_event(
    State(ctx): State<AdminContext>,
    headers: HeaderMap,
    auth_user: AuthenticatedUser,
    Path(delay_id): Path<i64>,
    MatrixJson(body): MatrixJson<ManageDelayedEventBody>,
) -> Result<Json<Value>, ApiError> {
    // A missing or non-string `action` is a typed deserialization failure, which
    // `MatrixJson` maps to M_BAD_JSON (400) rather than axum's default 422.
    let action = DelayedEventAction::parse(&body.action)?;

    apply_action(&ctx, &headers, &auth_user.user_id, delay_id, action).await
}

/// Apply one management action, returning MSC4140's empty-object success body.
///
/// Shared by the generic `POST …/{delay_id}` (action in the body) and the three
/// convenience sub-paths `…/{delay_id}/{cancel,restart,send}` that upstream
/// serves from separate servlets. Those are the *same* operation with the action
/// fixed by the path, so they must not grow a second implementation — and in
/// particular must not bypass the ownership check.
async fn apply_action(
    ctx: &AdminContext,
    headers: &HeaderMap,
    user_id: &str,
    delay_id: i64,
    action: DelayedEventAction,
) -> Result<Json<Value>, ApiError> {
    let request_id = crate::utils::auth::resolve_request_id(headers);

    // Ownership (fail-closed) and the pending-state transitions live in the
    // service; a foreign or missing delay_id both surface as M_NOT_FOUND.
    ctx.delayed_event_service.manage(delay_id, action, user_id, &request_id).await?;

    // MSC4140 success response is an empty JSON object.
    Ok(Json(json!({})))
}

/// MSC4140 — `POST …/delayed_events/{delay_id}/cancel`.
///
/// Upstream serves this from `CancelDelayedEventServlet`; the action is fixed by
/// the path, so no request body is read (an empty body is expected, and one sent
/// anyway is ignored — upstream does not parse it either).
pub(crate) async fn cancel_delayed_event(
    State(ctx): State<AdminContext>,
    headers: HeaderMap,
    auth_user: AuthenticatedUser,
    Path(delay_id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    apply_action(&ctx, &headers, &auth_user.user_id, delay_id, DelayedEventAction::Cancel).await
}

/// MSC4140 — `POST …/delayed_events/{delay_id}/restart` (heartbeat).
pub(crate) async fn restart_delayed_event(
    State(ctx): State<AdminContext>,
    headers: HeaderMap,
    auth_user: AuthenticatedUser,
    Path(delay_id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    apply_action(&ctx, &headers, &auth_user.user_id, delay_id, DelayedEventAction::Restart).await
}

/// MSC4140 — `POST …/delayed_events/{delay_id}/send` (send now).
pub(crate) async fn send_delayed_event(
    State(ctx): State<AdminContext>,
    headers: HeaderMap,
    auth_user: AuthenticatedUser,
    Path(delay_id): Path<i64>,
) -> Result<Json<Value>, ApiError> {
    apply_action(&ctx, &headers, &auth_user.user_id, delay_id, DelayedEventAction::Send).await
}

/// MSC4140 router — mounts the delayed-events endpoints under the unstable
/// namespace.
///
/// All surfaces live here because they are one feature: the dedicated scheduling
/// endpoint, the read endpoints, and the management endpoints that act on the
/// `delay_id` the scheduling endpoint returns.
///
/// The route list mirrors upstream's `register_servlets` in
/// `synapse/rest/client/delayed_events.py` one-for-one, including the three
/// `{delay_id}/{cancel,restart,send}` convenience paths — they exist so a client
/// can name the operation in the path instead of in a body, and they resolve to
/// the same `manage` call.
pub fn create_delayed_events_router() -> Router<AppState> {
    Router::new()
        .route("/delayed_events", get(list_delayed_events))
        .route("/delayed_events/{delay_id}", get(get_delayed_event).post(manage_delayed_event))
        .route("/delayed_events/{delay_id}/cancel", post(cancel_delayed_event))
        .route("/delayed_events/{delay_id}/restart", post(restart_delayed_event))
        .route("/delayed_events/{delay_id}/send", post(send_delayed_event))
        .route("/rooms/{room_id}/delayed_event/{event_type}/{txn_id}", put(schedule_delayed_event))
}

#[cfg(test)]
mod tests {
    /// Parse a body with no query-string hints (the common case).
    fn parse(body: &Value) -> Result<ScheduleRequest, ApiError> {
        parse_schedule_request(body, &RoomWriteQuery::default())
    }

    use super::*;
    use crate::routes::sticky_event::MSC4354_STICKY_DURATION_FIELD;
    use synapse_common::ApiErrorKind;

    #[test]
    fn test_delayed_event_action_parse_valid() {
        assert_eq!(DelayedEventAction::parse("send").unwrap(), DelayedEventAction::Send);
        assert_eq!(DelayedEventAction::parse("cancel").unwrap(), DelayedEventAction::Cancel);
        assert_eq!(DelayedEventAction::parse("restart").unwrap(), DelayedEventAction::Restart);
    }

    #[test]
    fn test_delayed_event_action_parse_invalid() {
        assert!(DelayedEventAction::parse("delete").is_err());
        assert!(DelayedEventAction::parse("").is_err());
    }

    // ── Dedicated scheduling endpoint: the rejection matrix ──────────────────
    //
    // These are pure: no room, no database, no auth. What they pin down is that
    // each malformed body fails *loudly and with the right status*, since the
    // alternative — a lenient parse — would schedule something the client did
    // not ask for and only surface it minutes later in the dispatcher's log.

    fn kind_of(body: &Value) -> ApiErrorKind {
        parse(body).expect_err("body must be rejected").kind
    }

    fn valid_body() -> Value {
        json!({ "delay_ms": 60_000, "content": { "body": "later" } })
    }

    #[test]
    fn schedule_body_accepts_the_minimal_valid_request() {
        let parsed = parse(&valid_body()).expect("minimal body must parse");
        assert_eq!(parsed.delay_ms, 60_000);
        assert_eq!(parsed.content, json!({ "body": "later" }));
        assert_eq!(parsed.state_key, None, "an absent state_key means a message event");
    }

    #[test]
    fn schedule_body_delay_ms_is_required_and_integral() {
        assert_eq!(kind_of(&json!({ "content": {} })), ApiErrorKind::BadRequest, "missing delay_ms");
        assert_eq!(
            kind_of(&json!({ "delay_ms": "60000", "content": {} })),
            ApiErrorKind::BadRequest,
            "string delay_ms"
        );
        assert_eq!(kind_of(&json!({ "delay_ms": 1.5, "content": {} })), ApiErrorKind::BadRequest, "float delay_ms");
        assert_eq!(kind_of(&json!({ "delay_ms": null, "content": {} })), ApiErrorKind::BadRequest, "null delay_ms");
    }

    #[test]
    fn schedule_body_delay_ms_must_be_positive_and_bounded() {
        assert_eq!(kind_of(&json!({ "delay_ms": 0, "content": {} })), ApiErrorKind::BadRequest);
        assert_eq!(kind_of(&json!({ "delay_ms": -1, "content": {} })), ApiErrorKind::BadRequest);
        assert_eq!(
            kind_of(&json!({ "delay_ms": MSC4140_MAX_DELAY_MS + 1, "content": {} })),
            ApiErrorKind::BadRequest,
            "one millisecond past the cap must be refused, not truncated"
        );
        // The boundary itself is inclusive on purpose: a client probing for the
        // limit must see the same answer whichever door it used.
        let at_cap = json!({ "delay_ms": MSC4140_MAX_DELAY_MS, "content": {} });
        assert_eq!(parse(&at_cap).expect("the cap is inclusive").delay_ms, MSC4140_MAX_DELAY_MS);
    }

    #[test]
    fn schedule_body_content_must_be_an_object() {
        assert_eq!(kind_of(&json!({ "delay_ms": 1000 })), ApiErrorKind::BadRequest, "missing content");
        assert_eq!(kind_of(&json!({ "delay_ms": 1000, "content": [] })), ApiErrorKind::BadRequest, "array content");
        assert_eq!(kind_of(&json!({ "delay_ms": 1000, "content": "hi" })), ApiErrorKind::BadRequest, "string content");
    }

    #[test]
    fn schedule_body_state_key_distinguishes_message_from_state() {
        let absent = parse(&valid_body()).unwrap();
        assert_eq!(absent.state_key, None);

        let explicit_null = parse(&json!({
            "delay_ms": 1000,
            "content": {},
            "state_key": null
        }))
        .unwrap();
        assert_eq!(explicit_null.state_key, None, "an explicit null is the absent state_key, not an error");

        let empty_key = parse(&json!({
            "delay_ms": 1000,
            "content": {},
            "state_key": ""
        }))
        .unwrap();
        assert_eq!(
            empty_key.state_key,
            Some(String::new()),
            "the empty string is a real state key (a state event), not a missing one"
        );

        let named = parse(&json!({
            "delay_ms": 1000,
            "content": {},
            "state_key": "topic"
        }))
        .unwrap();
        assert_eq!(named.state_key.as_deref(), Some("topic"));

        assert_eq!(
            kind_of(&json!({ "delay_ms": 1000, "content": {}, "state_key": 5 })),
            ApiErrorKind::BadRequest,
            "a non-string state_key must not be coerced"
        );
    }

    /// MSC4354's sticky duration is **carried**, not refused: the row stores it and
    /// the dispatcher applies it at send time. (Bounds, conflicts and the
    /// state-key rule are covered next to the parser, in
    /// `handlers::room::tests`.)
    #[test]
    fn schedule_body_carries_the_sticky_duration_hint() {
        for field in [MSC4354_STICKY_DURATION_FIELD, "sticky_duration_ms"] {
            let mut body = valid_body();
            body[field] = json!(60_000);
            assert_eq!(parse(&body).unwrap().sticky_duration_ms, Some(60_000), "field {field}");
        }
        assert_eq!(parse(&valid_body()).unwrap().sticky_duration_ms, None);
    }
}
