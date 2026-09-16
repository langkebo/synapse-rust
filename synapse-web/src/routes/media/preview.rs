use crate::routes::context::MediaContext;
use crate::routes::AuthenticatedUser;
use axum::{
    extract::{Json, Query, State},
    http::{header, HeaderValue},
    response::IntoResponse,
};
use serde_json::{json, Value};
use synapse_common::current_timestamp_millis;
use synapse_common::ApiError;

/// See [`media_config`].
pub(crate) async fn media_config(State(ctx): State<MediaContext>, _auth_user: AuthenticatedUser) -> impl IntoResponse {
    let route_owner = synapse_services::worker::topology_validator::current_instance_worker_type(&ctx.config.worker);
    (
        [(header::HeaderName::from_static("x-synapse-route-owner"), HeaderValue::from_static(route_owner.as_str()))],
        Json(json!({
            "m.upload.size": ctx.config.server.max_upload_size
        })),
    )
}

/// See [`preview_url`].
pub(crate) async fn preview_url(
    State(ctx): State<MediaContext>,
    _auth_user: AuthenticatedUser,
    Query(params): Query<Value>,
) -> Result<Json<Value>, ApiError> {
    // P2-11: MSC4452 endpoint-level 403 enforcement (Synapse v1.154 #19715).
    // When the `io.element.msc4452.preview_url` capability is disabled
    // (controlled by `config.experimental.msc4452_enabled`, default false),
    // the preview_url endpoint MUST return 403 Forbidden, matching the
    // capability declaration in capability_governance.rs. This is a
    // capability-driven feature gate, not a permission check — fail-closed
    // when the feature is off.
    if !ctx.config.experimental.msc4452_enabled {
        return Err(ApiError::forbidden("URL preview is disabled (MSC4452 not enabled)".to_string()));
    }

    let url =
        params.get("url").and_then(|v| v.as_str()).ok_or_else(|| ApiError::bad_request("URL required".to_string()))?;

    let blacklist = &ctx.config.url_preview.ip_range_blacklist;
    // S2: 使用 check_url_and_resolve（内部与 check_url_against_blacklist 校验一致，
    // 但额外返回已验证 IP）。当前 preview_url 为 stub 不发起实际请求；未来实现真实
    // 抓取时，必须将返回的 verified_ips 经 http_client::pinned_client_for_url 钉扎，
    // 否则存在 DNS rebinding TOCTOU 风险。
    if let Err(e) = synapse_common::check_url_and_resolve(url, blacklist) {
        return Err(ApiError::forbidden(format!("URL not allowed: {e}")));
    }

    let ts = params.get("ts").and_then(|v| v.as_i64()).unwrap_or_else(current_timestamp_millis);

    match ctx.media_domain_service.preview_url(url, ts) {
        Ok(preview) => Ok(Json(preview)),
        Err(e) => Ok(Json(json!({
            "url": url,
            "title": "Preview unavailable",
            "description": format!("Could not generate preview: {}", e.message())
        }))),
    }
}
