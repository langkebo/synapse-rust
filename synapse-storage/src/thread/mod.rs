//! Thread storage domain.
//!
//! Split from the former single-file module (P1 large-file decomposition):
//! - [`models`] — entity and parameter structs
//! - [`storage`] — [`storage::ThreadStorage`] pool methods and the [`storage::ThreadStoreApi`] trait

#[cfg(test)]
mod db_tests;
mod models;
mod storage;
#[cfg(test)]
mod tests;

pub use models::*;
pub use storage::*;
