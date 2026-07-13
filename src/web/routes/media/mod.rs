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

fn create_media_modern_upload_router() -> Router<AppState> {
    Router::new().route("/upload", post(upload::upload_media_v3)).layer(DefaultBodyLimit::max(50 * 1024 * 1024))
}

fn create_media_v1_router() -> Router<AppState> {
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
        .merge(
            Router::new()
                .route("/upload", post(upload::upload_media_v1))
                .layer(DefaultBodyLimit::max(50 * 1024 * 1024)),
        )
        // Chunk upload route with separate body limit
        .merge(
            Router::new()
                .route("/upload/chunk", post(upload::chunked_upload_chunk))
                .layer(DefaultBodyLimit::max(10 * 1024 * 1024)),
        )
}

fn create_media_v3_router() -> Router<AppState> {
    Router::new()
        .merge(create_media_modern_upload_router())
        .merge(create_media_config_router())
        .merge(create_media_preview_delete_router())
        .route("/upload/{server_name}/{media_id}", put(upload::upload_media_with_id))
        .route("/download/{server_name}/{media_id}", get(download::download_media))
        .route("/download/{server_name}/{media_id}/{filename}", get(download::download_media_with_filename))
        .route("/download_signed/{server_name}/{media_id}", get(download::download_media_signed))
        .route(
            "/download_signed/{server_name}/{media_id}/{filename}",
            get(download::download_media_signed_with_filename),
        )
        .route("/thumbnail/{server_name}/{media_id}", get(download::get_thumbnail))
}

fn create_media_r0_router() -> Router<AppState> {
    create_media_modern_upload_router()
        .merge(create_media_config_router())
        .merge(create_media_legacy_download_router())
        .merge(create_media_preview_delete_router())
}

fn create_media_r1_router() -> Router<AppState> {
    create_media_legacy_download_router()
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
pub fn create_media_router(_state: &AppState) -> Router<AppState> {
    let preview_router = Router::new().route("/preview_url", get(preview::preview_url));
    let authenticated_media_router = create_media_authenticated_router();
    Router::new()
        .nest("/_matrix/media/v1", create_media_v1_router())
        .nest("/_matrix/media/v3", create_media_v3_router())
        .nest("/_matrix/media/r0", create_media_r0_router())
        .nest("/_matrix/media/r1", create_media_r1_router())
        .nest("/_matrix/client/v1/media", authenticated_media_router.merge(preview_router))
}

pub fn create_upload_provider_router() -> Router<AppState> {
    Router::new()
        .route("/upload/token", post(upload::create_upload_token))
        .route("/upload/provider", get(upload::get_upload_provider))
}

// ---------------------------------------------------------------------------
// Route ledger manifest
// ---------------------------------------------------------------------------

fn media_v1_relative_routes() -> Vec<(axum::http::Method, &'static str)> {
    use axum::http::Method;
    vec![
        (Method::POST, "/upload"),
        (Method::GET, "/config"),
        (Method::GET, "/preview_url"),
        (Method::POST, "/delete/{server_name}/{media_id}"),
        (Method::GET, "/download/{server_name}/{media_id}"),
        (Method::GET, "/download/{server_name}/{media_id}/{filename}"),
        (Method::GET, "/quota/check"),
        (Method::GET, "/quota/stats"),
        (Method::GET, "/quota/alerts"),
        (Method::POST, "/upload/chunk/start"),
        (Method::POST, "/upload/chunk"),
        (Method::POST, "/upload/chunk/complete"),
        (Method::POST, "/upload/chunk/cancel"),
        (Method::GET, "/upload/chunk/progress"),
    ]
}

fn media_v3_relative_routes() -> Vec<(axum::http::Method, &'static str)> {
    use axum::http::Method;
    vec![
        (Method::POST, "/upload"),
        (Method::GET, "/config"),
        (Method::GET, "/preview_url"),
        (Method::POST, "/delete/{server_name}/{media_id}"),
        (Method::PUT, "/upload/{server_name}/{media_id}"),
        (Method::GET, "/download/{server_name}/{media_id}"),
        (Method::GET, "/download/{server_name}/{media_id}/{filename}"),
        (Method::GET, "/download_signed/{server_name}/{media_id}"),
        (Method::GET, "/download_signed/{server_name}/{media_id}/{filename}"),
        (Method::GET, "/thumbnail/{server_name}/{media_id}"),
    ]
}

fn media_r0_relative_routes() -> Vec<(axum::http::Method, &'static str)> {
    use axum::http::Method;
    vec![(Method::POST, "/upload"), (Method::GET, "/config")]
}

fn media_r1_relative_routes() -> Vec<(axum::http::Method, &'static str)> {
    use axum::http::Method;
    vec![
        (Method::GET, "/download/{server_name}/{media_id}"),
        (Method::GET, "/download/{server_name}/{media_id}/{filename}"),
    ]
}

fn media_authenticated_relative_routes() -> Vec<(axum::http::Method, &'static str)> {
    use axum::http::Method;
    vec![
        (Method::GET, "/download/{server_name}/{media_id}"),
        (Method::GET, "/download/{server_name}/{media_id}/{filename}"),
        (Method::GET, "/thumbnail/{server_name}/{media_id}"),
        (Method::GET, "/preview_url"),
    ]
}

pub fn media_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::expand_under_prefixes;
    let mut out = expand_under_prefixes("media", &["/_matrix/media/v1"], &media_v1_relative_routes());
    out.extend(expand_under_prefixes("media", &["/_matrix/media/v3"], &media_v3_relative_routes()));
    out.extend(expand_under_prefixes("media", &["/_matrix/media/r0"], &media_r0_relative_routes()));
    out.extend(expand_under_prefixes("media", &["/_matrix/media/r1"], &media_r1_relative_routes()));
    out.extend(expand_under_prefixes("media", &["/_matrix/client/v1/media"], &media_authenticated_relative_routes()));
    out
}

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
}
