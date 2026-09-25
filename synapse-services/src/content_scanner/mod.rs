/// The `models` module.
pub mod models;
/// The `service` module.
pub mod service;
pub mod verdict;

// Re-export the main service struct for convenience
pub use service::ContentScanner;
pub use verdict::{enforce_scan_verdict, scan_when_enabled};
// Re-export common types
pub use models::*;
