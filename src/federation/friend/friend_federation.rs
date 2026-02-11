use crate::common::{ApiError, ApiResult};
use crate::services::FriendRoomService;
use std::sync::Arc;
use serde_json::Value;

pub struct FriendFederation {
    friend_service: Arc<FriendRoomService>,
}

impl FriendFederation {
    pub fn new(friend_service: Arc<FriendRoomService>) -> Self {
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
        if !requester_id.ends_with(&format!(":{}", origin)) {
            return Err(ApiError::forbidden("Requester ID does not match origin".to_string()));
        }

        // 4. 调用 Service 处理请求
        self.friend_service
            .handle_incoming_friend_request(&target_user_id, &requester_id, event_content)
            .await?;

        Ok(())
    }
}
