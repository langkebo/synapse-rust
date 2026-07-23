use crate::common::ApiError;
use crate::web::routes::context::MediaContext;
use crate::web::AuthenticatedUser;
use axum::extract::{Json, Path, State};
use serde_json::{json, Value};

use super::upload::ensure_local_media_server_name;

pub(crate) async fn check_quota(
    State(ctx): State<MediaContext>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let quota_info = ctx.media_domain_service.get_user_quota(&auth_user.user_id).await?;

    let limit = quota_info.max_storage_bytes;
    let used = quota_info.current_storage_bytes;
    let remaining = if used >= limit { 0 } else { limit - used };

    Ok(Json(json!({
        "limit": limit,
        "used": used,
        "remaining": remaining,
        "rule": "global"
    })))
}

pub(crate) async fn quota_stats(
    State(ctx): State<MediaContext>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let quota_info = ctx.media_domain_service.get_user_quota(&auth_user.user_id).await?;
    let stats = ctx.media_domain_service.get_usage_stats(&auth_user.user_id).await?;

    Ok(Json(json!({
        "user_id": auth_user.user_id,
        "storage_bytes": quota_info.current_storage_bytes,
        "media_count": quota_info.current_files_count,
        "limit_bytes": quota_info.max_storage_bytes,
        "statistics": stats
    })))
}

pub(crate) async fn quota_alerts(
    State(ctx): State<MediaContext>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let alerts = ctx.media_domain_service.get_user_alerts(&auth_user.user_id, false).await?;

    let alerts_list: Vec<Value> = alerts
        .into_iter()
        .map(|alert| {
            json!({
                "alert_id": alert.id,
                "alert_type": alert.alert_type,
                "threshold_percent": alert.threshold_percent,
                "current_usage_bytes": alert.current_usage_bytes,
                "quota_limit_bytes": alert.quota_limit_bytes,
                "message": alert.message,
                "created_ts": alert.created_ts,
                "is_read": alert.is_read
            })
        })
        .collect();

    Ok(Json(json!({
        "alerts": alerts_list
    })))
}

pub(crate) async fn delete_media(
    State(ctx): State<MediaContext>,
    auth_user: AuthenticatedUser,
    Path((server_name, media_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    ensure_local_media_server_name(&ctx, &server_name)?;

    ctx.media_domain_service.delete_media_for_user(&server_name, &media_id, &auth_user.user_id).await?;

    Ok(Json(json!({
        "deleted": true,
        "media_id": media_id
    })))
}
