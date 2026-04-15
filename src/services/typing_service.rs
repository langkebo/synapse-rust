//! Typing Service - 打字提示服务
//!
//! 该模块提供 Matrix 打字提示（Typing Indicator）管理功能。
//!
//! # 功能
//!
//! - 设置和清除用户的打字状态
//! - 获取房间中正在打字的用户列表
//! - 自动清理过期的打字状态
//!
//! # 示例
//!
//! ```text
//! use synapse_rust::services::{TypingService, TypingServiceImpl};
//!
//! #[tokio::main]
//! async fn main() {
//!     let service = TypingServiceImpl::new();
//!     
//!     // 设置用户正在打字
//!     service.set_typing("!room:example.com", "@alice:example.com", 30000).await.unwrap();
//!     
//!     // 获取房间中正在打字的用户
//!     let typing_users = service.get_typing_users("!room:example.com").await.unwrap();
//!     assert!(typing_users.contains_key("@alice:example.com"));
//! }
//! ```

use crate::common::ApiResult;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 打字用户信息
///
/// 存储用户打字状态的详细信息。
#[derive(Debug, Clone)]
pub struct TypingUser {
    /// 用户 ID（例如：@alice:example.com）
    pub user_id: String,
    /// 房间 ID（例如：!room:example.com）
    pub room_id: String,
    /// 打字状态超时时间（毫秒）
    pub timeout_ms: u64,
    /// 打字状态开始时间戳（毫秒）
    pub started_ts: i64,
}

/// 打字提示服务 trait
///
/// 定义打字状态管理的核心接口。
/// 所有实现必须线程安全（`Send + Sync`）。
#[async_trait]
pub trait TypingService: Send + Sync {
    /// 设置用户正在打字
    ///
    /// # 参数
    ///
    /// * `room_id` - 房间 ID
    /// * `user_id` - 用户 ID
    /// * `timeout_ms` - 打字状态超时时间（毫秒），通常为 15000-30000ms
    ///
    /// # 错误
    ///
    /// 如果存储操作失败，返回错误。
    ///
    /// # 示例
    ///
    /// ```text
    /// service.set_typing("!room:example.com", "@alice:example.com", 30000).await?;
    /// ```
    async fn set_typing(&self, room_id: &str, user_id: &str, timeout_ms: u64) -> ApiResult<()>;

    /// 清除用户的打字状态
    ///
    /// 手动清除用户的打字状态，通常在用户停止打字或发送消息后调用。
    ///
    /// # 参数
    ///
    /// * `room_id` - 房间 ID
    /// * `user_id` - 用户 ID
    async fn clear_typing(&self, room_id: &str, user_id: &str) -> ApiResult<()>;

    /// 获取房间中正在打字的用户列表
    ///
    /// 返回房间中所有当前正在打字的用户及其超时时间。
    /// 自动清理已过期的打字状态。
    ///
    /// # 参数
    ///
    /// * `room_id` - 房间 ID
    ///
    /// # 返回
    ///
    /// 返回用户 ID 到超时时间的映射。
    async fn get_typing_users(&self, room_id: &str) -> ApiResult<HashMap<String, u64>>;

    /// 获取特定用户的打字状态
    ///
    /// # 参数
    ///
    /// * `room_id` - 房间 ID
    /// * `user_id` - 用户 ID
    ///
    /// # 返回
    ///
    /// 如果用户正在打字，返回剩余超时时间；否则返回 `None`。
    async fn get_user_typing(&self, room_id: &str, user_id: &str) -> ApiResult<Option<u64>>;

    /// 清理所有过期的打字状态
    ///
    /// 遍历所有打字状态，移除已过期的记录。
    /// 建议定期调用此方法进行清理。
    async fn clear_expired_typing(&self) -> ApiResult<()>;
}

/// 打字提示服务实现
///
/// 使用内存存储的打字服务实现，适用于开发和测试环境。
/// 生产环境应使用数据库或分布式缓存支持的实现。
pub struct TypingServiceImpl {
    /// 打字状态映射（"room_id:user_id" -> TypingUser）
    typing: Arc<RwLock<HashMap<String, TypingUser>>>,
}

impl TypingServiceImpl {
    /// 创建新的打字服务实例
    ///
    /// # 示例
    ///
    /// ```text
    /// let service = TypingServiceImpl::new();
    /// ```
    pub fn new() -> Self {
        Self {
            typing: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 生成打字状态的存储键
    ///
    /// # 参数
    ///
    /// * `room_id` - 房间 ID
    /// * `user_id` - 用户 ID
    ///
    /// # 返回
    ///
    /// 返回格式为 "room_id:user_id" 的键。
    fn make_key(room_id: &str, user_id: &str) -> String {
        format!("{}:{}", room_id, user_id)
    }

    /// 获取所有打字状态数量
    ///
    /// # 返回
    ///
    /// 返回当前存储的打字状态总数。
    pub async fn get_typing_count(&self) -> usize {
        let typing = self.typing.read().await;
        typing.len()
    }

    /// 清除房间中所有用户的打字状态
    ///
    /// # 参数
    ///
    /// * `room_id` - 房间 ID
    ///
    /// # 返回
    ///
    /// 返回被清除的打字状态数量。
    pub async fn clear_room_typing(&self, room_id: &str) -> usize {
        let mut typing = self.typing.write().await;
        let before = typing.len();
        typing.retain(|_, v| v.room_id != room_id);
        before - typing.len()
    }
}

impl Default for TypingServiceImpl {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TypingService for TypingServiceImpl {
    async fn set_typing(&self, room_id: &str, user_id: &str, timeout_ms: u64) -> ApiResult<()> {
        let key = Self::make_key(room_id, user_id);

        let typing_user = TypingUser {
            user_id: user_id.to_string(),
            room_id: room_id.to_string(),
            timeout_ms,
            started_ts: chrono::Utc::now().timestamp_millis(),
        };

        let mut typing = self.typing.write().await;
        typing.insert(key, typing_user);

        Ok(())
    }

    async fn clear_typing(&self, room_id: &str, user_id: &str) -> ApiResult<()> {
        let key = Self::make_key(room_id, user_id);

        let mut typing = self.typing.write().await;
        typing.remove(&key);

        Ok(())
    }

    async fn get_typing_users(&self, room_id: &str) -> ApiResult<HashMap<String, u64>> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut typing = self.typing.write().await;

        // Clear expired
        typing.retain(|_, v| {
            let expiry = v.started_ts + (v.timeout_ms as i64);
            expiry > now
        });

        // Get users in this room
        let result: HashMap<String, u64> = typing
            .iter()
            .filter(|(_, v)| v.room_id == room_id)
            .map(|(_k, v)| (v.user_id.clone(), v.timeout_ms))
            .collect();

        Ok(result)
    }

    async fn get_user_typing(&self, room_id: &str, user_id: &str) -> ApiResult<Option<u64>> {
        let key = Self::make_key(room_id, user_id);
        let typing = self.typing.read().await;

        if let Some(user) = typing.get(&key) {
            let now = chrono::Utc::now().timestamp_millis();
            let expiry = user.started_ts + (user.timeout_ms as i64);

            if expiry > now {
                return Ok(Some(user.timeout_ms));
            }
        }

        Ok(None)
    }

    async fn clear_expired_typing(&self) -> ApiResult<()> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut typing = self.typing.write().await;

        typing.retain(|_, v| {
            let expiry = v.started_ts + (v.timeout_ms as i64);
            expiry > now
        });

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_set_typing() {
        let service = TypingServiceImpl::new();

        service
            .set_typing("!room:example.com", "@user:example.com", 30000)
            .await
            .unwrap();

        let timeout = service
            .get_user_typing("!room:example.com", "@user:example.com")
            .await
            .unwrap();
        assert!(timeout.is_some());
        assert_eq!(timeout, Some(30000));
    }

    #[tokio::test]
    async fn test_clear_typing() {
        let service = TypingServiceImpl::new();

        service
            .set_typing("!room:example.com", "@user:example.com", 30000)
            .await
            .unwrap();
        service
            .clear_typing("!room:example.com", "@user:example.com")
            .await
            .unwrap();

        let timeout = service
            .get_user_typing("!room:example.com", "@user:example.com")
            .await
            .unwrap();
        assert!(timeout.is_none());
    }

    #[tokio::test]
    async fn test_get_typing_users() {
        let service = TypingServiceImpl::new();

        service
            .set_typing("!room:example.com", "@user1:example.com", 30000)
            .await
            .unwrap();
        service
            .set_typing("!room:example.com", "@user2:example.com", 30000)
            .await
            .unwrap();

        let users = service.get_typing_users("!room:example.com").await.unwrap();

        assert_eq!(users.len(), 2);
        assert!(users.contains_key("@user1:example.com"));
        assert!(users.contains_key("@user2:example.com"));
    }

    #[tokio::test]
    async fn test_get_user_not_typing() {
        let service = TypingServiceImpl::new();

        let timeout = service
            .get_user_typing("!room:example.com", "@user:example.com")
            .await
            .unwrap();
        assert!(timeout.is_none());
    }

    #[tokio::test]
    async fn test_typing_different_rooms() {
        let service = TypingServiceImpl::new();

        service
            .set_typing("!room1:example.com", "@user:example.com", 30000)
            .await
            .unwrap();
        service
            .set_typing("!room2:example.com", "@user:example.com", 30000)
            .await
            .unwrap();

        let users1 = service
            .get_typing_users("!room1:example.com")
            .await
            .unwrap();
        let users2 = service
            .get_typing_users("!room2:example.com")
            .await
            .unwrap();

        assert!(users1.contains_key("@user:example.com"));
        assert!(users2.contains_key("@user:example.com"));
    }

    #[tokio::test]
    async fn test_clear_expired_typing() {
        let service = TypingServiceImpl::new();

        // Set typing with 0 timeout (immediately expired)
        service
            .set_typing("!room:example.com", "@user:example.com", 0)
            .await
            .unwrap();

        // Clear expired
        service.clear_expired_typing().await.unwrap();

        // Should be cleared
        let timeout = service
            .get_user_typing("!room:example.com", "@user:example.com")
            .await
            .unwrap();
        assert!(timeout.is_none());
    }

    #[tokio::test]
    async fn test_typing_timeout() {
        let service = TypingServiceImpl::new();

        service
            .set_typing("!room:example.com", "@user:example.com", 5000)
            .await
            .unwrap();

        let timeout = service
            .get_user_typing("!room:example.com", "@user:example.com")
            .await
            .unwrap();
        assert_eq!(timeout, Some(5000));
    }

    #[tokio::test]
    async fn test_get_typing_count() {
        let service = TypingServiceImpl::new();

        assert_eq!(service.get_typing_count().await, 0);

        service
            .set_typing("!room1:example.com", "@user1:example.com", 30000)
            .await
            .unwrap();
        service
            .set_typing("!room2:example.com", "@user2:example.com", 30000)
            .await
            .unwrap();

        assert_eq!(service.get_typing_count().await, 2);
    }

    #[tokio::test]
    async fn test_clear_room_typing() {
        let service = TypingServiceImpl::new();

        service
            .set_typing("!room1:example.com", "@user1:example.com", 30000)
            .await
            .unwrap();
        service
            .set_typing("!room1:example.com", "@user2:example.com", 30000)
            .await
            .unwrap();
        service
            .set_typing("!room2:example.com", "@user3:example.com", 30000)
            .await
            .unwrap();

        let cleared = service.clear_room_typing("!room1:example.com").await;
        assert_eq!(cleared, 2);

        let users1 = service
            .get_typing_users("!room1:example.com")
            .await
            .unwrap();
        let users2 = service
            .get_typing_users("!room2:example.com")
            .await
            .unwrap();

        assert!(users1.is_empty());
        assert_eq!(users2.len(), 1);
    }

    #[tokio::test]
    async fn test_overwrite_typing() {
        let service = TypingServiceImpl::new();

        service
            .set_typing("!room:example.com", "@user:example.com", 30000)
            .await
            .unwrap();
        service
            .set_typing("!room:example.com", "@user:example.com", 60000)
            .await
            .unwrap();

        let timeout = service
            .get_user_typing("!room:example.com", "@user:example.com")
            .await
            .unwrap();
        assert_eq!(timeout, Some(60000));

        assert_eq!(service.get_typing_count().await, 1);
    }
}
