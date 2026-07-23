//! E2EE storage domain group.
//!
//! Re-exports E2EE-related storage modules under a single namespace so
//! that new E2EE storage modules can be added here without touching
//! `lib.rs`.
//!
//! Consumers should prefer `synapse_storage::e2ee::DehydratedDeviceStorage`
//! over the flat `synapse_storage::DehydratedDeviceStorage`.

pub use crate::dehydrated_device::{
    DehydratedDevice, DehydratedDeviceStorage, DehydratedDeviceStoreApi, UpsertDehydratedDeviceParams,
};
pub use crate::e2ee_audit::{E2eeAuditStorage, E2eeAuditStoreApi, KeyAuditEntry, KeyEvent};
