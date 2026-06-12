use super::types::*;
use super::SyncService;
use crate::common::*;
use crate::storage::{EventQueryFilter, RoomEvent};
use serde_json::{Map, Value};

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

        let stored = self.filter_storage.get_filter(user_id, filter_id).await?;
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

    // ========== matches_wildcard tests ==========

    #[test]
    fn test_matches_wildcard_exact() {
        assert!(SyncService::matches_wildcard("m.room.message", "m.room.message"));
    }

    #[test]
    fn test_matches_wildcard_no_match() {
        assert!(!SyncService::matches_wildcard("m.room.message", "m.room.topic"));
    }

    #[test]
    fn test_matches_wildcard_prefix() {
        assert!(SyncService::matches_wildcard("m.room.message", "m.room.*"));
    }

    #[test]
    fn test_matches_wildcard_prefix_no_match() {
        assert!(!SyncService::matches_wildcard("m.room.message", "m.space.*"));
    }

    #[test]
    fn test_matches_wildcard_empty_pattern() {
        assert!(!SyncService::matches_wildcard("anything", ""));
    }

    #[test]
    fn test_matches_wildcard_empty_actual() {
        assert!(!SyncService::matches_wildcard("", "m.room.*"));
    }

    #[test]
    fn test_matches_wildcard_star_only() {
        assert!(SyncService::matches_wildcard("anything", "*"));
    }

    // ========== json_string_array tests ==========

    #[test]
    fn test_json_string_array_none() {
        assert_eq!(SyncService::json_string_array(None), None);
    }

    #[test]
    fn test_json_string_array_valid() {
        let value = json!(["a", "b", "c"]);
        let result = SyncService::json_string_array(Some(&value));
        assert_eq!(result, Some(vec!["a".to_string(), "b".to_string(), "c".to_string()]));
    }

    #[test]
    fn test_json_string_array_empty() {
        let value = json!([]);
        let result = SyncService::json_string_array(Some(&value));
        assert_eq!(result, Some(vec![]));
    }

    #[test]
    fn test_json_string_array_not_array() {
        let value = json!("not_an_array");
        let result = SyncService::json_string_array(Some(&value));
        assert_eq!(result, None);
    }

    #[test]
    fn test_json_string_array_mixed_types() {
        let value = json!(["a", 42, "c"]);
        let result = SyncService::json_string_array(Some(&value));
        // 42 is not a string, so it's filtered out
        assert_eq!(result, Some(vec!["a".to_string(), "c".to_string()]));
    }

    // ========== event_format_from_json tests ==========

    #[test]
    fn test_event_format_from_json_client() {
        let result = SyncService::event_format_from_json(Some(&json!("client")));
        assert_eq!(result, SyncEventFormat::Client);
    }

    #[test]
    fn test_event_format_from_json_federation() {
        let result = SyncService::event_format_from_json(Some(&json!("federation")));
        assert_eq!(result, SyncEventFormat::Federation);
    }

    #[test]
    fn test_event_format_from_json_none() {
        let result = SyncService::event_format_from_json(None);
        assert_eq!(result, SyncEventFormat::Client);
    }

    #[test]
    fn test_event_format_from_json_unknown() {
        let result = SyncService::event_format_from_json(Some(&json!("unknown")));
        assert_eq!(result, SyncEventFormat::Client);
    }

    // ========== timeline_limit_from_room_filter tests ==========

    #[test]
    fn test_timeline_limit_from_room_filter_none() {
        assert_eq!(SyncService::timeline_limit_from_room_filter(None, 100), 100);
    }

    #[test]
    fn test_timeline_limit_from_room_filter_no_timeline() {
        let filter = RoomFilter { timeline: None, ..Default::default() };
        assert_eq!(SyncService::timeline_limit_from_room_filter(Some(&filter), 100), 100);
    }

    #[test]
    fn test_timeline_limit_from_room_filter_no_limit() {
        let filter = RoomFilter {
            timeline: Some(SyncFilter { limit: None, ..Default::default() }),
            ..Default::default()
        };
        assert_eq!(SyncService::timeline_limit_from_room_filter(Some(&filter), 100), 100);
    }

    #[test]
    fn test_timeline_limit_from_room_filter_with_limit() {
        let filter = RoomFilter {
            timeline: Some(SyncFilter { limit: Some(30), ..Default::default() }),
            ..Default::default()
        };
        assert_eq!(SyncService::timeline_limit_from_room_filter(Some(&filter), 100), 30);
    }

    #[test]
    fn test_timeline_limit_from_room_filter_limit_exceeds_default() {
        let filter = RoomFilter {
            timeline: Some(SyncFilter { limit: Some(200), ..Default::default() }),
            ..Default::default()
        };
        assert_eq!(SyncService::timeline_limit_from_room_filter(Some(&filter), 100), 100);
    }

    #[test]
    fn test_timeline_limit_from_room_filter_zero_limit() {
        let filter = RoomFilter {
            timeline: Some(SyncFilter { limit: Some(0), ..Default::default() }),
            ..Default::default()
        };
        assert_eq!(SyncService::timeline_limit_from_room_filter(Some(&filter), 100), 100);
    }

    #[test]
    fn test_timeline_limit_from_room_filter_negative_limit() {
        let filter = RoomFilter {
            timeline: Some(SyncFilter { limit: Some(-5), ..Default::default() }),
            ..Default::default()
        };
        assert_eq!(SyncService::timeline_limit_from_room_filter(Some(&filter), 100), 100);
    }

    // ========== event_query_filter_from_sync_filter tests ==========

    #[test]
    fn test_event_query_filter_from_sync_filter_none() {
        assert_eq!(SyncService::event_query_filter_from_sync_filter(None), None);
    }

    #[test]
    fn test_event_query_filter_from_sync_filter_empty() {
        let filter = SyncFilter::default();
        assert_eq!(SyncService::event_query_filter_from_sync_filter(Some(&filter)), None);
    }

    #[test]
    fn test_event_query_filter_from_sync_filter_with_types() {
        let filter = SyncFilter {
            types: Some(vec!["m.room.message".to_string()]),
            ..Default::default()
        };
        let result = SyncService::event_query_filter_from_sync_filter(Some(&filter));
        assert!(result.is_some());
        assert_eq!(result.unwrap().types, Some(vec!["m.room.message".to_string()]));
    }

    #[test]
    fn test_event_query_filter_from_sync_filter_with_senders() {
        let filter = SyncFilter {
            senders: Some(vec!["@alice:example.com".to_string()]),
            ..Default::default()
        };
        let result = SyncService::event_query_filter_from_sync_filter(Some(&filter));
        assert!(result.is_some());
        assert_eq!(result.unwrap().senders, Some(vec!["@alice:example.com".to_string()]));
    }

    // ========== room_filter_requests_lazy_members tests ==========

    #[test]
    fn test_room_filter_requests_lazy_members_none() {
        assert!(!SyncService::room_filter_requests_lazy_members(None));
    }

    #[test]
    fn test_room_filter_requests_lazy_members_state() {
        let filter = RoomFilter {
            state: Some(SyncFilter { lazy_load_members: Some(true), ..Default::default() }),
            ..Default::default()
        };
        assert!(SyncService::room_filter_requests_lazy_members(Some(&filter)));
    }

    #[test]
    fn test_room_filter_requests_lazy_members_timeline() {
        let filter = RoomFilter {
            timeline: Some(SyncFilter { lazy_load_members: Some(true), ..Default::default() }),
            ..Default::default()
        };
        assert!(SyncService::room_filter_requests_lazy_members(Some(&filter)));
    }

    #[test]
    fn test_room_filter_requests_lazy_members_false() {
        let filter = RoomFilter::default();
        assert!(!SyncService::room_filter_requests_lazy_members(Some(&filter)));
    }

    // ========== room_filter_requests_redundant_members tests ==========

    #[test]
    fn test_room_filter_requests_redundant_members_none() {
        assert!(!SyncService::room_filter_requests_redundant_members(None));
    }

    #[test]
    fn test_room_filter_requests_redundant_members_true() {
        let filter = RoomFilter {
            state: Some(SyncFilter { include_redundant_members: Some(true), ..Default::default() }),
            ..Default::default()
        };
        assert!(SyncService::room_filter_requests_redundant_members(Some(&filter)));
    }

    // ========== sync_filter_from_json tests ==========

    #[test]
    fn test_sync_filter_from_json_none() {
        assert_eq!(SyncService::sync_filter_from_json(None), None);
    }

    #[test]
    fn test_sync_filter_from_json_full() {
        let json = json!({
            "limit": 50,
            "types": ["m.room.message"],
            "not_types": ["m.room.member"],
            "rooms": ["!room1:example.com"],
            "not_rooms": ["!room2:example.com"],
            "contains_url": true,
            "lazy_load_members": true,
            "include_redundant_members": false,
            "senders": ["@alice:example.com"],
            "not_senders": ["@bob:example.com"]
        });
        let result = SyncService::sync_filter_from_json(Some(&json)).unwrap();
        assert_eq!(result.limit, Some(50));
        assert_eq!(result.types, Some(vec!["m.room.message".to_string()]));
        assert_eq!(result.not_types, Some(vec!["m.room.member".to_string()]));
        assert_eq!(result.rooms, Some(vec!["!room1:example.com".to_string()]));
        assert_eq!(result.not_rooms, Some(vec!["!room2:example.com".to_string()]));
        assert_eq!(result.contains_url, Some(true));
        assert_eq!(result.lazy_load_members, Some(true));
        assert_eq!(result.include_redundant_members, Some(false));
    }

    // ========== filter_event_fields tests ==========

    #[test]
    fn test_filter_event_fields_no_fields() {
        let event = json!({"type": "m.room.message", "content": {"body": "hello"}});
        let result = SyncService::filter_event_fields(event.clone(), None);
        assert_eq!(result, event);
    }

    #[test]
    fn test_filter_event_fields_with_fields() {
        let event = json!({"type": "m.room.message", "content": {"body": "hello"}, "sender": "@alice:example.com"});
        let fields = vec!["type".to_string(), "sender".to_string()];
        let result = SyncService::filter_event_fields(event, Some(&fields));
        assert_eq!(result["type"], json!("m.room.message"));
        assert_eq!(result["sender"], json!("@alice:example.com"));
        assert!(result.get("content").is_none());
    }

    #[test]
    fn test_filter_event_fields_nested() {
        let event = json!({"type": "m.room.message", "content": {"body": "hello", "msgtype": "m.text"}});
        let fields = vec!["content.body".to_string()];
        let result = SyncService::filter_event_fields(event, Some(&fields));
        assert_eq!(result["content"]["body"], json!("hello"));
    }

    #[test]
    fn test_filter_event_fields_not_object() {
        let event = json!("not_an_object");
        let result = SyncService::filter_event_fields(event.clone(), Some(&["type".to_string()]));
        assert_eq!(result, json!("not_an_object"));
    }

    // ========== value_matches_sync_filter tests ==========

    #[test]
    fn test_value_matches_sync_filter_empty() {
        let filter = SyncFilter::default();
        let event = json!({"type": "m.room.message", "room_id": "!room:example.com", "sender": "@alice:example.com"});
        assert!(SyncService::value_matches_sync_filter(&event, &filter));
    }

    #[test]
    fn test_value_matches_sync_filter_type_match() {
        let filter = SyncFilter {
            types: Some(vec!["m.room.message".to_string()]),
            ..Default::default()
        };
        let event = json!({"type": "m.room.message"});
        assert!(SyncService::value_matches_sync_filter(&event, &filter));
    }

    #[test]
    fn test_value_matches_sync_filter_type_no_match() {
        let filter = SyncFilter {
            types: Some(vec!["m.room.message".to_string()]),
            ..Default::default()
        };
        let event = json!({"type": "m.room.member"});
        assert!(!SyncService::value_matches_sync_filter(&event, &filter));
    }

    #[test]
    fn test_value_matches_sync_filter_not_type() {
        let filter = SyncFilter {
            not_types: Some(vec!["m.room.member".to_string()]),
            ..Default::default()
        };
        let event = json!({"type": "m.room.member"});
        assert!(!SyncService::value_matches_sync_filter(&event, &filter));
    }

    #[test]
    fn test_value_matches_sync_filter_room_match() {
        let filter = SyncFilter {
            rooms: Some(vec!["!room1:example.com".to_string()]),
            ..Default::default()
        };
        let event = json!({"room_id": "!room1:example.com", "type": "m.room.message"});
        assert!(SyncService::value_matches_sync_filter(&event, &filter));
    }

    #[test]
    fn test_value_matches_sync_filter_room_no_match() {
        let filter = SyncFilter {
            rooms: Some(vec!["!room1:example.com".to_string()]),
            ..Default::default()
        };
        let event = json!({"room_id": "!room2:example.com", "type": "m.room.message"});
        assert!(!SyncService::value_matches_sync_filter(&event, &filter));
    }

    #[test]
    fn test_value_matches_sync_filter_not_room() {
        let filter = SyncFilter {
            not_rooms: Some(vec!["!room1:example.com".to_string()]),
            ..Default::default()
        };
        let event = json!({"room_id": "!room1:example.com", "type": "m.room.message"});
        assert!(!SyncService::value_matches_sync_filter(&event, &filter));
    }

    #[test]
    fn test_value_matches_sync_filter_sender_match() {
        let filter = SyncFilter {
            senders: Some(vec!["@alice:example.com".to_string()]),
            ..Default::default()
        };
        let event = json!({"sender": "@alice:example.com", "type": "m.room.message"});
        assert!(SyncService::value_matches_sync_filter(&event, &filter));
    }

    #[test]
    fn test_value_matches_sync_filter_not_sender() {
        let filter = SyncFilter {
            not_senders: Some(vec!["@bob:example.com".to_string()]),
            ..Default::default()
        };
        let event = json!({"sender": "@bob:example.com", "type": "m.room.message"});
        assert!(!SyncService::value_matches_sync_filter(&event, &filter));
    }

    #[test]
    fn test_value_matches_sync_filter_contains_url_match() {
        let filter = SyncFilter {
            contains_url: Some(true),
            ..Default::default()
        };
        let event = json!({"type": "m.room.message", "content": {"url": "https://example.com"}});
        assert!(SyncService::value_matches_sync_filter(&event, &filter));
    }

    #[test]
    fn test_value_matches_sync_filter_contains_url_no_match() {
        let filter = SyncFilter {
            contains_url: Some(true),
            ..Default::default()
        };
        let event = json!({"type": "m.room.message", "content": {"body": "hello"}});
        assert!(!SyncService::value_matches_sync_filter(&event, &filter));
    }

    #[test]
    fn test_value_matches_sync_filter_no_contains_url() {
        let filter = SyncFilter {
            contains_url: Some(false),
            ..Default::default()
        };
        let event = json!({"type": "m.room.message", "content": {"url": "https://example.com"}});
        assert!(!SyncService::value_matches_sync_filter(&event, &filter));
    }

    #[test]
    fn test_value_matches_sync_filter_wildcard_type() {
        let filter = SyncFilter {
            types: Some(vec!["m.room.*".to_string()]),
            ..Default::default()
        };
        assert!(SyncService::value_matches_sync_filter(&json!({"type": "m.room.message"}), &filter));
        assert!(SyncService::value_matches_sync_filter(&json!({"type": "m.room.member"}), &filter));
        assert!(!SyncService::value_matches_sync_filter(&json!({"type": "m.space.child"}), &filter));
    }

    // ========== apply_event_fields_to_values tests ==========

    #[test]
    fn test_apply_event_fields_to_values_no_fields() {
        let events = vec![json!({"a": 1}), json!({"b": 2})];
        let result = SyncService::apply_event_fields_to_values(events.clone(), None);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], json!({"a": 1}));
        assert_eq!(result[1], json!({"b": 2}));
    }

    #[test]
    fn test_apply_event_fields_to_values_with_fields() {
        let events = vec![
            json!({"type": "m.room.message", "content": {"body": "hi"}}),
            json!({"type": "m.room.member", "content": {"body": "bye"}}),
        ];
        let fields = vec!["type".to_string()];
        let result = SyncService::apply_event_fields_to_values(events, Some(&fields));
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], json!({"type": "m.room.message"}));
        assert_eq!(result[1], json!({"type": "m.room.member"}));
    }

    // ========== apply_sync_filter_to_values tests ==========

    #[test]
    fn test_apply_sync_filter_to_values_no_filter() {
        let events = vec![json!({"type": "m.room.message"}), json!({"type": "m.room.member"})];
        let result = SyncService::apply_sync_filter_to_values(events.clone(), None);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_apply_sync_filter_to_values_with_filter() {
        let filter = SyncFilter {
            types: Some(vec!["m.room.message".to_string()]),
            ..Default::default()
        };
        let events = vec![
            json!({"type": "m.room.message"}),
            json!({"type": "m.room.member"}),
            json!({"type": "m.room.message"}),
        ];
        let result = SyncService::apply_sync_filter_to_values(events, Some(&filter));
        assert_eq!(result.len(), 2);
    }

    // ========== merge_json_object tests ==========

    #[test]
    fn test_merge_json_object_simple() {
        let mut target = serde_json::Map::new();
        SyncService::merge_json_object(&mut target, "key".to_string(), json!("value"));
        assert_eq!(target["key"], json!("value"));
    }

    #[test]
    fn test_merge_json_object_nested() {
        let mut target = serde_json::Map::new();
        target.insert("outer".to_string(), json!({"inner": "old"}));
        SyncService::merge_json_object(&mut target, "outer".to_string(), json!({"inner": "new", "extra": 42}));
        assert_eq!(target["outer"]["inner"], json!("new"));
        assert_eq!(target["outer"]["extra"], json!(42));
    }

    #[test]
    fn test_merge_json_object_overwrite() {
        let mut target = serde_json::Map::new();
        target.insert("key".to_string(), json!("old"));
        SyncService::merge_json_object(&mut target, "key".to_string(), json!("new"));
        assert_eq!(target["key"], json!("new"));
    }

    // ========== insert_nested_field tests ==========

    #[test]
    fn test_insert_nested_field_simple() {
        let mut target = serde_json::Map::new();
        let value = json!({"inner": "val"});
        SyncService::insert_nested_field(&mut target, "root", "inner", &value);
        assert_eq!(target["root"]["inner"], json!("val"));
    }

    #[test]
    fn test_insert_nested_field_deep() {
        let mut target = serde_json::Map::new();
        let value = json!({"a": {"b": {"c": "deep"}}});
        SyncService::insert_nested_field(&mut target, "root", "a.b.c", &value);
        assert_eq!(target["root"]["a"]["b"]["c"], json!("deep"));
    }

    #[test]
    fn test_insert_nested_field_not_object() {
        let mut target = serde_json::Map::new();
        let value = json!("not_object");
        SyncService::insert_nested_field(&mut target, "root", "inner", &value);
        // Should not insert anything since value is not an object
        assert!(target.is_empty());
    }

    // ========== sync_response_filter_from_filter_json tests ==========

    #[test]
    fn test_sync_response_filter_from_filter_json_empty() {
        let json = json!({});
        let result = SyncService::sync_response_filter_from_filter_json(&json);
        assert_eq!(result.event_format, SyncEventFormat::Client);
        assert!(result.room.is_some());
        assert!(result.presence.is_none());
    }

    #[test]
    fn test_sync_response_filter_from_filter_json_with_event_format() {
        let json = json!({"event_format": "federation"});
        let result = SyncService::sync_response_filter_from_filter_json(&json);
        assert_eq!(result.event_format, SyncEventFormat::Federation);
    }

    #[test]
    fn test_sync_response_filter_from_filter_json_with_presence() {
        let json = json!({"presence": {"limit": 10}});
        let result = SyncService::sync_response_filter_from_filter_json(&json);
        assert!(result.presence.is_some());
        assert_eq!(result.presence.unwrap().limit, Some(10));
    }

    // ========== room_filter_from_filter_json tests ==========

    #[test]
    fn test_room_filter_from_filter_json_empty() {
        let json = json!({});
        let result = SyncService::room_filter_from_filter_json(&json);
        assert!(result.rooms.is_none());
        assert!(result.not_rooms.is_none());
        assert!(result.include_leave.is_none());
    }

    #[test]
    fn test_room_filter_from_filter_json_with_rooms() {
        let json = json!({"room": {"rooms": ["!room1:example.com"]}});
        let result = SyncService::room_filter_from_filter_json(&json);
        assert_eq!(result.rooms, Some(vec!["!room1:example.com".to_string()]));
    }

    #[test]
    fn test_room_filter_from_filter_json_with_include_leave() {
        let json = json!({"room": {"include_leave": true}});
        let result = SyncService::room_filter_from_filter_json(&json);
        assert_eq!(result.include_leave, Some(true));
    }
}
