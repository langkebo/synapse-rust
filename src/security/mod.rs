//! Security helpers (Sprint 4+).
//!
//! Currently exposes:
//!   - `invite_signature`: HMAC-SHA256 binding for room-invite links.
//!   - `device_binding`: HMAC-SHA256 binding for device-signing-key uploads.

/// The `device_binding` module.
pub mod device_binding;
/// The `invite_signature` module.
pub mod invite_signature;

#[cfg(test)]
mod invite_cross_service_tests;
