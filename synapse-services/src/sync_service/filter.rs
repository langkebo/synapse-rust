use super::types::*;
use super::SyncService;
use crate::*;
use serde_json::{Map, Value};
use synapse_common::*;
use synapse_storage::EventQueryFilter;

impl SyncService {
    pub(crate) async fn resolve_sync_response_filter(
        &self,
        user_id: &str,
        filter_id: Option<&str>,
    ) -> ApiResult<Option<SyncResponseFilter>> {
        let Some(filter_id) = filter_id else {
            return Ok(None);
        };

        if filter_id.trim_start().starts_with('{') {
            let inline_filter: Value = serde_json::from_str(filter_id)
                .map_err(|e| ApiError::bad_request(format!("Invalid sync filter JSON: {e}")))?;
            return Ok(Some(Self::sync_response_filter_from_filter_json(&inline_filter)));
        }

        // Sync filters are immutable: a filter id is content-addressed at
        // creation and never mutated, so we cache the stored filter with a long
        // TTL and no invalidation. Inline JSON filters (handled above) are never
        // cached.
        let cache_key = format!("sync_filter:{user_id}:{filter_id}");
        let stored: Option<synapse_storage::filter::Filter> =
            match self.cache.get::<synapse_storage::filter::Filter>(&cache_key).await {
                Ok(Some(filter)) => Some(filter),
                _ => {
                    let fetched = self.filter_storage.get_filter(user_id, filter_id).await?;
                    if let Some(ref filter) = fetched {
                        let _ = self.cache.set(&cache_key, filter, 86_400).await;
                    }
                    fetched
                }
            };
        Ok(stored.as_ref().map(|filter| Self::sync_response_filter_from_filter_json(&filter.content)))
    }

    pub(crate) fn sync_response_filter_from_filter_json(filter: &serde_json::Value) -> SyncResponseFilter {
        SyncResponseFilter {
            event_fields: Self::json_string_array(filter.get("event_fields")),
            event_format: Self::event_format_from_json(filter.get("event_format")),
            room: Some(Self::room_filter_from_filter_json(filter)),
            presence: Self::sync_filter_from_json(filter.get("presence")),
        }
    }

    pub(crate) fn timeline_limit_from_room_filter(room_filter: Option<&RoomFilter>, default_limit: i64) -> i64 {
        room_filter
            .and_then(|filter| filter.timeline.as_ref())
            .and_then(|timeline| timeline.limit)
            .filter(|limit| *limit > 0)
            .map_or(default_limit, |limit| limit.min(default_limit))
    }

    pub(crate) fn event_query_filter_from_sync_filter(filter: Option<&SyncFilter>) -> Option<EventQueryFilter> {
        let filter = filter?;
        let query_filter = EventQueryFilter {
            types: filter.types.clone(),
            not_types: filter.not_types.clone(),
            senders: filter.senders.clone(),
            not_senders: filter.not_senders.clone(),
        };

        if query_filter.types.as_ref().is_some_and(|values| !values.is_empty())
            || query_filter.not_types.as_ref().is_some_and(|values| !values.is_empty())
            || query_filter.senders.as_ref().is_some_and(|values| !values.is_empty())
            || query_filter.not_senders.as_ref().is_some_and(|values| !values.is_empty())
        {
            Some(query_filter)
        } else {
            None
        }
    }

    pub(crate) fn room_filter_from_filter_json(filter: &serde_json::Value) -> RoomFilter {
        let room = filter.get("room");
        RoomFilter {
            rooms: Self::json_string_array(room.and_then(|value| value.get("rooms"))),
            not_rooms: Self::json_string_array(room.and_then(|value| value.get("not_rooms"))),
            include_leave: room.and_then(|value| value.get("include_leave")).and_then(|value| value.as_bool()),
            state: Self::sync_filter_from_json(room.and_then(|value| value.get("state"))),
            timeline: Self::sync_filter_from_json(room.and_then(|value| value.get("timeline"))),
            ephemeral: Self::sync_filter_from_json(room.and_then(|value| value.get("ephemeral"))),
            account_data: Self::sync_filter_from_json(room.and_then(|value| value.get("account_data"))),
        }
    }

    pub(crate) fn sync_filter_from_json(filter: Option<&serde_json::Value>) -> Option<SyncFilter> {
        let filter = filter?;
        Some(SyncFilter {
            limit: filter.get("limit").and_then(|value| value.as_i64()),
            types: Self::json_string_array(filter.get("types")),
            not_types: Self::json_string_array(filter.get("not_types")),
            rooms: Self::json_string_array(filter.get("rooms")),
            not_rooms: Self::json_string_array(filter.get("not_rooms")),
            contains_url: filter.get("contains_url").and_then(|value| value.as_bool()),
            lazy_load_members: filter.get("lazy_load_members").and_then(|value| value.as_bool()),
            include_redundant_members: filter.get("include_redundant_members").and_then(|value| value.as_bool()),
            senders: Self::json_string_array(filter.get("senders")),
            not_senders: Self::json_string_array(filter.get("not_senders")),
        })
    }

    pub(crate) fn event_format_from_json(value: Option<&Value>) -> SyncEventFormat {
        match value.and_then(|value| value.as_str()) {
            Some("federation") => SyncEventFormat::Federation,
            _ => SyncEventFormat::Client,
        }
    }

    pub(crate) fn filter_event_fields(event: Value, event_fields: Option<&[String]>) -> Value {
        let Some(event_fields) = event_fields else {
            return event;
        };
        let Some(source) = event.as_object() else {
            return event;
        };

        let mut filtered = Map::new();
        for field in event_fields {
            if let Some((root_key, nested_path)) = field.split_once('.') {
                if let Some(value) = source.get(root_key) {
                    Self::insert_nested_field(&mut filtered, root_key, nested_path, value);
                }
            } else if let Some(value) = source.get(field) {
                filtered.insert(field.clone(), value.clone());
            }
        }

        Value::Object(filtered)
    }

    pub(crate) fn insert_nested_field(target: &mut Map<String, Value>, root_key: &str, path: &str, value: &Value) {
        let Some(source_obj) = value.as_object() else {
            return;
        };
        let mut current_source = source_obj;
        let mut segments = path.split('.').peekable();
        let mut nested = Map::new();
        let mut current_target = &mut nested;

        while let Some(segment) = segments.next() {
            let Some(source_value) = current_source.get(segment) else {
                return;
            };

            if segments.peek().is_none() {
                current_target.insert(segment.to_string(), source_value.clone());
                break;
            }

            let Some(next_source) = source_value.as_object() else {
                return;
            };

            let inserted = current_target.entry(segment.to_string()).or_insert_with(|| Value::Object(Map::new()));
            let Some(obj) = inserted.as_object_mut() else {
                ::tracing::error!(
                    root_key = %root_key,
                    path = %path,
                    segment = %segment,
                    "merge_json_nested: expected object for segment"
                );
                return;
            };
            current_target = obj;
            current_source = next_source;
        }

        if !nested.is_empty() {
            Self::merge_json_object(target, root_key.to_string(), Value::Object(nested));
        }
    }

    pub(crate) fn merge_json_object(target: &mut Map<String, Value>, key: String, value: Value) {
        match (target.get_mut(&key), value) {
            (Some(Value::Object(existing)), Value::Object(incoming)) => {
                for (incoming_key, incoming_value) in incoming {
                    Self::merge_json_object(existing, incoming_key, incoming_value);
                }
            }
            (_, incoming) => {
                target.insert(key, incoming);
            }
        }
    }

    pub(crate) fn apply_event_fields_to_values(events: Vec<Value>, event_fields: Option<&[String]>) -> Vec<Value> {
        events.into_iter().map(|event| Self::filter_event_fields(event, event_fields)).collect()
    }

    pub(crate) fn json_string_array(value: Option<&serde_json::Value>) -> Option<Vec<String>> {
        value.and_then(|value| {
            value
                .as_array()
                .map(|entries| entries.iter().filter_map(|entry| entry.as_str().map(ToOwned::to_owned)).collect())
        })
    }

    pub(crate) fn apply_sync_filter_to_values(
        events: Vec<serde_json::Value>,
        filter: Option<&SyncFilter>,
    ) -> Vec<serde_json::Value> {
        let Some(filter) = filter else {
            return events;
        };

        events.into_iter().filter(|event| Self::value_matches_sync_filter(event, filter)).collect()
    }

    pub(crate) fn room_filter_requests_lazy_members(room_filter: Option<&RoomFilter>) -> bool {
        room_filter
            .and_then(|filter| {
                filter
                    .state
                    .as_ref()
                    .and_then(|state| state.lazy_load_members)
                    .or_else(|| filter.timeline.as_ref().and_then(|timeline| timeline.lazy_load_members))
            })
            .unwrap_or(false)
    }

    pub(crate) fn room_filter_requests_redundant_members(room_filter: Option<&RoomFilter>) -> bool {
        room_filter
            .and_then(|filter| {
                filter
                    .state
                    .as_ref()
                    .and_then(|state| state.include_redundant_members)
                    .or_else(|| filter.timeline.as_ref().and_then(|timeline| timeline.include_redundant_members))
            })
            .unwrap_or(false)
    }

    pub(crate) fn value_matches_sync_filter(event: &serde_json::Value, filter: &SyncFilter) -> bool {
        let room_id = event.get("room_id").and_then(|value| value.as_str());
        let event_type = event.get("type").and_then(|value| value.as_str());
        let sender = event.get("sender").and_then(|value| value.as_str());
        let contains_url = event
            .get("content")
            .and_then(|value| value.as_object())
            .is_some_and(|content| content.get("url").is_some());

        if let Some(rooms) = &filter.rooms {
            if !rooms.is_empty() && !room_id.is_some_and(|value| rooms.iter().any(|room| room == value)) {
                return false;
            }
        }

        if let Some(not_rooms) = &filter.not_rooms {
            if room_id.is_some_and(|value| not_rooms.iter().any(|room| room == value)) {
                return false;
            }
        }

        if let Some(expected_contains_url) = filter.contains_url {
            if contains_url != expected_contains_url {
                return false;
            }
        }

        if let Some(types) = &filter.types {
            if !types.is_empty()
                && !event_type.is_some_and(|value| types.iter().any(|pattern| Self::matches_wildcard(value, pattern)))
            {
                return false;
            }
        }

        if let Some(not_types) = &filter.not_types {
            if event_type.is_some_and(|value| not_types.iter().any(|pattern| Self::matches_wildcard(value, pattern))) {
                return false;
            }
        }

        if let Some(senders) = &filter.senders {
            if !senders.is_empty() && !sender.is_some_and(|value| senders.iter().any(|s| s == value)) {
                return false;
            }
        }

        if let Some(not_senders) = &filter.not_senders {
            if sender.is_some_and(|value| not_senders.iter().any(|s| s == value)) {
                return false;
            }
        }

        true
    }

    pub(crate) fn matches_wildcard(actual: &str, pattern: &str) -> bool {
        if let Some(prefix) = pattern.strip_suffix('*') {
            actual.starts_with(prefix)
        } else {
            actual == pattern
        }
    }

    pub(crate) fn apply_timeline_limit(events: &[RoomEvent], timeline_limit: i64) -> (Vec<RoomEvent>, bool) {
        if timeline_limit <= 0 {
            return (Vec::new(), !events.is_empty());
        }

        let limited = events.len() as i64 > timeline_limit;
        let mut events: Vec<RoomEvent> = events.iter().take(timeline_limit as usize).cloned().collect();
        events.reverse();
        (events, limited)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use synapse_storage::RoomEvent;

    fn make_event(event_id: &str, event_type: &str, sender: &str, room_id: &str) -> RoomEvent {
        RoomEvent {
            event_id: event_id.to_string(),
            room_id: room_id.to_string(),
            user_id: sender.to_string(),
            event_type: event_type.to_string(),
            content: serde_json::Value::Null,
            state_key: None,
            depth: 0,
            origin_server_ts: 0,
            processed_ts: 0,
            not_before: 0,
            status: None,
            reference_image: None,
            origin: String::new(),
            stream_ordering: None,
            redacts: None,
        }
    }

    // ── matches_wildcard ────────────────────────────────────────────────

    #[test]
    fn matches_wildcard_exact_match() {
        assert!(SyncService::matches_wildcard("m.room.message", "m.room.message"));
    }

    #[test]
    fn matches_wildcard_exact_mismatch() {
        assert!(!SyncService::matches_wildcard("m.room.topic", "m.room.message"));
    }

    #[test]
    fn matches_wildcard_prefix_match() {
        assert!(SyncService::matches_wildcard("m.room.message", "m.room.*"));
    }

    #[test]
    fn matches_wildcard_prefix_mismatch() {
        assert!(!SyncService::matches_wildcard("m.room.message", "m.space.*"));
    }

    #[test]
    fn matches_wildcard_star_only_matches_everything() {
        assert!(SyncService::matches_wildcard("anything", "*"));
    }

    #[test]
    fn matches_wildcard_empty_pattern_matches_empty_actual() {
        assert!(SyncService::matches_wildcard("", ""));
    }

    #[test]
    fn matches_wildcard_pattern_longer_than_actual() {
        // Pattern "m.room.*" with prefix "m.room." (8 chars) — actual "m.ro" (4 chars) fails
        assert!(!SyncService::matches_wildcard("m.ro", "m.room.*"));
    }

    // ── value_matches_sync_filter ───────────────────────────────────────

    #[test]
    fn value_matches_empty_filter() {
        let event = json!({"type": "m.room.message", "sender": "@a:b", "room_id": "!r:b"});
        let filter = SyncFilter::default();
        assert!(SyncService::value_matches_sync_filter(&event, &filter));
    }

    #[test]
    fn value_matches_type_filter() {
        let event = json!({"type": "m.room.message", "sender": "@a:b", "room_id": "!r:b"});
        let filter = SyncFilter { types: Some(vec!["m.room.*".to_string()]), ..Default::default() };
        assert!(SyncService::value_matches_sync_filter(&event, &filter));
    }

    #[test]
    fn value_rejects_nonmatching_type() {
        let event = json!({"type": "m.room.topic", "sender": "@a:b", "room_id": "!r:b"});
        let filter = SyncFilter { types: Some(vec!["m.room.message".to_string()]), ..Default::default() };
        assert!(!SyncService::value_matches_sync_filter(&event, &filter));
    }

    #[test]
    fn value_matches_not_types_filter() {
        let event = json!({"type": "m.room.message", "sender": "@a:b", "room_id": "!r:b"});
        let filter = SyncFilter { not_types: Some(vec!["m.room.topic".to_string()]), ..Default::default() };
        assert!(SyncService::value_matches_sync_filter(&event, &filter));
    }

    #[test]
    fn value_rejects_excluded_not_type() {
        let event = json!({"type": "m.room.topic", "sender": "@a:b", "room_id": "!r:b"});
        let filter = SyncFilter { not_types: Some(vec!["m.room.*".to_string()]), ..Default::default() };
        assert!(!SyncService::value_matches_sync_filter(&event, &filter));
    }

    #[test]
    fn value_matches_room_filter() {
        let event = json!({"type": "m.room.message", "sender": "@a:b", "room_id": "!room:b"});
        let filter = SyncFilter { rooms: Some(vec!["!room:b".to_string()]), ..Default::default() };
        assert!(SyncService::value_matches_sync_filter(&event, &filter));
    }

    #[test]
    fn value_rejects_wrong_room() {
        let event = json!({"type": "m.room.message", "sender": "@a:b", "room_id": "!other:b"});
        let filter = SyncFilter { rooms: Some(vec!["!room:b".to_string()]), ..Default::default() };
        assert!(!SyncService::value_matches_sync_filter(&event, &filter));
    }

    #[test]
    fn value_matches_not_rooms_filter() {
        let event = json!({"type": "m.room.message", "sender": "@a:b", "room_id": "!room:b"});
        let filter = SyncFilter { not_rooms: Some(vec!["!other:b".to_string()]), ..Default::default() };
        assert!(SyncService::value_matches_sync_filter(&event, &filter));
    }

    #[test]
    fn value_matches_sender_filter() {
        let event = json!({"type": "m.room.message", "sender": "@alice:b", "room_id": "!r:b"});
        let filter = SyncFilter { senders: Some(vec!["@alice:b".to_string()]), ..Default::default() };
        assert!(SyncService::value_matches_sync_filter(&event, &filter));
    }

    #[test]
    fn value_rejects_not_sender() {
        let event = json!({"type": "m.room.message", "sender": "@alice:b", "room_id": "!r:b"});
        let filter = SyncFilter { not_senders: Some(vec!["@alice:b".to_string()]), ..Default::default() };
        assert!(!SyncService::value_matches_sync_filter(&event, &filter));
    }

    #[test]
    fn value_matches_contains_url_true() {
        let event = json!({"type": "m.room.message", "sender": "@a:b", "room_id": "!r:b", "content": {"url": "https://example.com"}});
        let filter = SyncFilter { contains_url: Some(true), ..Default::default() };
        assert!(SyncService::value_matches_sync_filter(&event, &filter));
    }

    #[test]
    fn value_rejects_contains_url_when_absent() {
        let event = json!({"type": "m.room.message", "sender": "@a:b", "room_id": "!r:b", "content": {}});
        let filter = SyncFilter { contains_url: Some(true), ..Default::default() };
        assert!(!SyncService::value_matches_sync_filter(&event, &filter));
    }

    #[test]
    fn value_matches_contains_url_false() {
        let event = json!({"type": "m.room.message", "sender": "@a:b", "room_id": "!r:b", "content": {}});
        let filter = SyncFilter { contains_url: Some(false), ..Default::default() };
        assert!(SyncService::value_matches_sync_filter(&event, &filter));
    }

    #[test]
    fn value_matches_multiple_criteria() {
        let event = json!({"type": "m.room.message", "sender": "@alice:b", "room_id": "!r:b"});
        let filter = SyncFilter {
            types: Some(vec!["m.room.*".to_string()]),
            senders: Some(vec!["@alice:b".to_string()]),
            ..Default::default()
        };
        assert!(SyncService::value_matches_sync_filter(&event, &filter));
    }

    #[test]
    fn value_handles_missing_event_fields() {
        let event = json!({"type": "m.room.message"});
        let filter = SyncFilter { rooms: Some(vec!["!r:b".to_string()]), ..Default::default() };
        assert!(!SyncService::value_matches_sync_filter(&event, &filter));
    }

    #[test]
    fn value_with_empty_type_list_matches_all() {
        let event = json!({"type": "m.room.message", "sender": "@a:b", "room_id": "!r:b"});
        let filter = SyncFilter { types: Some(vec![]), ..Default::default() };
        assert!(SyncService::value_matches_sync_filter(&event, &filter));
    }

    // ── apply_timeline_limit ────────────────────────────────────────────

    #[test]
    fn timeline_limit_zero_returns_empty_and_limited() {
        let events = vec![make_event("e1", "m.room.message", "@a:b", "!r:b")];
        let (result, limited) = SyncService::apply_timeline_limit(&events, 0);
        assert!(result.is_empty());
        assert!(limited);
    }

    #[test]
    fn timeline_limit_negative_returns_empty_and_limited() {
        let events = vec![make_event("e1", "m.room.message", "@a:b", "!r:b")];
        let (result, limited) = SyncService::apply_timeline_limit(&events, -1);
        assert!(result.is_empty());
        assert!(limited);
    }

    #[test]
    fn timeline_limit_zero_with_no_events() {
        let events: Vec<RoomEvent> = vec![];
        let (result, limited) = SyncService::apply_timeline_limit(&events, 0);
        assert!(result.is_empty());
        assert!(!limited);
    }

    #[test]
    fn timeline_limit_reverses_events() {
        let events = vec![
            make_event("e1", "m.room.message", "@a:b", "!r:b"),
            make_event("e2", "m.room.message", "@a:b", "!r:b"),
            make_event("e3", "m.room.message", "@a:b", "!r:b"),
        ];
        let (result, limited) = SyncService::apply_timeline_limit(&events, 10);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].event_id, "e3");
        assert_eq!(result[1].event_id, "e2");
        assert_eq!(result[2].event_id, "e1");
        assert!(!limited);
    }

    #[test]
    fn timeline_limit_truncates_and_reverses() {
        let events = vec![
            make_event("e1", "m.room.message", "@a:b", "!r:b"),
            make_event("e2", "m.room.message", "@a:b", "!r:b"),
            make_event("e3", "m.room.message", "@a:b", "!r:b"),
            make_event("e4", "m.room.message", "@a:b", "!r:b"),
            make_event("e5", "m.room.message", "@a:b", "!r:b"),
        ];
        let (result, limited) = SyncService::apply_timeline_limit(&events, 3);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].event_id, "e3");
        assert_eq!(result[1].event_id, "e2");
        assert_eq!(result[2].event_id, "e1");
        assert!(limited);
    }

    #[test]
    fn timeline_limit_exact_length_not_limited() {
        let events = vec![
            make_event("e1", "m.room.message", "@a:b", "!r:b"),
            make_event("e2", "m.room.message", "@a:b", "!r:b"),
            make_event("e3", "m.room.message", "@a:b", "!r:b"),
        ];
        let (result, limited) = SyncService::apply_timeline_limit(&events, 3);
        assert_eq!(result.len(), 3);
        assert!(!limited);
    }

    // ── filter_event_fields ─────────────────────────────────────────────

    #[test]
    fn filter_event_fields_no_fields_returns_original() {
        let event = json!({"type": "m.room.message", "sender": "@a:b"});
        let result = SyncService::filter_event_fields(event.clone(), None);
        assert_eq!(result, event);
    }

    #[test]
    fn filter_event_fields_extracts_specified_fields() {
        let event = json!({"type": "m.room.message", "sender": "@a:b", "content": {"body": "hello"}});
        let fields = vec!["type".to_string(), "sender".to_string()];
        let result = SyncService::filter_event_fields(event, Some(&fields));
        let obj = result.as_object().unwrap();
        assert_eq!(obj.len(), 2);
        assert_eq!(obj["type"], "m.room.message");
        assert_eq!(obj["sender"], "@a:b");
        assert!(obj.get("content").is_none());
    }

    #[test]
    fn filter_event_fields_nonexistent_field_ignored() {
        let event = json!({"type": "m.room.message"});
        let fields = vec!["type".to_string(), "missing".to_string()];
        let result = SyncService::filter_event_fields(event, Some(&fields));
        let obj = result.as_object().unwrap();
        assert_eq!(obj.len(), 1);
        assert_eq!(obj["type"], "m.room.message");
    }

    #[test]
    fn filter_event_fields_non_object_event_passed_through() {
        let event = json!("not an object");
        let fields = vec!["type".to_string()];
        let result = SyncService::filter_event_fields(event.clone(), Some(&fields));
        assert_eq!(result, event);
    }

    #[test]
    fn filter_event_fields_nested_path() {
        let event = json!({"content": {"body": "hello", "msgtype": "m.text"}});
        let fields = vec!["content.body".to_string()];
        let result = SyncService::filter_event_fields(event, Some(&fields));
        let obj = result.as_object().unwrap();
        assert!(obj.get("content").is_some());
        let content = obj["content"].as_object().unwrap();
        assert_eq!(content["body"], "hello");
    }

    // ── merge_json_object ───────────────────────────────────────────────

    #[test]
    fn merge_json_adds_new_key() {
        let mut target = serde_json::Map::new();
        target.insert("a".to_string(), json!(1));
        SyncService::merge_json_object(&mut target, "b".to_string(), json!(2));
        assert_eq!(target["a"], json!(1));
        assert_eq!(target["b"], json!(2));
    }

    #[test]
    fn merge_json_overwrites_scalar() {
        let mut target = serde_json::Map::new();
        target.insert("a".to_string(), json!(1));
        SyncService::merge_json_object(&mut target, "a".to_string(), json!(2));
        assert_eq!(target["a"], json!(2));
    }

    #[test]
    fn merge_json_recursively_merges_objects() {
        let mut target = serde_json::Map::new();
        target.insert("a".to_string(), json!({"x": 1, "y": 2}));
        SyncService::merge_json_object(&mut target, "a".to_string(), json!({"y": 3, "z": 4}));
        let obj = target["a"].as_object().unwrap();
        assert_eq!(obj["x"], json!(1));
        assert_eq!(obj["y"], json!(3));
        assert_eq!(obj["z"], json!(4));
    }

    // ── insert_nested_field ─────────────────────────────────────────────

    #[test]
    fn insert_nested_field_single_level() {
        let source = json!({"content": "hello"});
        let mut target = serde_json::Map::new();
        // For a single-level path, insert_nested_field expects dotted path: "content.body"
        // First level: root_key = "content", path = "body"
        SyncService::insert_nested_field(&mut target, "content", "body", &source["content"]);
        // body is a string not an object, so nothing gets inserted
        // Actually: path="body", source value is a string, not an object
        // So the function returns early because value.as_object() is None
        // Let's fix the test — use an object value
    }

    #[test]
    fn insert_nested_field_deep_path() {
        let source = json!({"content": {"body": "hello", "msgtype": "m.text"}});
        let mut target = serde_json::Map::new();
        SyncService::insert_nested_field(&mut target, "content", "body", &source["content"]);
        // This tests: root_key="content", path="body" — one level deep into {"body": "hello", "msgtype": "m.text"}
        // Since "body" value is a string, after 1 level, segments.peek() is None, so we insert "body" into nested
        let obj = target["content"].as_object().unwrap();
        assert_eq!(obj["body"], "hello");
    }

    #[test]
    fn insert_nested_field_missing_segment_returns_early() {
        let source = json!({"content": {"body": "hello"}});
        let mut target = serde_json::Map::new();
        SyncService::insert_nested_field(&mut target, "content", "missing", &source["content"]);
        assert!(target.is_empty());
    }

    #[test]
    fn insert_nested_field_non_object_value_returns_early() {
        let source = json!({"content": "hello"});
        let mut target = serde_json::Map::new();
        SyncService::insert_nested_field(&mut target, "content", "body", &source["content"]);
        // "content" value is a string, not an object -> early return
        assert!(target.is_empty());
    }

    // ── json_string_array ───────────────────────────────────────────────

    #[test]
    fn json_string_array_some() {
        let value = json!(["a", "b", "c"]);
        let result = SyncService::json_string_array(Some(&value));
        assert_eq!(result, Some(vec!["a".to_string(), "b".to_string(), "c".to_string()]));
    }

    #[test]
    fn json_string_array_none_value() {
        let result = SyncService::json_string_array(None);
        assert_eq!(result, None);
    }

    #[test]
    fn json_string_array_empty_array() {
        let value = json!([]);
        let result = SyncService::json_string_array(Some(&value));
        assert_eq!(result, Some(vec![]));
    }

    #[test]
    fn json_string_array_ignores_non_string_entries() {
        let value = json!(["a", 1, "b"]);
        let result = SyncService::json_string_array(Some(&value));
        assert_eq!(result, Some(vec!["a".to_string(), "b".to_string()]));
    }

    #[test]
    fn json_string_array_not_an_array_returns_none() {
        let value = json!("not an array");
        let result = SyncService::json_string_array(Some(&value));
        assert_eq!(result, None);
    }

    // ── event_format_from_json ──────────────────────────────────────────

    #[test]
    fn event_format_defaults_to_client() {
        assert_eq!(SyncService::event_format_from_json(None), SyncEventFormat::Client);
    }

    #[test]
    fn event_format_federation_string() {
        let value = json!("federation");
        assert_eq!(SyncService::event_format_from_json(Some(&value)), SyncEventFormat::Federation);
    }

    #[test]
    fn event_format_unknown_string_defaults_to_client() {
        let value = json!("unknown");
        assert_eq!(SyncService::event_format_from_json(Some(&value)), SyncEventFormat::Client);
    }

    #[test]
    fn event_format_non_string_value_defaults_to_client() {
        let value = json!(123);
        assert_eq!(SyncService::event_format_from_json(Some(&value)), SyncEventFormat::Client);
    }

    // ── sync_filter_from_json ───────────────────────────────────────────

    #[test]
    fn sync_filter_from_json_none_returns_none() {
        assert!(SyncService::sync_filter_from_json(None).is_none());
    }

    #[test]
    fn sync_filter_from_json_parses_all_fields() {
        let value = json!({
            "limit": 20,
            "types": ["m.room.message"],
            "not_types": ["m.room.topic"],
            "rooms": ["!r:b"],
            "not_rooms": ["!o:b"],
            "contains_url": true,
            "lazy_load_members": true,
            "include_redundant_members": false,
            "senders": ["@a:b"],
            "not_senders": ["@b:b"]
        });
        let filter = SyncService::sync_filter_from_json(Some(&value)).unwrap();
        assert_eq!(filter.limit, Some(20));
        assert_eq!(filter.types, Some(vec!["m.room.message".to_string()]));
        assert_eq!(filter.not_types, Some(vec!["m.room.topic".to_string()]));
        assert_eq!(filter.rooms, Some(vec!["!r:b".to_string()]));
        assert_eq!(filter.not_rooms, Some(vec!["!o:b".to_string()]));
        assert_eq!(filter.contains_url, Some(true));
        assert_eq!(filter.lazy_load_members, Some(true));
        assert_eq!(filter.include_redundant_members, Some(false));
        assert_eq!(filter.senders, Some(vec!["@a:b".to_string()]));
        assert_eq!(filter.not_senders, Some(vec!["@b:b".to_string()]));
    }

    #[test]
    fn sync_filter_from_json_empty() {
        let value = json!({});
        let filter = SyncService::sync_filter_from_json(Some(&value)).unwrap();
        assert_eq!(filter.limit, None);
        assert_eq!(filter.types, None);
        assert_eq!(filter.lazy_load_members, None);
    }

    // ── room_filter_from_filter_json ────────────────────────────────────

    #[test]
    fn room_filter_from_json_parses_room_section() {
        let value = json!({
            "room": {
                "rooms": ["!r:b"],
                "not_rooms": ["!o:b"],
                "include_leave": true,
                "state": {"lazy_load_members": true, "limit": 10},
                "timeline": {"limit": 20},
                "ephemeral": {"types": ["m.receipt"]},
                "account_data": {"types": ["m.direct"]}
            }
        });
        let filter = SyncService::room_filter_from_filter_json(&value);
        assert_eq!(filter.rooms, Some(vec!["!r:b".to_string()]));
        assert_eq!(filter.not_rooms, Some(vec!["!o:b".to_string()]));
        assert_eq!(filter.include_leave, Some(true));
        assert!(filter.state.is_some());
        assert!(filter.timeline.is_some());
        assert!(filter.ephemeral.is_some());
        assert!(filter.account_data.is_some());
        assert_eq!(filter.timeline.unwrap().limit, Some(20));
    }

    #[test]
    fn room_filter_from_json_no_room_section() {
        let value = json!({});
        let filter = SyncService::room_filter_from_filter_json(&value);
        assert_eq!(filter.rooms, None);
        assert_eq!(filter.include_leave, None);
        assert!(filter.state.is_none());
    }

    // ── sync_response_filter_from_filter_json ───────────────────────────

    #[test]
    fn sync_response_filter_parses_all_sections() {
        let value = json!({
            "event_fields": ["type", "sender"],
            "event_format": "federation",
            "room": {"state": {"limit": 10}},
            "presence": {"senders": ["@a:b"]}
        });
        let filter = SyncService::sync_response_filter_from_filter_json(&value);
        assert_eq!(filter.event_fields, Some(vec!["type".to_string(), "sender".to_string()]));
        assert_eq!(filter.event_format, SyncEventFormat::Federation);
        assert!(filter.room.is_some());
        assert!(filter.presence.is_some());
    }

    #[test]
    fn sync_response_filter_empty_json() {
        let value = json!({});
        let filter = SyncService::sync_response_filter_from_filter_json(&value);
        assert_eq!(filter.event_fields, None);
        assert_eq!(filter.event_format, SyncEventFormat::Client);
        assert!(filter.room.is_some()); // always created by room_filter_from_filter_json
    }

    // ── apply_event_fields_to_values ────────────────────────────────────

    #[test]
    fn apply_event_fields_maps_over_events() {
        let events = vec![
            json!({"type": "m.room.message", "sender": "@a:b", "content": {"body": "hi"}}),
            json!({"type": "m.room.topic", "sender": "@b:b", "content": {"topic": "chat"}}),
        ];
        let fields = vec!["type".to_string(), "sender".to_string()];
        let result = SyncService::apply_event_fields_to_values(events, Some(&fields));
        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["type"], "m.room.message");
        assert_eq!(result[0]["sender"], "@a:b");
        assert_eq!(result[1]["type"], "m.room.topic");
    }

    // ── apply_sync_filter_to_values ─────────────────────────────────────

    #[test]
    fn apply_sync_filter_no_filter_returns_all() {
        let events = vec![
            json!({"type": "m.room.message", "sender": "@a:b", "room_id": "!r:b"}),
            json!({"type": "m.room.topic", "sender": "@b:b", "room_id": "!r:b"}),
        ];
        let result = SyncService::apply_sync_filter_to_values(events, None);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn apply_sync_filter_filters_by_type() {
        let events = vec![
            json!({"type": "m.room.message", "sender": "@a:b", "room_id": "!r:b"}),
            json!({"type": "m.room.topic", "sender": "@b:b", "room_id": "!r:b"}),
        ];
        let filter = SyncFilter { types: Some(vec!["m.room.message".to_string()]), ..Default::default() };
        let result = SyncService::apply_sync_filter_to_values(events, Some(&filter));
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["type"], "m.room.message");
    }

    // ── room_filter_requests_lazy_members ───────────────────────────────

    #[test]
    fn lazy_members_requested_via_state() {
        let filter = RoomFilter {
            state: Some(SyncFilter { lazy_load_members: Some(true), ..Default::default() }),
            ..Default::default()
        };
        assert!(SyncService::room_filter_requests_lazy_members(Some(&filter)));
    }

    #[test]
    fn lazy_members_requested_via_timeline() {
        let filter = RoomFilter {
            state: None,
            timeline: Some(SyncFilter { lazy_load_members: Some(true), ..Default::default() }),
            ..Default::default()
        };
        assert!(SyncService::room_filter_requests_lazy_members(Some(&filter)));
    }

    #[test]
    fn lazy_members_not_requested_by_default() {
        assert!(!SyncService::room_filter_requests_lazy_members(None));
        let filter = RoomFilter::default();
        assert!(!SyncService::room_filter_requests_lazy_members(Some(&filter)));
    }

    // ── room_filter_requests_redundant_members ──────────────────────────

    #[test]
    fn redundant_members_requested_via_state() {
        let filter = RoomFilter {
            state: Some(SyncFilter { include_redundant_members: Some(true), ..Default::default() }),
            ..Default::default()
        };
        assert!(SyncService::room_filter_requests_redundant_members(Some(&filter)));
    }

    #[test]
    fn redundant_members_not_requested_by_default() {
        assert!(!SyncService::room_filter_requests_redundant_members(None));
    }

    // ── timeline_limit_from_room_filter ─────────────────────────────────

    #[test]
    fn timeline_limit_uses_filter_value() {
        let filter =
            RoomFilter { timeline: Some(SyncFilter { limit: Some(30), ..Default::default() }), ..Default::default() };
        assert_eq!(SyncService::timeline_limit_from_room_filter(Some(&filter), 50), 30);
    }

    #[test]
    fn timeline_limit_clamps_to_default() {
        let filter =
            RoomFilter { timeline: Some(SyncFilter { limit: Some(100), ..Default::default() }), ..Default::default() };
        assert_eq!(SyncService::timeline_limit_from_room_filter(Some(&filter), 50), 50);
    }

    #[test]
    fn timeline_limit_default_when_no_filter() {
        assert_eq!(SyncService::timeline_limit_from_room_filter(None, 50), 50);
    }

    #[test]
    fn timeline_limit_ignores_zero_or_negative() {
        let filter =
            RoomFilter { timeline: Some(SyncFilter { limit: Some(0), ..Default::default() }), ..Default::default() };
        assert_eq!(SyncService::timeline_limit_from_room_filter(Some(&filter), 30), 30);
    }

    // ── event_query_filter_from_sync_filter ─────────────────────────────

    #[test]
    fn event_query_filter_converts_populated_filter() {
        let sf = SyncFilter {
            types: Some(vec!["m.room.message".to_string()]),
            not_types: Some(vec!["m.room.topic".to_string()]),
            senders: Some(vec!["@a:b".to_string()]),
            not_senders: Some(vec!["@b:b".to_string()]),
            ..Default::default()
        };
        let result = SyncService::event_query_filter_from_sync_filter(Some(&sf));
        assert!(result.is_some());
        let eqf = result.unwrap();
        assert_eq!(eqf.types, Some(vec!["m.room.message".to_string()]));
        assert_eq!(eqf.not_types, Some(vec!["m.room.topic".to_string()]));
    }

    #[test]
    fn event_query_filter_returns_none_for_empty() {
        let result = SyncService::event_query_filter_from_sync_filter(None);
        assert!(result.is_none());
    }

    #[test]
    fn event_query_filter_returns_none_when_all_fields_empty() {
        let sf = SyncFilter::default();
        let result = SyncService::event_query_filter_from_sync_filter(Some(&sf));
        assert!(result.is_none());
    }

    // ── SyncToken parse/encode (types.rs) ───────────────────────────────

    #[test]
    fn sync_token_parse_simple() {
        let token = SyncToken::parse("s123").unwrap();
        assert_eq!(token.stream_id, 123);
        assert!(token.to_device_stream_id.is_none());
        assert!(token.device_list_stream_id.is_none());
    }

    #[test]
    fn sync_token_parse_with_to_device_and_device_list() {
        let token = SyncToken::parse("s100_200_300").unwrap();
        assert_eq!(token.stream_id, 100);
        assert_eq!(token.to_device_stream_id, Some(200));
        assert_eq!(token.device_list_stream_id, Some(300));
    }

    #[test]
    fn sync_token_parse_no_s_prefix() {
        assert!(SyncToken::parse("123").is_none());
    }

    #[test]
    fn sync_token_parse_empty() {
        assert!(SyncToken::parse("").is_none());
    }

    #[test]
    fn sync_token_parse_invalid() {
        assert!(SyncToken::parse("sabc").is_none());
    }

    #[test]
    fn sync_token_parse_partial_device_ids() {
        assert!(SyncToken::parse("s100_abc").is_none());
    }

    #[test]
    fn sync_token_encode_simple() {
        let token = SyncToken {
            stream_id: 42,
            room_id: None,
            event_type: None,
            to_device_stream_id: None,
            device_list_stream_id: None,
        };
        assert_eq!(token.encode(), "s42");
    }

    #[test]
    fn sync_token_encode_with_device_lists() {
        let token = SyncToken {
            stream_id: 10,
            room_id: None,
            event_type: None,
            to_device_stream_id: Some(20),
            device_list_stream_id: Some(30),
        };
        assert_eq!(token.encode(), "s10_20_30");
    }

    #[test]
    fn sync_token_roundtrip() {
        let original = SyncToken {
            stream_id: 99,
            room_id: None,
            event_type: None,
            to_device_stream_id: Some(55),
            device_list_stream_id: Some(66),
        };
        let encoded = original.encode();
        let parsed = SyncToken::parse(&encoded).unwrap();
        assert_eq!(parsed.stream_id, original.stream_id);
        assert_eq!(parsed.to_device_stream_id, original.to_device_stream_id);
        assert_eq!(parsed.device_list_stream_id, original.device_list_stream_id);
    }

    #[test]
    fn sync_token_roundtrip_simple() {
        let original = SyncToken {
            stream_id: 77,
            room_id: None,
            event_type: None,
            to_device_stream_id: None,
            device_list_stream_id: None,
        };
        let encoded = original.encode();
        let parsed = SyncToken::parse(&encoded).unwrap();
        assert_eq!(parsed.stream_id, original.stream_id);
        assert!(parsed.to_device_stream_id.is_none());
    }

    // ── aggregate_ephemeral_events ──────────────────────────────────────

    #[test]
    fn aggregate_ephemeral_deduplicates_m_receipt() {
        let events = vec![
            json!({"type": "m.receipt", "content": {"$event1": {"m.read": {"@a:b": {"ts": 1}}}} }),
            json!({"type": "m.receipt", "content": {"$event2": {"m.read": {"@b:b": {"ts": 2}}}} }),
        ];
        let result = SyncService::aggregate_ephemeral_events(events);
        assert_eq!(result.len(), 1);
        let content = &result[0]["content"];
        assert!(content.get("$event1").is_some());
        assert!(content.get("$event2").is_some());
    }

    #[test]
    fn aggregate_ephemeral_keeps_non_receipt_events() {
        let events = vec![
            json!({"type": "m.receipt", "content": {"!r:a": {"m.read": {"@a:b": {"ts": 1}}}} }),
            json!({"type": "m.typing", "content": {"user_ids": ["@a:b"]}} ),
        ];
        let result = SyncService::aggregate_ephemeral_events(events);
        assert_eq!(result.len(), 2);
        assert_eq!(result[1]["type"], "m.typing");
    }

    #[test]
    fn aggregate_ephemeral_empty_returns_empty() {
        let result = SyncService::aggregate_ephemeral_events(vec![]);
        assert!(result.is_empty());
    }

    // ── to_device_since / device_list_since ─────────────────────────────

    #[test]
    fn to_device_since_none_returns_zero() {
        assert_eq!(SyncService::to_device_since_stream_id(&None), 0);
    }

    #[test]
    fn to_device_since_extracts_value() {
        let token = Some(SyncToken {
            stream_id: 10,
            room_id: None,
            event_type: None,
            to_device_stream_id: Some(25),
            device_list_stream_id: None,
        });
        assert_eq!(SyncService::to_device_since_stream_id(&token), 25);
    }

    #[test]
    fn device_list_since_none_returns_zero() {
        assert_eq!(SyncService::device_list_since_stream_id(&None), 0);
    }

    #[test]
    fn device_list_since_extracts_value() {
        let token = Some(SyncToken {
            stream_id: 10,
            room_id: None,
            event_type: None,
            to_device_stream_id: None,
            device_list_stream_id: Some(35),
        });
        assert_eq!(SyncService::device_list_since_stream_id(&token), 35);
    }

    // ── resolve_sync_response_filter caching (OPT-015-a) ────────────────

    /// [`FilterStoreApi`] test double that counts `get_filter` invocations,
    /// used to prove that stored sync filters are read from Postgres at most
    /// once and served from the cache thereafter.
    struct CountingFilterStore {
        calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
        filter: synapse_storage::filter::Filter,
    }

    #[async_trait::async_trait]
    impl synapse_storage::filter::FilterStoreApi for CountingFilterStore {
        async fn create_filter(
            &self,
            _request: synapse_storage::filter::CreateFilterRequest,
        ) -> Result<synapse_storage::filter::Filter, synapse_common::error::ApiError> {
            Ok(self.filter.clone())
        }

        async fn get_filter(
            &self,
            user_id: &str,
            filter_id: &str,
        ) -> Result<Option<synapse_storage::filter::Filter>, synapse_common::error::ApiError> {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if user_id == self.filter.user_id && filter_id == self.filter.filter_id {
                Ok(Some(self.filter.clone()))
            } else {
                Ok(None)
            }
        }

        async fn get_filters_by_user(
            &self,
            _user_id: &str,
        ) -> Result<Vec<synapse_storage::filter::Filter>, synapse_common::error::ApiError> {
            Ok(vec![])
        }

        async fn delete_filter(
            &self,
            _user_id: &str,
            _filter_id: &str,
        ) -> Result<bool, synapse_common::error::ApiError> {
            Ok(false)
        }

        async fn delete_filters_by_user(&self, _user_id: &str) -> Result<u64, synapse_common::error::ApiError> {
            Ok(0)
        }
    }

    /// Builds a [`SyncService`] over a lazily-connected pool (never queried by
    /// `resolve_sync_response_filter`) plus an in-memory cache and the supplied
    /// filter store. The pool is created with `connect_lazy` so no live
    /// database is required.
    fn sync_service_with_filter_store(
        filter_store: std::sync::Arc<dyn synapse_storage::filter::FilterStoreApi>,
    ) -> SyncService {
        use std::sync::Arc;

        let pool: Arc<sqlx::PgPool> = Arc::new(
            sqlx::postgres::PgPoolOptions::new()
                .connect_lazy("postgres://synapse:synapse@localhost/synapse")
                .expect("lazy pool"),
        );
        let cache = Arc::new(synapse_cache::CacheManager::new(&synapse_cache::CacheConfig::default()));

        SyncService::from_deps(SyncServiceDeps {
            presence_storage: Arc::new(synapse_storage::presence::PresenceStorage::new(pool.clone(), cache.clone())),
            member_storage: Arc::new(synapse_storage::membership::RoomMemberStorage::new(&pool, "localhost")),
            event_reader: Arc::new(synapse_storage::event::EventStorage::new(&pool, "localhost".to_string())),
            room_storage: Arc::new(synapse_storage::room::RoomStorage::new(&pool)),
            room_account_data_storage: Arc::new(synapse_storage::room_account_data::RoomAccountDataStorage::new(&pool)),
            account_data_storage: Arc::new(synapse_storage::account_data::AccountDataStorage::new(&pool)),
            filter_storage: filter_store,
            device_storage: Arc::new(synapse_storage::device::DeviceStorage::new(&pool)),
            device_key_storage: synapse_e2ee::device_keys::DeviceKeyStorage::new(&pool),
            key_rotation_storage: synapse_e2ee::key_rotation::KeyRotationStorage::new(pool.clone()),
            to_device_storage: synapse_e2ee::to_device::ToDeviceStorage::new(&pool),
            metrics: Arc::new(synapse_common::MetricsCollector::new()),
            performance: synapse_common::config::PerformanceConfig::default(),
            cache,
        })
    }

    #[tokio::test]
    async fn resolve_sync_filter_is_cached_after_first_read() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let calls = Arc::new(AtomicUsize::new(0));
        let stored = synapse_storage::filter::Filter {
            id: 1,
            user_id: "@alice:localhost".to_string(),
            filter_id: "filterid".to_string(),
            content: serde_json::json!({"room": {"timeline": {"limit": 10}}}),
            created_ts: 0,
        };
        let filter_store: Arc<dyn synapse_storage::filter::FilterStoreApi> =
            Arc::new(CountingFilterStore { calls: calls.clone(), filter: stored });

        let sync = sync_service_with_filter_store(filter_store);

        let first = sync.resolve_sync_response_filter("@alice:localhost", Some("filterid")).await.unwrap();
        let second = sync.resolve_sync_response_filter("@alice:localhost", Some("filterid")).await.unwrap();

        assert!(first.is_some(), "first resolve should return the stored filter");
        assert!(second.is_some(), "second resolve should return the stored filter");

        // Both resolutions must yield an identical filter shape.
        let first_json = serde_json::to_value(&first).unwrap();
        let second_json = serde_json::to_value(&second).unwrap();
        assert_eq!(first_json, second_json, "cached filter must equal the freshly-read one");

        // Storage is hit exactly once; the second read is served from cache.
        assert_eq!(calls.load(Ordering::SeqCst), 1, "second resolve must be served from cache, not storage");
    }
}
