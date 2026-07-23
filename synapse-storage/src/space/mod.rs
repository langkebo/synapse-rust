//! Space (Matrix spaces) storage: hierarchy, membership, and summary queries.
//!
//! Split from the original monolithic `space.rs` following the `event/` pattern:
//! - [`models`] — space structs and request/response types
//! - [`repository`] — [`SpaceStorage`] struct + inherent query methods
//! - [`api`] — [`SpaceStoreApi`] trait + its impl for [`SpaceStorage`]
//!
//! Space domain group — also re-exports `sticky_event` types under `space::`.
//! Consumers should prefer `synapse_storage::space::StickyEventStorage` over
//! the flat `synapse_storage::StickyEventStorage`.

mod api;
mod models;
mod repository;

pub use api::SpaceStoreApi;
pub use models::*;
pub use repository::SpaceStorage;

// Space domain group — re-exports sticky_event types under `space::`.
pub use crate::sticky_event::{StickyEvent, StickyEventStorage, StickyEventStoreApi};

#[cfg(test)]
mod db_tests;
#[cfg(test)]
mod tests;
