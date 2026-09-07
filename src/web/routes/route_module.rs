use axum::extract::FromRef;
use axum::Router;

#[cfg(feature = "burn-after-read")]
use crate::web::routes::burn_after_read;
#[cfg(feature = "cas-sso")]
use crate::web::routes::cas;
use crate::web::routes::context::SsoContext;
#[cfg(feature = "external-services")]
use crate::web::routes::external_service;
#[cfg(feature = "friends")]
use crate::web::routes::friend_room;
#[cfg(feature = "saml-sso")]
use crate::web::routes::saml;
#[cfg(feature = "voice-extended")]
use crate::web::routes::voice;
#[cfg(feature = "widgets")]
use crate::web::routes::widget;
use crate::web::routes::{federation, oidc, room, route_ledger::RouteEntry, state::AppState, worker};

/// Pure-data profile flags that drive the conditional route surfaces in
/// `route_modules()`. Used by the offline ledger-export tool
/// (`synapse_ledger_export` binary) and any other consumer that needs to
/// enumerate the manifest without standing up a full `AppState` (which
/// requires a Postgres pool).
///
/// Live router assembly continues to use `AppState`-backed `manifest_for`;
/// this struct is the slimmer projection that captures every `state.…`
/// boolean read by an existing `manifest_for` impl.
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
    /// Project a live `AppState` down to the boolean flags consumed by the
    /// route-module trait. Kept consistent with the inline `state.…` reads in
    /// each `RouteModule::manifest_for_profile` implementation.
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
}

/// State-aware route modules that participate in both live Axum assembly and
/// route-ledger declaration.
///
/// Contributor rule: if a new feature-gated route is merged through assembly,
/// the same PR must either add a `RouteModule` here or extend the owning
/// module's explicit `*_route_manifest()` / `assembly_compat_manifest()`
/// coverage. Do not land feature-gated `Router::merge` / `.route(...)` changes
/// without updating the ledger and its snapshots in lockstep.
pub trait RouteModule: Send + Sync {
    /// Pure-data manifest enumeration. Implementors read `flags` only — never
    /// reach into a live service container. This is what
    /// `synapse_ledger_export` calls. The default `manifest_for` impl below
    /// projects `AppState` through `ProfileFlags::from_state` so live routing
    /// stays a thin wrapper over the same logic.
    fn manifest_for_profile(&self, flags: &ProfileFlags) -> Vec<RouteEntry>;

    /// Live-state convenience used by `assembly::declared_route_manifest_for`.
    /// Default: project to `ProfileFlags` and delegate. Override only when a
    /// module needs richer state than the four flags expose.
    fn manifest_for(&self, state: &AppState) -> Vec<RouteEntry> {
        self.manifest_for_profile(&ProfileFlags::from_state(state))
    }

    /// See [`merge_into`].
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
/// `assembly::declared_route_manifest_for(&AppState)` and
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
    fn manifest_for_profile(&self, _flags: &ProfileFlags) -> Vec<RouteEntry> {
        room::room_route_manifest()
    }

    fn merge_into(&self, router: Router<AppState>, _state: AppState) -> Router<AppState> {
        router.merge(room::create_room_router())
    }
}

impl RouteModule for FederationModule {
    fn manifest_for_profile(&self, _flags: &ProfileFlags) -> Vec<RouteEntry> {
        federation::federation_route_manifest()
    }

    fn merge_into(&self, router: Router<AppState>, state: AppState) -> Router<AppState> {
        router.merge(federation::create_federation_router(&state))
    }
}

impl RouteModule for OidcModule {
    fn manifest_for_profile(&self, flags: &ProfileFlags) -> Vec<RouteEntry> {
        if flags.oidc_enabled {
            #[cfg(not(feature = "builtin-oidc"))]
            let mut entries = oidc::oidc_route_manifest();
            #[cfg(feature = "builtin-oidc")]
            let entries = oidc::oidc_route_manifest();
            #[cfg(not(feature = "builtin-oidc"))]
            {
                let fallback = oidc::oidc_fallback_manifest();
                let has_jwks = entries.iter().any(|e| e.path == "/.well-known/jwks.json");
                let has_discovery = entries.iter().any(|e| e.path == "/.well-known/openid-configuration");
                for e in &fallback {
                    if e.path == "/.well-known/jwks.json" && !has_jwks {
                        entries.push(e.clone());
                    }
                    if e.path == "/.well-known/openid-configuration" && !has_discovery {
                        entries.push(e.clone());
                    }
                }
            }
            entries
        } else {
            oidc::oidc_fallback_manifest()
        }
    }

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
    fn manifest_for_profile(&self, flags: &ProfileFlags) -> Vec<RouteEntry> {
        if flags.worker_enabled {
            worker::worker_body_route_manifest()
        } else {
            Vec::new()
        }
    }

    fn merge_into(&self, router: Router<AppState>, state: AppState) -> Router<AppState> {
        if state.services.core.config.worker.enabled {
            router.merge(worker::create_worker_body_router(&state))
        } else {
            router
        }
    }
}

#[cfg(feature = "saml-sso")]
impl RouteModule for SamlModule {
    fn manifest_for_profile(&self, _flags: &ProfileFlags) -> Vec<RouteEntry> {
        saml::saml_route_manifest()
    }

    fn merge_into(&self, router: Router<AppState>, state: AppState) -> Router<AppState> {
        router.merge(saml::create_saml_router(state))
    }
}

#[cfg(feature = "cas-sso")]
impl RouteModule for CasModule {
    fn manifest_for_profile(&self, _flags: &ProfileFlags) -> Vec<RouteEntry> {
        cas::cas_route_manifest()
    }

    fn merge_into(&self, router: Router<AppState>, state: AppState) -> Router<AppState> {
        router.merge(cas::cas_routes(state))
    }
}

#[cfg(feature = "burn-after-read")]
impl RouteModule for BurnAfterReadModule {
    fn manifest_for_profile(&self, _flags: &ProfileFlags) -> Vec<RouteEntry> {
        burn_after_read::burn_after_read_route_manifest()
    }

    fn merge_into(&self, router: Router<AppState>, state: AppState) -> Router<AppState> {
        router.merge(burn_after_read::create_burn_after_read_router(state))
    }
}

#[cfg(feature = "widgets")]
impl RouteModule for WidgetModule {
    fn manifest_for_profile(&self, _flags: &ProfileFlags) -> Vec<RouteEntry> {
        widget::widget_route_manifest()
    }

    fn merge_into(&self, router: Router<AppState>, _state: AppState) -> Router<AppState> {
        router.merge(widget::create_widget_router())
    }
}

#[cfg(feature = "friends")]
impl RouteModule for FriendModule {
    fn manifest_for_profile(&self, _flags: &ProfileFlags) -> Vec<RouteEntry> {
        friend_room::friend_route_manifest()
    }

    fn merge_into(&self, router: Router<AppState>, state: AppState) -> Router<AppState> {
        router.merge(friend_room::create_friend_router(state))
    }
}

#[cfg(feature = "voice-extended")]
impl RouteModule for VoiceModule {
    fn manifest_for_profile(&self, _flags: &ProfileFlags) -> Vec<RouteEntry> {
        voice::voice_route_manifest()
    }

    fn merge_into(&self, router: Router<AppState>, state: AppState) -> Router<AppState> {
        router.merge(voice::create_voice_router(state))
    }
}

#[cfg(feature = "external-services")]
impl RouteModule for ExternalServiceModule {
    fn manifest_for_profile(&self, _flags: &ProfileFlags) -> Vec<RouteEntry> {
        external_service::external_service_route_manifest()
    }

    fn merge_into(&self, router: Router<AppState>, state: AppState) -> Router<AppState> {
        router.merge(external_service::create_external_service_router(state))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Method;

    fn contains(entries: &[RouteEntry], method: &Method, path: &str) -> bool {
        entries.iter().any(|entry| entry.method == method && entry.path == path)
    }

    #[cfg(feature = "friends")]
    #[test]
    fn friend_manifest_declares_core_routes() {
        let entries = friend_room::friend_route_manifest();
        assert!(contains(&entries, &Method::GET, "/_matrix/client/v3/friends"));
        assert!(contains(&entries, &Method::DELETE, "/_matrix/client/v1/friends/{user_id}"));
    }

    #[cfg(feature = "saml-sso")]
    #[test]
    fn saml_manifest_declares_core_routes() {
        let entries = saml::saml_route_manifest();
        assert!(contains(&entries, &Method::GET, "/_matrix/client/r0/login/sso/redirect/saml"));
        assert!(contains(&entries, &Method::POST, "/_synapse/admin/v1/saml/metadata/refresh"));
    }

    #[cfg(feature = "cas-sso")]
    #[test]
    fn cas_manifest_declares_core_routes() {
        let entries = cas::cas_route_manifest();
        assert!(contains(&entries, &Method::GET, "/login"));
        assert!(contains(&entries, &Method::GET, "/_synapse/admin/v1/cas/services"));
    }

    #[cfg(feature = "widgets")]
    #[test]
    fn widget_manifest_declares_core_routes() {
        let entries = widget::widget_route_manifest();
        assert!(contains(&entries, &Method::POST, "/_matrix/client/v1/widgets"));
        assert!(contains(&entries, &Method::GET, "/_matrix/client/v1/widgets/{widget_id}/config"));
    }

    #[cfg(feature = "burn-after-read")]
    #[test]
    fn burn_after_read_manifest_declares_core_routes() {
        let entries = burn_after_read::burn_after_read_route_manifest();
        assert!(contains(&entries, &Method::PUT, "/_matrix/client/v1/rooms/{room_id}/burn"));
        assert!(contains(&entries, &Method::GET, "/_matrix/client/v1/user/burn/stats"));
    }

    #[cfg(feature = "voice-extended")]
    #[test]
    fn voice_manifest_declares_core_routes() {
        let entries = voice::voice_route_manifest();
        assert!(contains(&entries, &Method::GET, "/_matrix/client/r0/voice/config"));
        assert!(contains(&entries, &Method::GET, "/_matrix/client/v1/voice/config"));
        assert!(contains(&entries, &Method::POST, "/_matrix/client/r0/voice/upload"));
    }

    #[cfg(feature = "external-services")]
    #[test]
    fn external_service_manifest_declares_core_routes() {
        let entries = external_service::external_service_route_manifest();
        assert!(contains(&entries, &Method::GET, "/_synapse/admin/v1/external_services"));
        assert!(contains(&entries, &Method::POST, "/_synapse/external/webhook/{service_id}"));
    }
}
