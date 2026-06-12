use crate::common::*;
use crate::storage::{
    DeviceStorage, EventStorage, FilterStorage, PresenceStorage, RoomEvent, RoomMemberStorage, RoomStorage,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncToken {
    pub stream_id: i64,
    pub room_id: Option<String>,
    pub event_type: Option<String>,
    pub to_device_stream_id: Option<i64>,
    pub device_list_stream_id: Option<i64>,
}

impl SyncToken {
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

    pub fn encode(&self) -> String {
        match (self.to_device_stream_id, self.device_list_stream_id) {
            (Some(to_device), Some(device_list)) => {
                format!("s{}_{}_{}", self.stream_id, to_device, device_list)
            }
            _ => format!("s{}", self.stream_id),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SyncFilter {
    pub limit: Option<i64>,
    pub types: Option<Vec<String>>,
    pub not_types: Option<Vec<String>>,
    pub rooms: Option<Vec<String>>,
    pub not_rooms: Option<Vec<String>>,
    pub contains_url: Option<bool>,
    pub lazy_load_members: Option<bool>,
    pub include_redundant_members: Option<bool>,
    pub senders: Option<Vec<String>>,
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SyncEventFormat {
    #[default]
    Client,
    Federation,
}

#[derive(Debug, Clone, Default)]
pub struct RoomSyncCounts {
    pub highlight_count: i64,
    pub notification_count: i64,
}

pub struct SyncServiceDeps {
    pub presence_storage: PresenceStorage,
    pub member_storage: RoomMemberStorage,
    pub event_storage: EventStorage,
    pub room_storage: RoomStorage,
    pub filter_storage: FilterStorage,
    pub device_storage: DeviceStorage,
    pub to_device_storage: crate::e2ee::to_device::ToDeviceStorage,
    pub metrics: Arc<MetricsCollector>,
    pub performance: crate::common::config::PerformanceConfig,
}

pub struct SyncServiceRequest<'a> {
    pub user_id: &'a str,
    pub device_id: Option<&'a str>,
    pub timeout: u64,
    pub is_full_state: bool,
    pub set_presence: &'a str,
    pub filter_id: Option<&'a str>,
    pub since: Option<&'a str>,
}

pub(crate) struct FetchEventsRequest<'a> {
    pub(crate) user_id: &'a str,
    pub(crate) device_id: Option<&'a str>,
    pub(crate) room_ids: &'a [String],
    pub(crate) since_token: Option<&'a SyncToken>,
    pub(crate) timeout: u64,
    pub(crate) limit: i64,
    pub(crate) timeline_filter: Option<&'a SyncFilter>,
    pub(crate) is_incremental: bool,
}

pub(crate) struct BuildSyncResponseRequest<'a> {
    pub(crate) user_id: &'a str,
    pub(crate) device_id: Option<&'a str>,
    pub(crate) room_ids: &'a [String],
    pub(crate) room_sections: &'a HashMap<String, SyncRoomSection>,
    pub(crate) room_events: HashMap<String, Vec<RoomEvent>>,
    pub(crate) response_filter: Option<&'a SyncResponseFilter>,
    pub(crate) timeline_limit: i64,
    pub(crate) since_token: &'a Option<SyncToken>,
    pub(crate) is_incremental: bool,
}

pub(crate) struct BuildRoomSyncRequest<'a> {
    pub(crate) room_id: &'a str,
    pub(crate) user_id: &'a str,
    pub(crate) device_id: Option<&'a str>,
    pub(crate) events: Vec<RoomEvent>,
    pub(crate) since_token: Option<&'a SyncToken>,
    pub(crate) is_incremental: bool,
    pub(crate) room_filter: Option<&'a RoomFilter>,
}

pub(crate) struct BuildRoomSyncValueRequest<'a> {
    pub(crate) events: Vec<RoomEvent>,
    pub(crate) state_list: Vec<Value>,
    pub(crate) ephemeral_events: Vec<Value>,
    pub(crate) account_data_events: Vec<Value>,
    pub(crate) timeline_limit: i64,
    pub(crate) counts: RoomSyncCounts,
    pub(crate) event_fields: Option<&'a [String]>,
    pub(crate) event_format: SyncEventFormat,
}

pub(crate) struct LazyLoadMembersRequest<'a> {
    pub(crate) state_events: Vec<Value>,
    pub(crate) timeline_events: &'a [RoomEvent],
    pub(crate) user_id: &'a str,
    pub(crate) device_id: Option<&'a str>,
    pub(crate) room_id: &'a str,
    pub(crate) room_filter: Option<&'a RoomFilter>,
    pub(crate) changed_member_ids: Option<&'a HashSet<String>>,
    pub(crate) timeline_limited: bool,
    pub(crate) enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomFilter {
    pub rooms: Option<Vec<String>>,
    pub not_rooms: Option<Vec<String>>,
    pub include_leave: Option<bool>,
    pub state: Option<SyncFilter>,
    pub timeline: Option<SyncFilter>,
    pub ephemeral: Option<SyncFilter>,
    pub account_data: Option<SyncFilter>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResponseFilter {
    pub event_fields: Option<Vec<String>>,
    pub event_format: SyncEventFormat,
    pub room: Option<RoomFilter>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SyncRoomSection {
    Join,
    Leave,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRequest {
    pub since: Option<String>,
    pub filter: Option<String>,
    #[serde(rename = "full_state")]
    pub is_full_state: bool,
    pub set_presence: Option<String>,
    pub timeout: u64,
}

#[derive(Debug, Clone)]
pub struct SyncState {
    pub rooms: HashMap<String, RoomSyncState>,
    pub last_stream_id: i64,
}

#[derive(Debug, Clone)]
pub struct RoomSyncState {
    pub timeline_limit: i64,
    pub last_event_id: Option<String>,
    pub last_stream_id: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct SyncPerformanceSnapshot<'a> {
    pub(crate) request_kind: &'a str,
    pub(crate) user_id: &'a str,
    pub(crate) total_ms: f64,
    pub(crate) room_count: usize,
    pub(crate) event_count: usize,
    pub(crate) is_incremental: bool,
    pub(crate) phases: [(&'a str, f64); 3],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IncrementalUpdate {
    Events,
    ToDevice,
    DeviceLists,
    Timeout,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct LazyLoadedMembersCacheKey {
    user_id: String,
    device_id: Option<String>,
    room_id: String,
}

impl LazyLoadedMembersCacheKey {
    pub(crate) fn new(user_id: &str, device_id: Option<&str>, room_id: &str) -> Self {
        Self { user_id: user_id.to_string(), device_id: device_id.map(str::to_string), room_id: room_id.to_string() }
    }
}

pub(crate) struct StateEventsBatchParams<'a> {
    pub(crate) since_ts: i64,
    pub(crate) since_stream_ordering: Option<i64>,
    pub(crate) is_incremental: bool,
    pub(crate) lazy_load_members: bool,
    pub(crate) user_id: &'a str,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========== SyncToken parse/encode tests ==========

    #[test]
    fn test_sync_token_parse_simple() {
        let token = SyncToken::parse("s12345").unwrap();
        assert_eq!(token.stream_id, 12345);
        assert_eq!(token.to_device_stream_id, None);
        assert_eq!(token.device_list_stream_id, None);
    }

    #[test]
    fn test_sync_token_parse_with_to_device_and_device_list() {
        let token = SyncToken::parse("s100_200_300").unwrap();
        assert_eq!(token.stream_id, 100);
        assert_eq!(token.to_device_stream_id, Some(200));
        assert_eq!(token.device_list_stream_id, Some(300));
    }

    #[test]
    fn test_sync_token_parse_no_s_prefix() {
        assert!(SyncToken::parse("12345").is_none());
    }

    #[test]
    fn test_sync_token_parse_invalid_number() {
        assert!(SyncToken::parse("sabc").is_none());
    }

    #[test]
    fn test_sync_token_parse_empty() {
        assert!(SyncToken::parse("").is_none());
    }

    #[test]
    fn test_sync_token_parse_just_s() {
        assert!(SyncToken::parse("s").is_none());
    }

    #[test]
    fn test_sync_token_parse_zero() {
        let token = SyncToken::parse("s0").unwrap();
        assert_eq!(token.stream_id, 0);
    }

    #[test]
    fn test_sync_token_parse_negative() {
        let token = SyncToken::parse("s-1").unwrap();
        assert_eq!(token.stream_id, -1);
    }

    #[test]
    fn test_sync_token_parse_large_number() {
        let token = SyncToken::parse("s9223372036854775807").unwrap();
        assert_eq!(token.stream_id, i64::MAX);
    }

    #[test]
    fn test_sync_token_parse_with_invalid_to_device() {
        assert!(SyncToken::parse("s100_abc_300").is_none());
    }

    #[test]
    fn test_sync_token_parse_with_missing_device_list() {
        // Only one underscore after the stream_id, can't parse to_device and device_list
        assert!(SyncToken::parse("s100_200").is_none());
    }

    #[test]
    fn test_sync_token_encode_simple() {
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
    fn test_sync_token_encode_with_to_device() {
        let token = SyncToken {
            stream_id: 10,
            room_id: None,
            event_type: None,
            to_device_stream_id: Some(20),
            device_list_stream_id: None,
        };
        // Only one is Some, so it falls to the simple format
        assert_eq!(token.encode(), "s10");
    }

    #[test]
    fn test_sync_token_encode_with_device_list_only() {
        let token = SyncToken {
            stream_id: 10,
            room_id: None,
            event_type: None,
            to_device_stream_id: None,
            device_list_stream_id: Some(30),
        };
        assert_eq!(token.encode(), "s10");
    }

    #[test]
    fn test_sync_token_encode_with_both() {
        let token = SyncToken {
            stream_id: 1,
            room_id: None,
            event_type: None,
            to_device_stream_id: Some(2),
            device_list_stream_id: Some(3),
        };
        assert_eq!(token.encode(), "s1_2_3");
    }

    #[test]
    fn test_sync_token_roundtrip_simple() {
        let original = SyncToken {
            stream_id: 999,
            room_id: None,
            event_type: None,
            to_device_stream_id: None,
            device_list_stream_id: None,
        };
        let encoded = original.encode();
        let parsed = SyncToken::parse(&encoded).unwrap();
        assert_eq!(parsed.stream_id, original.stream_id);
        assert_eq!(parsed.to_device_stream_id, original.to_device_stream_id);
        assert_eq!(parsed.device_list_stream_id, original.device_list_stream_id);
    }

    #[test]
    fn test_sync_token_roundtrip_full() {
        let original = SyncToken {
            stream_id: 500,
            room_id: None,
            event_type: None,
            to_device_stream_id: Some(600),
            device_list_stream_id: Some(700),
        };
        let encoded = original.encode();
        let parsed = SyncToken::parse(&encoded).unwrap();
        assert_eq!(parsed.stream_id, original.stream_id);
        assert_eq!(parsed.to_device_stream_id, original.to_device_stream_id);
        assert_eq!(parsed.device_list_stream_id, original.device_list_stream_id);
    }

    // ========== SyncFilter default tests ==========

    #[test]
    fn test_sync_filter_default() {
        let filter = SyncFilter::default();
        assert_eq!(filter.limit, Some(100));
        assert!(filter.types.is_none());
        assert!(filter.not_types.is_none());
        assert!(filter.rooms.is_none());
        assert!(filter.not_rooms.is_none());
        assert!(filter.contains_url.is_none());
        assert!(filter.lazy_load_members.is_none());
        assert!(filter.include_redundant_members.is_none());
        assert!(filter.senders.is_none());
        assert!(filter.not_senders.is_none());
    }

    #[test]
    fn test_sync_filter_custom() {
        let filter = SyncFilter {
            limit: Some(50),
            types: Some(vec!["m.room.message".to_string()]),
            not_types: None,
            rooms: None,
            not_rooms: None,
            contains_url: Some(true),
            lazy_load_members: Some(true),
            include_redundant_members: None,
            senders: None,
            not_senders: None,
        };
        assert_eq!(filter.limit, Some(50));
        assert_eq!(filter.types, Some(vec!["m.room.message".to_string()]));
        assert_eq!(filter.contains_url, Some(true));
        assert_eq!(filter.lazy_load_members, Some(true));
    }

    // ========== SyncEventFormat tests ==========

    #[test]
    fn test_sync_event_format_default() {
        assert_eq!(SyncEventFormat::default(), SyncEventFormat::Client);
    }

    #[test]
    fn test_sync_event_format_serialization() {
        let client = SyncEventFormat::Client;
        let json = serde_json::to_string(&client).unwrap();
        assert_eq!(json, "\"client\"");

        let federation = SyncEventFormat::Federation;
        let json = serde_json::to_string(&federation).unwrap();
        assert_eq!(json, "\"federation\"");
    }

    #[test]
    fn test_sync_event_format_deserialization() {
        let client: SyncEventFormat = serde_json::from_str("\"client\"").unwrap();
        assert_eq!(client, SyncEventFormat::Client);

        let federation: SyncEventFormat = serde_json::from_str("\"federation\"").unwrap();
        assert_eq!(federation, SyncEventFormat::Federation);
    }

    // ========== RoomFilter default tests ==========

    #[test]
    fn test_room_filter_default() {
        let filter = RoomFilter::default();
        assert_eq!(filter.include_leave, Some(false));
        assert!(filter.state.is_some());
        assert!(filter.timeline.is_some());
        assert!(filter.ephemeral.is_some());
        assert!(filter.account_data.is_some());
    }

    // ========== SyncResponseFilter default tests ==========

    #[test]
    fn test_sync_response_filter_default() {
        let filter = SyncResponseFilter::default();
        assert!(filter.event_fields.is_none());
        assert_eq!(filter.event_format, SyncEventFormat::Client);
        assert!(filter.room.is_some());
        assert!(filter.presence.is_some());
    }

    // ========== LazyLoadedMembersCacheKey tests ==========

    #[test]
    fn test_lazy_loaded_members_cache_key() {
        let key = LazyLoadedMembersCacheKey::new("@alice:example.com", Some("device1"), "!room:example.com");
        assert_eq!(key.user_id, "@alice:example.com");
        assert_eq!(key.device_id, Some("device1".to_string()));
        assert_eq!(key.room_id, "!room:example.com");
    }

    #[test]
    fn test_lazy_loaded_members_cache_key_no_device() {
        let key = LazyLoadedMembersCacheKey::new("@bob:example.com", None, "!room:example.com");
        assert_eq!(key.user_id, "@bob:example.com");
        assert_eq!(key.device_id, None);
        assert_eq!(key.room_id, "!room:example.com");
    }

    #[test]
    fn test_sync_room_section() {
        assert_eq!(SyncRoomSection::Join, SyncRoomSection::Join);
        assert_ne!(SyncRoomSection::Join, SyncRoomSection::Leave);
    }

    #[test]
    fn test_incremental_update() {
        assert_eq!(IncrementalUpdate::Events, IncrementalUpdate::Events);
        assert_ne!(IncrementalUpdate::Events, IncrementalUpdate::Timeout);
        assert_ne!(IncrementalUpdate::ToDevice, IncrementalUpdate::DeviceLists);
    }
}
