//! 版本相关处理器
//!
//! HTTP handler functions for `/versions`, `/capabilities`, `.well-known`,
//! and server-version endpoints. Domain logic lives in
//! [`CapabilityGovernance`](crate::services::CapabilityGovernance); this
//! module is a thin adapter that collects route-surface manifests and
//! delegates.

use crate::common::config::Config;
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
        HeaderMap, HeaderValue,
    },
    Json,
};
use serde::Deserialize;
use serde_json::json;
use url::Url;

use crate::services::CapabilityGovernance;
use synapse_services::capability_governance::RouteCheck;

/// Empty query params marker used as the last handler parameter so that
/// `OptionalAuthenticatedUser` (a `FromRequestParts` type) is not the final
/// param — axum requires the last param to implement `FromRequest`, and
/// rust-analyzer cannot follow the cross-crate blanket impl.
#[derive(Deserialize, Default)]
pub struct EmptyQuery {}

// ---------------------------------------------------------------------------
// Route-surface collection
// ---------------------------------------------------------------------------

fn to_route_checks(entries: &[RouteEntry]) -> Vec<RouteCheck> {
    entries
        .iter()
        .map(|e| RouteCheck::new(e.method.as_str().to_string(), e.path))
        .collect()
}

/// Collect all registered route entries from every manifest that capability
/// functions may check. This is the bridge between the main crate's route
/// registration and [`CapabilityGovernance`]'s route-surface gating.
pub(crate) fn collect_route_surface() -> Vec<RouteCheck> {
    let mut routes = Vec::new();

    // Always-on route manifests
    routes.extend(to_route_checks(&room_summary::room_summary_route_manifest()));
    routes.extend(to_route_checks(&crate::web::routes::assembly::top_level_inline_manifest()));
    routes.extend(to_route_checks(&sliding_sync::sliding_sync_route_manifest()));
    routes.extend(to_route_checks(&account_compat::account_compat_route_manifest()));
    routes.extend(to_route_checks(&crate::web::routes::voip::voip_route_manifest()));
    routes.extend(to_route_checks(
        &crate::web::routes::handlers::search::search_route_manifest(),
    ));
    routes.extend(to_route_checks(
        &crate::web::routes::handlers::thread::thread_route_manifest(),
    ));

    // Feature-gated manifests — when the feature is disabled, the manifest
    // function may not exist at compile time. CapabilityGovernance will
    // return disabled for any route not found in the surface.
    #[cfg(feature = "friends")]
    {
        routes.extend(to_route_checks(&friend_room::friend_route_manifest()));
    }
    #[cfg(feature = "voice-extended")]
    {
        routes.extend(to_route_checks(&voice::voice_route_manifest()));
    }
    #[cfg(feature = "burn-after-read")]
    {
        routes.extend(to_route_checks(&burn_after_read::burn_after_read_route_manifest()));
    }
    #[cfg(feature = "widgets")]
    {
        routes.extend(to_route_checks(&widget::widget_route_manifest()));
    }
    #[cfg(feature = "external-services")]
    {
        routes.extend(to_route_checks(
            &crate::web::routes::external_service::external_service_route_manifest(),
        ));
    }

    routes
}

fn build_governance(config: &Config) -> CapabilityGovernance {
    CapabilityGovernance::new(config, collect_route_surface())
}

// ---------------------------------------------------------------------------
// HTTP headers (keep here — these are HTTP-layer concerns)
// ---------------------------------------------------------------------------

fn client_versions_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=600, s-maxage=3600, stale-while-revalidate=600"),
    );
    headers.insert(VARY, HeaderValue::from_static("Authorization"));
    headers
}

// ---------------------------------------------------------------------------
// Public helpers (used by other handler modules)
// ---------------------------------------------------------------------------

/// Public entry-point for external callers (e.g. federation discovery) that
/// need the `m.change_password` capability boolean without depending on the
/// private `CapabilityFlag` type.
pub(crate) fn change_password_capability_enabled(config: &Config) -> bool {
    build_governance(config).change_password_capability_enabled()
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// 获取客户端 API 版本
pub async fn get_client_versions(State(state): State<AppState>) -> impl axum::response::IntoResponse {
    let governance = build_governance(&state.services.core.config);
    (client_versions_headers(), Json(governance.build_client_versions()))
}

/// 获取服务端版本
pub async fn get_server_version(State(state): State<AppState>) -> impl axum::response::IntoResponse {
    Json(json!({
        "server_version": env!("CARGO_PKG_VERSION"),
        "python_version": "Rust",
        "server_name": state.services.core.server_name
    }))
}

/// 获取服务端能力
pub async fn get_capabilities(
    State(state): State<AppState>,
    auth: OptionalAuthenticatedUser,
    Query(_): Query<EmptyQuery>,
) -> Json<serde_json::Value> {
    let governance = build_governance(&state.services.core.config);
    Json(governance.build_capabilities_response(auth.user_id.is_some()))
}

// ---------------------------------------------------------------------------
// Server discovery helpers (unrelated to capability governance)
// ---------------------------------------------------------------------------

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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::header::{CACHE_CONTROL, VARY};

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
