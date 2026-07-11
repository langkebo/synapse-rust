use crate::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use synapse_common::config::PerformanceConfig;
use synapse_common::*;
use synapse_e2ee::device_keys::DeviceKeyStorage;
use synapse_e2ee::key_rotation::KeyRotationStorage;
use synapse_e2ee::to_device::ToDeviceStorage;
use synapse_storage::room_account_data::RoomAccountDataStorage;

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
    pub presence_storage: std::sync::Arc<dyn synapse_storage::presence::PresenceStoreApi>,
    pub member_storage: Arc<dyn synapse_storage::membership::MemberStoreApi>,
    pub event_storage: Arc<dyn synapse_storage::event::EventStoreApi>,
    pub room_storage: Arc<dyn synapse_storage::room::RoomStoreApi>,
    pub room_account_data_storage: RoomAccountDataStorage,
    pub account_data_storage: Arc<dyn synapse_storage::account_data::AccountDataStoreApi>,
    pub filter_storage: Arc<dyn synapse_storage::filter::FilterStoreApi>,
    pub device_storage: Arc<dyn synapse_storage::device::DeviceListStoreApi>,
    pub device_key_storage: DeviceKeyStorage,
    pub key_rotation_storage: KeyRotationStorage,
    pub to_device_storage: ToDeviceStorage,
    pub metrics: Arc<MetricsCollector>,
    pub performance: PerformanceConfig,
    pub cache: Arc<synapse_cache::CacheManager>,
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

pub struct FetchEventsRequest<'a> {
    pub user_id: &'a str,
    pub device_id: Option<&'a str>,
    pub room_ids: &'a [String],
    pub since_token: Option<&'a SyncToken>,
    pub timeout: u64,
    pub limit: i64,
    pub timeline_filter: Option<&'a SyncFilter>,
    pub is_incremental: bool,
}

pub struct BuildSyncResponseRequest<'a> {
    pub user_id: &'a str,
    pub device_id: Option<&'a str>,
    pub room_ids: &'a [String],
    pub room_sections: &'a HashMap<String, SyncRoomSection>,
    pub room_events: HashMap<String, Vec<RoomEvent>>,
    pub response_filter: Option<&'a SyncResponseFilter>,
    pub timeline_limit: i64,
    pub since_token: &'a Option<SyncToken>,
    pub is_incremental: bool,
}

pub struct BuildRoomSyncRequest<'a> {
    pub room_id: &'a str,
    pub user_id: &'a str,
    pub device_id: Option<&'a str>,
    pub events: Vec<RoomEvent>,
    pub since_token: Option<&'a SyncToken>,
    pub is_incremental: bool,
    pub room_filter: Option<&'a RoomFilter>,
}

pub struct BuildRoomSyncValueRequest<'a> {
    pub events: Vec<RoomEvent>,
    pub state_list: Vec<Value>,
    pub ephemeral_events: Vec<Value>,
    pub account_data_events: Vec<Value>,
    pub timeline_limit: i64,
    pub counts: RoomSyncCounts,
    pub event_fields: Option<&'a [String]>,
    pub event_format: SyncEventFormat,
}

pub struct LazyLoadMembersRequest<'a> {
    pub state_events: Vec<Value>,
    pub timeline_events: &'a [RoomEvent],
    pub user_id: &'a str,
    pub device_id: Option<&'a str>,
    pub room_id: &'a str,
    pub room_filter: Option<&'a RoomFilter>,
    pub changed_member_ids: Option<&'a HashSet<String>>,
    pub timeline_limited: bool,
    pub enabled: bool,
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
pub enum SyncRoomSection {
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
pub struct SyncPerformanceSnapshot<'a> {
    pub request_kind: &'a str,
    pub user_id: &'a str,
    pub total_ms: f64,
    pub room_count: usize,
    pub event_count: usize,
    pub is_incremental: bool,
    pub phases: [(&'a str, f64); 3],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncrementalUpdate {
    Events,
    ToDevice,
    DeviceLists,
    Timeout,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LazyLoadedMembersCacheKey {
    pub user_id: String,
    pub device_id: Option<String>,
    pub room_id: String,
}

impl LazyLoadedMembersCacheKey {
    pub fn new(user_id: &str, device_id: Option<&str>, room_id: &str) -> Self {
        Self { user_id: user_id.to_string(), device_id: device_id.map(str::to_string), room_id: room_id.to_string() }
    }
}

pub struct StateEventsBatchParams<'a> {
    pub since_ts: i64,
    pub since_stream_ordering: Option<i64>,
    pub is_incremental: bool,
    pub lazy_load_members: bool,
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
