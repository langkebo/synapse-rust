use crate::common::ApiError;
use crate::storage::server_notification::{CreateNotificationRequest, CreateTemplateRequest};
use crate::web::routes::{AdminUser, AppState, AuthenticatedUser};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct ListQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct MarkReadPath {
    notification_id: i32,
}

#[derive(Debug, Deserialize)]
struct CreateNotificationBody {
    title: String,
    content: String,
    notification_type: Option<String>,
    priority: Option<i32>,
    target_audience: Option<String>,
    target_user_ids: Option<Vec<String>>,
    starts_at: Option<i64>,
    expires_at: Option<i64>,
    is_dismissible: Option<bool>,
    action_url: Option<String>,
    action_text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CreateTemplateBody {
    name: String,
    title_template: String,
    content_template: String,
    notification_type: Option<String>,
    variables: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct CreateFromTemplateBody {
    template_name: String,
    variables: std::collections::HashMap<String, String>,
    target_audience: Option<String>,
    target_user_ids: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct ScheduleBody {
    scheduled_for: i64,
}

#[derive(Debug, Deserialize)]
struct BroadcastBody {
    delivery_method: String,
}

pub fn create_server_notification_router() -> Router<AppState> {
    Router::new()
        .route(
            "/_matrix/client/v1/notifications",
            get(get_user_notifications),
        )
        .route(
            "/_matrix/client/v1/notifications/{notification_id}/read",
            put(mark_as_read),
        )
        .route(
            "/_matrix/client/v1/notifications/{notification_id}/dismiss",
            put(dismiss_notification),
        )
        .route(
            "/_matrix/client/v1/notifications/read-all",
            put(mark_all_read),
        )
        .route(
            "/_matrix/admin/v1/notifications",
            get(list_all_notifications),
        )
        .route("/_matrix/admin/v1/notifications", post(create_notification))
        .route(
            "/_matrix/admin/v1/notifications/{notification_id}",
            get(get_notification),
        )
        .route(
            "/_matrix/admin/v1/notifications/{notification_id}",
            put(update_notification),
        )
        .route(
            "/_matrix/admin/v1/notifications/{notification_id}",
            delete(delete_notification),
        )
        .route(
            "/_matrix/admin/v1/notifications/{notification_id}/deactivate",
            post(deactivate_notification),
        )
        .route(
            "/_matrix/admin/v1/notifications/{notification_id}/schedule",
            post(schedule_notification),
        )
        .route(
            "/_matrix/admin/v1/notifications/{notification_id}/broadcast",
            post(broadcast_notification),
        )
        .route(
            "/_matrix/admin/v1/notification-templates",
            get(list_templates),
        )
        .route(
            "/_matrix/admin/v1/notification-templates",
            post(create_template),
        )
        .route(
            "/_matrix/admin/v1/notification-templates/{name}",
            get(get_template),
        )
        .route(
            "/_matrix/admin/v1/notification-templates/{name}",
            delete(delete_template),
        )
        .route(
            "/_matrix/admin/v1/notification-templates/create-notification",
            post(create_from_template),
        )
}

async fn get_user_notifications(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let notifications = state
        .services
        .server_notification_service
        .get_user_notifications(&auth_user.user_id)
        .await?;
    Ok(Json(notifications))
}

async fn mark_as_read(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(notification_id): Path<i32>,
) -> Result<impl IntoResponse, ApiError> {
    state
        .services
        .server_notification_service
        .mark_as_read(&auth_user.user_id, notification_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn dismiss_notification(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(notification_id): Path<i32>,
) -> Result<impl IntoResponse, ApiError> {
    state
        .services
        .server_notification_service
        .mark_as_dismissed(&auth_user.user_id, notification_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn mark_all_read(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    let count = state
        .services
        .server_notification_service
        .mark_all_as_read(&auth_user.user_id)
        .await?;
    Ok(Json(serde_json::json!({ "marked_count": count })))
}

async fn list_all_notifications(
    State(state): State<AppState>,
    _admin: AdminUser,
    Query(query): Query<ListQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let notifications = state
        .services
        .server_notification_service
        .list_all_notifications(query.limit.unwrap_or(100), query.offset.unwrap_or(0))
        .await?;
    Ok(Json(notifications))
}

async fn create_notification(
    State(state): State<AppState>,
    admin: AdminUser,
    Json(body): Json<CreateNotificationBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = CreateNotificationRequest {
        title: body.title,
        content: body.content,
        notification_type: body.notification_type,
        priority: body.priority,
        target_audience: body.target_audience,
        target_user_ids: body.target_user_ids,
        starts_at: body.starts_at,
        expires_at: body.expires_at,
        is_dismissable: body.is_dismissible,
        action_url: body.action_url,
        action_text: body.action_text,
        created_by: Some(admin.user_id),
    };

    let notification = state
        .services
        .server_notification_service
        .create_notification(request)
        .await?;
    Ok((StatusCode::CREATED, Json(notification)))
}

async fn get_notification(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(notification_id): Path<i32>,
) -> Result<impl IntoResponse, ApiError> {
    let notification = state
        .services
        .server_notification_service
        .get_notification(notification_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Notification not found"))?;
    Ok(Json(notification))
}

async fn update_notification(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(notification_id): Path<i32>,
    Json(body): Json<CreateNotificationBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = CreateNotificationRequest {
        title: body.title,
        content: body.content,
        notification_type: body.notification_type,
        priority: body.priority,
        target_audience: body.target_audience,
        target_user_ids: body.target_user_ids,
        starts_at: body.starts_at,
        expires_at: body.expires_at,
        is_dismissable: body.is_dismissible,
        action_url: body.action_url,
        action_text: body.action_text,
        created_by: None,
    };

    let notification = state
        .services
        .server_notification_service
        .update_notification(notification_id, request)
        .await?;
    Ok(Json(notification))
}

async fn delete_notification(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(notification_id): Path<i32>,
) -> Result<impl IntoResponse, ApiError> {
    let deleted = state
        .services
        .server_notification_service
        .delete_notification(notification_id)
        .await?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found("Notification not found"))
    }
}

async fn deactivate_notification(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(notification_id): Path<i32>,
) -> Result<impl IntoResponse, ApiError> {
    state
        .services
        .server_notification_service
        .deactivate_notification(notification_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn schedule_notification(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(notification_id): Path<i32>,
    Json(body): Json<ScheduleBody>,
) -> Result<impl IntoResponse, ApiError> {
    let scheduled = state
        .services
        .server_notification_service
        .schedule_notification(notification_id, body.scheduled_for)
        .await?;
    Ok(Json(scheduled))
}

async fn broadcast_notification(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(notification_id): Path<i32>,
    Json(body): Json<BroadcastBody>,
) -> Result<impl IntoResponse, ApiError> {
    state
        .services
        .server_notification_service
        .broadcast_notification(notification_id, &body.delivery_method)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_templates(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> Result<impl IntoResponse, ApiError> {
    let templates = state
        .services
        .server_notification_service
        .list_templates()
        .await?;
    Ok(Json(templates))
}

async fn create_template(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(body): Json<CreateTemplateBody>,
) -> Result<impl IntoResponse, ApiError> {
    let request = CreateTemplateRequest {
        name: body.name,
        title_template: body.title_template,
        content_template: body.content_template,
        notification_type: body.notification_type,
        variables: body.variables,
    };

    let template = state
        .services
        .server_notification_service
        .create_template(request)
        .await?;
    Ok((StatusCode::CREATED, Json(template)))
}

async fn get_template(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let template = state
        .services
        .server_notification_service
        .get_template(&name)
        .await?
        .ok_or_else(|| ApiError::not_found("Template not found"))?;
    Ok(Json(template))
}

async fn delete_template(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let deleted = state
        .services
        .server_notification_service
        .delete_template(&name)
        .await?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found("Template not found"))
    }
}

async fn create_from_template(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(body): Json<CreateFromTemplateBody>,
) -> Result<impl IntoResponse, ApiError> {
    let notification = state
        .services
        .server_notification_service
        .create_from_template(
            &body.template_name,
            body.variables,
            body.target_audience,
            body.target_user_ids,
        )
        .await?;
    Ok((StatusCode::CREATED, Json(notification)))
}
