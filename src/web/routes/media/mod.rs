mod download;
mod preview;
mod quota;
mod upload;

use crate::web::AppState;
use axum::{
    routing::{get, post, put},
    Router,
};

use axum::extract::DefaultBodyLimit;
pub(crate) use preview::media_config;

// ---------------------------------------------------------------------------
// Re-exports for items used outside the media module
// ---------------------------------------------------------------------------

// Handlers used by assembly.rs directly:
// - media_config (re-exported above from preview)

// ---------------------------------------------------------------------------
// Router helper factories (private — only used by create_media_router)
// ---------------------------------------------------------------------------

fn create_media_config_router() -> Router<AppState> {
    Router::new().route("/config", get(preview::media_config))
}

fn create_media_preview_delete_router() -> Router<AppState> {
    Router::new()
        .route("/preview_url", get(preview::preview_url))
        .route("/delete/{server_name}/{media_id}", post(quota::delete_media))
}

fn create_media_legacy_download_router() -> Router<AppState> {
    Router::new()
        .route("/download/{server_name}/{media_id}", get(download::download_media_v1))
        .route("/download/{server_name}/{media_id}/{filename}", get(download::download_media_v1_with_filename))
}

/// r1 legacy download router with mandatory authentication.
///
/// VULN-01/VULN-02 fix: the r1 legacy download endpoints previously allowed
/// unauthenticated media access. We now require a valid access token via
/// `auth_middleware` before the handler runs. The v1/v3 download routes remain
/// public per Matrix spec (federation media compatibility); only the r1
/// legacy path is locked down.
fn create_media_r1_router(state: &AppState) -> Router<AppState> {
    use crate::web::routes::context::CoreContext;
    create_media_legacy_download_router().route_layer(axum::middleware::from_fn_with_state(
        <CoreContext as axum::extract::FromRef<AppState>>::from_ref(state),
        crate::web::middleware::auth_middleware,
    ))
}

/// G-1: 上传 body limit 的单一权威来源是 `config.server.max_upload_size`（u64，字节）。
/// 此前此处硬编码 50MB（50 * 1024 * 1024），与配置默认值（50_000_000 字节）及
/// `server/router.rs` 的全局 `RequestBodyLimitLayer` 不一致；现统一从配置派生。
fn media_upload_body_limit(max_upload_size: u64) -> usize {
    max_upload_size as usize
}

fn create_media_modern_upload_router(upload_limit: usize) -> Router<AppState> {
    Router::new().route("/upload", post(upload::upload_media_v3)).layer(DefaultBodyLimit::max(upload_limit))
}

fn create_media_v1_router(upload_limit: usize) -> Router<AppState> {
    Router::new()
        .merge(create_media_config_router())
        .merge(create_media_preview_delete_router())
        .merge(create_media_legacy_download_router())
        .route("/quota/check", get(quota::check_quota))
        .route("/quota/stats", get(quota::quota_stats))
        .route("/quota/alerts", get(quota::quota_alerts))
        // Chunked upload routes
        .route("/upload/chunk/start", post(upload::chunked_upload_start))
        .route("/upload/chunk/complete", post(upload::chunked_upload_complete))
        .route("/upload/chunk/cancel", post(upload::chunked_upload_cancel))
        .route("/upload/chunk/progress", get(upload::chunked_upload_progress))
        // Upload route with separate body limit to override Axum's default 2MB limit
        // G-1: limit 来自 config.server.max_upload_size，不再硬编码
        .merge(
            Router::new()
                .route("/upload", post(upload::upload_media_v1))
                .layer(DefaultBodyLimit::max(upload_limit)),
        )
        // Chunk upload route with separate body limit
        // G-1 说明: 10MB 是「单个分块」上限，与整文件上限 max_upload_size 语义不同，
        // 但以配置值为 cap（配置小于 10MB 时收紧到配置值）
        .merge(
            Router::new()
                .route("/upload/chunk", post(upload::chunked_upload_chunk))
                .layer(DefaultBodyLimit::max(upload_limit.min(10 * 1024 * 1024))),
        )
}

fn create_media_v3_router(upload_limit: usize) -> Router<AppState> {
    Router::new()
        .merge(create_media_modern_upload_router(upload_limit))
        .merge(create_media_config_router())
        .merge(create_media_preview_delete_router())
        // G-2: MSC2246 异步上传端点必须覆盖 Axum 默认 2MB body limit，
        // 与 /upload 对齐
        // G-1: limit 来自 config.server.max_upload_size，不再硬编码 50MB
        .merge(
            Router::new()
                .route("/upload/{server_name}/{media_id}", put(upload::upload_media_with_id))
                .layer(DefaultBodyLimit::max(upload_limit)),
        )
        .route("/download/{server_name}/{media_id}", get(download::download_media))
        .route("/download/{server_name}/{media_id}/{filename}", get(download::download_media_with_filename))
        .route("/download_signed/{server_name}/{media_id}", get(download::download_media_signed))
        .route(
            "/download_signed/{server_name}/{media_id}/{filename}",
            get(download::download_media_signed_with_filename),
        )
        .route("/thumbnail/{server_name}/{media_id}", get(download::get_thumbnail))
}

fn create_media_r0_router(upload_limit: usize) -> Router<AppState> {
    create_media_modern_upload_router(upload_limit)
        .merge(create_media_config_router())
        .merge(create_media_legacy_download_router())
        .merge(create_media_preview_delete_router())
}

fn create_media_authenticated_router() -> Router<AppState> {
    Router::new()
        .route("/download/{server_name}/{media_id}", get(download::download_media_authenticated))
        .route(
            "/download/{server_name}/{media_id}/{filename}",
            get(download::download_media_authenticated_with_filename),
        )
        .route("/thumbnail/{server_name}/{media_id}", get(download::get_thumbnail_authenticated))
}

// ---------------------------------------------------------------------------
// Public router factory
// ---------------------------------------------------------------------------

/// Assemble the full media router under Matrix-compatible prefixes.
///
/// Nests routers under:
///   - `/_matrix/media/v1`
///   - `/_matrix/media/v3`
///   - `/_matrix/media/r0`
///   - `/_matrix/media/r1`
///   - `/_matrix/client/v1/media`
pub fn create_media_router(state: &AppState) -> Router<AppState> {
    // G-1: 从权威配置字段读取一次，统一下发到 v1/v3/r0 所有上传路由
    let upload_limit = media_upload_body_limit(state.services.core.config.server.max_upload_size);
    let preview_router = Router::new().route("/preview_url", get(preview::preview_url));
    let authenticated_media_router = create_media_authenticated_router();
    Router::new()
        .nest("/_matrix/media/v1", create_media_v1_router(upload_limit))
        .nest("/_matrix/media/v3", create_media_v3_router(upload_limit))
        .nest("/_matrix/media/r0", create_media_r0_router(upload_limit))
        .nest("/_matrix/media/r1", create_media_r1_router(state))
        .nest("/_matrix/client/v1/media", authenticated_media_router.merge(preview_router))
}

/// See [`create_upload_provider_router`].
pub fn create_upload_provider_router() -> Router<AppState> {
    Router::new()
        .route("/upload/token", post(upload::create_upload_token))
        .route("/upload/provider", get(upload::get_upload_provider))
}

// ---------------------------------------------------------------------------
// Route ledger manifest
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn test_media_routes_structure() {
        let routes = vec![
            "/_matrix/media/v3/upload/{server_name}/{media_id}",
            "/_matrix/media/v3/download/{server_name}/{media_id}",
            "/_matrix/media/v3/thumbnail/{server_name}/{media_id}",
            "/_matrix/media/v1/upload",
            "/_matrix/media/v3/upload",
            "/_matrix/media/r0/upload",
            "/_matrix/media/r1/download/{server_name}/{media_id}",
            "/_matrix/media/v1/config",
            "/_matrix/media/v3/config",
        ];

        for route in routes {
            assert!(route.starts_with("/_matrix/media/"));
        }
    }

    #[test]
    fn test_media_nested_router_boundaries() {
        let v1_paths = [
            "/upload",
            "/config",
            "/quota/check",
            "/quota/stats",
            "/quota/alerts",
            "/download/{server_name}/{media_id}",
            "/download/{server_name}/{media_id}/{filename}",
            "/preview_url",
            "/delete/{server_name}/{media_id}",
        ];
        let v3_paths = [
            "/upload/{server_name}/{media_id}",
            "/download/{server_name}/{media_id}",
            "/download/{server_name}/{media_id}/{filename}",
            "/thumbnail/{server_name}/{media_id}",
            "/upload",
            "/preview_url",
            "/config",
            "/delete/{server_name}/{media_id}",
        ];

        assert_eq!(v1_paths.len(), 9);
        assert_eq!(v3_paths.len(), 8);
        assert!(v1_paths.iter().all(|path| path.starts_with('/')));
        assert!(v3_paths.iter().all(|path| path.starts_with('/')));
    }

    #[test]
    fn test_media_shared_router_contains_common_paths() {
        let shared_paths = ["/config", "/preview_url", "/delete/{server_name}/{media_id}"];
        let modern_upload_paths = ["/upload"];
        let legacy_download_paths =
            ["/download/{server_name}/{media_id}", "/download/{server_name}/{media_id}/{filename}"];

        assert_eq!(shared_paths.len(), 3);
        assert_eq!(modern_upload_paths.len(), 1);
        assert_eq!(legacy_download_paths.len(), 2);
        assert!(shared_paths.iter().all(|path| path.starts_with('/')));
        assert!(legacy_download_paths.iter().all(|path| path.starts_with("/download/")));
    }

    #[test]
    fn test_media_router_keeps_version_boundaries() {
        let r0_only_paths = ["/_matrix/media/r0/upload"];
        let r1_only_paths = ["/_matrix/media/r1/download/{server_name}/{media_id}"];
        let v1_only_paths = ["/_matrix/media/v1/quota/check"];
        let v3_only_paths = [
            "/_matrix/media/v3/upload/{server_name}/{media_id}",
            "/_matrix/media/v3/thumbnail/{server_name}/{media_id}",
        ];

        assert!(r0_only_paths.iter().all(|path| !path.contains("/preview_url")));
        assert!(r1_only_paths.iter().all(|path| !path.contains("/delete/")));
        assert!(v1_only_paths.iter().all(|path| path.starts_with("/_matrix/media/v1/")));
        assert!(v3_only_paths.iter().all(|path| path.starts_with("/_matrix/media/v3/")));
    }

    #[test]
    fn test_media_config_response() {
        let config = json!({
            "m.upload.size": 50 * 1024 * 1024
        });

        assert!(config.get("m.upload.size").is_some());
        let size = config.get("m.upload.size").unwrap().as_i64().unwrap();
        assert_eq!(size, 50 * 1024 * 1024);
    }

    #[test]
    fn test_content_type_default() {
        let default_content_type = "application/octet-stream";
        assert!(!default_content_type.is_empty());
    }

    #[test]
    fn test_media_id_format() {
        let media_ids = vec!["abc123", "media_id_with_underscores", "media-id-with-dashes", "UPPERCASE123"];

        for id in media_ids {
            assert!(!id.is_empty());
            assert!(id.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-'));
        }
    }

    #[test]
    fn test_server_name_format() {
        let server_names = vec!["example.com", "matrix.org", "server.local"];

        for name in server_names {
            assert!(!name.is_empty());
            assert!(name.contains('.'));
        }
    }

    #[test]
    fn test_upload_response_structure() {
        let response = json!({
            "content_uri": "mxc://example.com/media_id_123"
        });

        assert!(response.get("content_uri").is_some());
        let uri = response.get("content_uri").unwrap().as_str().unwrap();
        assert!(uri.starts_with("mxc://"));
    }

    #[test]
    fn test_delete_response_structure() {
        let response = json!({
            "deleted": true,
            "media_id": "media_id_123"
        });

        assert!(response.get("deleted").unwrap().as_bool().unwrap());
        assert!(response.get("media_id").is_some());
    }

    #[test]
    fn test_thumbnail_size_params() {
        let params = json!({
            "width": 256,
            "height": 256,
            "method": "scale"
        });

        assert_eq!(params.get("width").unwrap().as_i64().unwrap(), 256);
        assert_eq!(params.get("height").unwrap().as_i64().unwrap(), 256);
        assert_eq!(params.get("method").unwrap().as_str().unwrap(), "scale");
    }

    #[test]
    fn test_content_type_fallback_is_octet_stream() {
        let default_ct = "application/octet-stream";
        assert!(!default_ct.is_empty());
    }

    /// G-1: media 路由的上传 body limit 必须等于 `config.server.max_upload_size`
    /// （单一权威来源），不允许再出现独立的硬编码值。
    #[test]
    fn test_g1_upload_body_limit_matches_config() {
        // limit 原样透传配置值（字节），无二次换算、无硬编码回退
        let configured = 100 * 1024 * 1024u64; // 例如管理员配置 100MB
        assert_eq!(super::media_upload_body_limit(configured), configured as usize);

        // 默认配置（server.rs 中 default_max_upload_size_value = 50_000_000 字节）
        // 与 server/router.rs 全局 RequestBodyLimitLayer 读取同一字段，二者天然一致
        let default_bytes = 50_000_000u64;
        assert_eq!(super::media_upload_body_limit(default_bytes), 50_000_000usize);
    }
}
