//! User storage domain.
//!
//! Split from the former single-file module (P1 large-file decomposition):
//! - [`models`] — entity struct definitions
//! - [`storage`] — [`UserStore`] trait and [`UserStorage`] implementation
//! - [`tests`] — unit tests (models only, no DB)
//! - [`db_tests`] — DB-backed integration tests

#[cfg(test)]
mod tests;

#[cfg(test)]
mod db_tests;

mod models;
mod storage;

pub use models::*;
#[cfg(test)]
pub(crate) use storage::escape_like_pattern;
pub use storage::{UserStorage, UserStore};
