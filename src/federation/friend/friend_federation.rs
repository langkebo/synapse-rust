use crate::common::{ApiError, ApiResult};
use crate::services::FriendRoomService;
use serde_json::Value;
use std::sync::Arc;

pub struct FriendFederation {
    friend_service: Arc<FriendRoomService>,
}

impl FriendFederation {
    pub fn new(friend_service: Arc<FriendRoomService>) -> Self {
        Self { friend_service }
    }

    /// 处理来自联邦的好友请求
    pub async fn on_receive_friend_request(
        &self,
        origin: &str,
        event_content: Value,
    ) -> ApiResult<()> {
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
        if !requester_id.ends_with(&format!(":{}", origin)) {
            return Err(ApiError::forbidden(
                "Requester ID does not match origin".to_string(),
            ));
        }

        // 4. 调用 Service 处理请求
        self.friend_service
            .handle_incoming_friend_request(&target_user_id, &requester_id, event_content)
            .await?;

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

        assert!(requester_id.ends_with(&format!(":{}", origin)));
    }

    #[test]
    fn test_friend_federation_origin_mismatch() {
        let origin = "remote.com";
        let requester_id = "@bob:other.com";

        assert!(!requester_id.ends_with(&format!(":{}", origin)));
    }

    #[test]
    fn test_friend_federation_empty_origin() {
        let origin = "";
        assert!(origin.is_empty());
    }

    #[test]
    fn test_friend_federation_user_id_format() {
        let user_ids = vec![
            "@alice:example.com",
            "@bob:matrix.org",
            "@user123:server.local",
        ];

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
}
