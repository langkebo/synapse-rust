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

pub mod aliases;
pub mod backfill;
pub mod burn_after_read;
pub mod create;
pub mod create_events;
pub mod events;
pub mod federation_membership;
pub mod info;
pub mod infrastructure;
pub mod membership;
pub use membership::service::MembershipService;
pub mod messaging;
pub use messaging::service::MessagingService;
pub mod membership_actions;
pub mod membership_moderation;
pub mod messages;
pub mod read_markers;
pub mod receipts;
pub mod service;
pub mod space;
pub mod state;
pub use state::service::RoomStateService;
pub mod lifecycle;
pub use lifecycle::service::LifecycleService;

pub mod summary;
pub mod summary_state;
pub mod summary_stats;
pub mod tags;
pub mod upgrade;
pub mod utils;
