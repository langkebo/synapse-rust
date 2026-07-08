//! Room summary storage: per-room summary rows, members, and heroes.
//!
//! Split from the original monolithic `room_summary.rs` following the
//! `event/` pattern:
//! - [`models`] — summary structs, request/response types, and model impls
//! - [`repository`] — [`RoomSummaryStorage`] struct + inherent query methods
//! - [`api`] — [`RoomSummaryStoreApi`] trait + its impl for [`RoomSummaryStorage`]

mod api;
mod models;
mod repository;

pub use api::RoomSummaryStoreApi;
pub use models::*;
pub use repository::RoomSummaryStorage;

#[cfg(test)]
mod db_tests;
#[cfg(test)]
mod tests;
