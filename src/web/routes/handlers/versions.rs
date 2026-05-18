//! 版本相关处理器

use crate::web::AppState;
use axum::{extract::State, Json};
use serde_json::{json, Value};
use url::Url;

const CLIENT_VERSIONS_JSON_BASE: &str = r#"{"versions":["r0.5.0","r0.6.0","r0.6.1","v1.1","v1.2","v1.3","v1.4","v1.5","v1.6","v1.7","v1.8","v1.9","v1.10","v1.11","v1.12","v1.13"],"unstable_features":{"m.lazy_load_members":true,"m.require_identity_server":false,"m.supports_login_via_phone_number":true,"org.matrix.msc3882":true,"org.matrix.msc3983":true,"org.matrix.msc3245":true,"org.matrix.msc3266":true,"org.matrix.msc3916":true,"uk.tcpip.msc4133":false,"org.matrix.msc3886.sliding_sync":true,"org.matrix.msc4261.widget":true,"io.hula.burn_after_read":true}}"#;

/// 获取客户端 API 版本
#[allow(clippy::expect_used)]
pub async fn get_client_versions(
    State(state): State<AppState>,
) -> impl axum::response::IntoResponse {
    let config = &state.services.config;
    let mut versions: Value =
        serde_json::from_str(CLIENT_VERSIONS_JSON_BASE).expect("base versions json is valid");

    if let Some(unstable_features) = versions
        .get_mut("unstable_features")
        .and_then(|f| f.as_object_mut())
    {
        if config.experimental.msc3814_enabled {
            unstable_features.insert("org.matrix.msc3814".to_string(), json!(true));
        }
    }

    Json(versions)
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
) -> impl axum::response::IntoResponse {
    let saml_enabled = state.services.config.saml.enabled;
    let mut capabilities = json!({
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
        "io.hula.friends": true,
        "m.sso": {
            "enabled": saml_enabled,
            "providers": if saml_enabled { json!(["saml"]) } else { json!([]) }
        },
        "m.voice": { "enabled": true },
        "io.hula.burn_after_read": { "enabled": true },
        "m.thread": { "enabled": true },
        "io.hula.sliding_sync": { "enabled": true },
        "io.hula.widget": { "enabled": true }
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
    use super::{build_well_known_client, derive_well_known_server, get_client_versions};
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
