// =============================================================================
// Route handlers — business logic layer separated from HTTP route definitions
// =============================================================================
// Handlers contain the core logic for route operations (data validation, service
// calls, response formatting). They are decoupled from the HTTP endpoint
// definitions in `web/routes/` to maintain separation of concerns.
// =============================================================================
pub mod health;
pub mod presence;
pub mod room;
pub mod search;
pub mod sync;
pub mod thread;
pub mod versions;

pub use health::*;
pub use versions::*;
