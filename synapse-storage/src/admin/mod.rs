//! Admin storage domain group.
//!
//! Re-exports admin-related storage modules under a single namespace so
//! that new admin storage modules can be added here without touching
//! `lib.rs`.
//!
//! Consumers should prefer `synapse_storage::admin::AdminFederationStorage`
//! over the flat `synapse_storage::AdminFederationStorage`.

pub use crate::admin_federation::{
    AdminFederationStorage, AdminFederationStoreApi, FederationCacheRecord, FederationDestinationRecord,
    PendingFederationRecord,
};
pub use crate::admin_media::{
    decode_media_cursor, encode_media_cursor, AdminMediaInfo, AdminMediaPage, AdminMediaQuotaSummary,
    AdminMediaStorage, AdminMediaStoreApi, MediaCursor,
};
pub use crate::audit::{
    decode_audit_event_cursor, encode_audit_event_cursor, AuditEvent, AuditEventCursor, AuditEventFilters,
    AuditEventStorage, AuditEventStoreApi, CreateAuditEventRequest,
};
