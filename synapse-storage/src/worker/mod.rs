//! Worker storage: distributed worker registry, task queue, and stream positions.
//!
//! Split from the original monolithic `worker.rs` following the `event/` pattern:
//! - [`models`] — worker enums, row types, and request/response structs
//! - [`repository`] — [`WorkerStorage`] struct + inherent query methods
//! - [`api`] — [`WorkerStoreApi`] trait + its impl for [`WorkerStorage`]

mod api;
mod models;
mod repository;

pub use api::WorkerStoreApi;
pub use models::*;
pub use repository::WorkerStorage;

#[cfg(test)]
mod db_tests;
#[cfg(test)]
mod tests;
