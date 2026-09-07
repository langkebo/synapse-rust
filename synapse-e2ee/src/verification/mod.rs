//! Device Verification Module
//!
//! Implements SAS (Short Authentication String) and QR code verification

/// The `models` module.
pub mod models;
/// The `service` module.
pub mod service;
/// The `storage` module.
pub mod storage;

pub use models::*;
pub use service::VerificationService;
pub use storage::VerificationStorage;
