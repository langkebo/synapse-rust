//! 版本相关处理器

use crate::web::AppState;
use axum::{
    extract::State,
    http::{header, StatusCode},
    Json,
};
use serde_json::json;
use url::Url;

const CLIENT_VERSIONS_JSON: &str = r#"{"versions":["r0.5.0","r0.6.0","r0.6.1","v1.1","v1.2","v1.3","v1.4","v1.5","v1.6","v1.7","v1.8","v1.9","v1.10","v1.11","v1.12","v1.13"],"unstable_features":{"m.lazy_load_members":true,"m.require_identity_server":false,"m.supports_login_via_phone_number":true,"org.matrix.msc3882":true,"org.matrix.msc3983":true,"org.matrix.msc3245":true,"org.matrix.msc3266":true,"org.matrix.msc3814":true,"uk.tcpip.msc4133":false}}"#;

/// 获取客户端 API 版本
pub async fn get_client_versions() -> impl axum::response::IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json")],
        CLIENT_VERSIONS_JSON,
    )
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
        format!("[{}]:{}", host, port)
    } else {
        format!("{}:{}", host, port)
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
pub async fn get_capabilities() -> impl axum::response::IntoResponse {
    Json(json!({
        "capabilities": {
            "m.change_password": { "enabled": true },
            "m.room_versions": {
                "default": "10",
                "available": {
                    "1": "stable", "2": "stable", "3": "stable",
                    "4": "stable", "5": "stable", "6": "stable",
                    "7": "stable", "8": "stable", "9": "stable",
                    "10": "stable", "11": "stable"
                }
            },
            "m.set_displayname": { "enabled": true },
            "m.set_avatar_url": { "enabled": true },
            "m.3pid_changes": { "enabled": true },
            "m.room.summary": { "enabled": true },
            "m.room.suggested": { "enabled": true },
            "io.hula.friends": { "enabled": true }
        },
        "unstable_features": {
            "io.hula.friends": true
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::{build_well_known_client, derive_well_known_server, CLIENT_VERSIONS_JSON};
    use serde_json::Value;

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

    #[test]
    fn test_client_versions_advertises_modern_spec_versions() {
        let parsed: Value = serde_json::from_str(CLIENT_VERSIONS_JSON)
            .expect("CLIENT_VERSIONS_JSON must be valid JSON");
        let versions = parsed["versions"]
            .as_array()
            .expect("versions must be an array");
        let strs: Vec<&str> = versions.iter().filter_map(|v| v.as_str()).collect();
        // Older spec versions Element relies on for backwards compat.
        assert!(strs.contains(&"v1.1"));
        // Modern spec versions clients gate features on (e.g. authenticated media,
        // get_login_token / MSC3882 stable in v1.7+).
        assert!(strs.contains(&"v1.7"));
        assert!(strs.contains(&"v1.13"));
        // MSC3814 is still unstable and must remain advertised under
        // unstable_features for client opt-in.
        assert_eq!(parsed["unstable_features"]["org.matrix.msc3814"], true);
    }
}
