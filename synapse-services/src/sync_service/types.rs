use crate::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use synapse_common::config::PerformanceConfig;
use synapse_common::*;
use synapse_e2ee::device_keys::DeviceKeyStoreApi;
use synapse_e2ee::key_rotation::KeyRotationStorage;
use synapse_e2ee::to_device::ToDeviceStorage;

/// The `SyncToken` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncToken {
    /// The `stream_id` field.
    pub stream_id: i64,
    /// The `room_id` field.
    pub room_id: Option<String>,
    /// The `event_type` field.
    pub event_type: Option<String>,
    /// The `to_device_stream_id` field.
    pub to_device_stream_id: Option<i64>,
    /// The `device_list_stream_id` field.
    pub device_list_stream_id: Option<i64>,
}

impl SyncToken {
    /// See [`parse`].
    pub fn parse(token: &str) -> Option<Self> {
        if let Some(stripped) = token.strip_prefix('s') {
            if let Some((event_stream_id, rest)) = stripped.split_once('_') {
                let (to_device_stream_id, device_list_stream_id) =
                    rest.split_once('_').and_then(|(to_device, device_list)| {
                        let to_device_id = to_device.parse::<i64>().ok()?;
                        let device_list_id = device_list.parse::<i64>().ok()?;
                        Some((to_device_id, device_list_id))
                    })?;

                let stream_id = event_stream_id.parse::<i64>().ok()?;
                Some(Self {
                    stream_id,
                    room_id: None,
                    event_type: None,
                    to_device_stream_id: Some(to_device_stream_id),
                    device_list_stream_id: Some(device_list_stream_id),
                })
            } else {
                stripped.parse::<i64>().ok().map(|stream_id| Self {
                    stream_id,
                    room_id: None,
                    event_type: None,
                    to_device_stream_id: None,
                    device_list_stream_id: None,
                })
            }
        } else {
            None
        }
    }

    /// See [`encode`].
    pub fn encode(&self) -> String {
        match (self.to_device_stream_id, self.device_list_stream_id) {
            (Some(to_device), Some(device_list)) => {
                format!("s{}_{}_{}", self.stream_id, to_device, device_list)
            }
            _ => format!("s{}", self.stream_id),
        }
    }
}

/// The `SyncFilter` struct.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SyncFilter {
    /// The `limit` field.
    pub limit: Option<i64>,
    /// The `types` field.
    pub types: Option<Vec<String>>,
    /// The `not_types` field.
    pub not_types: Option<Vec<String>>,
    /// The `rooms` field.
    pub rooms: Option<Vec<String>>,
    /// The `not_rooms` field.
    pub not_rooms: Option<Vec<String>>,
    /// The `contains_url` field.
    pub contains_url: Option<bool>,
    /// The `lazy_load_members` field.
    pub lazy_load_members: Option<bool>,
    /// The `include_redundant_members` field.
    pub include_redundant_members: Option<bool>,
    /// The `senders` field.
    pub senders: Option<Vec<String>>,
    /// The `not_senders` field.
    pub not_senders: Option<Vec<String>>,
}

impl Default for SyncFilter {
    fn default() -> Self {
        Self {
            limit: Some(100),
            types: None,
            not_types: None,
            rooms: None,
            not_rooms: None,
            contains_url: None,
            lazy_load_members: None,
            include_redundant_members: None,
            senders: None,
            not_senders: None,
        }
    }
}

/// The `SyncEventFormat` enum.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SyncEventFormat {
    #[default]
    /// The `Client` variant.
    Client,
    /// The `Federation` variant.
    Federation,
}

/// The `RoomSyncCounts` struct.
#[derive(Debug, Clone, Default)]
pub struct RoomSyncCounts {
    /// The `highlight_count` field.
    pub highlight_count: i64,
    /// The `notification_count` field.
    pub notification_count: i64,
}

/// The `SyncServiceDeps` struct.
pub struct SyncServiceDeps {
    /// The `presence_storage` field.
    pub presence_storage: std::sync::Arc<dyn synapse_storage::presence::PresenceStoreApi>,
    /// The `member_storage` field.
    pub member_storage: Arc<dyn synapse_storage::membership::MemberStoreApi>,
    /// The `event_reader` field.
    pub event_reader: Arc<dyn synapse_storage::event::EventReader>,
    /// The `room_account_data_storage` field.
    pub room_account_data_storage: Arc<dyn synapse_storage::room_account_data::RoomAccountDataStoreApi>,
    /// The `account_data_storage` field.
    pub account_data_storage: Arc<dyn synapse_storage::account_data::AccountDataStoreApi>,
    /// The `filter_storage` field.
    pub filter_storage: Arc<dyn synapse_storage::filter::FilterStoreApi>,
    /// The `device_storage` field.
    pub device_storage: Arc<dyn synapse_storage::device::DeviceListStoreApi>,
    /// The `device_key_storage` field.
    pub device_key_storage: Arc<dyn DeviceKeyStoreApi>,
    /// The `key_rotation_storage` field.
    pub key_rotation_storage: KeyRotationStorage,
    /// The `to_device_storage` field.
    pub to_device_storage: ToDeviceStorage,
    /// The `metrics` field.
    pub metrics: Arc<MetricsCollector>,
    /// The `performance` field.
    pub performance: PerformanceConfig,
    /// The `cache` field.
    pub cache: Arc<synapse_cache::CacheManager>,
    /// S6: event-driven wake-up for v2 /sync long-polling. When `Some`,
    /// `wait_for_incremental_update` parks on `Notify` slots instead of
    /// polling the database every 250ms.
    pub event_notifier: Option<crate::event_notifier::EventNotifier>,
    /// MSC4354: sticky events injection. When `Some`, each room in the sync
    /// response includes a `sticky_events` array with the user's sticky
    /// event metadata for that room.
    pub sticky_event_storage: Option<Arc<dyn synapse_storage::sticky_event::StickyEventStoreApi>>,
}

/// The `SyncServiceRequest` struct.
pub struct SyncServiceRequest<'a> {
    /// The `user_id` field.
    pub user_id: &'a str,
    /// The `device_id` field.
    pub device_id: Option<&'a str>,
    /// The `timeout` field.
    pub timeout: u64,
    /// The `is_full_state` field.
    pub is_full_state: bool,
    /// The `set_presence` field.
    pub set_presence: &'a str,
    /// The `filter_id` field.
    pub filter_id: Option<&'a str>,
    /// The `since` field.
    pub since: Option<&'a str>,
}

/// The `FetchEventsRequest` struct.
pub struct FetchEventsRequest<'a> {
    /// The `user_id` field.
    pub user_id: &'a str,
    /// The `device_id` field.
    pub device_id: Option<&'a str>,
    /// The `room_ids` field.
    pub room_ids: &'a [String],
    /// The `since_token` field.
    pub since_token: Option<&'a SyncToken>,
    /// The `timeout` field.
    pub timeout: u64,
    /// The `limit` field.
    pub limit: i64,
    /// The `timeline_filter` field.
    pub timeline_filter: Option<&'a SyncFilter>,
    /// The `is_incremental` field.
    pub is_incremental: bool,
}

/// The `BuildSyncResponseRequest` struct.
pub struct BuildSyncResponseRequest<'a> {
    /// The `user_id` field.
    pub user_id: &'a str,
    /// The `device_id` field.
    pub device_id: Option<&'a str>,
    /// The `room_ids` field.
    pub room_ids: &'a [String],
    /// The `room_sections` field.
    pub room_sections: &'a HashMap<String, SyncRoomSection>,
    /// The `room_events` field.
    pub room_events: HashMap<String, Vec<RoomEvent>>,
    /// The `response_filter` field.
    pub response_filter: Option<&'a SyncResponseFilter>,
    /// The `timeline_limit` field.
    pub timeline_limit: i64,
    /// The `since_token` field.
    pub since_token: &'a Option<SyncToken>,
    /// The `is_incremental` field.
    pub is_incremental: bool,
}

/// The `BuildRoomSyncRequest` struct.
pub struct BuildRoomSyncRequest<'a> {
    /// The `room_id` field.
    pub room_id: &'a str,
    /// The `user_id` field.
    pub user_id: &'a str,
    /// The `device_id` field.
    pub device_id: Option<&'a str>,
    /// The `events` field.
    pub events: Vec<RoomEvent>,
    /// The `since_token` field.
    pub since_token: Option<&'a SyncToken>,
    /// The `is_incremental` field.
    pub is_incremental: bool,
    /// The `room_filter` field.
    pub room_filter: Option<&'a RoomFilter>,
}

/// The `BuildRoomSyncValueRequest` struct.
pub struct BuildRoomSyncValueRequest<'a> {
    /// The `events` field.
    pub events: Vec<RoomEvent>,
    /// The `state_list` field.
    pub state_list: Vec<Value>,
    /// The `ephemeral_events` field.
    pub ephemeral_events: Vec<Value>,
    /// The `account_data_events` field.
    pub account_data_events: Vec<Value>,
    /// The `timeline_limit` field.
    pub timeline_limit: i64,
    /// The `counts` field.
    pub counts: RoomSyncCounts,
    /// The `event_fields` field.
    pub event_fields: Option<&'a [String]>,
    /// The `event_format` field.
    pub event_format: SyncEventFormat,
}

/// The `LazyLoadMembersRequest` struct.
pub struct LazyLoadMembersRequest<'a> {
    /// The `state_events` field.
    pub state_events: Vec<Value>,
    /// The `timeline_events` field.
    pub timeline_events: &'a [RoomEvent],
    /// The `user_id` field.
    pub user_id: &'a str,
    /// The `device_id` field.
    pub device_id: Option<&'a str>,
    /// The `room_id` field.
    pub room_id: &'a str,
    /// The `room_filter` field.
    pub room_filter: Option<&'a RoomFilter>,
    /// The `changed_member_ids` field.
    pub changed_member_ids: Option<&'a HashSet<String>>,
    /// The `timeline_limited` field.
    pub timeline_limited: bool,
    /// The `enabled` field.
    pub enabled: bool,
}

/// The `RoomFilter` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomFilter {
    /// The `rooms` field.
    pub rooms: Option<Vec<String>>,
    /// The `not_rooms` field.
    pub not_rooms: Option<Vec<String>>,
    /// The `include_leave` field.
    pub include_leave: Option<bool>,
    /// The `state` field.
    pub state: Option<SyncFilter>,
    /// The `timeline` field.
    pub timeline: Option<SyncFilter>,
    /// The `ephemeral` field.
    pub ephemeral: Option<SyncFilter>,
    /// The `account_data` field.
    pub account_data: Option<SyncFilter>,
}

/// The `SyncResponseFilter` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResponseFilter {
    /// The `event_fields` field.
    pub event_fields: Option<Vec<String>>,
    /// The `event_format` field.
    pub event_format: SyncEventFormat,
    /// The `room` field.
    pub room: Option<RoomFilter>,
    /// The `presence` field.
    pub presence: Option<SyncFilter>,
}

impl Default for RoomFilter {
    fn default() -> Self {
        Self {
            rooms: None,
            not_rooms: None,
            include_leave: Some(false),
            state: Some(SyncFilter::default()),
            timeline: Some(SyncFilter { limit: Some(50), ..Default::default() }),
            ephemeral: Some(SyncFilter::default()),
            account_data: Some(SyncFilter::default()),
        }
    }
}

impl Default for SyncResponseFilter {
    fn default() -> Self {
        Self {
            event_fields: None,
            event_format: SyncEventFormat::Client,
            room: Some(RoomFilter::default()),
            presence: Some(SyncFilter::default()),
        }
    }
}

/// The `SyncRoomSection` enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncRoomSection {
    /// The `Join` variant.
    Join,
    /// The `Leave` variant.
    Leave,
    /// Invited rooms — sync response includes stripped state (MSC4311:
    /// must include m.room.create so invitees can determine room version).
    Invite,
}

/// The `SyncRequest` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRequest {
    /// The `since` field.
    pub since: Option<String>,
    /// The `filter` field.
    pub filter: Option<String>,
    #[serde(rename = "full_state")]
    /// The `is_full_state` field.
    pub is_full_state: bool,
    /// The `set_presence` field.
    pub set_presence: Option<String>,
    /// The `timeout` field.
    pub timeout: u64,
}

/// The `SyncState` struct.
#[derive(Debug, Clone)]
pub struct SyncState {
    /// The `rooms` field.
    pub rooms: HashMap<String, RoomSyncState>,
    /// The `last_stream_id` field.
    pub last_stream_id: i64,
}

/// The `RoomSyncState` struct.
#[derive(Debug, Clone)]
pub struct RoomSyncState {
    /// The `timeline_limit` field.
    pub timeline_limit: i64,
    /// The `last_event_id` field.
    pub last_event_id: Option<String>,
    /// The `last_stream_id` field.
    pub last_stream_id: i64,
}

/// The `SyncPerformanceSnapshot` struct.
#[derive(Debug, Clone)]
pub struct SyncPerformanceSnapshot<'a> {
    /// The `request_kind` field.
    pub request_kind: &'a str,
    /// The `user_id` field.
    pub user_id: &'a str,
    /// The `total_ms` field.
    pub total_ms: f64,
    /// The `room_count` field.
    pub room_count: usize,
    /// The `event_count` field.
    pub event_count: usize,
    /// The `is_incremental` field.
    pub is_incremental: bool,
    /// The `phases` field.
    pub phases: [(&'a str, f64); 3],
}

/// The `IncrementalUpdate` enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncrementalUpdate {
    /// The `Events` variant.
    Events,
    /// The `ToDevice` variant.
    ToDevice,
    /// The `DeviceLists` variant.
    DeviceLists,
    /// The `Timeout` variant.
    Timeout,
}

/// The `LazyLoadedMembersCacheKey` struct.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LazyLoadedMembersCacheKey {
    /// The `user_id` field.
    pub user_id: String,
    /// The `device_id` field.
    pub device_id: Option<String>,
    /// The `room_id` field.
    pub room_id: String,
}

impl LazyLoadedMembersCacheKey {
    /// See [`new`].
    pub fn new(user_id: &str, device_id: Option<&str>, room_id: &str) -> Self {
        Self { user_id: user_id.to_string(), device_id: device_id.map(str::to_string), room_id: room_id.to_string() }
    }
}

/// The `StateEventsBatchParams` struct.
pub struct StateEventsBatchParams<'a> {
    /// The `since_ts` field.
    pub since_ts: i64,
    /// The `since_stream_ordering` field.
    pub since_stream_ordering: Option<i64>,
    /// The `is_incremental` field.
    pub is_incremental: bool,
    /// The `lazy_load_members` field.
    pub lazy_load_members: bool,
    /// The `user_id` field.
    pub user_id: &'a str,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── SyncToken::parse / SyncToken::encode ──────────────────────────

    #[test]
    fn sync_token_roundtrip_simple() {
        let token_str = "s42";
        let token = SyncToken::parse(token_str).unwrap();
        assert_eq!(token.stream_id, 42);
        assert_eq!(token.to_device_stream_id, None);
        assert_eq!(token.device_list_stream_id, None);
        assert_eq!(token.encode(), "s42");
    }

    #[test]
    fn sync_token_roundtrip_with_to_device_and_device_list() {
        let token_str = "s100_200_300";
        let token = SyncToken::parse(token_str).unwrap();
        assert_eq!(token.stream_id, 100);
        assert_eq!(token.to_device_stream_id, Some(200));
        assert_eq!(token.device_list_stream_id, Some(300));
        assert_eq!(token.encode(), "s100_200_300");
    }

    #[test]
    fn sync_token_encode_simple() {
        let token = SyncToken {
            stream_id: 7,
            room_id: None,
            event_type: None,
            to_device_stream_id: None,
            device_list_stream_id: None,
        };
        assert_eq!(token.encode(), "s7");
    }

    #[test]
    fn sync_token_encode_with_to_device() {
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
    fn sync_token_parse_invalid_no_s_prefix() {
        assert!(SyncToken::parse("42").is_none());
    }

    #[test]
    fn sync_token_parse_invalid_empty() {
        assert!(SyncToken::parse("").is_none());
    }

    #[test]
    fn sync_token_parse_invalid_non_numeric() {
        assert!(SyncToken::parse("sabc").is_none());
    }

    #[test]
    fn sync_token_parse_partial_triplet_returns_none() {
        // Only one underscore: triplet parsing requires two underscores
        assert!(SyncToken::parse("s1_2").is_none());
    }

    #[test]
    fn sync_token_parse_negative_stream_id() {
        let token = SyncToken::parse("s-1_-2_-3").unwrap();
        assert_eq!(token.stream_id, -1);
        assert_eq!(token.to_device_stream_id, Some(-2));
        assert_eq!(token.device_list_stream_id, Some(-3));
    }
}
