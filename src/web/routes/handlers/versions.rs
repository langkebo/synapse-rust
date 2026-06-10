//! 版本相关处理器

use crate::common::config::Config;
use crate::common::room_versions::client_room_versions_capability;
use crate::web::routes::extractors::auth::OptionalAuthenticatedUser;
use crate::web::AppState;
use axum::{
    extract::{Query, State},
    http::{
        header::{CACHE_CONTROL, VARY},
        HeaderMap, HeaderValue,
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
    ("org.matrix.msc3916", true),
    ("uk.tcpip.msc4133", true),
    ("org.matrix.msc3886.sliding_sync", true),
    ("org.matrix.msc4261.widget", cfg!(feature = "widgets")),
    ("io.hula.burn_after_read", cfg!(feature = "burn-after-read")),
    ("io.hula.friends", cfg!(feature = "friends")),
];

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
    (client_versions_headers(), Json(build_client_versions(&state.services.config)))
}

/// 获取服务端版本
pub async fn get_server_version(State(state): State<AppState>) -> impl axum::response::IntoResponse {
    Json(json!({
        "server_version": env!("CARGO_PKG_VERSION"),
        "python_version": "Rust",
        "server_name": state.services.config.server.name
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
        "io.hula.friends": cfg!(feature = "friends"),
        "org.matrix.msc3245.voice": true,
        "org.matrix.msc3983.thread": true,
        "org.matrix.msc3886.sliding_sync": true,
        "org.matrix.msc4261.widget": cfg!(feature = "widgets"),
        "io.hula.burn_after_read": cfg!(feature = "burn-after-read")
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

fn build_capabilities_response(config: &Config, authenticated: bool) -> Value {
    let mut capabilities = Map::new();
    let sso_providers = sso_providers(config);

    insert_enabled_capability(&mut capabilities, "m.change_password", true);
    capabilities.insert("m.room_versions".to_string(), client_room_versions_capability());
    insert_enabled_capability(&mut capabilities, "m.set_displayname", true);
    insert_enabled_capability(&mut capabilities, "m.set_avatar_url", true);
    insert_enabled_capability(&mut capabilities, "m.3pid_changes", true);
    insert_enabled_capability(&mut capabilities, "m.room.summary", true);
    insert_enabled_capability(&mut capabilities, "m.room.suggested", true);
    insert_enabled_capability(&mut capabilities, "m.voice", true);
    insert_enabled_capability(&mut capabilities, "m.thread", true);
    insert_enabled_capability(&mut capabilities, "io.hula.sliding_sync", true);

    if authenticated {
        let openclaw_enabled = openclaw_routes_enabled(config);

        capabilities.insert("io.hula.friends".to_string(), json!(cfg!(feature = "friends")));
        capabilities.insert(
            "m.sso".to_string(),
            json!({
                "enabled": !sso_providers.is_empty(),
                "providers": sso_providers
            }),
        );
        insert_enabled_capability(&mut capabilities, "ai_connection", openclaw_enabled);
        insert_enabled_capability(&mut capabilities, "openclaw", openclaw_enabled);
        insert_enabled_capability(&mut capabilities, "external_services", cfg!(feature = "external-services"));
        insert_enabled_capability(&mut capabilities, "io.hula.voice_extended", cfg!(feature = "voice-extended"));
        insert_enabled_capability(&mut capabilities, "io.hula.widget", cfg!(feature = "widgets"));
        insert_enabled_capability(&mut capabilities, "io.hula.burn_after_read", cfg!(feature = "burn-after-read"));
    }

    json!({
        "capabilities": capabilities,
        "unstable_features": build_capabilities_unstable_features()
    })
}

/// .well-known: Matrix 服务器发现
pub async fn get_well_known_server(State(state): State<AppState>) -> Json<serde_json::Value> {
    let server_name = state.services.config.server.get_server_name();
    let public_baseurl = state.services.config.server.get_public_baseurl();
    let federation_port = state.services.config.federation.federation_port;
    Json(json!({
        "m.server": derive_well_known_server(&public_baseurl, server_name, federation_port)
    }))
}

/// .well-known: Matrix 客户端发现
pub async fn get_well_known_client(State(state): State<AppState>) -> Json<serde_json::Value> {
    let base_url = state.services.config.server.get_public_baseurl();
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
    Json(build_capabilities_response(&state.services.config, auth.user_id.is_some()))
}

#[cfg(test)]
mod tests {
    use super::{
        build_capabilities_response, build_client_versions, build_well_known_client, client_versions_headers,
        derive_well_known_server, get_client_versions, ClientApiVersionFamily, CLIENT_API_VERSION_SUPPORT,
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

        assert!(capabilities.contains_key("m.change_password"));
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

        assert_eq!(capabilities["m.sso"]["enabled"], true);
        assert_eq!(capabilities["m.sso"]["providers"][0], "saml");
        assert_eq!(capabilities["openclaw"]["enabled"], false);
        assert_eq!(capabilities["io.hula.widget"]["enabled"], cfg!(feature = "widgets"));
        assert_eq!(capabilities["io.hula.burn_after_read"]["enabled"], cfg!(feature = "burn-after-read"));
        assert_eq!(body["unstable_features"]["org.matrix.msc4261.widget"], cfg!(feature = "widgets"));
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
}
