use std::sync::Arc;
use synapse_common::error::ApiError;
use synapse_storage::widget::{CreateWidgetParams, Widget, WidgetPermission, WidgetSession, WidgetStoreApi};
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
    storage: Arc<dyn WidgetStoreApi>,
}

impl WidgetService {
    pub fn new(storage: Arc<dyn WidgetStoreApi>) -> Self {
        Self { storage }
    }

    pub async fn create_widget(&self, user_id: &str, request: CreateWidgetRequest) -> Result<Widget, ApiError> {
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
            .map_err(|e| ApiError::internal_with_context("Failed to create widget", &e))?;

        info!(
            widget_id = %widget.widget_id,
            user_id = %widget.user_id,
            room_id = ?widget.room_id,
            widget_type = %widget.widget_type,
            "Created widget"
        );
        Ok(widget)
    }

    pub async fn get_widget(&self, widget_id: &str) -> Result<Option<Widget>, ApiError> {
        let widget = self
            .storage
            .get_widget(widget_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to get widget", &e))?;

        Ok(widget)
    }

    pub async fn get_room_widgets(&self, room_id: &str) -> Result<Vec<Widget>, ApiError> {
        let widgets = self
            .storage
            .get_room_widgets(room_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to get room widgets", &e))?;

        Ok(widgets)
    }

    pub async fn get_user_widgets(&self, user_id: &str) -> Result<Vec<Widget>, ApiError> {
        let widgets = self
            .storage
            .get_user_widgets(user_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to get user widgets", &e))?;

        Ok(widgets)
    }

    pub async fn update_widget(
        &self,
        widget_id: &str,
        request: UpdateWidgetRequest,
    ) -> Result<Option<Widget>, ApiError> {
        let widget = self
            .storage
            .update_widget(widget_id, request.url.as_deref(), request.name.as_deref(), request.data.as_ref())
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to update widget", &e))?;

        if widget.is_some() {
            info!(widget_id = %widget_id, "Updated widget");
        }

        Ok(widget)
    }

    pub async fn delete_widget(&self, widget_id: &str) -> Result<bool, ApiError> {
        let deleted = self
            .storage
            .delete_widget(widget_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to delete widget", &e))?;

        if deleted {
            info!(widget_id = %widget_id, "Deleted widget");
        }

        Ok(deleted)
    }

    pub async fn set_permission(
        &self,
        widget_id: &str,
        request: SetPermissionRequest,
    ) -> Result<WidgetPermission, ApiError> {
        let permissions = serde_json::to_value(&request.permissions).unwrap_or(serde_json::json!([]));

        let permission = self
            .storage
            .set_widget_permission(widget_id, &request.user_id, permissions)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to set widget permission", &e))?;

        info!(
            widget_id = %widget_id,
            user_id = %request.user_id,
            permission_count = request.permissions.len(),
            "Set widget permissions"
        );
        Ok(permission)
    }

    pub async fn get_permissions(&self, widget_id: &str) -> Result<Vec<WidgetPermission>, ApiError> {
        let permissions = self
            .storage
            .get_widget_permissions(widget_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to get widget permissions", &e))?;

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
            .map_err(|e| ApiError::internal_with_context("Failed to get user widget permission", &e))?;

        Ok(permission)
    }

    pub async fn delete_permission(&self, widget_id: &str, user_id: &str) -> Result<bool, ApiError> {
        let deleted = self
            .storage
            .delete_widget_permission(widget_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to delete widget permission", &e))?;

        if deleted {
            info!(widget_id = %widget_id, user_id = %user_id, "Deleted widget permission");
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
            .map_err(|e| ApiError::internal_with_context("Failed to create widget session", &e))?;

        info!(
            session_id = %session.session_id,
            widget_id = %session.widget_id,
            user_id = %session.user_id,
            device_id = ?session.device_id,
            expires_at = ?session.expires_at,
            "Created widget session"
        );
        Ok(session)
    }

    pub async fn get_session(&self, session_id: &str) -> Result<Option<WidgetSession>, ApiError> {
        let session = self
            .storage
            .get_session(session_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to get widget session", &e))?;

        Ok(session)
    }

    pub async fn update_session_activity(&self, session_id: &str) -> Result<bool, ApiError> {
        let updated = self
            .storage
            .update_session_activity(session_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to update session activity", &e))?;

        Ok(updated)
    }

    pub async fn terminate_session(&self, session_id: &str) -> Result<bool, ApiError> {
        let terminated = self
            .storage
            .terminate_session(session_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to terminate session", &e))?;

        if terminated {
            info!(session_id = %session_id, "Terminated widget session");
        }

        Ok(terminated)
    }

    pub async fn get_widget_sessions(&self, widget_id: &str) -> Result<Vec<WidgetSession>, ApiError> {
        let sessions = self
            .storage
            .get_widget_sessions(widget_id)
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to get widget sessions", &e))?;

        Ok(sessions)
    }

    pub async fn cleanup_expired_sessions(&self) -> Result<u64, ApiError> {
        let count = self
            .storage
            .cleanup_expired_sessions()
            .await
            .map_err(|e| ApiError::internal_with_context("Failed to cleanup expired sessions", &e))?;

        if count > 0 {
            info!(expired_session_count = count, "Cleaned up expired widget sessions");
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
                let has_permission =
                    perms.iter().any(|p| p.as_str() == Some(required_permission) || p.as_str() == Some("*"));
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
        let request =
            UpdateWidgetRequest { url: Some("https://example.com/new-widget".to_string()), name: None, data: None };

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
            expires_in_ms: Some(3_600_000),
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
    // --- Mock WidgetStore + Service-layer tests -----------------------------

    use async_trait::async_trait;

    #[derive(Clone)]
    struct MockWidgetStore {
        create_widget: Arc<std::sync::Mutex<Option<Result<Widget, sqlx::Error>>>>,
        get_widget: Arc<std::sync::Mutex<Option<Result<Option<Widget>, sqlx::Error>>>>,
        get_room_widgets: Arc<std::sync::Mutex<Option<Result<Vec<Widget>, sqlx::Error>>>>,
        get_user_widgets: Arc<std::sync::Mutex<Option<Result<Vec<Widget>, sqlx::Error>>>>,
        update_widget: Arc<std::sync::Mutex<Option<Result<Option<Widget>, sqlx::Error>>>>,
        delete_widget: Arc<std::sync::Mutex<Option<Result<bool, sqlx::Error>>>>,
        set_permission: Arc<std::sync::Mutex<Option<Result<WidgetPermission, sqlx::Error>>>>,
        get_permissions: Arc<std::sync::Mutex<Option<Result<Vec<WidgetPermission>, sqlx::Error>>>>,
        get_user_permission: Arc<std::sync::Mutex<Option<Result<Option<WidgetPermission>, sqlx::Error>>>>,
        delete_permission: Arc<std::sync::Mutex<Option<Result<bool, sqlx::Error>>>>,
        create_session: Arc<std::sync::Mutex<Option<Result<WidgetSession, sqlx::Error>>>>,
        get_session: Arc<std::sync::Mutex<Option<Result<Option<WidgetSession>, sqlx::Error>>>>,
        update_activity: Arc<std::sync::Mutex<Option<Result<bool, sqlx::Error>>>>,
        terminate_session: Arc<std::sync::Mutex<Option<Result<bool, sqlx::Error>>>>,
        get_sessions: Arc<std::sync::Mutex<Option<Result<Vec<WidgetSession>, sqlx::Error>>>>,
        cleanup_expired: Arc<std::sync::Mutex<Option<Result<u64, sqlx::Error>>>>,
    }

    impl MockWidgetStore {
        fn new() -> Self {
            Self {
                create_widget: Arc::new(std::sync::Mutex::new((Err(sqlx::Error::RowNotFound), 0))),
                get_widget: Arc::new(std::sync::Mutex::new((Ok(None), 0))),
                get_room_widgets: Arc::new(std::sync::Mutex::new((Ok(vec![]), 0))),
                get_user_widgets: Arc::new(std::sync::Mutex::new((Ok(vec![]), 0))),
                update_widget: Arc::new(std::sync::Mutex::new((Ok(None), 0))),
                delete_widget: Arc::new(std::sync::Mutex::new((Ok(false), 0))),
                set_permission: Arc::new(std::sync::Mutex::new((Err(sqlx::Error::RowNotFound), 0))),
                get_permissions: Arc::new(std::sync::Mutex::new((Ok(vec![]), 0))),
                get_user_permission: Arc::new(std::sync::Mutex::new((Ok(None), 0))),
                delete_permission: Arc::new(std::sync::Mutex::new((Ok(false), 0))),
                create_session: Arc::new(std::sync::Mutex::new((Err(sqlx::Error::RowNotFound), 0))),
                get_session: Arc::new(std::sync::Mutex::new((Ok(None), 0))),
                update_activity: Arc::new(std::sync::Mutex::new((Ok(false), 0))),
                terminate_session: Arc::new(std::sync::Mutex::new((Ok(false), 0))),
                get_sessions: Arc::new(std::sync::Mutex::new((Ok(vec![]), 0))),
                cleanup_expired: Arc::new(std::sync::Mutex::new((Ok(0), 0))),
            }
        }


        fn set_create_widget(&self, r: Result<Widget, sqlx::Error>) { self.create_widget.lock().unwrap().0 = r; }
        fn set_get_widget(&self, r: Result<Option<Widget>, sqlx::Error>) { self.get_widget.lock().unwrap().0 = r; }
        fn set_get_room_widgets(&self, r: Result<Vec<Widget>, sqlx::Error>) { self.get_room_widgets.lock().unwrap().0 = r; }
        fn set_get_user_widgets(&self, r: Result<Vec<Widget>, sqlx::Error>) { self.get_user_widgets.lock().unwrap().0 = r; }
        fn set_update_widget(&self, r: Result<Option<Widget>, sqlx::Error>) { self.update_widget.lock().unwrap().0 = r; }
        fn set_delete_widget(&self, r: Result<bool, sqlx::Error>) { self.delete_widget.lock().unwrap().0 = r; }
        fn set_set_permission(&self, r: Result<WidgetPermission, sqlx::Error>) { self.set_permission.lock().unwrap().0 = r; }
        fn set_get_permissions(&self, r: Result<Vec<WidgetPermission>, sqlx::Error>) { self.get_permissions.lock().unwrap().0 = r; }
        fn set_get_user_permission(&self, r: Result<Option<WidgetPermission>, sqlx::Error>) { self.get_user_permission.lock().unwrap().0 = r; }
        fn set_delete_permission(&self, r: Result<bool, sqlx::Error>) { self.delete_permission.lock().unwrap().0 = r; }
        fn set_create_session(&self, r: Result<WidgetSession, sqlx::Error>) { self.create_session.lock().unwrap().0 = r; }
        fn set_get_session(&self, r: Result<Option<WidgetSession>, sqlx::Error>) { self.get_session.lock().unwrap().0 = r; }
        fn set_update_activity(&self, r: Result<bool, sqlx::Error>) { self.update_activity.lock().unwrap().0 = r; }
        fn set_terminate_session(&self, r: Result<bool, sqlx::Error>) { self.terminate_session.lock().unwrap().0 = r; }
        fn set_get_sessions(&self, r: Result<Vec<WidgetSession>, sqlx::Error>) { self.get_sessions.lock().unwrap().0 = r; }
        fn set_cleanup_expired(&self, r: Result<u64, sqlx::Error>) { self.cleanup_expired.lock().unwrap().0 = r; }

        async fn pop_field<T>(field: &Arc<std::sync::Mutex<Option<Result<T, sqlx::Error>>>>) -> Result<T, sqlx::Error> {
            let r = field.lock().unwrap().take()
                .unwrap_or(Err(sqlx::Error::RowNotFound));
            r
        }
    }


    #[async_trait]
    impl WidgetStoreApi for MockWidgetStore {
        async fn create_widget(&self, _: CreateWidgetParams) -> Result<Widget, sqlx::Error> { Self::pop_field(&self.create_widget).await }
        async fn get_widget(&self, _: &str) -> Result<Option<Widget>, sqlx::Error> { Self::pop_field(&self.get_widget).await }
        async fn get_room_widgets(&self, _: &str) -> Result<Vec<Widget>, sqlx::Error> { Self::pop_field(&self.get_room_widgets).await }
        async fn get_user_widgets(&self, _: &str) -> Result<Vec<Widget>, sqlx::Error> { Self::pop_field(&self.get_user_widgets).await }
        async fn update_widget(&self, _: &str, _: Option<&str>, _: Option<&str>, _: Option<&serde_json::Value>) -> Result<Option<Widget>, sqlx::Error> { Self::pop_field(&self.update_widget).await }
        async fn delete_widget(&self, _: &str) -> Result<bool, sqlx::Error> { Self::pop_field(&self.delete_widget).await }
        async fn set_widget_permission(&self, _: &str, _: &str, _: serde_json::Value) -> Result<WidgetPermission, sqlx::Error> { Self::pop_field(&self.set_permission).await }
        async fn get_widget_permissions(&self, _: &str) -> Result<Vec<WidgetPermission>, sqlx::Error> { Self::pop_field(&self.get_permissions).await }
        async fn get_user_widget_permission(&self, _: &str, _: &str) -> Result<Option<WidgetPermission>, sqlx::Error> { Self::pop_field(&self.get_user_permission).await }
        async fn delete_widget_permission(&self, _: &str, _: &str) -> Result<bool, sqlx::Error> { Self::pop_field(&self.delete_permission).await }
        async fn create_session(&self, _: &str, _: &str, _: &str, _: Option<&str>, _: Option<i64>) -> Result<WidgetSession, sqlx::Error> { Self::pop_field(&self.create_session).await }
        async fn get_session(&self, _: &str) -> Result<Option<WidgetSession>, sqlx::Error> { Self::pop_field(&self.get_session).await }
        async fn update_session_activity(&self, _: &str) -> Result<bool, sqlx::Error> { Self::pop_field(&self.update_activity).await }
        async fn terminate_session(&self, _: &str) -> Result<bool, sqlx::Error> { Self::pop_field(&self.terminate_session).await }
        async fn get_widget_sessions(&self, _: &str) -> Result<Vec<WidgetSession>, sqlx::Error> { Self::pop_field(&self.get_sessions).await }
        async fn cleanup_expired_sessions(&self) -> Result<u64, sqlx::Error> { Self::pop_field(&self.cleanup_expired).await }
    }

    fn make_widget() -> Widget {
        Widget { id: 1, widget_id: "widget_test123".to_string(), room_id: Some("!room:test.com".to_string()),
            user_id: "@alice:test.com".to_string(), widget_type: "m.custom".to_string(),
            url: "https://example.com/widget".to_string(), name: "Test Widget".to_string(),
            data: serde_json::json!({}), created_ts: 1_700_000_000_000, updated_ts: None, is_active: true }
    }

    fn make_session() -> WidgetSession {
        WidgetSession { id: 1, session_id: "session_test123".to_string(), widget_id: "widget_test123".to_string(),
            user_id: "@alice:test.com".to_string(), device_id: Some("DEVICE1".to_string()),
            created_ts: 1_700_000_000_000, last_active_ts: Some(1_700_000_000_000),
            expires_at: Some(1_700_010_000_000), is_active: true }
    }

    fn make_permission() -> WidgetPermission {
        WidgetPermission { id: 1, widget_id: "widget_test123".to_string(), user_id: "@alice:test.com".to_string(),
            permissions: serde_json::json!(["read", "write"]), created_ts: 1_700_000_000_000, updated_ts: None }
    }

    // --- Widget CRUD ---

    #[tokio::test]
    async fn create_widget_success() {
        let store = MockWidgetStore::new();
        store.set_create_widget(Ok(make_widget()));
        let svc = WidgetService::new(Arc::new(store));
        let widget = svc.create_widget("@alice:test.com",
            CreateWidgetRequest { room_id: Some("!room:test.com".to_string()), widget_type: "m.custom".to_string(),
                url: "https://example.com/widget".to_string(), name: "Test Widget".to_string(), data: None })
            .await.expect("create_widget should succeed");
        assert_eq!(widget.widget_id, "widget_test123");
        assert_eq!(widget.widget_type, "m.custom");
    }

    #[tokio::test]
    async fn create_widget_storage_error() {
        let store = MockWidgetStore::new();
        store.set_create_widget(Err(sqlx::Error::Protocol("db error".to_string())));
        let svc = WidgetService::new(Arc::new(store));
        let err = svc.create_widget("@alice:test.com",
            CreateWidgetRequest { room_id: None, widget_type: "m.custom".to_string(),
                url: "https://example.com".to_string(), name: "W".to_string(), data: None })
            .await.expect_err("should propagate storage error");
        assert!(err.message.contains("Failed to create widget"));
    }

    #[tokio::test]
    async fn get_widget_found() {
        let store = MockWidgetStore::new();
        store.set_get_widget(Ok(Some(make_widget())));
        let svc = WidgetService::new(Arc::new(store));
        let widget = svc.get_widget("widget_test123").await.expect("should succeed");
        assert!(widget.is_some());
        assert_eq!(widget.unwrap().widget_id, "widget_test123");
    }

    #[tokio::test]
    async fn get_widget_not_found() {
        let store = MockWidgetStore::new();
        store.set_get_widget(Ok(None));
        let svc = WidgetService::new(Arc::new(store));
        let widget = svc.get_widget("nonexistent").await.expect("should succeed");
        assert!(widget.is_none());
    }

    #[tokio::test]
    async fn get_room_widgets_returns_list() {
        let store = MockWidgetStore::new();
        store.set_get_room_widgets(Ok(vec![make_widget()]));
        let svc = WidgetService::new(Arc::new(store));
        let widgets = svc.get_room_widgets("!room:test.com").await.expect("should succeed");
        assert_eq!(widgets.len(), 1);
    }

    #[tokio::test]
    async fn get_user_widgets_returns_list() {
        let store = MockWidgetStore::new();
        store.set_get_user_widgets(Ok(vec![make_widget()]));
        let svc = WidgetService::new(Arc::new(store));
        let widgets = svc.get_user_widgets("@alice:test.com").await.expect("should succeed");
        assert_eq!(widgets.len(), 1);
    }

    #[tokio::test]
    async fn update_widget_found() {
        let store = MockWidgetStore::new();
        let mut w = make_widget(); w.name = "Updated Name".to_string();
        store.set_update_widget(Ok(Some(w)));
        let svc = WidgetService::new(Arc::new(store));
        let result = svc.update_widget("widget_test123",
            UpdateWidgetRequest { url: None, name: Some("Updated Name".to_string()), data: None })
            .await.expect("should succeed");
        assert!(result.is_some());
        assert_eq!(result.unwrap().name, "Updated Name");
    }

    #[tokio::test]
    async fn update_widget_not_found() {
        let store = MockWidgetStore::new();
        store.set_update_widget(Ok(None));
        let svc = WidgetService::new(Arc::new(store));
        let result = svc.update_widget("nonexistent",
            UpdateWidgetRequest { url: None, name: None, data: None })
            .await.expect("should succeed");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn delete_widget_success() {
        let store = MockWidgetStore::new();
        store.set_delete_widget(Ok(true));
        let svc = WidgetService::new(Arc::new(store));
        let deleted = svc.delete_widget("widget_test123").await.expect("should succeed");
        assert!(deleted);
    }

    #[tokio::test]
    async fn delete_widget_not_found() {
        let store = MockWidgetStore::new();
        store.set_delete_widget(Ok(false));
        let svc = WidgetService::new(Arc::new(store));
        let deleted = svc.delete_widget("nonexistent").await.expect("should succeed");
        assert!(!deleted);
    }

    // --- Permissions ---

    #[tokio::test]
    async fn set_permission_success() {
        let store = MockWidgetStore::new();
        store.set_set_permission(Ok(make_permission()));
        let svc = WidgetService::new(Arc::new(store));
        let result = svc.set_permission("widget_test123",
            SetPermissionRequest { user_id: "@alice:test.com".to_string(),
                permissions: vec!["read".to_string(), "write".to_string()] })
            .await.expect("set_permission should succeed");
        assert_eq!(result.widget_id, "widget_test123");
    }

    #[tokio::test]
    async fn set_permission_storage_error() {
        let store = MockWidgetStore::new();
        store.set_set_permission(Err(sqlx::Error::RowNotFound));
        let svc = WidgetService::new(Arc::new(store));
        let err = svc.set_permission("widget_test123",
            SetPermissionRequest { user_id: "@bob:test.com".to_string(),
                permissions: vec!["read".to_string()] })
            .await.expect_err("should propagate storage error");
        assert!(err.message.contains("Failed to set widget permission"));
    }

    #[tokio::test]
    async fn get_permissions_returns_list() {
        let store = MockWidgetStore::new();
        store.set_get_permissions(Ok(vec![make_permission()]));
        let svc = WidgetService::new(Arc::new(store));
        let perms = svc.get_permissions("widget_test123").await.expect("should succeed");
        assert_eq!(perms.len(), 1);
    }

    #[tokio::test]
    async fn get_user_permission_found() {
        let store = MockWidgetStore::new();
        store.set_get_user_permission(Ok(Some(make_permission())));
        let svc = WidgetService::new(Arc::new(store));
        let perm = svc.get_user_permission("widget_test123", "@alice:test.com").await.expect("should succeed");
        assert!(perm.is_some());
    }

    #[tokio::test]
    async fn get_user_permission_not_found() {
        let store = MockWidgetStore::new();
        store.set_get_user_permission(Ok(None));
        let svc = WidgetService::new(Arc::new(store));
        let perm = svc.get_user_permission("widget_test123", "@bob:test.com").await.expect("should succeed");
        assert!(perm.is_none());
    }

    #[tokio::test]
    async fn delete_permission_success() {
        let store = MockWidgetStore::new();
        store.set_delete_permission(Ok(true));
        let svc = WidgetService::new(Arc::new(store));
        let deleted = svc.delete_permission("widget_test123", "@alice:test.com").await.expect("should succeed");
        assert!(deleted);
    }

    #[tokio::test]
    async fn delete_permission_not_found() {
        let store = MockWidgetStore::new();
        store.set_delete_permission(Ok(false));
        let svc = WidgetService::new(Arc::new(store));
        let deleted = svc.delete_permission("widget_test123", "@bob:test.com").await.expect("should succeed");
        assert!(!deleted);
    }

    // --- Sessions ---

    #[tokio::test]
    async fn create_session_success() {
        let store = MockWidgetStore::new();
        store.set_create_session(Ok(make_session()));
        let svc = WidgetService::new(Arc::new(store));
        let session = svc.create_session("@alice:test.com",
            CreateSessionRequest { widget_id: "widget_test123".to_string(),
                device_id: Some("DEVICE1".to_string()), expires_in_ms: Some(3_600_000) })
            .await.expect("create_session should succeed");
        assert_eq!(session.session_id, "session_test123");
    }

    #[tokio::test]
    async fn create_session_storage_error() {
        let store = MockWidgetStore::new();
        store.set_create_session(Err(sqlx::Error::RowNotFound));
        let svc = WidgetService::new(Arc::new(store));
        let err = svc.create_session("@alice:test.com",
            CreateSessionRequest { widget_id: "widget_test123".to_string(), device_id: None, expires_in_ms: None })
            .await.expect_err("should propagate storage error");
        assert!(err.message.contains("Failed to create widget session"));
    }

    #[tokio::test]
    async fn get_session_found() {
        let store = MockWidgetStore::new();
        store.set_get_session(Ok(Some(make_session())));
        let svc = WidgetService::new(Arc::new(store));
        let session = svc.get_session("session_test123").await.expect("should succeed");
        assert!(session.is_some());
    }

    #[tokio::test]
    async fn get_session_not_found() {
        let store = MockWidgetStore::new();
        store.set_get_session(Ok(None));
        let svc = WidgetService::new(Arc::new(store));
        let session = svc.get_session("nonexistent").await.expect("should succeed");
        assert!(session.is_none());
    }

    #[tokio::test]
    async fn update_session_activity_success() {
        let store = MockWidgetStore::new();
        store.set_update_activity(Ok(true));
        let svc = WidgetService::new(Arc::new(store));
        let updated = svc.update_session_activity("session_test123").await.expect("should succeed");
        assert!(updated);
    }

    #[tokio::test]
    async fn terminate_session_success() {
        let store = MockWidgetStore::new();
        store.set_terminate_session(Ok(true));
        let svc = WidgetService::new(Arc::new(store));
        let terminated = svc.terminate_session("session_test123").await.expect("should succeed");
        assert!(terminated);
    }

    #[tokio::test]
    async fn get_widget_sessions_returns_list() {
        let store = MockWidgetStore::new();
        store.set_get_sessions(Ok(vec![make_session()]));
        let svc = WidgetService::new(Arc::new(store));
        let sessions = svc.get_widget_sessions("widget_test123").await.expect("should succeed");
        assert_eq!(sessions.len(), 1);
    }

    #[tokio::test]
    async fn cleanup_expired_sessions_returns_count() {
        let store = MockWidgetStore::new();
        store.set_cleanup_expired(Ok(3));
        let svc = WidgetService::new(Arc::new(store));
        let count = svc.cleanup_expired_sessions().await.expect("should succeed");
        assert_eq!(count, 3);
    }

    // --- check_permission logic ---

    #[tokio::test]
    async fn check_permission_exact_match() {
        let store = MockWidgetStore::new();
        store.set_get_user_permission(Ok(Some(make_permission())));
        let svc = WidgetService::new(Arc::new(store));
        let has_read = svc.check_permission("widget_test123", "@alice:test.com", "read").await
            .expect("check_permission should succeed");
        assert!(has_read);
    }

    #[tokio::test]
    async fn check_permission_no_match() {
        let store = MockWidgetStore::new();
        store.set_get_user_permission(Ok(Some(make_permission())));
        let svc = WidgetService::new(Arc::new(store));
        let has_admin = svc.check_permission("widget_test123", "@alice:test.com", "admin").await
            .expect("check_permission should succeed");
        assert!(!has_admin);
    }

    #[tokio::test]
    async fn check_permission_no_permission_record() {
        let store = MockWidgetStore::new();
        store.set_get_user_permission(Ok(None));
        let svc = WidgetService::new(Arc::new(store));
        let has_read = svc.check_permission("widget_test123", "@bob:test.com", "read").await
            .expect("check_permission should succeed");
        assert!(!has_read);
    }

}
