//! Admin services domain group.
//!
//! Re-exports all admin-related service modules under a single namespace so
//! that new admin services can be added here without touching `lib.rs`.
//!
//! Consumers may use either:
//! - `synapse_services::admin::AdminAuditService` (preferred, grouped path)
//! - `synapse_services::AdminAuditService`      (legacy flat path, via `pub use admin::*` in lib.rs)

pub use crate::admin_audit_service::AdminAuditService;
pub use crate::admin_federation_service::{
    decode_destination_cursor, decode_pending_federation_cursor, encode_destination_cursor,
    encode_pending_federation_cursor, AdminFederationService, ConfirmFederationResult, DestinationCursor,
    DestinationInfo, FederationCacheEntry, PendingFederationCursor, PendingFederationInfo, ResolveFederationResult,
};
pub use crate::admin_registration_service::{
    AdminRegisterRequest, AdminRegisterResponse, AdminRegistrationService, NonceResponse,
};
pub use crate::admin_user_service::{
    decode_user_cursor, encode_user_cursor, AdminEvictionFailure, AdminLegacyUsersPage, AdminSingleUserStats,
    AdminUserCursor, AdminUserDetails, AdminUserDeviceInfo, AdminUserEvictionResult, AdminUserListItem,
    AdminUserProfile, AdminUserService, AdminUserStats, AdminUsersPage, BatchUsersResult,
};
