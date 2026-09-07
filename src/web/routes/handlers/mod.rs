// =============================================================================
// Route handlers — business logic layer separated from HTTP route definitions
// =============================================================================
// Handlers contain the core logic for route operations (data validation, service
// calls, response formatting). They are decoupled from the HTTP endpoint
// definitions in `web/routes/` to maintain separation of concerns.
// =============================================================================
/// The `auth_discovery` module.
pub mod auth_discovery;
/// The `client_config` module.
pub mod client_config;
/// The `dehydrated_device` module.
pub mod dehydrated_device;
/// The `extended_profile` module.
pub mod extended_profile;
/// The `health` module.
pub mod health;
/// The `presence` module.
pub mod presence;
/// The `room` module.
pub mod room;
/// The `rtc_transports` module.
pub mod rtc_transports;
/// The `search` module.
pub mod search;
/// The `sync` module.
pub mod sync;
/// The `thread` module.
pub mod thread;
/// The `versions` module.
pub mod versions;

pub use health::*;
pub use versions::*;
