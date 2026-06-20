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
#[cfg(feature = "widgets")]
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
    ("org.matrix.msc3983", true),
    ("org.matrix.msc3245", true),
    ("org.matrix.msc3266", true),
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

    unstable_features.insert("org.matrix.msc3886.sliding_sync".to_string(), json!(sliding_sync_capability().enabled()));
    unstable_features.insert("org.matrix.msc4261.widget".to_string(), json!(widget_capability().enabled()));
    unstable_features.insert("io.hula.burn_after_read".to_string(), json!(burn_after_read_capability().enabled()));
    unstable_features.insert("io.hula.friends".to_string(), json!(friends_capability().enabled()));

    if config.experimental.msc3814_enabled {
        unstable_features.insert("org.matrix.msc3814".to_string(), json!(true));
    }

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
    #[cfg(feature = "cas-sso")]
    {
        providers.push("cas");
    }
    providers
}

fn build_capabilities_unstable_features() -> Value {
    json!({
        "io.hula.friends": friends_capability().enabled(),
        "org.matrix.msc3245.voice": voice_capability().enabled(),
        "org.matrix.msc3983.thread": thread_capability().enabled(),
        "org.matrix.msc3886.sliding_sync": sliding_sync_capability().enabled(),
        "org.matrix.msc4261.widget": widget_capability().enabled(),
        "io.hula.burn_after_read": burn_after_read_capability().enabled()
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

fn room_suggested_capability() -> CapabilityFlag {
    CapabilityFlag::route_surface(room_summary_capability().enabled())
}

fn voice_capability() -> CapabilityFlag {
    CapabilityFlag::route_surface(room_summary_capability().enabled())
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

fn friends_capability() -> CapabilityFlag {
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

fn voice_extended_capability() -> CapabilityFlag {
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

fn burn_after_read_capability() -> CapabilityFlag {
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
    insert_enabled_capability(&mut capabilities, "io.hula.sliding_sync", sliding_sync_capability().enabled());

    // MSC4452: Preview URL capabilities API (Synapse v1.154 #19715).
    // Declares the `io.element.msc4452.preview_url` capability so clients
    // know whether the `preview_url` endpoint is gated behind the capability.
    insert_enabled_capability(&mut capabilities, "io.element.msc4452.preview_url", config.experimental.msc4452_enabled);

    if authenticated {
        let openclaw_enabled = openclaw_capability(config).enabled();

        capabilities.insert("io.hula.friends".to_string(), json!(friends_capability().enabled()));
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
        insert_enabled_capability(&mut capabilities, "io.hula.voice_extended", voice_extended_capability().enabled());
        insert_enabled_capability(&mut capabilities, "io.hula.widget", widget_capability().enabled());
        insert_enabled_capability(&mut capabilities, "io.hula.burn_after_read", burn_after_read_capability().enabled());
    }

    json!({
        "capabilities": capabilities,
        "unstable_features": build_capabilities_unstable_features()
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
        external_services_capability, friends_capability, get_client_versions, openclaw_capability,
        room_suggested_capability, room_summary_capability, set_avatar_url_capability, set_displayname_capability,
        sliding_sync_capability, sso_capability, thread_capability, threepid_changes_capability, voice_capability,
        voice_extended_capability, widget_capability, CapabilityFlag, CapabilityGovernance, ClientApiVersionFamily,
        CLIENT_API_VERSION_SUPPORT,
    };
    use crate::cache::{CacheConfig, CacheManager};
    use crate::common::config::Config;
    use crate::services::ServiceContainer;
    use crate::web::AppState;
    use axum::http::header::{CACHE_CONTROL, VARY};
    use std::sync::Arc;

    async fn make_test_state() -> AppState {
        let pool = crate::test_utils::take_prepared_test_pool().unwrap_or_else(|| {
            let db_url = std::env::var("TEST_DATABASE_URL")
                .or_else(|_| std::env::var("DATABASE_URL"))
                .unwrap_or_else(|_| "postgres://localhost/test".to_string());
            Arc::new(
                sqlx::postgres::PgPoolOptions::new()
                    .max_connections(crate::test_utils::configured_test_pool_max_connections())
                    .min_connections(crate::test_utils::configured_test_pool_min_connections())
                    .connect_lazy(&db_url)
                    .expect("Failed to create test database pool"),
            )
        });

        let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
        let mut config = Config::default();
        config.experimental.msc3814_enabled = true;
        let services = ServiceContainer::new(&pool, cache.clone(), config, None).await;
        AppState::new(services, cache)
    }

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
    fn test_build_client_versions_omits_disabled_msc3814() {
        let mut config = Config::default();
        config.experimental.msc3814_enabled = false;
        let body = build_client_versions(&config);

        assert!(body["unstable_features"].get("org.matrix.msc3814").is_none());
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
        assert_eq!(capabilities["io.hula.sliding_sync"]["enabled"], sliding_sync_capability().enabled());
        assert!(capabilities.contains_key("m.room_versions"));
        assert!(!capabilities.contains_key("m.sso"));
        assert!(!capabilities.contains_key("io.hula.friends"));
        assert!(!capabilities.contains_key("io.hula.widget"));
        assert!(!capabilities.contains_key("io.hula.burn_after_read"));
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
        assert_eq!(capabilities["io.hula.friends"], friends_capability().enabled());
        assert_eq!(capabilities["external_services"]["enabled"], external_services_capability().enabled());
        assert_eq!(capabilities["io.hula.voice_extended"]["enabled"], voice_extended_capability().enabled());
        assert_eq!(capabilities["io.hula.widget"]["enabled"], widget_capability().enabled());
        assert_eq!(capabilities["io.hula.burn_after_read"]["enabled"], burn_after_read_capability().enabled());
        assert_eq!(body["unstable_features"]["org.matrix.msc3245.voice"], voice_capability().enabled());
        assert_eq!(body["unstable_features"]["org.matrix.msc3983.thread"], thread_capability().enabled());
        assert_eq!(body["unstable_features"]["org.matrix.msc3886.sliding_sync"], sliding_sync_capability().enabled());
        assert_eq!(body["unstable_features"]["org.matrix.msc4261.widget"], widget_capability().enabled());
        assert_eq!(body["unstable_features"]["io.hula.friends"], friends_capability().enabled());
        assert_eq!(body["unstable_features"]["io.hula.burn_after_read"], burn_after_read_capability().enabled());
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
        assert_eq!(friends_capability().governance, CapabilityGovernance::RouteSurface);
        assert_eq!(external_services_capability().governance, CapabilityGovernance::RouteSurface);
        assert_eq!(voice_extended_capability().governance, CapabilityGovernance::RouteSurface);
        assert_eq!(widget_capability().governance, CapabilityGovernance::RouteSurface);
        assert_eq!(burn_after_read_capability().governance, CapabilityGovernance::RouteSurface);
        assert_eq!(sso_capability(&Config::default()).governance, CapabilityGovernance::ConfigControlled);
        assert_eq!(openclaw_capability(&Config::default()).governance, CapabilityGovernance::ConfigControlled);
    }

    #[tokio::test]
    async fn test_get_client_versions_includes_msc3814() {
        use axum::response::IntoResponse;
        let state = make_test_state().await;
        let response = get_client_versions(axum::extract::State(state)).await.into_response();
        let body_bytes = axum::body::to_bytes(response.into_body(), 10000).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

        assert_eq!(body["unstable_features"]["org.matrix.msc3814"], true);
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
            "org.matrix.msc3983",
            "org.matrix.msc3245",
            "org.matrix.msc3266",
            "uk.tcpip.msc4133",
            "org.matrix.msc3886.sliding_sync",
            "org.matrix.msc4261.widget",
            "io.hula.burn_after_read",
            "io.hula.friends",
        ];
        for key in expected_unstable {
            assert!(unstable.contains_key(*key), "missing unstable feature: {key}");
        }
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
            "io.hula.sliding_sync",
        ];
        for key in expected_public {
            assert!(capabilities.contains_key(*key), "missing public capability: {key}");
        }

        let private_only: &[&str] = &["m.sso", "io.hula.friends", "io.hula.widget", "io.hula.burn_after_read"];
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
            "io.hula.widget",
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
            "io.hula.sliding_sync",
            "io.hula.friends",
            "m.sso",
            "ai_connection",
            "openclaw",
            "external_services",
            "io.hula.voice_extended",
            "io.hula.widget",
            "io.hula.burn_after_read",
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
            friends_capability(),
            external_services_capability(),
            voice_extended_capability(),
            widget_capability(),
            burn_after_read_capability(),
        ];

        for flag in all_capabilities {
            match flag.governance {
                CapabilityGovernance::RouteSurface | CapabilityGovernance::ConfigControlled => {}
            }
        }
    }
}
