use crate::routes::context::RoomContext;
use crate::routes::{ensure_room_member_strict_ctx, AuthenticatedUser};
use axum::extract::{Json, Path, Query, State};
use serde_json::{json, Value};
use std::collections::HashSet;
use synapse_common::ApiError;
use synapse_services::search_service::{EventContextEntry, TimestampDirection};

use crate::routes::extractors::{EventId, RoomId};

/// The `GET /rooms/{roomId}/context/{eventId}` query parameters.
///
/// Deliberately **not** `deny_unknown_fields`: the handler used to take a bare
/// query map and ignored unknown parameters.
#[derive(Debug, Default, serde::Deserialize)]
pub(crate) struct EventContextQuery {
    pub(crate) limit: Option<String>,
    /// A `RoomEventFilter` serialised as JSON (D-8). The storage layer's
    /// context window query takes no filter, so this is applied after the
    /// window is fetched — see [`apply_event_filter`].
    pub(crate) filter: Option<String>,
}

/// The subset of `RoomEventFilter` that `/context` supports.
///
/// Matrix defines the filter as generated-by-evolution JSON: unknown keys are
/// ignored, and a missing key means "no constraint".
#[derive(Debug, Default, serde::Deserialize)]
struct RoomEventFilter {
    types: Option<Vec<String>>,
    not_types: Option<Vec<String>>,
    senders: Option<Vec<String>>,
    not_senders: Option<Vec<String>>,
    contains_url: Option<bool>,
    lazy_load_members: Option<bool>,
}

impl RoomEventFilter {
    fn matches_event(&self, event_type: &str, sender: &str, content: &Value) -> bool {
        if !self.includes_type(event_type) {
            return false;
        }
        if let Some(senders) = &self.senders {
            if !senders.iter().any(|pattern| glob_matches(pattern, sender)) {
                return false;
            }
        }
        if let Some(not_senders) = &self.not_senders {
            if not_senders.iter().any(|pattern| glob_matches(pattern, sender)) {
                return false;
            }
        }
        if self.contains_url == Some(true) && !has_url(content) {
            return false;
        }
        true
    }

    /// `types` / `not_types` matching, with the glob syntax the spec mandates.
    fn includes_type(&self, event_type: &str) -> bool {
        if let Some(types) = &self.types {
            if !types.iter().any(|pattern| glob_matches(pattern, event_type)) {
                return false;
            }
        }
        if let Some(not_types) = &self.not_types {
            if not_types.iter().any(|pattern| glob_matches(pattern, event_type)) {
                return false;
            }
        }
        true
    }

    fn lazy_load_members(&self) -> bool {
        self.lazy_load_members == Some(true)
    }
}

/// `*` matches any run of characters, `?` exactly one — the filter glob syntax.
fn glob_matches(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let mut star: Option<usize> = None;
    let mut star_text = 0usize;

    while ti < text.len() {
        if pi < pattern.len() && (pattern[pi] == '?' || pattern[pi] == text[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < pattern.len() && pattern[pi] == '*' {
            star = Some(pi);
            star_text = ti;
            pi += 1;
        } else if let Some(star_pi) = star {
            pi = star_pi + 1;
            star_text += 1;
            ti = star_text;
        } else {
            return false;
        }
    }
    pattern[pi..].iter().all(|c| *c == '*')
}

/// `contains_url` accepts `content.url` or `content.info.url` (both shapes are
/// produced by media-bearing message events).
fn has_url(content: &Value) -> bool {
    content.get("url").and_then(|v| v.as_str()).is_some()
        || content.get("info").and_then(|info| info.get("url")).and_then(|v| v.as_str()).is_some()
}

/// Parse the `filter` query parameter; `None` means "no filter".
fn parse_event_filter(raw: Option<&str>) -> Result<Option<RoomEventFilter>, ApiError> {
    let Some(raw) = raw else { return Ok(None) };
    let filter = serde_json::from_str::<RoomEventFilter>(raw)
        .map_err(|e| ApiError::bad_request(format!("Invalid filter parameter: {e}")))?;
    Ok(Some(filter))
}

/// Apply `filter` to a fetched context window.
///
/// The storage query takes no filter, so the window is fetched with `limit` and
/// then narrowed here; the returned slice can therefore be shorter than `limit`.
fn apply_event_filter<'a>(
    events: &'a [EventContextEntry],
    filter: Option<&RoomEventFilter>,
) -> Vec<&'a EventContextEntry> {
    match filter {
        Some(filter) => events.iter().filter(|e| filter.matches_event(&e.event_type, &e.sender, &e.content)).collect(),
        None => events.iter().collect(),
    }
}

/// The wire shape of one context event.
fn context_event_json(event: &EventContextEntry) -> Value {
    json!({
        "event_id": event.event_id,
        "sender": event.sender,
        "type": event.event_type,
        "content": event.content,
        "origin_server_ts": event.origin_server_ts
    })
}

/// The `GET /rooms/{roomId}/timestamp_to_event` query parameters.
#[derive(Debug, Default, serde::Deserialize)]
pub(crate) struct TimestampToEventQuery {
    pub(crate) ts: Option<String>,
    pub(crate) dir: Option<String>,
}

/// See [`get_event_context`].
pub(crate) async fn get_event_context(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(RoomId, EventId)>,
    Query(query): Query<EventContextQuery>,
) -> Result<Json<Value>, ApiError> {
    let room_id = room_id.replace("%21", "!").replace("%3A", ":");
    let event_id = event_id.replace("%24", "$");

    crate::routes::validate_room_id(&room_id)?;
    crate::routes::validate_event_id(&event_id)?;

    let limit = query.limit.as_deref().and_then(|v| v.parse::<i32>().ok()).unwrap_or(10).clamp(1, 100);

    ensure_room_member_strict_ctx(&ctx, &auth_user, &room_id, "Not a member of this room").await?;

    // `get_event_for_client`: under `invited`/`joined`, a target event from before
    // the caller was involved in the room is reported as not found, so `/context`
    // cannot be used to read pre-join history by event id.
    let target_event =
        ctx.room_service.messaging().get_event_for_client(&room_id, &auth_user.user_id, &event_id).await?;

    let target_ts = target_event.get("origin_server_ts").and_then(|v| v.as_i64()).unwrap_or(0);

    let filter = parse_event_filter(query.filter.as_deref())?;

    let context_window = ctx.search_service.get_event_context_window(&room_id, target_ts, limit as i64).await?;

    let events_before = apply_event_filter(&context_window.events_before, filter.as_ref());
    let events_after = apply_event_filter(&context_window.events_after, filter.as_ref());

    let events_before_list: Vec<Value> = events_before.iter().map(|event| context_event_json(event)).collect();
    let events_after_list: Vec<Value> = events_after.iter().map(|event| context_event_json(event)).collect();

    // With `lazy_load_members` the client is asking for the membership state of
    // exactly the people it can see in this window, instead of the whole room
    // state. Any other filter leaves `state` empty, matching the previous
    // behaviour.
    let state_list: Vec<Value> = if filter.as_ref().is_some_and(RoomEventFilter::lazy_load_members) {
        let mut senders: HashSet<&str> = HashSet::new();
        senders.insert(target_event.get("sender").and_then(|v| v.as_str()).unwrap_or(""));
        for event in events_before.iter().chain(events_after.iter()) {
            senders.insert(event.sender.as_str());
        }
        let state_events = ctx
            .room_service
            .messaging()
            .get_state_events_at_or_before(&room_id, target_ts)
            .await
            .map_err(ApiError::from)?;
        state_events
            .iter()
            .filter(|event| event.event_type.as_deref() == Some("m.room.member"))
            .filter(|event| event.state_key.as_deref().is_some_and(|key| senders.contains(key)))
            .map(|event| {
                json!({
                    "event_id": event.event_id,
                    "sender": event.sender,
                    "type": event.event_type,
                    "state_key": event.state_key,
                    "content": event.content,
                    "origin_server_ts": event.origin_server_ts
                })
            })
            .collect()
    } else {
        Vec::new()
    };

    Ok(Json(json!({
        "event": {
            "event_id": target_event.get("event_id").and_then(|v| v.as_str()).unwrap_or(""),
            "sender": target_event.get("sender").and_then(|v| v.as_str()).unwrap_or(""),
            "type": target_event.get("type").and_then(|v| v.as_str()).unwrap_or(""),
            "content": target_event.get("content"),
            "origin_server_ts": target_ts
        },
        "events_before": events_before_list,
        "events_after": events_after_list,
        "state": state_list,
        "start": events_before_list.last().and_then(|e| e.get("event_id").and_then(|v| v.as_str())).unwrap_or(&event_id),
        "end": events_after_list.last().and_then(|e| e.get("event_id").and_then(|v| v.as_str())).unwrap_or(&event_id)
    })))
}

/// See [`timestamp_to_event`].
pub(crate) async fn timestamp_to_event(
    State(ctx): State<RoomContext>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<RoomId>,
    Query(query): Query<TimestampToEventQuery>,
) -> Result<Json<Value>, ApiError> {
    let ts: i64 = query
        .ts
        .as_deref()
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| ApiError::bad_request("Missing ts parameter".to_string()))?;

    let dir = query.dir.as_deref().unwrap_or("f");

    ensure_room_member_strict_ctx(&ctx, &auth_user, &room_id, "Not a member of this room").await?;

    let direction = if dir == "b" { TimestampDirection::Backward } else { TimestampDirection::Forward };

    let event = ctx.search_service.find_event_by_timestamp(&room_id, ts, direction).await?;

    match event {
        Some(event) => Ok(Json(json!({
            "event_id": event.event_id,
            "origin_server_ts": event.origin_server_ts
        }))),
        None => Err(ApiError::not_found("No event found at this timestamp".to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn entry(event_type: &str, sender: &str, content: Value) -> EventContextEntry {
        EventContextEntry {
            event_id: format!("${event_type}:example.org"),
            sender: sender.to_string(),
            event_type: event_type.to_string(),
            content,
            origin_server_ts: 1,
        }
    }

    #[test]
    fn test_context_response_structure() {
        let response = json!({
            "event": {},
            "events_before": [],
            "events_after": [],
            "state": [],
            "start": "start_token",
            "end": "end_token"
        });

        assert!(response.get("event").is_some());
        assert!(response.get("events_before").is_some());
        assert!(response.get("events_after").is_some());
    }

    #[test]
    fn glob_matches_supports_star_and_question_mark() {
        assert!(glob_matches("m.room.*", "m.room.message"));
        assert!(glob_matches("*", "anything"));
        assert!(glob_matches("m.room.messag?", "m.room.message"));
        assert!(!glob_matches("m.room.messag?", "m.room.messag"));
        assert!(!glob_matches("m.room.messag?", "m.room.messages"));
        assert!(!glob_matches("m.room.message", "m.room.member"));
        assert!(glob_matches("m.*.member", "m.room.member"));
    }

    #[test]
    fn has_url_accepts_both_shapes() {
        assert!(has_url(&json!({ "url": "mxc://example.org/a" })));
        assert!(has_url(&json!({ "info": { "url": "mxc://example.org/a" } })));
        assert!(!has_url(&json!({ "body": "no media here" })));
    }

    #[test]
    fn parse_event_filter_rejects_invalid_json() {
        assert!(parse_event_filter(None).unwrap().is_none());
        assert!(parse_event_filter(Some("not json")).is_err());
    }

    #[test]
    fn apply_event_filter_honours_types_and_senders() {
        let events = vec![
            entry("m.room.message", "@alice:example.org", json!({ "body": "hi" })),
            entry("m.room.member", "@bob:example.org", json!({ "membership": "join" })),
            entry("m.room.message", "@bob:example.org", json!({ "body": "yo" })),
        ];

        let filter = parse_event_filter(Some(r#"{"types":["m.room.*"]}"#)).unwrap();
        let kept = apply_event_filter(&events, filter.as_ref());
        assert_eq!(kept.len(), 3);

        let filter =
            parse_event_filter(Some(r#"{"types":["m.room.message"],"senders":["@bob:example.org"]}"#)).unwrap();
        let kept = apply_event_filter(&events, filter.as_ref());
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].sender, "@bob:example.org");
    }

    #[test]
    fn apply_event_filter_without_filter_keeps_everything() {
        let events = vec![entry("m.room.message", "@alice:example.org", json!({}))];
        assert_eq!(apply_event_filter(&events, None).len(), 1);
    }
}
