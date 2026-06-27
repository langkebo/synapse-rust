// =============================================================================
// Route handlers — business logic layer separated from HTTP route definitions
// =============================================================================
// Handlers contain the core logic for route operations (data validation, service
// calls, response formatting). They are decoupled from the HTTP endpoint
// definitions in `web/routes/` to maintain separation of concerns.
// =============================================================================
pub mod auth_discovery;
pub mod client_config;
pub mod dehydrated_device;
pub mod extended_profile;
pub mod health;
pub mod presence;
pub mod room;
pub mod rtc_transports;
pub mod search;
pub mod sync;
pub mod thread;
pub mod versions;

pub use health::*;
pub use versions::*;
