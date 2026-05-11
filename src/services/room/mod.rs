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

pub mod service;
pub mod space;
pub mod summary;
