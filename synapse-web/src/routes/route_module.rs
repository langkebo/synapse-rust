use axum::extract::FromRef;
use axum::Router;

#[cfg(feature = "burn-after-read")]
use crate::routes::burn_after_read;
#[cfg(feature = "cas-sso")]
use crate::routes::cas;
use crate::routes::context::SsoContext;
#[cfg(feature = "external-services")]
use crate::routes::external_service;
#[cfg(feature = "friends")]
use crate::routes::friend_room;
#[cfg(feature = "saml-sso")]
use crate::routes::saml;
#[cfg(feature = "voice-extended")]
use crate::routes::voice;
#[cfg(feature = "widgets")]
use crate::routes::widget;
use crate::routes::{federation, oidc, room, state::AppState, worker};

/// Pure-data profile flags that select the conditional route surfaces exposed
/// by `derived_routes::derived_route_manifest`. Used by the offline
/// ledger-export tool (`synapse_ledger_export` binary) and any other consumer
/// that needs to enumerate the manifest without standing up a full `AppState`
/// (which requires a Postgres pool).
///
/// Live router assembly projects `AppState` through [`ProfileFlags::from_state`]
/// and then reads the same derived table, so the offline and online views can
/// never drift apart.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProfileFlags {
    /// The `oidc_enabled` field.
    pub oidc_enabled: bool,
    /// The `worker_enabled` field.
    pub worker_enabled: bool,
    /// The `saml_enabled` field.
    pub saml_enabled: bool,
}

impl ProfileFlags {
    /// Project a live `AppState` down to the boolean flags that select the
    /// conditional route surfaces in `derived_routes`. Every flag here must be
    /// readable without a database round-trip.
    pub fn from_state(state: &AppState) -> Self {
        #[cfg(feature = "saml-sso")]
        let saml_enabled = state.services.sso.saml_service.is_enabled();
        #[cfg(not(feature = "saml-sso"))]
        let saml_enabled = false;
        Self {
            oidc_enabled: oidc::oidc_enabled(&SsoContext::from_ref(state)),
            worker_enabled: state.services.core.config.worker.enabled,
            saml_enabled,
        }
    }

    /// Convenience: every conditional route surface off. Equivalent to a
    /// freshly-default `ProfileFlags` and used as the canonical "minimal"
    /// profile for offline tooling.
    pub const DEFAULT: Self = Self { oidc_enabled: false, worker_enabled: false, saml_enabled: false };

    /// Convenience: every conditional route surface on, i.e. the widest
    /// [`RouteProfile`] ceiling. Offline consumers that need the full
    /// `#[cfg]`-visible route surface (capability gating, contract tests) use
    /// this so no row is filtered out by the runtime ceiling.
    ///
    /// [`RouteProfile`]: super::derived_routes::RouteProfile
    pub const ALL: Self = Self { oidc_enabled: true, worker_enabled: true, saml_enabled: true };
}

/// State-aware route modules that participate in live Axum assembly.
///
/// Contributor rule: route *metadata* is no longer declared here — it is
/// derived from the `.route(...)` registration sites by
/// `scripts/contract/extract_registered.py` and materialised into
/// `derived_routes.rs`. A `RouteModule` therefore only owns `merge_into`; if a
/// new feature-gated route is merged through assembly, the same PR must
/// regenerate `derived_routes.rs` (`scripts/contract/gen_derived_routes.py`)
/// so the contract gate stays green.
pub trait RouteModule: Send + Sync {
    /// Merge this module's routes into the given router.
    fn merge_into(&self, router: Router<AppState>, state: AppState) -> Router<AppState>;
}

/// The `RoomModule` struct.
pub struct RoomModule;
/// The `FederationModule` struct.
pub struct FederationModule;
/// The `OidcModule` struct.
pub struct OidcModule;
/// The `WorkerBodyModule` struct.
pub struct WorkerBodyModule;
/// The `SamlModule` struct.
#[cfg(feature = "saml-sso")]
pub struct SamlModule;
/// The `CasModule` struct.
#[cfg(feature = "cas-sso")]
pub struct CasModule;
/// The `BurnAfterReadModule` struct.
#[cfg(feature = "burn-after-read")]
pub struct BurnAfterReadModule;
/// The `WidgetModule` struct.
#[cfg(feature = "widgets")]
pub struct WidgetModule;
/// The `FriendModule` struct.
#[cfg(feature = "friends")]
pub struct FriendModule;
/// The `VoiceModule` struct.
#[cfg(feature = "voice-extended")]
pub struct VoiceModule;
/// The `ExternalServiceModule` struct.
#[cfg(feature = "external-services")]
pub struct ExternalServiceModule;

/// Static `ROOM_MODULE`.
pub static ROOM_MODULE: RoomModule = RoomModule;
/// Static `FEDERATION_MODULE`.
pub static FEDERATION_MODULE: FederationModule = FederationModule;
/// Static `OIDC_MODULE`.
pub static OIDC_MODULE: OidcModule = OidcModule;
/// Static `WORKER_BODY_MODULE`.
pub static WORKER_BODY_MODULE: WorkerBodyModule = WorkerBodyModule;
/// Static `SAML_MODULE`.
#[cfg(feature = "saml-sso")]
pub static SAML_MODULE: SamlModule = SamlModule;
/// Static `CAS_MODULE`.
#[cfg(feature = "cas-sso")]
pub static CAS_MODULE: CasModule = CasModule;
/// Static `BURN_AFTER_READ_MODULE`.
#[cfg(feature = "burn-after-read")]
pub static BURN_AFTER_READ_MODULE: BurnAfterReadModule = BurnAfterReadModule;
/// Static `WIDGET_MODULE`.
#[cfg(feature = "widgets")]
pub static WIDGET_MODULE: WidgetModule = WidgetModule;
/// Static `FRIEND_MODULE`.
#[cfg(feature = "friends")]
pub static FRIEND_MODULE: FriendModule = FriendModule;
/// Static `VOICE_MODULE`.
#[cfg(feature = "voice-extended")]
pub static VOICE_MODULE: VoiceModule = VoiceModule;
/// Static `EXTERNAL_SERVICE_MODULE`.
#[cfg(feature = "external-services")]
pub static EXTERNAL_SERVICE_MODULE: ExternalServiceModule = ExternalServiceModule;

/// Ordered list of state-aware route modules appended by
/// `assembly::declared_ledger_for(&AppState)` and
/// `assembly::create_router`.
///
/// Keep this list aligned with feature-gated router assembly. Adding a new
/// conditional route surface without updating this list (or the explicit
/// compat-manifest path in `assembly.rs`) is treated as a regression.
pub fn route_modules() -> Vec<&'static dyn RouteModule> {
    let mut modules: Vec<&'static dyn RouteModule> = vec![&ROOM_MODULE, &FEDERATION_MODULE];
    #[cfg(feature = "saml-sso")]
    modules.push(&SAML_MODULE);
    modules.push(&OIDC_MODULE);
    modules.push(&WORKER_BODY_MODULE);
    #[cfg(feature = "cas-sso")]
    modules.push(&CAS_MODULE);
    #[cfg(feature = "burn-after-read")]
    modules.push(&BURN_AFTER_READ_MODULE);
    #[cfg(feature = "widgets")]
    modules.push(&WIDGET_MODULE);
    #[cfg(feature = "friends")]
    modules.push(&FRIEND_MODULE);
    #[cfg(feature = "voice-extended")]
    modules.push(&VOICE_MODULE);
    #[cfg(feature = "external-services")]
    modules.push(&EXTERNAL_SERVICE_MODULE);
    modules
}

impl RouteModule for RoomModule {
    fn merge_into(&self, router: Router<AppState>, _state: AppState) -> Router<AppState> {
        router.merge(room::create_room_router())
    }
}

impl RouteModule for FederationModule {
    fn merge_into(&self, router: Router<AppState>, state: AppState) -> Router<AppState> {
        router.merge(federation::create_federation_router(&state))
    }
}

impl RouteModule for OidcModule {
    fn merge_into(&self, router: Router<AppState>, state: AppState) -> Router<AppState> {
        let sso_ctx = SsoContext::from_ref(&state);
        if oidc::oidc_enabled(&sso_ctx) {
            router.merge(oidc::create_oidc_router(state))
        } else {
            router.merge(oidc::create_oidc_fallback_router())
        }
    }
}

impl RouteModule for WorkerBodyModule {
    fn merge_into(&self, router: Router<AppState>, state: AppState) -> Router<AppState> {
        // The worker-side (body) surface is only meaningful when HTTP
        // replication is switched on, and that same switch is what
        // `replication_http_auth_middleware` consults before demanding the shared
        // secret:
        //
        //     if !ctx.config.worker.replication.http.enabled { return next.run(request).await; }
        //
        // Mounting on `worker.enabled` alone therefore exposed these routes behind
        // a pass-through middleware: with `worker.enabled: true` and the default
        // `replication.http.enabled: false`, anyone could PUT replication
        // positions, read the event stream, and forge worker heartbeats / command
        // completions without any credential. Require both switches.
        let worker = &state.services.core.config.worker;
        if worker.enabled && worker.replication.http.enabled {
            router.merge(worker::create_worker_body_router(&state))
        } else {
            router
        }
    }
}

#[cfg(feature = "saml-sso")]
impl RouteModule for SamlModule {
    fn merge_into(&self, router: Router<AppState>, state: AppState) -> Router<AppState> {
        router.merge(saml::create_saml_router(state))
    }
}

#[cfg(feature = "cas-sso")]
impl RouteModule for CasModule {
    fn merge_into(&self, router: Router<AppState>, state: AppState) -> Router<AppState> {
        router.merge(cas::cas_routes(state))
    }
}

#[cfg(feature = "burn-after-read")]
impl RouteModule for BurnAfterReadModule {
    fn merge_into(&self, router: Router<AppState>, state: AppState) -> Router<AppState> {
        router.merge(burn_after_read::create_burn_after_read_router(state))
    }
}

#[cfg(feature = "widgets")]
impl RouteModule for WidgetModule {
    fn merge_into(&self, router: Router<AppState>, _state: AppState) -> Router<AppState> {
        router.merge(widget::create_widget_router())
    }
}

#[cfg(feature = "friends")]
impl RouteModule for FriendModule {
    fn merge_into(&self, router: Router<AppState>, state: AppState) -> Router<AppState> {
        router.merge(friend_room::create_friend_router(state))
    }
}

#[cfg(feature = "voice-extended")]
impl RouteModule for VoiceModule {
    fn merge_into(&self, router: Router<AppState>, state: AppState) -> Router<AppState> {
        router.merge(voice::create_voice_router(state))
    }
}

#[cfg(feature = "external-services")]
impl RouteModule for ExternalServiceModule {
    fn merge_into(&self, router: Router<AppState>, state: AppState) -> Router<AppState> {
        router.merge(external_service::create_external_service_router(state))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::derived_routes::derived_route_manifest;
    use crate::routes::route_ledger::RouteEntry;
    use axum::http::Method;

    /// Widest possible feature ceiling: `oidc_enabled` selects
    /// [`RouteProfile::Oidc`], the highest rank, so every `#[cfg]`-visible row
    /// in the derived table is surfaced.
    fn all_routes() -> Vec<RouteEntry> {
        derived_route_manifest(&ProfileFlags { oidc_enabled: true, worker_enabled: true, saml_enabled: true })
    }

    fn contains(entries: &[RouteEntry], method: &Method, path: &str) -> bool {
        entries.iter().any(|entry| entry.method == method && entry.path == path)
    }

    /// The derived table must carry the friend routes when `friends` is on.
    #[cfg(feature = "friends")]
    #[test]
    fn friend_manifest_declares_core_routes() {
        let entries = all_routes();
        assert!(contains(&entries, &Method::GET, "/_matrix/client/v3/friends"));
        assert!(contains(&entries, &Method::DELETE, "/_matrix/client/v1/friends/{user_id}"));
    }

    /// The derived table must carry the SAML routes when `saml-sso` is on.
    #[cfg(feature = "saml-sso")]
    #[test]
    fn saml_manifest_declares_core_routes() {
        let entries = all_routes();
        assert!(contains(&entries, &Method::GET, "/_matrix/client/v3/login/sso/redirect/saml"));
        assert!(contains(&entries, &Method::POST, "/_synapse/admin/v1/saml/metadata/refresh"));
    }

    /// The derived table must carry the CAS routes when `cas-sso` is on.
    #[cfg(feature = "cas-sso")]
    #[test]
    fn cas_manifest_declares_core_routes() {
        let entries = all_routes();
        assert!(contains(&entries, &Method::GET, "/login"));
        assert!(contains(&entries, &Method::GET, "/_synapse/admin/v1/cas/services"));
    }

    /// The derived table must carry the widget routes when `widgets` is on.
    #[cfg(feature = "widgets")]
    #[test]
    fn widget_manifest_declares_core_routes() {
        let entries = all_routes();
        assert!(contains(&entries, &Method::POST, "/_matrix/client/v1/widgets"));
        assert!(contains(&entries, &Method::GET, "/_matrix/client/v1/widgets/{widget_id}/config"));
    }

    /// The derived table must carry the burn-after-read routes when the feature is on.
    #[cfg(feature = "burn-after-read")]
    #[test]
    fn burn_after_read_manifest_declares_core_routes() {
        let entries = all_routes();
        assert!(contains(&entries, &Method::PUT, "/_matrix/client/v1/rooms/{room_id}/burn"));
        assert!(contains(&entries, &Method::GET, "/_matrix/client/v1/user/burn/stats"));
    }

    /// The derived table must carry the voice routes when `voice-extended` is on.
    #[cfg(feature = "voice-extended")]
    #[test]
    fn voice_manifest_declares_core_routes() {
        let entries = all_routes();
        assert!(contains(&entries, &Method::GET, "/_matrix/client/v3/voice/config"));
        assert!(contains(&entries, &Method::GET, "/_matrix/client/v1/voice/config"));
        assert!(contains(&entries, &Method::POST, "/_matrix/client/v3/voice/upload"));
    }

    /// The derived table must carry the external-service routes when the feature is on.
    #[cfg(feature = "external-services")]
    #[test]
    fn external_service_manifest_declares_core_routes() {
        let entries = all_routes();
        assert!(contains(&entries, &Method::GET, "/_synapse/admin/v1/external_services"));
        assert!(contains(&entries, &Method::POST, "/_synapse/external/webhook/{service_id}"));
    }
}
