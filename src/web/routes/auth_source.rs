//! The trait seam the authentication extractors pull their dependencies through.
//!
//! Before this existed, every extractor (`AuthenticatedUser`,
//! `OptionalAuthenticatedUser`, `AdminUser`) had one `FromRequestParts` impl per
//! route context — a hand-maintained cartesian product of 21 impls, each a
//! near-verbatim copy of the same body that differed only in how it reached
//! `token_auth` / `admin_audit_service` / `user_service` / `security`. Adding a
//! context meant adding three more copies, and a fix to the auth sequence had to
//! be repeated in each.
//!
//! Now each context implements [`AuthSource`] (and, when it can authorize admins,
//! [`AdminAuthSource`]) once, and the extractors have a single generic impl. The
//! capability set is the set of trait impls below, so it is stated in one place
//! rather than implied by which copies happen to exist.

use std::sync::Arc;

use synapse_common::config::SecurityConfig;
use synapse_services::auth::TokenAuth;
use synapse_services::{AdminAuditService, UserService};

use super::context::{
    AdminContext, AuthContext, DeviceContext, E2eeRoomContext, FederationContext, MediaContext, RoomContext,
    SyncContext,
};
use super::AppState;

/// What the authentication extractors need from a route's state type.
pub trait AuthSource: Clone + Send + Sync + 'static {
    /// Validates bearer tokens.
    fn token_auth(&self) -> &Arc<dyn TokenAuth>;
    /// Records authenticated mutating requests on client routes.
    ///
    /// `None` disables the audit trail for that context — `AdminContext` and
    /// `AppState` always have one, the room/sync contexts carry it optionally.
    fn admin_audit_service(&self) -> Option<&AdminAuditService>;
}

/// The extra dependencies the `AdminUser` extractor needs.
///
/// Separate from [`AuthSource`] on purpose: only the contexts that can actually
/// authorize an admin endpoint implement it, so `AdminUser` cannot be used on a
/// context that has no way to check the admin role.
pub trait AdminAuthSource: AuthSource {
    /// Looks up the live user row (revocation check, role, MFA enrolment).
    fn user_service(&self) -> &Arc<UserService>;
    /// Admin RBAC / MFA rules.
    fn security_config(&self) -> &SecurityConfig;
}

macro_rules! optional_audit {
    ($ctx:ty) => {
        impl AuthSource for $ctx {
            fn token_auth(&self) -> &Arc<dyn TokenAuth> {
                &self.token_auth
            }
            fn admin_audit_service(&self) -> Option<&AdminAuditService> {
                self.admin_audit_service.as_deref()
            }
        }
    };
}

optional_audit!(RoomContext);
optional_audit!(E2eeRoomContext);
optional_audit!(SyncContext);
optional_audit!(DeviceContext);
optional_audit!(AuthContext);
optional_audit!(FederationContext);
optional_audit!(MediaContext);

/// `AdminContext` carries a mandatory audit service, so the trait's `Option` is
/// always `Some` here — that is what keeps `AuthenticatedUser`'s audit
/// unconditional on admin routes, exactly as before this trait existed.
impl AuthSource for AdminContext {
    fn token_auth(&self) -> &Arc<dyn TokenAuth> {
        &self.token_auth
    }
    fn admin_audit_service(&self) -> Option<&AdminAuditService> {
        Some(self.admin_audit_service.as_ref())
    }
}

impl AuthSource for AppState {
    fn token_auth(&self) -> &Arc<dyn TokenAuth> {
        &self.services.core.token_auth
    }
    fn admin_audit_service(&self) -> Option<&AdminAuditService> {
        Some(self.services.admin.security.admin_audit_service.as_ref())
    }
}

macro_rules! admin_auth_source {
    ($ctx:ty) => {
        impl AdminAuthSource for $ctx {
            fn user_service(&self) -> &Arc<UserService> {
                &self.user_service
            }
            fn security_config(&self) -> &SecurityConfig {
                &self.config.security
            }
        }
    };
}

admin_auth_source!(AdminContext);
admin_auth_source!(FederationContext);
admin_auth_source!(MediaContext);

impl AdminAuthSource for AppState {
    fn user_service(&self) -> &Arc<UserService> {
        &self.services.account.user_service
    }
    fn security_config(&self) -> &SecurityConfig {
        &self.services.core.config.security
    }
}
