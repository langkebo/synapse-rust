// =============================================================================
// Room service module — consolidated room-related services
// =============================================================================
//
// This module combines room_service, room_summary_service, and space_service
// under a single `room/` module for structural convergence (Phase P2-1, P2-2).
//
// Backward-compatible re-exports are maintained in `services/mod.rs` via:
//   pub use room::service as room_service;
//   pub use room::summary as room_summary_service;
//   pub use room::space as space_service;

/// The `api_trait` module.
pub mod api_trait;
/// The `backfill` module.
pub mod backfill;
/// The `infrastructure` module.
pub mod infrastructure;
/// Canonical MSC3083 `allow`-array parsing shared by membership and summary.
mod join_rules;
/// The `lifecycle` module.
pub mod lifecycle;
pub use lifecycle::service::LifecycleService;
/// The `membership` module.
pub mod membership;
pub use membership::service::MembershipService;
/// The `messaging` module.
pub mod messaging;
pub use messaging::service::MessagingService;
/// The `service` module.
pub mod service;
/// The `space` module.
pub mod space;
/// The `state` module.
pub mod state;
pub use state::service::RoomStateService;
/// The `summary` module.
pub mod summary;
/// The `utils` module.
pub mod utils;

pub use api_trait::RoomServiceApi;

// Room domain group — re-exports room sub-module types and sibling room-related
// service modules (typing_service) under `room::` so that
// `pub use room::*;` in lib.rs covers the legacy flat re-exports.
pub use crate::typing_service::{TypingService, TypingUser};
pub use service::{CreateRoomConfig, RoomService, RoomServiceConfig, RoomTag, StickyEvent};
pub use space::SpaceService;
pub use summary::{
    CreateRoomSummaryRequest, CreateSummaryMemberRequest, RoomSummaryMember, RoomSummaryResponse, RoomSummaryService,
    RoomSummaryState, RoomSummaryStats, UpdateRoomSummaryRequest, UpdateSummaryMemberRequest,
};

// P7.4 — additional room-domain service re-exports (previously flat in lib.rs).
#[cfg(feature = "beacons")]
pub use crate::beacon_service::BeaconService;
#[cfg(feature = "friends")]
pub use crate::friend_room_service::{
    decode_friend_list_cursor, encode_friend_list_cursor, DirectMapUpdateAction, DirectRoomSnapshot, DmPartnerInfo,
    EnsureDirectRoomResult, FriendListCursor, FriendListEntry, FriendListPage, FriendListRequest,
    FriendRoomCreateRoomConfig, FriendRoomService,
};
pub use crate::relations_service::*;
pub use crate::retention_service::*;
pub use crate::thread_service::*;
