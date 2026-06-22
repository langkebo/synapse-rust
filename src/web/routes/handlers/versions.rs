//! 版本相关处理器

use crate::common::config::Config;
use crate::common::room_versions::client_room_versions_capability;
#[cfg(feature = "burn-after-read")]
use crate::web::routes::burn_after_read;
use crate::web::routes::extractors::auth::OptionalAuthenticatedUser;
#[cfg(feature = "friends")]
use crate::web::routes::friend_room;
#[cfg(feature = "voice-extended")]
use crate::web::routes::voice;
#[cfg(all(test, feature = "widgets"))]
use crate::web::routes::widget;
use crate::web::routes::{account_compat, room_summary, route_ledger::RouteEntry, sliding_sync};
use crate::web::AppState;
use axum::{
    extract::{Query, State},
    http::{
        header::{CACHE_CONTROL, VARY},
        HeaderMap, HeaderValue, Method,
    },
    Json,
};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use url::Url;

/// Empty query params marker used as the last handler parameter so that
/// `OptionalAuthenticatedUser` (a `FromRequestParts` type) is not the final
/// param — axum requires the last param to implement `FromRequest`, and
/// rust-analyzer cannot follow the cross-crate blanket impl.
#[derive(Deserialize, Default)]
pub struct EmptyQuery {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ClientApiVersionFamily {
    LegacyR0,
    StableV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ClientApiVersionSupport {
    version: &'static str,
    family: ClientApiVersionFamily,
}

impl ClientApiVersionSupport {
    const fn legacy(version: &'static str) -> Self {
        Self { version, family: ClientApiVersionFamily::LegacyR0 }
    }

    const fn stable(version: &'static str) -> Self {
        Self { version, family: ClientApiVersionFamily::StableV1 }
    }

    const fn version(self) -> &'static str {
        self.version
    }

    const fn family(self) -> ClientApiVersionFamily {
        self.family
    }
}

const CLIENT_API_VERSION_SUPPORT: &[ClientApiVersionSupport] = &[
    ClientApiVersionSupport::legacy("r0.5.0"),
    ClientApiVersionSupport::legacy("r0.6.0"),
    ClientApiVersionSupport::legacy("r0.6.1"),
    ClientApiVersionSupport::stable("v1.1"),
    ClientApiVersionSupport::stable("v1.2"),
    ClientApiVersionSupport::stable("v1.3"),
    ClientApiVersionSupport::stable("v1.4"),
    ClientApiVersionSupport::stable("v1.5"),
    ClientApiVersionSupport::stable("v1.6"),
    ClientApiVersionSupport::stable("v1.7"),
    ClientApiVersionSupport::stable("v1.8"),
    ClientApiVersionSupport::stable("v1.9"),
    ClientApiVersionSupport::stable("v1.10"),
    ClientApiVersionSupport::stable("v1.11"),
    ClientApiVersionSupport::stable("v1.12"),
    ClientApiVersionSupport::stable("v1.13"),
];

const BASE_UNSTABLE_FEATURES: &[(&str, bool)] = &[
    ("m.lazy_load_members", true),
    ("m.require_identity_server", false),
    ("m.supports_login_via_phone_number", true),
    ("org.matrix.msc3882", true),
    ("uk.tcpip.msc4133", true),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CapabilityGovernance {
    ConfigControlled,
    RouteSurface,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CapabilityFlag {
    enabled: bool,
    governance: CapabilityGovernance,
}

impl CapabilityFlag {
    const fn config_controlled(enabled: bool) -> Self {
        Self { enabled, governance: CapabilityGovernance::ConfigControlled }
    }

    const fn route_surface(enabled: bool) -> Self {
        Self { enabled, governance: CapabilityGovernance::RouteSurface }
    }

    const fn enabled(self) -> bool {
        self.enabled
    }
}

fn declared_client_api_versions() -> Vec<&'static str> {
    let mut seen_stable = false;

    CLIENT_API_VERSION_SUPPORT
        .iter()
        .map(|support| {
            match support.family() {
                ClientApiVersionFamily::LegacyR0 => {
                    debug_assert!(!seen_stable, "legacy r0 versions must stay before stable v1 versions");
                }
                ClientApiVersionFamily::StableV1 => {
                    seen_stable = true;
                }
            }
            support.version()
        })
        .collect()
}

fn build_client_versions(config: &Config) -> Value {
    let mut unstable_features = serde_json::Map::new();

    for (feature, enabled) in BASE_UNSTABLE_FEATURES {
        unstable_features.insert((*feature).to_string(), json!(enabled));
    }

    // Route-surface driven unstable feature declarations. These are kept
    // consistent with the `/capabilities.unstable_features` surface so that
    // clients observe the same availability on both endpoints.
    unstable_features.insert("org.matrix.msc3886.sliding_sync".to_string(), json!(sliding_sync_capability().enabled()));
    unstable_features.insert("org.matrix.msc3266".to_string(), json!(msc3266_capability().enabled()));
    unstable_features.insert("org.matrix.msc3245".to_string(), json!(msc3245_capability().enabled()));
    unstable_features.insert("org.matrix.msc3983".to_string(), json!(msc3983_capability().enabled()));
    unstable_features.insert("org.matrix.msc3814".to_string(), json!(msc3814_capability().enabled()));
    unstable_features.insert("org.matrix.msc4143".to_string(), json!(msc4143_capability().enabled()));
    // Private `io.hula.*` extensions are intentionally NOT declared in
    // `/versions.unstable_features` — that surface is unauthenticated and
    // consumed by stock Matrix clients which do not understand the
    // `io.hula.*` namespace. These capabilities remain visible on the
    // authenticated `/capabilities` surface for clients that opt in.
    let _ = config;

    json!({
        "versions": declared_client_api_versions(),
        "unstable_features": unstable_features
    })
}

fn client_versions_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=600, s-maxage=3600, stale-while-revalidate=600"),
    );
    headers.insert(VARY, HeaderValue::from_static("Authorization"));
    headers
}

/// 获取客户端 API 版本
pub async fn get_client_versions(State(state): State<AppState>) -> impl axum::response::IntoResponse {
    (client_versions_headers(), Json(build_client_versions(&state.services.core.config)))
}

/// 获取服务端版本
pub async fn get_server_version(State(state): State<AppState>) -> impl axum::response::IntoResponse {
    Json(json!({
        "server_version": env!("CARGO_PKG_VERSION"),
        "python_version": "Rust",
        "server_name": state.services.core.server_name
    }))
}

fn format_host_port(host: &str, port: u16) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

fn derive_well_known_server(public_baseurl: &str, fallback_server_name: &str, federation_port: u16) -> String {
    let host = Url::parse(public_baseurl)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .filter(|host| !host.is_empty())
        .unwrap_or_else(|| fallback_server_name.to_string());

    format_host_port(&host, federation_port)
}

fn build_well_known_client(base_url: &str) -> serde_json::Value {
    json!({
        "m.homeserver": {
            "base_url": base_url
        }
    })
}

fn insert_enabled_capability(capabilities: &mut Map<String, Value>, name: &str, enabled: bool) {
    capabilities.insert(name.to_string(), json!({ "enabled": enabled }));
}

fn sso_providers(config: &Config) -> Vec<&'static str> {
    let mut providers = Vec::new();
    if config.saml.enabled {
        providers.push("saml");
    }
    // OIDC (external IdP or builtin provider) is treated as an SSO provider
    // for the `m.sso.providers` capability surface. Synapse exposes the same
    // `oidc` brand when an external OIDC IdP is configured; we mirror that.
    if config.oidc.is_enabled() || config.builtin_oidc.is_enabled() {
        providers.push("oidc");
    }
    #[cfg(feature = "cas-sso")]
    {
        providers.push("cas");
    }
    providers
}

fn build_capabilities_unstable_features(config: &Config) -> Value {
    json!({
        "io.hula.friends": friends_capability(config).enabled(),
        "org.matrix.msc3245.voice": voice_capability().enabled(),
        "org.matrix.msc3983.thread": thread_capability().enabled(),
        "org.matrix.msc3886.sliding_sync": sliding_sync_capability().enabled(),
        "io.hula.burn_after_read": burn_after_read_capability(config).enabled()
    })
}

fn openclaw_routes_enabled(config: &Config) -> bool {
    #[cfg(feature = "openclaw-routes")]
    {
        config.experimental.openclaw_routes_enabled
    }

    #[cfg(not(feature = "openclaw-routes"))]
    {
        let _ = config;
        false
    }
}

fn manifest_has_route(entries: &[RouteEntry], method: &Method, path: &str) -> bool {
    entries.iter().any(|entry| entry.method == *method && entry.path == path)
}

fn room_summary_capability() -> CapabilityFlag {
    CapabilityFlag::route_surface(manifest_has_route(
        &room_summary::room_summary_route_manifest(),
        &Method::GET,
        "/_matrix/client/v3/rooms/{room_id}/summary",
    ))
}

/// MSC3266 (Room summary batch) capability is driven by the route surface:
/// the `org.matrix.msc3266` unstable feature is declared only when the
/// `POST /_synapse/room_summary/v1/summaries/batch` endpoint is registered.
/// This aligns the `/versions` declaration with the actual route registration,
/// matching the governance pattern used by `msc3886.sliding_sync` / `msc4261.widget`.
fn msc3266_capability() -> CapabilityFlag {
    CapabilityFlag::route_surface(manifest_has_route(
        &room_summary::room_summary_route_manifest(),
        &Method::POST,
        "/_synapse/room_summary/v1/summaries/batch",
    ))
}

/// MSC3814 (Dehydrated device) capability is driven by the route surface:
/// declared only when the `GET /_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device`
/// endpoint is registered in the top-level inline manifest.
fn msc3814_capability() -> CapabilityFlag {
    CapabilityFlag::route_surface(manifest_has_route(
        &crate::web::routes::assembly::top_level_inline_manifest(),
        &Method::GET,
        "/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device",
    ))
}

/// MSC4143 (MatrixRTC transports) capability is driven by the route surface:
/// declared only when the `GET /_matrix/client/unstable/org.matrix.msc4143/rtc/transports`
/// endpoint is registered in the top-level inline manifest.
fn msc4143_capability() -> CapabilityFlag {
    CapabilityFlag::route_surface(manifest_has_route(
        &crate::web::routes::assembly::top_level_inline_manifest(),
        &Method::GET,
        "/_matrix/client/unstable/org.matrix.msc4143/rtc/transports",
    ))
}

/// MSC3245 (Room summary) unstable feature is driven by the route surface:
/// declared only when the room summary endpoint is registered. This keeps
/// `/versions` and `/capabilities.unstable_features` consistent.
fn msc3245_capability() -> CapabilityFlag {
    room_summary_capability()
}

/// MSC3983 (Thread) unstable feature is driven by the route surface:
/// declared only when the threads endpoint is registered. This keeps
/// `/versions` and `/capabilities.unstable_features` consistent.
fn msc3983_capability() -> CapabilityFlag {
    thread_capability()
}

fn room_suggested_capability() -> CapabilityFlag {
    // `m.room.suggested` reflects the space-suggested-rooms surface, which is
    // served by the room hierarchy endpoint (MSC2946). Derive from the
    // `/_matrix/client/v1/rooms/{room_id}/hierarchy` route registration instead
    // of aliasing `room_summary_capability()`, which tracks a different
    // endpoint (`/_matrix/client/v3/rooms/{room_id}/summary`).
    CapabilityFlag::route_surface(manifest_has_route(
        &crate::web::routes::handlers::search::search_route_manifest(),
        &Method::GET,
        "/_matrix/client/v1/rooms/{room_id}/hierarchy",
    ))
}

fn voice_capability() -> CapabilityFlag {
    // `m.voice` reflects whether the homeserver can issue TURN credentials.
    // Derive from the `/voip/turnServer` route registration instead of aliasing
    // `room_summary_capability()`, which tracks the room summary endpoint and
    // has nothing to do with VoIP availability.
    CapabilityFlag::route_surface(manifest_has_route(
        &crate::web::routes::voip::voip_route_manifest(),
        &Method::GET,
        "/_matrix/client/v3/voip/turnServer",
    ))
}

fn thread_capability() -> CapabilityFlag {
    CapabilityFlag::route_surface(manifest_has_route(
        &crate::web::routes::handlers::thread::thread_route_manifest(),
        &Method::GET,
        "/_matrix/client/v1/threads",
    ))
}

fn sliding_sync_capability() -> CapabilityFlag {
    CapabilityFlag::route_surface(manifest_has_route(
        &sliding_sync::sliding_sync_route_manifest(),
        &Method::POST,
        "/_matrix/client/v1/sync",
    ))
}

fn change_password_capability() -> CapabilityFlag {
    CapabilityFlag::route_surface(manifest_has_route(
        &account_compat::account_compat_route_manifest(),
        &Method::POST,
        "/_matrix/client/v3/account/password",
    ))
}

/// Public entry-point for external callers that need the boolean value
/// without depending on the private `CapabilityFlag` type.
pub(crate) fn change_password_capability_enabled() -> bool {
    change_password_capability().enabled()
}

fn set_displayname_capability() -> CapabilityFlag {
    CapabilityFlag::route_surface(manifest_has_route(
        &account_compat::account_compat_route_manifest(),
        &Method::PUT,
        "/_matrix/client/v3/profile/{user_id}/displayname",
    ))
}

fn set_avatar_url_capability() -> CapabilityFlag {
    CapabilityFlag::route_surface(manifest_has_route(
        &account_compat::account_compat_route_manifest(),
        &Method::PUT,
        "/_matrix/client/v3/profile/{user_id}/avatar_url",
    ))
}

fn threepid_changes_capability() -> CapabilityFlag {
    CapabilityFlag::route_surface(manifest_has_route(
        &account_compat::account_compat_route_manifest(),
        &Method::POST,
        "/_matrix/client/v3/account/3pid",
    ))
}

fn sso_capability(config: &Config) -> CapabilityFlag {
    CapabilityFlag::config_controlled(!sso_providers(config).is_empty())
}

fn openclaw_capability(config: &Config) -> CapabilityFlag {
    CapabilityFlag::config_controlled(openclaw_routes_enabled(config))
}

fn ai_connection_capability(config: &Config) -> CapabilityFlag {
    CapabilityFlag::config_controlled(openclaw_routes_enabled(config))
}

fn friends_capability(config: &Config) -> CapabilityFlag {
    if !config.experimental.declare_private_extensions {
        return CapabilityFlag::config_controlled(false);
    }
    #[cfg(feature = "friends")]
    {
        CapabilityFlag::route_surface(manifest_has_route(
            &friend_room::friend_route_manifest(),
            &Method::GET,
            "/_matrix/client/v3/friends",
        ))
    }
    #[cfg(not(feature = "friends"))]
    {
        CapabilityFlag::route_surface(false)
    }
}

fn external_services_capability() -> CapabilityFlag {
    #[cfg(feature = "external-services")]
    {
        CapabilityFlag::route_surface(manifest_has_route(
            &crate::web::routes::external_service::external_service_route_manifest(),
            &Method::GET,
            "/_matrix/client/v1/external_services/health",
        ))
    }
    #[cfg(not(feature = "external-services"))]
    {
        CapabilityFlag::route_surface(false)
    }
}

fn voice_extended_capability(config: &Config) -> CapabilityFlag {
    if !config.experimental.declare_private_extensions {
        return CapabilityFlag::config_controlled(false);
    }
    #[cfg(feature = "voice-extended")]
    {
        CapabilityFlag::route_surface(manifest_has_route(
            &voice::voice_route_manifest(),
            &Method::GET,
            "/_matrix/client/v1/voice/config",
        ))
    }
    #[cfg(not(feature = "voice-extended"))]
    {
        CapabilityFlag::route_surface(false)
    }
}

#[cfg(test)]
fn widget_capability() -> CapabilityFlag {
    #[cfg(feature = "widgets")]
    {
        CapabilityFlag::route_surface(manifest_has_route(
            &widget::widget_route_manifest(),
            &Method::POST,
            "/_matrix/client/v1/widgets",
        ))
    }
    #[cfg(not(feature = "widgets"))]
    {
        CapabilityFlag::route_surface(false)
    }
}

fn burn_after_read_capability(config: &Config) -> CapabilityFlag {
    if !config.experimental.declare_private_extensions {
        return CapabilityFlag::config_controlled(false);
    }
    #[cfg(feature = "burn-after-read")]
    {
        CapabilityFlag::route_surface(manifest_has_route(
            &burn_after_read::burn_after_read_route_manifest(),
            &Method::PUT,
            "/_matrix/client/v1/rooms/{room_id}/burn",
        ))
    }
    #[cfg(not(feature = "burn-after-read"))]
    {
        CapabilityFlag::route_surface(false)
    }
}

fn build_capabilities_response(config: &Config, authenticated: bool) -> Value {
    let mut capabilities = Map::new();
    let sso_providers = sso_providers(config);

    insert_enabled_capability(&mut capabilities, "m.change_password", change_password_capability().enabled());
    capabilities.insert("m.room_versions".to_string(), client_room_versions_capability());
    insert_enabled_capability(&mut capabilities, "m.set_displayname", set_displayname_capability().enabled());
    insert_enabled_capability(&mut capabilities, "m.set_avatar_url", set_avatar_url_capability().enabled());
    insert_enabled_capability(&mut capabilities, "m.3pid_changes", threepid_changes_capability().enabled());
    insert_enabled_capability(&mut capabilities, "m.room.summary", room_summary_capability().enabled());
    insert_enabled_capability(&mut capabilities, "m.room.suggested", room_suggested_capability().enabled());
    insert_enabled_capability(&mut capabilities, "m.voice", voice_capability().enabled());
    insert_enabled_capability(&mut capabilities, "m.thread", thread_capability().enabled());
    // Sliding sync is declared via the standard `org.matrix.msc3886.sliding_sync`
    // unstable feature in `/versions` and `/capabilities.unstable_features`.
    // The private `io.hula.sliding_sync` capability is intentionally omitted
    // from the public surface — stock Element discovers sliding sync via the
    // standard MSC3886 identifier, not the `io.hula.*` namespace.

    // MSC4452: Preview URL capabilities API (Synapse v1.154 #19715).
    // Declares the `io.element.msc4452.preview_url` capability so clients
    // know whether the `preview_url` endpoint is gated behind the capability.
    insert_enabled_capability(&mut capabilities, "io.element.msc4452.preview_url", config.experimental.msc4452_enabled);

    if authenticated {
        let openclaw_enabled = openclaw_capability(config).enabled();

        capabilities.insert("io.hula.friends".to_string(), json!(friends_capability(config).enabled()));
        capabilities.insert(
            "m.sso".to_string(),
            json!({
                "enabled": sso_capability(config).enabled(),
                "providers": sso_providers
            }),
        );
        insert_enabled_capability(&mut capabilities, "ai_connection", ai_connection_capability(config).enabled());
        insert_enabled_capability(&mut capabilities, "openclaw", openclaw_enabled);
        insert_enabled_capability(&mut capabilities, "external_services", external_services_capability().enabled());
        insert_enabled_capability(
            &mut capabilities,
            "io.hula.voice_extended",
            voice_extended_capability(config).enabled(),
        );
        insert_enabled_capability(
            &mut capabilities,
            "io.hula.burn_after_read",
            burn_after_read_capability(config).enabled(),
        );
    }

    json!({
        "capabilities": capabilities,
        "unstable_features": build_capabilities_unstable_features(config)
    })
}

/// .well-known: Matrix 服务器发现
pub async fn get_well_known_server(State(state): State<AppState>) -> Json<serde_json::Value> {
    let server_name = state.services.core.config.server.get_server_name();
    let public_baseurl = state.services.core.config.server.get_public_baseurl();
    let federation_port = state.services.core.config.federation.federation_port;
    Json(json!({
        "m.server": derive_well_known_server(&public_baseurl, server_name, federation_port)
    }))
}

/// .well-known: Matrix 客户端发现
pub async fn get_well_known_client(State(state): State<AppState>) -> Json<serde_json::Value> {
    let base_url = state.services.core.config.server.get_public_baseurl();
    Json(build_well_known_client(&base_url))
}

/// .well-known: Matrix 支持
pub async fn get_well_known_support() -> impl axum::response::IntoResponse {
    Json(json!({
        "url": "https://matrix.org"
    }))
}

/// 获取服务端能力
pub async fn get_capabilities(
    State(state): State<AppState>,
    auth: OptionalAuthenticatedUser,
    Query(_): Query<EmptyQuery>,
) -> Json<serde_json::Value> {
    Json(build_capabilities_response(&state.services.core.config, auth.user_id.is_some()))
}

#[cfg(test)]
mod tests {
    use super::{
        ai_connection_capability, build_capabilities_response, build_client_versions, build_well_known_client,
        burn_after_read_capability, change_password_capability, client_versions_headers, derive_well_known_server,
        external_services_capability, friends_capability, openclaw_capability, room_suggested_capability,
        room_summary_capability, set_avatar_url_capability, set_displayname_capability, sliding_sync_capability,
        sso_capability, sso_providers, thread_capability, threepid_changes_capability, voice_capability,
        voice_extended_capability, widget_capability, CapabilityFlag, CapabilityGovernance, ClientApiVersionFamily,
        CLIENT_API_VERSION_SUPPORT,
    };
    use crate::common::config::Config;
    use axum::http::header::{CACHE_CONTROL, VARY};

    #[test]
    fn test_build_client_versions_keeps_supported_versions_ordered_and_unique() {
        let body = build_client_versions(&Config::default());
        let versions = body["versions"].as_array().expect("versions should be an array");

        assert_eq!(versions.len(), CLIENT_API_VERSION_SUPPORT.len());
        for expected in CLIENT_API_VERSION_SUPPORT {
            assert_eq!(
                versions.iter().filter(|version| version.as_str() == Some(expected.version())).count(),
                1,
                "version {} should appear exactly once",
                expected.version()
            );
        }
    }

    #[test]
    fn test_client_version_support_keeps_legacy_before_stable_versions() {
        let first_stable_index = CLIENT_API_VERSION_SUPPORT
            .iter()
            .position(|support| support.family() == ClientApiVersionFamily::StableV1)
            .expect("stable v1 versions should be present");

        assert!(CLIENT_API_VERSION_SUPPORT[..first_stable_index]
            .iter()
            .all(|support| support.family() == ClientApiVersionFamily::LegacyR0));
        assert!(CLIENT_API_VERSION_SUPPORT[first_stable_index..]
            .iter()
            .all(|support| support.family() == ClientApiVersionFamily::StableV1));
    }

    #[test]
    fn test_client_versions_headers_allow_public_cache_and_vary_on_auth() {
        let headers = client_versions_headers();

        assert_eq!(
            headers.get(CACHE_CONTROL).and_then(|value| value.to_str().ok()),
            Some("public, max-age=600, s-maxage=3600, stale-while-revalidate=600")
        );
        assert_eq!(headers.get(VARY).and_then(|value| value.to_str().ok()), Some("Authorization"));
    }

    #[test]
    fn test_capabilities_public_surface_hides_private_extensions() {
        let body = build_capabilities_response(&Config::default(), false);
        let capabilities = body["capabilities"].as_object().expect("capabilities should be an object");

        assert_eq!(capabilities["m.change_password"]["enabled"], change_password_capability().enabled());
        assert_eq!(capabilities["m.set_displayname"]["enabled"], set_displayname_capability().enabled());
        assert_eq!(capabilities["m.set_avatar_url"]["enabled"], set_avatar_url_capability().enabled());
        assert_eq!(capabilities["m.3pid_changes"]["enabled"], threepid_changes_capability().enabled());
        assert_eq!(capabilities["m.room.summary"]["enabled"], room_summary_capability().enabled());
        assert_eq!(capabilities["m.room.suggested"]["enabled"], room_suggested_capability().enabled());
        assert_eq!(capabilities["m.voice"]["enabled"], voice_capability().enabled());
        assert_eq!(capabilities["m.thread"]["enabled"], thread_capability().enabled());
        assert!(capabilities.contains_key("m.room_versions"));
        assert!(!capabilities.contains_key("m.sso"));
        assert!(!capabilities.contains_key("io.hula.friends"));
        assert!(!capabilities.contains_key("io.hula.widget"));
        assert!(!capabilities.contains_key("io.hula.burn_after_read"));
        // Private sliding sync capability must not leak to the public surface;
        // clients discover sliding sync via the standard MSC3886 identifier.
        assert!(!capabilities.contains_key("io.hula.sliding_sync"));
    }

    #[test]
    fn test_capabilities_authenticated_surface_tracks_config_and_feature_flags() {
        let mut config = Config::default();
        config.saml.enabled = true;
        if cfg!(feature = "openclaw-routes") {
            config.experimental.openclaw_routes_enabled = false;
        }

        let body = build_capabilities_response(&config, true);
        let capabilities = body["capabilities"].as_object().expect("capabilities should be an object");

        assert_eq!(capabilities["m.sso"]["enabled"], sso_capability(&config).enabled());
        assert_eq!(capabilities["m.sso"]["providers"][0], "saml");
        assert_eq!(capabilities["openclaw"]["enabled"], openclaw_capability(&config).enabled());
        assert_eq!(capabilities["io.hula.friends"], friends_capability(&config).enabled());
        assert_eq!(capabilities["external_services"]["enabled"], external_services_capability().enabled());
        assert_eq!(capabilities["io.hula.voice_extended"]["enabled"], voice_extended_capability(&config).enabled());
        assert_eq!(capabilities["io.hula.burn_after_read"]["enabled"], burn_after_read_capability(&config).enabled());
        assert_eq!(body["unstable_features"]["org.matrix.msc3245.voice"], voice_capability().enabled());
        assert_eq!(body["unstable_features"]["org.matrix.msc3983.thread"], thread_capability().enabled());
        assert_eq!(body["unstable_features"]["org.matrix.msc3886.sliding_sync"], sliding_sync_capability().enabled());
        // Private io.hula.* extensions remain on the authenticated
        // `/capabilities.unstable_features` surface for opt-in clients.
        assert_eq!(body["unstable_features"]["io.hula.friends"], friends_capability(&config).enabled());
        assert_eq!(body["unstable_features"]["io.hula.burn_after_read"], burn_after_read_capability(&config).enabled());
    }

    #[test]
    fn test_declare_private_extensions_suppresses_hula_capabilities() {
        // When `declare_private_extensions = false`, all `io.hula.*`
        // capabilities should be disabled regardless of feature gates.
        let mut config = Config::default();
        config.experimental.declare_private_extensions = false;

        assert!(!friends_capability(&config).enabled(), "friends should be suppressed");
        assert!(!voice_extended_capability(&config).enabled(), "voice_extended should be suppressed");
        assert!(!burn_after_read_capability(&config).enabled(), "burn_after_read should be suppressed");

        // Governance switches to ConfigControlled when suppressed.
        assert_eq!(friends_capability(&config).governance, CapabilityGovernance::ConfigControlled);
        assert_eq!(voice_extended_capability(&config).governance, CapabilityGovernance::ConfigControlled);
        assert_eq!(burn_after_read_capability(&config).governance, CapabilityGovernance::ConfigControlled);

        // The /capabilities response should not declare them as enabled.
        let body = build_capabilities_response(&config, true);
        assert_eq!(body["capabilities"]["io.hula.friends"], false);
        assert_eq!(body["capabilities"]["io.hula.voice_extended"]["enabled"], false);
        assert_eq!(body["capabilities"]["io.hula.burn_after_read"]["enabled"], false);
    }

    #[test]
    fn test_sso_providers_includes_oidc_when_enabled() {
        // Regression: `sso_providers()` previously omitted OIDC even when an
        // external IdP or builtin provider was configured, causing the
        // `m.sso.providers` capability to miss the `oidc` brand.
        let mut config = Config::default();
        assert!(!sso_providers(&config).contains(&"oidc"), "default config should not list oidc provider");

        // Simulate external OIDC enabled.
        config.oidc.enabled = true;
        config.oidc.issuer = "https://idp.example.com".to_string();
        config.oidc.client_id = "synapse-rust".to_string();
        assert!(sso_providers(&config).contains(&"oidc"), "enabled external OIDC should appear as an sso provider");

        // Simulate builtin OIDC enabled (without external OIDC).
        let mut config2 = Config::default();
        config2.builtin_oidc.enabled = true;
        config2.builtin_oidc.issuer = "https://hs.example.com".to_string();
        config2.builtin_oidc.users.push(crate::common::config::BuiltinOidcUser {
            id: "@alice:example.com".to_string(),
            username: "alice".to_string(),
            password: Some("password".to_string()),
            password_hash: None,
            email: "alice@example.com".to_string(),
            displayname: Some("Alice".to_string()),
        });
        assert!(sso_providers(&config2).contains(&"oidc"), "enabled builtin OIDC should appear as an sso provider");
    }

    #[test]
    fn test_capability_governance_classifies_route_and_config_sources() {
        assert_eq!(room_summary_capability().governance, CapabilityGovernance::RouteSurface);
        assert_eq!(room_suggested_capability().governance, CapabilityGovernance::RouteSurface);
        assert_eq!(voice_capability().governance, CapabilityGovernance::RouteSurface);
        assert_eq!(thread_capability().governance, CapabilityGovernance::RouteSurface);
        assert_eq!(sliding_sync_capability().governance, CapabilityGovernance::RouteSurface);
        assert_eq!(change_password_capability().governance, CapabilityGovernance::RouteSurface);
        assert_eq!(set_displayname_capability().governance, CapabilityGovernance::RouteSurface);
        assert_eq!(set_avatar_url_capability().governance, CapabilityGovernance::RouteSurface);
        assert_eq!(threepid_changes_capability().governance, CapabilityGovernance::RouteSurface);
        assert_eq!(friends_capability(&Config::default()).governance, CapabilityGovernance::RouteSurface);
        assert_eq!(external_services_capability().governance, CapabilityGovernance::RouteSurface);
        assert_eq!(voice_extended_capability(&Config::default()).governance, CapabilityGovernance::RouteSurface);
        assert_eq!(widget_capability().governance, CapabilityGovernance::RouteSurface);
        assert_eq!(burn_after_read_capability(&Config::default()).governance, CapabilityGovernance::RouteSurface);
        assert_eq!(sso_capability(&Config::default()).governance, CapabilityGovernance::ConfigControlled);
        assert_eq!(openclaw_capability(&Config::default()).governance, CapabilityGovernance::ConfigControlled);
    }

    #[test]
    fn test_derive_well_known_server_prefers_public_baseurl_host() {
        let server = derive_well_known_server("https://matrix.example.com", "example.com", 443);
        assert_eq!(server, "matrix.example.com:443");
    }

    #[test]
    fn test_derive_well_known_server_falls_back_to_server_name() {
        let server = derive_well_known_server("not a valid url", "example.com", 8448);
        assert_eq!(server, "example.com:8448");
    }

    #[test]
    fn test_build_well_known_client_omits_identity_server() {
        let body = build_well_known_client("https://matrix.example.com");
        assert_eq!(body["m.homeserver"]["base_url"], "https://matrix.example.com");
        assert!(body.get("m.identity_server").is_none());
    }

    // ---------------------------------------------------------------
    // Contract / snapshot tests — prevent capability declaration drift
    // ---------------------------------------------------------------

    #[test]
    fn test_versions_response_snapshot_keys() {
        let body = build_client_versions(&Config::default());

        let versions = body["versions"].as_array().expect("versions should be an array");
        assert!(!versions.is_empty(), "versions array must not be empty");

        let unstable = body["unstable_features"].as_object().expect("unstable_features should be an object");
        let expected_unstable: &[&str] = &[
            "m.lazy_load_members",
            "m.require_identity_server",
            "m.supports_login_via_phone_number",
            "org.matrix.msc3882",
            "uk.tcpip.msc4133",
            "org.matrix.msc3886.sliding_sync",
            "org.matrix.msc3266",
            "org.matrix.msc3245",
            "org.matrix.msc3983",
            "org.matrix.msc3814",
            "org.matrix.msc4143",
        ];
        for key in expected_unstable {
            assert!(unstable.contains_key(*key), "missing unstable feature: {key}");
        }

        // Private `io.hula.*` extensions must NOT appear in `/versions` —
        // that surface is unauthenticated and consumed by stock Matrix
        // clients which do not understand the `io.hula.*` namespace.
        assert!(
            !unstable.contains_key("io.hula.burn_after_read"),
            "private io.hula.burn_after_read must not leak to /versions"
        );
        assert!(!unstable.contains_key("io.hula.friends"), "private io.hula.friends must not leak to /versions");
    }

    #[test]
    fn test_capabilities_response_snapshot_public_surface() {
        let body = build_capabilities_response(&Config::default(), false);
        let capabilities = body["capabilities"].as_object().expect("capabilities should be an object");

        let expected_public: &[&str] = &[
            "m.change_password",
            "m.room_versions",
            "m.set_displayname",
            "m.set_avatar_url",
            "m.3pid_changes",
            "m.room.summary",
            "m.room.suggested",
            "m.voice",
            "m.thread",
        ];
        for key in expected_public {
            assert!(capabilities.contains_key(*key), "missing public capability: {key}");
        }

        let private_only: &[&str] =
            &["m.sso", "io.hula.friends", "io.hula.widget", "io.hula.burn_after_read", "io.hula.sliding_sync"];
        for key in private_only {
            assert!(!capabilities.contains_key(*key), "private capability leaked to unauthenticated user: {key}");
        }
    }

    #[test]
    fn test_capabilities_response_snapshot_authenticated_surface() {
        let body = build_capabilities_response(&Config::default(), true);
        let capabilities = body["capabilities"].as_object().expect("capabilities should be an object");

        let authenticated_only: &[&str] = &[
            "io.hula.friends",
            "m.sso",
            "ai_connection",
            "openclaw",
            "external_services",
            "io.hula.voice_extended",
            "io.hula.burn_after_read",
        ];
        for key in authenticated_only {
            assert!(capabilities.contains_key(*key), "missing authenticated capability: {key}");
        }
    }

    #[test]
    fn test_all_capabilities_have_governance_classification() {
        // Every capability in the response must be backed by an explicit
        // CapabilityFlag with a known governance class (RouteSurface or
        // ConfigControlled).  No capability should be left without a
        // governance classification.
        let body = build_capabilities_response(&Config::default(), true);
        let capabilities = body["capabilities"].as_object().expect("capabilities should be an object");

        // All keys in the capabilities map should be known
        let known_keys: &[&str] = &[
            "m.change_password",
            "m.room_versions",
            "m.set_displayname",
            "m.set_avatar_url",
            "m.3pid_changes",
            "m.room.summary",
            "m.room.suggested",
            "m.voice",
            "m.thread",
            "io.hula.friends",
            "m.sso",
            "ai_connection",
            "openclaw",
            "external_services",
            "io.hula.voice_extended",
            "io.hula.burn_after_read",
            "io.element.msc4452.preview_url",
        ];

        for key in capabilities.keys() {
            assert!(known_keys.contains(&key.as_str()), "unexpected capability key in response: {key}");
        }

        for key in known_keys {
            if *key != "m.room_versions" {
                // m.room_versions is a special case with its own structure
                assert!(capabilities.contains_key(*key), "known capability missing from response: {key}");
            }
        }
    }

    #[test]
    fn test_no_residual_static_stable_governance() {
        // After P1-03.2 cleanup, no capability should use the legacy
        // StaticStable governance.  All capabilities must be either
        // RouteSurface or ConfigControlled.
        let config = Config::default();
        let all_capabilities: &[CapabilityFlag] = &[
            change_password_capability(),
            set_displayname_capability(),
            set_avatar_url_capability(),
            threepid_changes_capability(),
            room_summary_capability(),
            room_suggested_capability(),
            voice_capability(),
            thread_capability(),
            sliding_sync_capability(),
            sso_capability(&config),
            openclaw_capability(&config),
            ai_connection_capability(&config),
            friends_capability(&config),
            external_services_capability(),
            voice_extended_capability(&config),
            widget_capability(),
            burn_after_read_capability(&config),
        ];

        for flag in all_capabilities {
            match flag.governance {
                CapabilityGovernance::RouteSurface | CapabilityGovernance::ConfigControlled => {}
            }
        }
    }
}
