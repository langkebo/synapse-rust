use crate::common::error::ApiError;
use crate::services::widget_service::{
    CreateSessionRequest, CreateWidgetRequest, SessionListResponse, SessionResponse,
    SetPermissionRequest, UpdateWidgetRequest, WidgetListResponse, WidgetResponse,
};
use crate::web::routes::{AppState, AuthenticatedUser};
use axum::{
    extract::{Path, State},
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Deserialize)]
pub struct CreateWidgetBody {
    pub room_id: Option<String>,
    pub widget_type: String,
    pub url: String,
    pub name: String,
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateWidgetBody {
    pub url: Option<String>,
    pub name: Option<String>,
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct SetPermissionBody {
    pub user_id: String,
    pub permissions: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateSessionBody {
    pub widget_id: String,
    pub device_id: Option<String>,
    pub expires_in_ms: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct WidgetApiResponse {
    pub widget_id: String,
    pub room_id: Option<String>,
    pub user_id: String,
    #[serde(rename = "type")]
    pub widget_type: String,
    pub url: String,
    pub name: String,
    pub data: serde_json::Value,
    pub creator: String,
    pub active: bool,
}

impl From<crate::storage::widget::Widget> for WidgetApiResponse {
    fn from(widget: crate::storage::widget::Widget) -> Self {
        Self {
            widget_id: widget.widget_id,
            room_id: widget.room_id,
            user_id: widget.user_id.clone(),
            widget_type: widget.widget_type,
            url: widget.url,
            name: widget.name,
            data: widget.data,
            creator: widget.user_id,
            active: widget.is_active,
        }
    }
}

pub fn create_widget_router() -> Router<AppState> {
    Router::new()
        .route("/_matrix/client/v1/widgets", post(create_widget))
        .route("/_matrix/client/v1/widgets/{widget_id}", get(get_widget))
        .route("/_matrix/client/v1/widgets/{widget_id}", put(update_widget))
        .route(
            "/_matrix/client/v1/widgets/{widget_id}",
            delete(delete_widget),
        )
        .route(
            "/_matrix/client/v1/widgets/{widget_id}/config",
            get(get_widget_config),
        )
        .route(
            "/_matrix/client/v1/rooms/{room_id}/widgets",
            get(get_room_widgets),
        )
        .route(
            "/_matrix/client/v1/rooms/{room_id}/widgets/jitsi/config",
            get(get_jitsi_config),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/widgets/{widget_id}/capabilities",
            get(get_room_widget_capabilities).put(set_room_widget_capabilities),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/widgets/{widget_id}/send",
            post(send_room_widget_message),
        )
        .route(
            "/_matrix/client/v1/widgets/{widget_id}/permissions",
            post(set_widget_permission),
        )
        .route(
            "/_matrix/client/v1/widgets/{widget_id}/permissions",
            get(get_widget_permissions),
        )
        .route(
            "/_matrix/client/v1/widgets/{widget_id}/permissions/{user_id}",
            delete(delete_widget_permission),
        )
        .route(
            "/_matrix/client/v1/widgets/{widget_id}/sessions",
            post(create_widget_session),
        )
        .route(
            "/_matrix/client/v1/widgets/{widget_id}/sessions",
            get(get_widget_sessions),
        )
        .route(
            "/_matrix/client/v1/widgets/sessions/{session_id}",
            get(get_widget_session),
        )
        .route(
            "/_matrix/client/v1/widgets/sessions/{session_id}",
            delete(terminate_widget_session),
        )
}

async fn create_widget(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<CreateWidgetBody>,
) -> Result<Json<WidgetResponse>, ApiError> {
    if let Some(room_id) = body.room_id.as_deref() {
        let room_exists = state
            .services
            .room_storage
            .room_exists(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to validate room: {}", e)))?;
        if !room_exists {
            return Err(ApiError::not_found("Room not found"));
        }
    }

    let request = CreateWidgetRequest {
        room_id: body.room_id,
        widget_type: body.widget_type,
        url: body.url,
        name: body.name,
        data: body.data,
    };

    let widget = state
        .services
        .widget_service
        .create_widget(&auth_user.user_id, request)
        .await?;

    Ok(Json(WidgetResponse { widget }))
}

async fn get_widget(
    State(state): State<AppState>,
    Path(widget_id): Path<String>,
) -> Result<Json<WidgetResponse>, ApiError> {
    let widget = state
        .services
        .widget_service
        .get_widget(&widget_id)
        .await?
        .ok_or(ApiError::not_found("Widget not found"))?;

    Ok(Json(WidgetResponse { widget }))
}

async fn update_widget(
    State(state): State<AppState>,
    Path(widget_id): Path<String>,
    Json(body): Json<UpdateWidgetBody>,
) -> Result<Json<WidgetResponse>, ApiError> {
    let request = UpdateWidgetRequest {
        url: body.url,
        name: body.name,
        data: body.data,
    };

    let widget = state
        .services
        .widget_service
        .update_widget(&widget_id, request)
        .await?
        .ok_or(ApiError::not_found("Widget not found"))?;

    Ok(Json(WidgetResponse { widget }))
}

async fn delete_widget(
    State(state): State<AppState>,
    Path(widget_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let deleted = state
        .services
        .widget_service
        .delete_widget(&widget_id)
        .await?;

    if !deleted {
        return Err(ApiError::not_found("Widget not found"));
    }

    Ok(Json(json!({"deleted": true})))
}

async fn get_room_widgets(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<WidgetListResponse>, ApiError> {
    let widgets = state
        .services
        .widget_service
        .get_room_widgets(&room_id)
        .await?;

    Ok(Json(WidgetListResponse {
        total: widgets.len(),
        widgets,
    }))
}

async fn get_widget_config(
    State(state): State<AppState>,
    Path(widget_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let widget = state
        .services
        .widget_service
        .get_widget(&widget_id)
        .await?
        .ok_or(ApiError::not_found("Widget not found"))?;

    Ok(Json(json!({
        "widget_id": widget.widget_id,
        "room_id": widget.room_id,
        "url": widget.url,
        "name": widget.name,
        "data": widget.data,
        "type": widget.widget_type
    })))
}

async fn get_jitsi_config(
    State(_state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(json!({
        "conf_id": format!("{}_jitsi_conference", room_id.replace("!", "").replace(":", "_")),
        "name": "Jitsi Conference",
        "domain": "meet.jit.si",
        "app_id": null,
        "jwt": null
    })))
}

async fn set_widget_permission(
    State(state): State<AppState>,
    Path(widget_id): Path<String>,
    Json(body): Json<SetPermissionBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let request = SetPermissionRequest {
        user_id: body.user_id,
        permissions: body.permissions,
    };

    let permission = state
        .services
        .widget_service
        .set_permission(&widget_id, request)
        .await?;

    Ok(Json(
        json!({"success": true, "permission_id": permission.id}),
    ))
}

async fn get_widget_permissions(
    State(state): State<AppState>,
    Path(widget_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let permissions = state
        .services
        .widget_service
        .get_permissions(&widget_id)
        .await?;

    Ok(Json(json!({"permissions": permissions})))
}

async fn delete_widget_permission(
    State(state): State<AppState>,
    Path((widget_id, user_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let deleted = state
        .services
        .widget_service
        .delete_permission(&widget_id, &user_id)
        .await?;

    Ok(Json(json!({"deleted": deleted})))
}

async fn create_widget_session(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<CreateSessionBody>,
) -> Result<Json<SessionResponse>, ApiError> {
    let request = CreateSessionRequest {
        widget_id: body.widget_id,
        device_id: body.device_id,
        expires_in_ms: body.expires_in_ms,
    };

    let session = state
        .services
        .widget_service
        .create_session(&auth_user.user_id, request)
        .await?;

    Ok(Json(SessionResponse { session }))
}

async fn get_widget_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<SessionResponse>, ApiError> {
    let session = state
        .services
        .widget_service
        .get_session(&session_id)
        .await?
        .ok_or(ApiError::not_found("Session not found"))?;

    Ok(Json(SessionResponse { session }))
}

async fn get_widget_sessions(
    State(state): State<AppState>,
    Path(widget_id): Path<String>,
) -> Result<Json<SessionListResponse>, ApiError> {
    let sessions = state
        .services
        .widget_service
        .get_widget_sessions(&widget_id)
        .await?;

    Ok(Json(SessionListResponse {
        total: sessions.len(),
        sessions,
    }))
}

async fn terminate_widget_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let terminated = state
        .services
        .widget_service
        .terminate_session(&session_id)
        .await?;

    Ok(Json(json!({"terminated": terminated})))
}

#[derive(Debug, Deserialize)]
pub struct WidgetCapabilitiesBody {
    pub capabilities: Vec<String>,
}

async fn get_room_widget_capabilities(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, widget_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let is_member = state
        .services
        .member_storage
        .is_member(&room_id, &auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check membership: {}", e)))?;

    if !is_member && !auth_user.is_admin {
        return Err(ApiError::forbidden(
            "You must be a member of this room to view widget capabilities".to_string(),
        ));
    }

    let widget = state
        .services
        .widget_service
        .get_widget(&widget_id)
        .await?
        .ok_or(ApiError::not_found("Widget not found"))?;

    if widget.room_id.as_deref() != Some(&room_id) {
        return Err(ApiError::bad_request(
            "Widget does not belong to this room".to_string(),
        ));
    }

    Ok(Json(json!({
        "capabilities": widget.data.get("capabilities").unwrap_or(&json!([])),
        "widget_id": widget_id,
        "room_id": room_id
    })))
}

async fn set_room_widget_capabilities(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, widget_id)): Path<(String, String)>,
    Json(body): Json<WidgetCapabilitiesBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let is_member = state
        .services
        .member_storage
        .is_member(&room_id, &auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check membership: {}", e)))?;

    if !is_member && !auth_user.is_admin {
        return Err(ApiError::forbidden(
            "You must be a member of this room to set widget capabilities".to_string(),
        ));
    }

    let widget = state
        .services
        .widget_service
        .get_widget(&widget_id)
        .await?
        .ok_or(ApiError::not_found("Widget not found"))?;

    if widget.room_id.as_deref() != Some(&room_id) {
        return Err(ApiError::bad_request(
            "Widget does not belong to this room".to_string(),
        ));
    }

    let mut data = widget.data.clone();
    data["capabilities"] = json!(body.capabilities);

    let update_request = UpdateWidgetRequest {
        url: None,
        name: None,
        data: Some(data),
    };

    state
        .services
        .widget_service
        .update_widget(&widget_id, update_request)
        .await?;

    Ok(Json(json!({
        "capabilities": body.capabilities,
        "widget_id": widget_id,
        "room_id": room_id
    })))
}

#[derive(Debug, Deserialize)]
pub struct SendWidgetMessageBody {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub content: serde_json::Value,
}

async fn send_room_widget_message(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, widget_id)): Path<(String, String)>,
    Json(body): Json<SendWidgetMessageBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let is_member = state
        .services
        .member_storage
        .is_member(&room_id, &auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check membership: {}", e)))?;

    if !is_member && !auth_user.is_admin {
        return Err(ApiError::forbidden(
            "You must be a member of this room to send widget messages".to_string(),
        ));
    }

    let widget = state
        .services
        .widget_service
        .get_widget(&widget_id)
        .await?
        .ok_or(ApiError::not_found("Widget not found"))?;

    if widget.room_id.as_deref() != Some(&room_id) {
        return Err(ApiError::bad_request(
            "Widget does not belong to this room".to_string(),
        ));
    }

    let event_id = format!(
        "${}_{}",
        uuid::Uuid::new_v4(),
        chrono::Utc::now().timestamp_millis()
    );

    Ok(Json(json!({
        "event_id": event_id,
        "widget_id": widget_id,
        "room_id": room_id,
        "type": body.msg_type,
        "content": body.content
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_widget_body() {
        let body = CreateWidgetBody {
            room_id: Some("!room:example.com".to_string()),
            widget_type: "customwidget".to_string(),
            url: "https://example.com/widget".to_string(),
            name: "My Widget".to_string(),
            data: Some(serde_json::json!({"key": "value"})),
        };

        assert_eq!(body.widget_type, "customwidget");
        assert!(body.room_id.is_some());
    }

    #[test]
    fn test_update_widget_body() {
        let body = UpdateWidgetBody {
            url: Some("https://example.com/new-widget".to_string()),
            name: Some("Updated Widget".to_string()),
            data: None,
        };

        assert!(body.url.is_some());
        assert!(body.name.is_some());
    }

    #[test]
    fn test_set_permission_body() {
        let body = SetPermissionBody {
            user_id: "@user:example.com".to_string(),
            permissions: vec!["read".to_string(), "write".to_string()],
        };

        assert_eq!(body.permissions.len(), 2);
    }

    #[test]
    fn test_create_session_body() {
        let body = CreateSessionBody {
            widget_id: "widget_123".to_string(),
            device_id: Some("DEVICE123".to_string()),
            expires_in_ms: Some(3600000),
        };

        assert_eq!(body.widget_id, "widget_123");
    }
}
