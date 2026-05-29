//! 版本相关处理器

use crate::common::config::Config;
use crate::common::room_versions::client_room_versions_capability;
use crate::web::AppState;
use crate::web::routes::extractors::auth::OptionalAuthenticatedUser;
use axum::{
    extract::{Query, State},
    http::{
        header::{CACHE_CONTROL, VARY},
        HeaderMap, HeaderValue,
    },
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use url::Url;

/// Empty query params marker used as the last handler parameter so that
/// `OptionalAuthenticatedUser` (a `FromRequestParts` type) is not the final
/// param — axum requires the last param to implement `FromRequest`, and
/// rust-analyzer cannot follow the cross-crate blanket impl.
#[derive(Deserialize, Default)]
pub struct EmptyQuery {}

const CLIENT_API_VERSIONS: &[&str] = &[
    "r0.5.0", "r0.6.0", "r0.6.1", "v1.1", "v1.2", "v1.3", "v1.4", "v1.5", "v1.6",
    "v1.7", "v1.8", "v1.9", "v1.10", "v1.11", "v1.12", "v1.13",
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
    ("org.matrix.msc4261.widget", true),
    ("io.hula.burn_after_read", true),
    ("io.hula.friends", true),
];

fn build_client_versions(config: &Config) -> Value {
    let mut unstable_features = serde_json::Map::new();

    for (feature, enabled) in BASE_UNSTABLE_FEATURES {
        unstable_features.insert((*feature).to_string(), json!(enabled));
    }

    if config.experimental.msc3814_enabled {
        unstable_features.insert("org.matrix.msc3814".to_string(), json!(true));
    }

    json!({
        "versions": CLIENT_API_VERSIONS,
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
pub async fn get_client_versions(
    State(state): State<AppState>,
) -> impl axum::response::IntoResponse {
    (client_versions_headers(), Json(build_client_versions(&state.services.config)))
}

/// 获取服务端版本
pub async fn get_server_version(
    State(state): State<AppState>,
) -> impl axum::response::IntoResponse {
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

fn derive_well_known_server(
    public_baseurl: &str,
    fallback_server_name: &str,
    federation_port: u16,
) -> String {
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
    let saml_enabled = state.services.config.saml.enabled;
    let mut capabilities = json!({
        "m.change_password": { "enabled": true },
        "m.room_versions": client_room_versions_capability(),
        "m.set_displayname": { "enabled": true },
        "m.set_avatar_url": { "enabled": true },
        "m.3pid_changes": { "enabled": true },
        "m.room.summary": { "enabled": true },
        "m.room.suggested": { "enabled": true },
        "io.hula.friends": true,
        "m.sso": {
            "enabled": saml_enabled,
            "providers": if saml_enabled { json!(["saml"]) } else { json!([]) }
        },
        "m.voice": { "enabled": true },
        "io.hula.burn_after_read": { "enabled": cfg!(feature = "burn-after-read") },
        "m.thread": { "enabled": true },
        "io.hula.sliding_sync": { "enabled": true },
        "io.hula.widget": { "enabled": cfg!(feature = "widgets") }
    });

    #[cfg(feature = "openclaw-routes")]
    {
        let ai_connection_enabled = state.services.config.experimental.openclaw_routes_enabled;
        if let Some(caps) = capabilities.as_object_mut() {
            caps.insert(
                "ai_connection".to_string(),
                json!({ "enabled": ai_connection_enabled }),
            );
            caps.insert(
                "openclaw".to_string(),
                json!({ "enabled": ai_connection_enabled }),
            );
        }
    }
    #[cfg(not(feature = "openclaw-routes"))]
    {
        if let Some(caps) = capabilities.as_object_mut() {
            caps.insert("ai_connection".to_string(), json!({ "enabled": false }));
            caps.insert("openclaw".to_string(), json!({ "enabled": false }));
        }
    }

    #[cfg(feature = "external-services")]
    {
        if let Some(caps) = capabilities.as_object_mut() {
            caps.insert("external_services".to_string(), json!({ "enabled": true }));
        }
    }
    #[cfg(not(feature = "external-services"))]
    {
        if let Some(caps) = capabilities.as_object_mut() {
            caps.insert("external_services".to_string(), json!({ "enabled": false }));
        }
    }

    // Voice extended (MSC3245 extended server-side features)
    #[cfg(feature = "voice-extended")]
    {
        if let Some(caps) = capabilities.as_object_mut() {
            caps.insert("io.hula.voice_extended".to_string(), json!({ "enabled": true }));
        }
    }
    #[cfg(not(feature = "voice-extended"))]
    {
        if let Some(caps) = capabilities.as_object_mut() {
            caps.insert("io.hula.voice_extended".to_string(), json!({ "enabled": false }));
        }
    }

    // CAS SSO
    #[cfg(feature = "cas-sso")]
    {
        if let Some(caps) = capabilities.as_object_mut() {
            if let Some(sso) = caps.get_mut("m.sso").and_then(|v| v.as_object_mut()) {
                if let Some(providers) = sso.get_mut("providers").and_then(|v| v.as_array_mut()) {
                    providers.push(json!("cas"));
                }
            }
        }
    }

    // Widgets
    #[cfg(feature = "widgets")]
    {
        if let Some(caps) = capabilities.as_object_mut() {
            caps.insert("io.hula.widget".to_string(), json!({ "enabled": true }));
        }
    }
    #[cfg(not(feature = "widgets"))]
    {
        if let Some(caps) = capabilities.as_object_mut() {
            caps.insert("io.hula.widget".to_string(), json!({ "enabled": false }));
        }
    }

    // Burn after read
    #[cfg(feature = "burn-after-read")]
    {
        if let Some(caps) = capabilities.as_object_mut() {
            caps.insert("io.hula.burn_after_read".to_string(), json!({ "enabled": true }));
        }
    }
    #[cfg(not(feature = "burn-after-read"))]
    {
        if let Some(caps) = capabilities.as_object_mut() {
            caps.insert("io.hula.burn_after_read".to_string(), json!({ "enabled": false }));
        }
    }

    // For unauthenticated users, only return public capabilities
    let capabilities = if auth.user_id.is_none() {
        if let Some(caps) = capabilities.as_object_mut() {
            // Remove sensitive capabilities for unauthenticated access
            caps.remove("io.hula.friends");
            caps.remove("io.hula.burn_after_read");
            caps.remove("io.hula.widget");
            caps.remove("ai_connection");
            caps.remove("openclaw");
            caps.remove("external_services");
            caps.remove("io.hula.voice_extended");
            caps.remove("m.sso");
        }
        capabilities
    } else {
        capabilities
    };

    Json(json!({
        "capabilities": capabilities,
        "unstable_features": {
            "io.hula.friends": true,
            "org.matrix.msc3245.voice": true,
            "org.matrix.msc3983.thread": true,
            "org.matrix.msc3886.sliding_sync": true,
            "org.matrix.msc4261.widget": true,
            "io.hula.burn_after_read": true
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::{
        build_client_versions, build_well_known_client, client_versions_headers,
        derive_well_known_server, get_client_versions, CLIENT_API_VERSIONS,
    };
    use axum::http::header::{CACHE_CONTROL, VARY};
    use crate::cache::{CacheConfig, CacheManager};
    use crate::common::config::Config;
    use crate::services::ServiceContainer;
    use crate::web::AppState;
    use std::sync::Arc;

    async fn make_test_state() -> AppState {
        let mut config = Config::default();
        config.experimental.msc3814_enabled = true;

        let pool = crate::test_utils::take_prepared_test_pool().unwrap_or_else(|| {
            Arc::new(
                sqlx::postgres::PgPoolOptions::new()
                    .connect_lazy("postgres://localhost/test")
                    .expect("Failed to create test database pool"),
            )
        });
        let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
        let services = ServiceContainer::new(&pool, cache.clone(), config, None).await;
        AppState::new(services, cache)
    }

    #[test]
    fn test_build_client_versions_keeps_supported_versions_ordered_and_unique() {
        let body = build_client_versions(&Config::default());
        let versions = body["versions"].as_array().expect("versions should be an array");

        assert_eq!(versions.len(), CLIENT_API_VERSIONS.len());
        for expected in CLIENT_API_VERSIONS {
            assert_eq!(
                versions.iter().filter(|version| version.as_str() == Some(expected)).count(),
                1,
                "version {expected} should appear exactly once"
            );
        }
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
        assert_eq!(
            headers.get(VARY).and_then(|value| value.to_str().ok()),
            Some("Authorization")
        );
    }

    #[tokio::test]
    async fn test_get_client_versions_includes_msc3814() {
        use axum::response::IntoResponse;
        let state = make_test_state().await;
        let response = get_client_versions(axum::extract::State(state))
            .await
            .into_response();
        let body_bytes = axum::body::to_bytes(response.into_body(), 10000)
            .await
            .unwrap();
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
        assert_eq!(
            body["m.homeserver"]["base_url"],
            "https://matrix.example.com"
        );
        assert!(body.get("m.identity_server").is_none());
    }
}
