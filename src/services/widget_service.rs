use crate::common::error::ApiError;
use crate::storage::widget::{
    CreateWidgetParams, Widget, WidgetPermission, WidgetSession, WidgetStorage,
};
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateWidgetRequest {
    pub room_id: Option<String>,
    pub widget_type: String,
    pub url: String,
    pub name: String,
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UpdateWidgetRequest {
    pub url: Option<String>,
    pub name: Option<String>,
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SetPermissionRequest {
    pub user_id: String,
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateSessionRequest {
    pub widget_id: String,
    pub device_id: Option<String>,
    pub expires_in_ms: Option<i64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WidgetResponse {
    pub widget: Widget,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WidgetListResponse {
    pub widgets: Vec<Widget>,
    pub total: usize,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PermissionResponse {
    pub permission: WidgetPermission,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionResponse {
    pub session: WidgetSession,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionListResponse {
    pub sessions: Vec<WidgetSession>,
    pub total: usize,
}

pub struct WidgetService {
    storage: Arc<WidgetStorage>,
}

impl WidgetService {
    pub fn new(storage: Arc<WidgetStorage>) -> Self {
        Self { storage }
    }

    pub async fn create_widget(
        &self,
        user_id: &str,
        request: CreateWidgetRequest,
    ) -> Result<Widget, ApiError> {
        let widget_id = format!("widget_{}", Uuid::new_v4());

        let params = CreateWidgetParams {
            widget_id: widget_id.clone(),
            room_id: request.room_id,
            user_id: user_id.to_string(),
            widget_type: request.widget_type,
            url: request.url,
            name: request.name,
            data: request.data.unwrap_or(serde_json::json!({})),
        };

        let widget = self
            .storage
            .create_widget(params)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to create widget: {}", e)))?;

        info!("Created widget {} for user {}", widget_id, user_id);
        Ok(widget)
    }

    pub async fn get_widget(&self, widget_id: &str) -> Result<Option<Widget>, ApiError> {
        let widget = self
            .storage
            .get_widget(widget_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get widget: {}", e)))?;

        Ok(widget)
    }

    pub async fn get_room_widgets(&self, room_id: &str) -> Result<Vec<Widget>, ApiError> {
        let widgets = self
            .storage
            .get_room_widgets(room_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get room widgets: {}", e)))?;

        Ok(widgets)
    }

    pub async fn get_user_widgets(&self, user_id: &str) -> Result<Vec<Widget>, ApiError> {
        let widgets = self
            .storage
            .get_user_widgets(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get user widgets: {}", e)))?;

        Ok(widgets)
    }

    pub async fn update_widget(
        &self,
        widget_id: &str,
        request: UpdateWidgetRequest,
    ) -> Result<Option<Widget>, ApiError> {
        let widget = self
            .storage
            .update_widget(
                widget_id,
                request.url.as_deref(),
                request.name.as_deref(),
                request.data.as_ref(),
            )
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update widget: {}", e)))?;

        if widget.is_some() {
            info!("Updated widget {}", widget_id);
        }

        Ok(widget)
    }

    pub async fn delete_widget(&self, widget_id: &str) -> Result<bool, ApiError> {
        let deleted = self
            .storage
            .delete_widget(widget_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to delete widget: {}", e)))?;

        if deleted {
            info!("Deleted widget {}", widget_id);
        }

        Ok(deleted)
    }

    pub async fn set_permission(
        &self,
        widget_id: &str,
        request: SetPermissionRequest,
    ) -> Result<WidgetPermission, ApiError> {
        let permissions =
            serde_json::to_value(&request.permissions).unwrap_or(serde_json::json!([]));

        let permission = self
            .storage
            .set_widget_permission(widget_id, &request.user_id, permissions)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to set widget permission: {}", e)))?;

        info!(
            "Set permissions for widget {} user {}",
            widget_id, request.user_id
        );
        Ok(permission)
    }

    pub async fn get_permissions(
        &self,
        widget_id: &str,
    ) -> Result<Vec<WidgetPermission>, ApiError> {
        let permissions = self
            .storage
            .get_widget_permissions(widget_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get widget permissions: {}", e)))?;

        Ok(permissions)
    }

    pub async fn get_user_permission(
        &self,
        widget_id: &str,
        user_id: &str,
    ) -> Result<Option<WidgetPermission>, ApiError> {
        let permission = self
            .storage
            .get_user_widget_permission(widget_id, user_id)
            .await
            .map_err(|e| {
                ApiError::internal(format!("Failed to get user widget permission: {}", e))
            })?;

        Ok(permission)
    }

    pub async fn delete_permission(
        &self,
        widget_id: &str,
        user_id: &str,
    ) -> Result<bool, ApiError> {
        let deleted = self
            .storage
            .delete_widget_permission(widget_id, user_id)
            .await
            .map_err(|e| {
                ApiError::internal(format!("Failed to delete widget permission: {}", e))
            })?;

        if deleted {
            info!(
                "Deleted permission for widget {} user {}",
                widget_id, user_id
            );
        }

        Ok(deleted)
    }

    pub async fn create_session(
        &self,
        user_id: &str,
        request: CreateSessionRequest,
    ) -> Result<WidgetSession, ApiError> {
        let session_id = format!("session_{}", Uuid::new_v4());

        let session = self
            .storage
            .create_session(
                &session_id,
                &request.widget_id,
                user_id,
                request.device_id.as_deref(),
                request.expires_in_ms,
            )
            .await
            .map_err(|e| ApiError::internal(format!("Failed to create widget session: {}", e)))?;

        info!(
            "Created session {} for widget {} user {}",
            session_id, request.widget_id, user_id
        );
        Ok(session)
    }

    pub async fn get_session(&self, session_id: &str) -> Result<Option<WidgetSession>, ApiError> {
        let session = self
            .storage
            .get_session(session_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get widget session: {}", e)))?;

        Ok(session)
    }

    pub async fn update_session_activity(&self, session_id: &str) -> Result<bool, ApiError> {
        let updated = self
            .storage
            .update_session_activity(session_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update session activity: {}", e)))?;

        Ok(updated)
    }

    pub async fn terminate_session(&self, session_id: &str) -> Result<bool, ApiError> {
        let terminated = self
            .storage
            .terminate_session(session_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to terminate session: {}", e)))?;

        if terminated {
            info!("Terminated session {}", session_id);
        }

        Ok(terminated)
    }

    pub async fn get_widget_sessions(
        &self,
        widget_id: &str,
    ) -> Result<Vec<WidgetSession>, ApiError> {
        let sessions = self
            .storage
            .get_widget_sessions(widget_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get widget sessions: {}", e)))?;

        Ok(sessions)
    }

    pub async fn cleanup_expired_sessions(&self) -> Result<u64, ApiError> {
        let count = self.storage.cleanup_expired_sessions().await.map_err(|e| {
            ApiError::internal(format!("Failed to cleanup expired sessions: {}", e))
        })?;

        if count > 0 {
            info!("Cleaned up {} expired widget sessions", count);
        }

        Ok(count)
    }

    pub async fn check_permission(
        &self,
        widget_id: &str,
        user_id: &str,
        required_permission: &str,
    ) -> Result<bool, ApiError> {
        let permission = self.get_user_permission(widget_id, user_id).await?;

        if let Some(perm) = permission {
            if let Some(perms) = perm.permissions.as_array() {
                let has_permission = perms
                    .iter()
                    .any(|p| p.as_str() == Some(required_permission) || p.as_str() == Some("*"));
                return Ok(has_permission);
            }
        }

        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_widget_request() {
        let request = CreateWidgetRequest {
            room_id: Some("!room:example.com".to_string()),
            widget_type: "customwidget".to_string(),
            url: "https://example.com/widget".to_string(),
            name: "My Widget".to_string(),
            data: Some(serde_json::json!({"key": "value"})),
        };

        assert_eq!(request.widget_type, "customwidget");
        assert!(request.room_id.is_some());
    }

    #[test]
    fn test_update_widget_request() {
        let request = UpdateWidgetRequest {
            url: Some("https://example.com/new-widget".to_string()),
            name: None,
            data: None,
        };

        assert!(request.url.is_some());
        assert!(request.name.is_none());
    }

    #[test]
    fn test_set_permission_request() {
        let request = SetPermissionRequest {
            user_id: "@user:example.com".to_string(),
            permissions: vec!["read".to_string(), "write".to_string()],
        };

        assert_eq!(request.permissions.len(), 2);
    }

    #[test]
    fn test_create_session_request() {
        let request = CreateSessionRequest {
            widget_id: "widget_123".to_string(),
            device_id: Some("DEVICE123".to_string()),
            expires_in_ms: Some(3600000),
        };

        assert_eq!(request.widget_id, "widget_123");
        assert!(request.expires_in_ms.is_some());
    }

    #[test]
    fn test_widget_response() {
        let widget = Widget {
            id: 1,
            widget_id: "widget_123".to_string(),
            room_id: Some("!room:example.com".to_string()),
            user_id: "@user:example.com".to_string(),
            widget_type: "customwidget".to_string(),
            url: "https://example.com/widget".to_string(),
            name: "My Widget".to_string(),
            data: serde_json::json!({}),
            created_ts: 1234567890000,
            updated_ts: None,
            is_active: true,
        };

        let response = WidgetResponse { widget };
        assert_eq!(response.widget.widget_id, "widget_123");
    }
}
