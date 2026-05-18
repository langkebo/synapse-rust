use super::types::*;
use super::SyncService;
use crate::common::*;
use crate::services::*;
use crate::storage::EventQueryFilter;
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
            return Ok(Some(Self::sync_response_filter_from_filter_json(
                &inline_filter,
            )));
        }

        let stored = self.filter_storage.get_filter(user_id, filter_id).await?;
        Ok(stored
            .as_ref()
            .map(|filter| Self::sync_response_filter_from_filter_json(&filter.content)))
    }

    pub(crate) fn sync_response_filter_from_filter_json(
        filter: &serde_json::Value,
    ) -> SyncResponseFilter {
        SyncResponseFilter {
            event_fields: Self::json_string_array(filter.get("event_fields")),
            event_format: Self::event_format_from_json(filter.get("event_format")),
            room: Some(Self::room_filter_from_filter_json(filter)),
            presence: Self::sync_filter_from_json(filter.get("presence")),
        }
    }

    pub(crate) fn timeline_limit_from_room_filter(
        room_filter: Option<&RoomFilter>,
        default_limit: i64,
    ) -> i64 {
        room_filter
            .and_then(|filter| filter.timeline.as_ref())
            .and_then(|timeline| timeline.limit)
            .filter(|limit| *limit > 0)
            .map_or(default_limit, |limit| limit.min(default_limit))
    }

    pub(crate) fn event_query_filter_from_sync_filter(
        filter: Option<&SyncFilter>,
    ) -> Option<EventQueryFilter> {
        let filter = filter?;
        let query_filter = EventQueryFilter {
            types: filter.types.clone(),
            not_types: filter.not_types.clone(),
            senders: filter.senders.clone(),
            not_senders: filter.not_senders.clone(),
        };

        if query_filter
            .types
            .as_ref()
            .is_some_and(|values| !values.is_empty())
            || query_filter
                .not_types
                .as_ref()
                .is_some_and(|values| !values.is_empty())
            || query_filter
                .senders
                .as_ref()
                .is_some_and(|values| !values.is_empty())
            || query_filter
                .not_senders
                .as_ref()
                .is_some_and(|values| !values.is_empty())
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
            include_leave: room
                .and_then(|value| value.get("include_leave"))
                .and_then(|value| value.as_bool()),
            state: Self::sync_filter_from_json(room.and_then(|value| value.get("state"))),
            timeline: Self::sync_filter_from_json(room.and_then(|value| value.get("timeline"))),
            ephemeral: Self::sync_filter_from_json(room.and_then(|value| value.get("ephemeral"))),
            account_data: Self::sync_filter_from_json(
                room.and_then(|value| value.get("account_data")),
            ),
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
            lazy_load_members: filter
                .get("lazy_load_members")
                .and_then(|value| value.as_bool()),
            include_redundant_members: filter
                .get("include_redundant_members")
                .and_then(|value| value.as_bool()),
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

    pub(crate) fn insert_nested_field(
        target: &mut Map<String, Value>,
        root_key: &str,
        path: &str,
        value: &Value,
    ) {
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

            let inserted = current_target
                .entry(segment.to_string())
                .or_insert_with(|| Value::Object(Map::new()));
            let Some(obj) = inserted.as_object_mut() else {
                ::tracing::error!("merge_json_nested: expected object for segment {}", segment);
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

    pub(crate) fn apply_event_fields_to_values(
        events: Vec<Value>,
        event_fields: Option<&[String]>,
    ) -> Vec<Value> {
        events
            .into_iter()
            .map(|event| Self::filter_event_fields(event, event_fields))
            .collect()
    }

    pub(crate) fn json_string_array(value: Option<&serde_json::Value>) -> Option<Vec<String>> {
        value.and_then(|value| {
            value.as_array().map(|entries| {
                entries
                    .iter()
                    .filter_map(|entry| entry.as_str().map(ToOwned::to_owned))
                    .collect()
            })
        })
    }

    pub(crate) fn apply_sync_filter_to_values(
        events: Vec<serde_json::Value>,
        filter: Option<&SyncFilter>,
    ) -> Vec<serde_json::Value> {
        let Some(filter) = filter else {
            return events;
        };

        events
            .into_iter()
            .filter(|event| Self::value_matches_sync_filter(event, filter))
            .collect()
    }

    pub(crate) fn room_filter_requests_lazy_members(room_filter: Option<&RoomFilter>) -> bool {
        room_filter
            .and_then(|filter| {
                filter
                    .state
                    .as_ref()
                    .and_then(|state| state.lazy_load_members)
                    .or_else(|| {
                        filter
                            .timeline
                            .as_ref()
                            .and_then(|timeline| timeline.lazy_load_members)
                    })
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
                    .or_else(|| {
                        filter
                            .timeline
                            .as_ref()
                            .and_then(|timeline| timeline.include_redundant_members)
                    })
            })
            .unwrap_or(false)
    }

    pub(crate) fn value_matches_sync_filter(
        event: &serde_json::Value,
        filter: &SyncFilter,
    ) -> bool {
        let room_id = event.get("room_id").and_then(|value| value.as_str());
        let event_type = event.get("type").and_then(|value| value.as_str());
        let sender = event.get("sender").and_then(|value| value.as_str());
        let contains_url = event
            .get("content")
            .and_then(|value| value.as_object())
            .is_some_and(|content| content.get("url").is_some());

        if let Some(rooms) = &filter.rooms {
            if !rooms.is_empty()
                && !room_id.is_some_and(|value| rooms.iter().any(|room| room == value))
            {
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
                && !event_type.is_some_and(|value| {
                    types
                        .iter()
                        .any(|pattern| Self::matches_wildcard(value, pattern))
                })
            {
                return false;
            }
        }

        if let Some(not_types) = &filter.not_types {
            if event_type.is_some_and(|value| {
                not_types
                    .iter()
                    .any(|pattern| Self::matches_wildcard(value, pattern))
            }) {
                return false;
            }
        }

        if let Some(senders) = &filter.senders {
            if !senders.is_empty()
                && !sender.is_some_and(|value| senders.iter().any(|s| s == value))
            {
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

    pub(crate) fn apply_timeline_limit(
        events: &[RoomEvent],
        timeline_limit: i64,
    ) -> (Vec<RoomEvent>, bool) {
        if timeline_limit <= 0 {
            return (Vec::new(), !events.is_empty());
        }

        let limited = events.len() as i64 > timeline_limit;
        let mut events: Vec<RoomEvent> = events
            .iter()
            .take(timeline_limit as usize)
            .cloned()
            .collect();
        events.reverse();
        (events, limited)
    }
}
