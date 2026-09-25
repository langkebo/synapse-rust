/// The `aliases` module.
pub mod aliases;
/// `auth_events` selection for locally-created events (spec "Auth events selection").
pub mod auth_events;
/// Domain error types for room state.
pub mod error;
/// The `info` module.
pub mod info;
/// The `service` module.
pub mod service;
/// The `tags` module.
pub mod tags;
pub use error::RoomStateError;
