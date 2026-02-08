use crate::storage::{PrivateChatStorage, PrivateSession, PrivateMessage, UserStorage};
use crate::common::ApiError;
use serde_json::Value;

#[derive(Clone)]
pub struct PrivateChatService {
    storage: PrivateChatStorage,
    user_storage: UserStorage,
}

impl PrivateChatService {
    pub fn new(storage: PrivateChatStorage, user_storage: UserStorage) -> Self {
        Self {
            storage,
            user_storage,
        }
    }

    /// 创建会话
    pub async fn create_session(&self, user_id: &str, participant: &str) -> Result<PrivateSession, ApiError> {
        // 1. 验证参与者是否存在
        if !self.user_storage.user_exists(participant).await.unwrap_or(false) {
            return Err(ApiError::not_found("Participant not found"));
        }

        // 2. 不能和自己创建会话
        if user_id == participant {
            return Err(ApiError::bad_request("Cannot create session with yourself"));
        }

        // 3. 获取或创建会话
        self.storage
            .get_or_create_session(user_id, participant)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))
    }

    /// 获取会话列表
    pub async fn get_sessions(&self, user_id: &str) -> Result<Vec<PrivateSession>, ApiError> {
        self.storage
            .get_user_sessions(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Database error: {}", e)))
    }

    /// 获取单个会话（带鉴权）
    pub async fn get_session(&self, user_id: &str, session_id: &str) -> Result<PrivateSession, ApiError> {
        // 目前 storage 层只能通过 user1/user2 获取，或者列表获取
        // 这是一个优化点：Storage 应该支持按 session_id 获取
        // 这里暂时通过遍历列表实现（性能较低，仅作为演示）
        let sessions = self.get_sessions(user_id).await?;
        sessions
            .into_iter()
            .find(|s| s.session_id == session_id)
            .ok_or_else(|| ApiError::not_found("Session not found"))
    }

    /// 发送消息
    pub async fn send_message(
        &self, 
        user_id: &str, 
        session_id: &str, 
        content: Value
    ) -> Result<PrivateMessage, ApiError> {
        // 1. 验证会话归属权
        let _session = self.get_session(user_id, session_id).await?;

        // 2. 保存消息
        self.storage
            .save_message(session_id, user_id, content)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to save message: {}", e)))
    }

    /// 获取消息历史
    pub async fn get_messages(
        &self,
        user_id: &str,
        session_id: &str,
        limit: i64,
        before: Option<String>
    ) -> Result<Vec<PrivateMessage>, ApiError> {
        // 1. 验证会话归属权
        let _session = self.get_session(user_id, session_id).await?;

        // 2. 查询
        self.storage
            .get_messages(session_id, limit, before)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to fetch messages: {}", e)))
    }
}
