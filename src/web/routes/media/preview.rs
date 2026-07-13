use crate::common::ApiError;
use crate::web::routes::context::MediaContext;
use crate::web::routes::OptionalAuthenticatedUser;
use axum::{
    extract::{Json, Query, State},
    http::{header, HeaderValue},
    response::IntoResponse,
};
use serde_json::{json, Value};

pub(crate) async fn media_config(State(ctx): State<MediaContext>) -> impl IntoResponse {
    let route_owner = synapse_services::worker::topology_validator::current_instance_worker_type(&ctx.config.worker);
    (
        [(header::HeaderName::from_static("x-synapse-route-owner"), HeaderValue::from_static(route_owner.as_str()))],
        Json(json!({
            "m.upload.size": ctx.config.server.max_upload_size
        })),
    )
}

#[allow(clippy::unused_async)]
#[allow(dead_code)]
async fn _preview_url(State(_ctx): State<MediaContext>, Query(params): Query<Value>) -> Result<Json<Value>, ApiError> {
    let url =
        params.get("url").and_then(|v| v.as_str()).ok_or_else(|| ApiError::bad_request("URL required".to_string()))?;

    Ok(Json(json!({
        "url": url,
        "title": "Preview",
        "description": "URL preview"
    })))
}

pub(crate) async fn preview_url(
    State(ctx): State<MediaContext>,
    _auth_user: OptionalAuthenticatedUser,
    Query(params): Query<Value>,
) -> Result<Json<Value>, ApiError> {
    let url =
        params.get("url").and_then(|v| v.as_str()).ok_or_else(|| ApiError::bad_request("URL required".to_string()))?;

    let blacklist = &ctx.config.url_preview.ip_range_blacklist;
    if let Err(e) = crate::common::check_url_against_blacklist(url, blacklist) {
        return Err(ApiError::forbidden(format!("URL not allowed: {e}")));
    }

    let ts = params.get("ts").and_then(|v| v.as_i64()).unwrap_or_else(|| chrono::Utc::now().timestamp_millis());

    match ctx.media_domain_service.preview_url(url, ts) {
        Ok(preview) => Ok(Json(preview)),
        Err(e) => Ok(Json(json!({
            "url": url,
            "title": "Preview unavailable",
            "description": format!("Could not generate preview: {}", e.message())
        }))),
    }
}
