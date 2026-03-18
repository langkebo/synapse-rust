//! Device Verification Module
//! 
//! Implements SAS (Short Authentication String) and QR code verification

pub mod models;
pub mod service;
pub mod storage;

pub use models::*;
pub use service::VerificationService;
pub use storage::VerificationStorage;
