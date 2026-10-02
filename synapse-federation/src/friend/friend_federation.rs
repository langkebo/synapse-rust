use serde_json::Value;
use std::sync::Arc;
use synapse_common::traits::FriendRoomProvider;
use synapse_common::{ApiError, ApiResult};

/// The `FriendFederation` type.
pub struct FriendFederation {
    friend_service: Arc<dyn FriendRoomProvider>,
}

/// Implementation of [`FriendFederation`] methods.
impl FriendFederation {
    /// See [`new`.
    pub fn new(friend_service: Arc<dyn FriendRoomProvider>) -> Self {
        Self { friend_service }
    }

    /// 处理来自联邦的好友请求
    pub async fn on_receive_friend_request(&self, origin: &str, event_content: Value) -> ApiResult<()> {
        // 1. 验证 Origin (简单检查)
        if origin.is_empty() {
            return Err(ApiError::forbidden("Missing origin".to_string()));
        }

        // 2. 解析请求内容
        // 提取并立即转换为 String，避免借用 event_content
        let target_user_id = event_content
            .get("target_user_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ApiError::bad_request("Missing target_user_id".to_string()))?
            .to_string();

        let requester_id = event_content
            .get("requester_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ApiError::bad_request("Missing requester_id".to_string()))?
            .to_string();

        // 3. 验证 requester_id 是否属于 origin
        if !requester_id.ends_with(&format!(":{origin}")) {
            return Err(ApiError::forbidden("Requester ID does not match origin".to_string()));
        }

        // 4. 调用 Service 处理请求
        self.friend_service.handle_incoming_friend_request(&target_user_id, &requester_id, event_content).await?;

        Ok(())
    }

    /// 处理来自联邦的好友接受通知 (`m.friend_request.accepted`)。
    ///
    /// 发送方是 accepter 所在的 homeserver（`origin`）；`requester_id` 指向本方
    /// 发起好友请求的本地用户，`room_id` 是对端创建的共享 DM 房间。
    pub async fn on_receive_friend_accept(&self, origin: &str, event_content: Value) -> ApiResult<()> {
        // 1. 验证 Origin (简单检查)
        if origin.is_empty() {
            return Err(ApiError::forbidden("Missing origin".to_string()));
        }

        // 2. 解析请求内容
        let requester_id = event_content
            .get("requester_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ApiError::bad_request("Missing requester_id".to_string()))?
            .to_string();

        let accepter_id = event_content
            .get("accepter_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ApiError::bad_request("Missing accepter_id".to_string()))?
            .to_string();

        let room_id = event_content
            .get("room_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ApiError::bad_request("Missing room_id".to_string()))?
            .to_string();

        // 3. 验证 accepter_id 是否属于 origin
        if !accepter_id.ends_with(&format!(":{origin}")) {
            return Err(ApiError::forbidden("Accepter ID does not match origin".to_string()));
        }

        // 4. 调用 Service 处理接受通知
        self.friend_service.handle_incoming_friend_accept(&requester_id, &accepter_id, &room_id).await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_friend_federation_request_content_validation() {
        let valid_content = serde_json::json!({
            "target_user_id": "@alice:example.com",
            "requester_id": "@bob:remote.com"
        });

        assert!(valid_content.get("target_user_id").is_some());
        assert!(valid_content.get("requester_id").is_some());
    }

    #[test]
    fn test_friend_federation_missing_target_user_id() {
        let invalid_content = serde_json::json!({
            "requester_id": "@bob:remote.com"
        });

        assert!(invalid_content.get("target_user_id").is_none());
    }

    #[test]
    fn test_friend_federation_missing_requester_id() {
        let invalid_content = serde_json::json!({
            "target_user_id": "@alice:example.com"
        });

        assert!(invalid_content.get("requester_id").is_none());
    }

    #[test]
    fn test_friend_federation_origin_validation() {
        let origin = "remote.com";
        let requester_id = "@bob:remote.com";

        assert!(requester_id.ends_with(&format!(":{origin}")));
    }

    #[test]
    fn test_friend_federation_origin_mismatch() {
        let origin = "remote.com";
        let requester_id = "@bob:other.com";

        assert!(!requester_id.ends_with(&format!(":{origin}")));
    }

    #[test]
    fn test_friend_federation_empty_origin() {
        let origin = "";
        assert!(origin.is_empty());
    }

    #[test]
    fn test_friend_federation_user_id_format() {
        let user_ids = vec!["@alice:example.com", "@bob:matrix.org", "@user123:server.local"];

        for user_id in user_ids {
            assert!(user_id.starts_with('@'));
            assert!(user_id.contains(':'));
        }
    }

    #[test]
    fn test_friend_federation_request_content_with_additional_fields() {
        let content = serde_json::json!({
            "target_user_id": "@alice:example.com",
            "requester_id": "@bob:remote.com",
            "message": "Hello!",
            "timestamp": 1234567890
        });

        assert!(content.get("message").is_some());
        assert!(content.get("timestamp").is_some());
    }

    #[test]
    fn test_friend_federation_request_content_serialization() {
        let content = serde_json::json!({
            "target_user_id": "@alice:example.com",
            "requester_id": "@bob:remote.com"
        });

        let json_str = serde_json::to_string(&content).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        assert_eq!(parsed.get("target_user_id").unwrap().as_str().unwrap(), "@alice:example.com");
    }

    #[test]
    fn test_friend_federation_content_with_null_fields() {
        let content = serde_json::json!({
            "target_user_id": "@alice:example.com",
            "requester_id": "@bob:remote.com",
            "message": null
        });

        assert!(content.get("message").is_some());
        assert!(content.get("message").unwrap().is_null());
    }

    #[test]
    fn test_friend_federation_empty_user_id() {
        let requester_id = "";
        assert!(!requester_id.contains(':'));
    }

    #[test]
    fn test_friend_federation_local_origin() {
        let origin = "localhost";
        let requester_id = "@user:localhost";

        assert!(requester_id.ends_with(&format!(":{origin}")));
    }

    #[test]
    fn test_friend_federation_complex_server_name() {
        let _origin = "server.example.com:8448";
        let requester_id = "@user:server.example.com";

        assert!(requester_id.ends_with(":server.example.com"));
    }

    #[test]
    fn test_friend_federation_origin_with_port() {
        let origin = "example.com:8080";
        let requester_id = "@bob:example.com";

        // Should not match because of port
        assert!(!requester_id.ends_with(&format!(":{origin}")));
    }

    // ── B.3 batch 5/6 — real coverage for FriendFederation::on_receive_friend_request ──
    //
    // The tests above only exercise plain JSON / string operations. These tests
    // instantiate `FriendFederation` with an in-memory `FriendRoomProvider`
    // mock and exercise every branch of `on_receive_friend_request`.

    use super::*;
    use std::sync::Mutex;
    use synapse_common::traits::FriendRoomProvider;

    /// Records every call to `handle_incoming_friend_request` and returns a
    /// pre-configured result. Wrap inner state in `Mutex` so the mock is
    /// `Sync` (required by `Arc<dyn FriendRoomProvider>`).
    struct MockFriendRoomProvider {
        calls: Mutex<Vec<(String, String, serde_json::Value)>>,
        accept_calls: Mutex<Vec<(String, String, String)>>,
        next_result: Mutex<Result<(), ApiError>>,
    }

    /// Implementation of [`MockFriendRoomProvider`] methods.
    impl MockFriendRoomProvider {
        fn new_returning_ok() -> Arc<Self> {
            Arc::new(Self {
                calls: Mutex::new(Vec::new()),
                accept_calls: Mutex::new(Vec::new()),
                next_result: Mutex::new(Ok(())),
            })
        }

        fn new_returning_err(err: ApiError) -> Arc<Self> {
            Arc::new(Self {
                calls: Mutex::new(Vec::new()),
                accept_calls: Mutex::new(Vec::new()),
                next_result: Mutex::new(Err(err)),
            })
        }

        fn calls(&self) -> Vec<(String, String, serde_json::Value)> {
            self.calls.lock().expect("mock mutex poisoned").clone()
        }

        fn accept_calls(&self) -> Vec<(String, String, String)> {
            self.accept_calls.lock().expect("mock mutex poisoned").clone()
        }
    }

    #[async_trait::async_trait]
    /// Implementation of [`FriendRoomProvider`] methods.
    impl FriendRoomProvider for MockFriendRoomProvider {
        async fn handle_incoming_friend_request(
            &self,
            user_id: &str,
            requester_id: &str,
            content: serde_json::Value,
        ) -> Result<(), ApiError> {
            self.calls.lock().expect("mock mutex poisoned").push((
                user_id.to_string(),
                requester_id.to_string(),
                content,
            ));
            self.next_result.lock().expect("mock mutex poisoned").clone()
        }

        async fn handle_incoming_friend_accept(
            &self,
            requester_id: &str,
            accepter_id: &str,
            room_id: &str,
        ) -> Result<(), ApiError> {
            self.accept_calls.lock().expect("mock mutex poisoned").push((
                requester_id.to_string(),
                accepter_id.to_string(),
                room_id.to_string(),
            ));
            self.next_result.lock().expect("mock mutex poisoned").clone()
        }
    }

    #[tokio::test]
    async fn on_receive_friend_request_empty_origin_returns_forbidden() {
        let mock = MockFriendRoomProvider::new_returning_ok();
        let svc = FriendFederation::new(mock.clone());
        let content = serde_json::json!({
            "target_user_id": "@alice:example.com",
            "requester_id": "@bob:remote.com"
        });
        let err = svc.on_receive_friend_request("", content).await.unwrap_err();
        // Should be a 403 forbidden (not bad_request) per the production code.
        let msg = err.to_string();
        assert!(msg.contains("origin") || msg.contains("Origin"), "err={msg}");
        // Mock must NOT have been invoked.
        assert!(mock.calls().is_empty(), "mock should not be called on origin validation failure");
    }

    #[tokio::test]
    async fn on_receive_friend_request_missing_target_user_id_returns_bad_request() {
        let mock = MockFriendRoomProvider::new_returning_ok();
        let svc = FriendFederation::new(mock.clone());
        let content = serde_json::json!({
            "requester_id": "@bob:remote.com"
        });
        let err = svc.on_receive_friend_request("remote.com", content).await.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("target_user_id"), "err={msg}");
        assert!(mock.calls().is_empty());
    }

    #[tokio::test]
    async fn on_receive_friend_request_missing_requester_id_returns_bad_request() {
        let mock = MockFriendRoomProvider::new_returning_ok();
        let svc = FriendFederation::new(mock.clone());
        let content = serde_json::json!({
            "target_user_id": "@alice:example.com"
        });
        let err = svc.on_receive_friend_request("remote.com", content).await.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("requester_id"), "err={msg}");
        assert!(mock.calls().is_empty());
    }

    #[tokio::test]
    async fn on_receive_friend_request_requester_id_not_matching_origin_returns_forbidden() {
        let mock = MockFriendRoomProvider::new_returning_ok();
        let svc = FriendFederation::new(mock.clone());
        let content = serde_json::json!({
            "target_user_id": "@alice:example.com",
            "requester_id": "@bob:other.com"
        });
        let err = svc.on_receive_friend_request("remote.com", content).await.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("origin") || msg.contains("Origin"), "err={msg}");
        assert!(mock.calls().is_empty(), "mock must not be called when origin mismatch");
    }

    #[tokio::test]
    async fn on_receive_friend_request_valid_input_invokes_mock_and_returns_ok() {
        let mock = MockFriendRoomProvider::new_returning_ok();
        let svc = FriendFederation::new(mock.clone());
        let content = serde_json::json!({
            "target_user_id": "@alice:example.com",
            "requester_id": "@bob:remote.com",
            "message": "Hello",
            "extra": 42
        });
        svc.on_receive_friend_request("remote.com", content.clone()).await.unwrap();
        let calls = mock.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "@alice:example.com", "user_id should be target_user_id");
        assert_eq!(calls[0].1, "@bob:remote.com", "requester_id should be passed through");
        assert_eq!(calls[0].2, content, "full content should be forwarded to provider");
    }

    #[tokio::test]
    async fn on_receive_friend_request_provider_error_propagates() {
        let mock = MockFriendRoomProvider::new_returning_err(ApiError::internal("downstream boom".to_string()));
        let svc = FriendFederation::new(mock.clone());
        let content = serde_json::json!({
            "target_user_id": "@alice:example.com",
            "requester_id": "@bob:remote.com"
        });
        let err = svc.on_receive_friend_request("remote.com", content).await.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("downstream boom"), "provider error should propagate: {msg}");
        assert_eq!(mock.calls().len(), 1);
    }

    #[tokio::test]
    async fn on_receive_friend_request_target_user_id_on_different_origin_is_ok() {
        // target_user_id does NOT need to belong to `origin`; only requester_id must.
        let mock = MockFriendRoomProvider::new_returning_ok();
        let svc = FriendFederation::new(mock.clone());
        let content = serde_json::json!({
            "target_user_id": "@alice:localhost",
            "requester_id": "@bob:remote.com"
        });
        svc.on_receive_friend_request("remote.com", content).await.unwrap();
        assert_eq!(mock.calls().len(), 1);
    }

    // ── P1b — on_receive_friend_accept branches ──

    #[tokio::test]
    async fn on_receive_friend_accept_empty_origin_returns_forbidden() {
        let mock = MockFriendRoomProvider::new_returning_ok();
        let svc = FriendFederation::new(mock.clone());
        let content = serde_json::json!({
            "requester_id": "@alice:localhost",
            "accepter_id": "@bob:remote.com",
            "room_id": "!dm:remote.com"
        });
        let err = svc.on_receive_friend_accept("", content).await.unwrap_err();
        assert!(err.to_string().contains("origin") || err.to_string().contains("Origin"), "err={err}");
        assert!(mock.accept_calls().is_empty());
    }

    #[tokio::test]
    async fn on_receive_friend_accept_missing_requester_id_returns_bad_request() {
        let mock = MockFriendRoomProvider::new_returning_ok();
        let svc = FriendFederation::new(mock.clone());
        let content = serde_json::json!({
            "accepter_id": "@bob:remote.com",
            "room_id": "!dm:remote.com"
        });
        let err = svc.on_receive_friend_accept("remote.com", content).await.unwrap_err();
        assert!(err.to_string().contains("requester_id"), "err={err}");
        assert!(mock.accept_calls().is_empty());
    }

    #[tokio::test]
    async fn on_receive_friend_accept_missing_accepter_id_returns_bad_request() {
        let mock = MockFriendRoomProvider::new_returning_ok();
        let svc = FriendFederation::new(mock.clone());
        let content = serde_json::json!({
            "requester_id": "@alice:localhost",
            "room_id": "!dm:remote.com"
        });
        let err = svc.on_receive_friend_accept("remote.com", content).await.unwrap_err();
        assert!(err.to_string().contains("accepter_id"), "err={err}");
        assert!(mock.accept_calls().is_empty());
    }

    #[tokio::test]
    async fn on_receive_friend_accept_missing_room_id_returns_bad_request() {
        let mock = MockFriendRoomProvider::new_returning_ok();
        let svc = FriendFederation::new(mock.clone());
        let content = serde_json::json!({
            "requester_id": "@alice:localhost",
            "accepter_id": "@bob:remote.com"
        });
        let err = svc.on_receive_friend_accept("remote.com", content).await.unwrap_err();
        assert!(err.to_string().contains("room_id"), "err={err}");
        assert!(mock.accept_calls().is_empty());
    }

    #[tokio::test]
    async fn on_receive_friend_accept_accepter_id_not_matching_origin_returns_forbidden() {
        let mock = MockFriendRoomProvider::new_returning_ok();
        let svc = FriendFederation::new(mock.clone());
        let content = serde_json::json!({
            "requester_id": "@alice:localhost",
            "accepter_id": "@bob:other.com",
            "room_id": "!dm:remote.com"
        });
        let err = svc.on_receive_friend_accept("remote.com", content).await.unwrap_err();
        assert!(err.to_string().contains("origin") || err.to_string().contains("Origin"), "err={err}");
        assert!(mock.accept_calls().is_empty());
    }

    #[tokio::test]
    async fn on_receive_friend_accept_valid_input_invokes_mock_with_room_id() {
        let mock = MockFriendRoomProvider::new_returning_ok();
        let svc = FriendFederation::new(mock.clone());
        let content = serde_json::json!({
            "requester_id": "@alice:localhost",
            "accepter_id": "@bob:remote.com",
            "room_id": "!dm:remote.com",
            "timestamp": 12345
        });
        svc.on_receive_friend_accept("remote.com", content).await.unwrap();
        let calls = mock.accept_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "@alice:localhost");
        assert_eq!(calls[0].1, "@bob:remote.com");
        assert_eq!(calls[0].2, "!dm:remote.com");
    }

    #[tokio::test]
    async fn on_receive_friend_accept_provider_error_propagates() {
        let mock = MockFriendRoomProvider::new_returning_err(ApiError::internal("accept downstream boom".to_string()));
        let svc = FriendFederation::new(mock.clone());
        let content = serde_json::json!({
            "requester_id": "@alice:localhost",
            "accepter_id": "@bob:remote.com",
            "room_id": "!dm:remote.com"
        });
        let err = svc.on_receive_friend_accept("remote.com", content).await.unwrap_err();
        assert!(err.to_string().contains("accept downstream boom"), "err={err}");
        assert_eq!(mock.accept_calls().len(), 1);
    }
}
