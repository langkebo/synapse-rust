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

pub mod api_trait;
pub mod backfill;
pub mod infrastructure;
pub mod lifecycle;
pub use lifecycle::service::LifecycleService;
pub mod membership;
pub use membership::service::MembershipService;
pub mod messaging;
pub use messaging::service::MessagingService;
pub mod service;
pub mod space;
pub mod state;
pub use state::service::RoomStateService;
pub mod summary;
pub mod utils;

pub use api_trait::RoomServiceApi;

// Room domain group — re-exports room sub-module types and sibling room-related
// service modules (directory_service, typing_service) under `room::` so that
// `pub use room::*;` in lib.rs covers the legacy flat re-exports.
pub use crate::directory_service::{DirectoryRoom, DirectoryService};
pub use crate::typing_service::{TypingService, TypingUser};
pub use service::{CreateRoomConfig, RoomService, RoomServiceConfig};
pub use space::SpaceService;
pub use summary::{
    CreateRoomSummaryRequest, CreateSummaryMemberRequest, RoomSummaryMember, RoomSummaryResponse, RoomSummaryService,
    RoomSummaryState, RoomSummaryStats, UpdateRoomSummaryRequest, UpdateSummaryMemberRequest,
};
