//! Space (Matrix spaces) storage: hierarchy, membership, and summary queries.
//!
//! Split from the original monolithic `space.rs` following the `event/` pattern:
//! - [`models`] — space structs and request/response types
//! - [`repository`] — [`SpaceStorage`] struct + inherent query methods
//! - [`api`] — [`SpaceStoreApi`] trait + its impl for [`SpaceStorage`]

mod api;
mod models;
mod repository;

pub use api::SpaceStoreApi;
pub use models::*;
pub use repository::SpaceStorage;

#[cfg(test)]
mod db_tests;
#[cfg(test)]
mod tests;
