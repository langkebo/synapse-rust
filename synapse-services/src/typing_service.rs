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
//! # 多 Worker 一致性
//!
//! 打字状态通过 CacheManager（L1 本地 + L2 Redis）存储，确保多 worker
//! 部署时各节点共享同一份打字状态。Redis 不可用时自动降级为本地缓存。
//!
//! # 示例
//!
//! ```text
//! use synapse_services::TypingService;
//!
//! #[tokio::main]
//! async fn main() {
//!     let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
//!     let service = TypingService::new(cache);
//!
//!     // 设置用户正在打字
//!     service.set_typing("!room:example.com", "@alice:example.com", 30000).await.unwrap();
//!
//!     // 获取房间中正在打字的用户
//!     let typing_users = service.get_typing_users("!room:example.com").await.unwrap();
//!     assert!(typing_users.contains_key("@alice:example.com"));
//! }
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use synapse_cache::CacheManager;
use synapse_common::current_timestamp_millis;
use synapse_common::ApiResult;

/// 打字用户信息
///
/// 存储用户打字状态的详细信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// 房间级别打字状态快照，用于 Redis 存储
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RoomTypingState {
    users: Vec<TypingUser>,
}

/// 生成房间级别打字状态缓存键
fn room_typing_key(room_id: &str) -> String {
    format!("typing:room:{room_id}")
}

/// 默认打字状态 TTL（秒）：最大超时 + 缓冲
const DEFAULT_TYPING_TTL: u64 = 120;

pub struct TypingService {
    cache: Arc<CacheManager>,
}

impl TypingService {
    /// 创建新的打字服务实例
    ///
    /// # 参数
    ///
    /// * `cache` - 缓存管理器，支持 L1 本地 + L2 Redis 双层缓存
    pub fn new(cache: Arc<CacheManager>) -> Self {
        Self { cache }
    }

    /// 获取所有打字状态数量（估算）
    ///
    /// 注意：在多 worker 部署时，此方法返回的是近似值，
    /// 因为 Redis 中没有高效的方式统计所有 typing 键。
    pub fn get_typing_count(&self) -> usize {
        // 在多 worker 场景下，无法高效获取全局计数，返回 0 作为占位
        0
    }

    /// 清除房间中所有用户的打字状态
    pub async fn clear_room_typing(&self, room_id: &str) -> usize {
        let key = room_typing_key(room_id);
        self.cache.delete(&key).await;
        // 无法精确知道清除了多少用户，返回 0
        0
    }

    pub async fn set_typing(&self, room_id: &str, user_id: &str, timeout_ms: u64) -> ApiResult<()> {
        let key = room_typing_key(room_id);

        let typing_user = TypingUser {
            user_id: user_id.to_string(),
            room_id: room_id.to_string(),
            timeout_ms,
            started_ts: current_timestamp_millis(),
        };

        // 读取当前房间的打字状态，更新或添加用户
        let mut state: RoomTypingState = self
            .cache
            .get::<RoomTypingState>(&key)
            .await
            .unwrap_or_default()
            .unwrap_or(RoomTypingState { users: Vec::new() });

        // 移除同一用户的旧状态（如果存在）
        state.users.retain(|u| u.user_id != user_id);
        state.users.push(typing_user);

        // 使用较长的 TTL 确保打字状态不会过早过期
        let ttl = DEFAULT_TYPING_TTL;
        self.cache.set(&key, &state, ttl).await?;

        Ok(())
    }

    pub async fn clear_typing(&self, room_id: &str, user_id: &str) -> ApiResult<()> {
        let key = room_typing_key(room_id);

        let mut state: RoomTypingState = match self.cache.get::<RoomTypingState>(&key).await {
            Ok(Some(s)) => s,
            _ => return Ok(()),
        };

        state.users.retain(|u| u.user_id != user_id);

        if state.users.is_empty() {
            self.cache.delete(&key).await;
        } else {
            let ttl = DEFAULT_TYPING_TTL;
            self.cache.set(&key, &state, ttl).await?;
        }

        Ok(())
    }

    pub async fn get_typing_users(&self, room_id: &str) -> ApiResult<HashMap<String, u64>> {
        let now = current_timestamp_millis();
        let key = room_typing_key(room_id);

        let mut state: RoomTypingState = match self.cache.get::<RoomTypingState>(&key).await {
            Ok(Some(s)) => s,
            _ => return Ok(HashMap::new()),
        };

        // 清除过期用户
        let original_len = state.users.len();
        state.users.retain(|u| {
            let expiry = u.started_ts + (u.timeout_ms as i64);
            expiry > now
        });

        let result: HashMap<String, u64> = state.users.iter().map(|u| (u.user_id.clone(), u.timeout_ms)).collect();

        // 如果清除了过期用户，更新缓存
        if state.users.len() != original_len {
            if state.users.is_empty() {
                self.cache.delete(&key).await;
            } else {
                let ttl = DEFAULT_TYPING_TTL;
                if let Err(e) = self.cache.set(&key, &state, ttl).await {
                    ::tracing::warn!(room_id = %room_id, cache_key = %key, error = %e, "Failed to refresh typing cache");
                }
            }
        }

        Ok(result)
    }

    pub async fn get_typing_users_batch(&self, room_ids: &[String]) -> ApiResult<HashMap<String, Vec<String>>> {
        let now = current_timestamp_millis();
        let mut result: HashMap<String, Vec<String>> = HashMap::with_capacity(room_ids.len());

        for room_id in room_ids {
            result.insert(room_id.clone(), Vec::new());

            let key = room_typing_key(room_id);
            if let Ok(Some(mut state)) = self.cache.get::<RoomTypingState>(&key).await {
                let original_len = state.users.len();
                state.users.retain(|u| {
                    let expiry = u.started_ts + (u.timeout_ms as i64);
                    expiry > now
                });

                let user_ids: Vec<String> = state.users.iter().map(|u| u.user_id.clone()).collect();
                result.insert(room_id.clone(), user_ids);

                if state.users.len() != original_len {
                    if state.users.is_empty() {
                        self.cache.delete(&key).await;
                    } else {
                        let ttl = DEFAULT_TYPING_TTL;
                        if let Err(e) = self.cache.set(&key, &state, ttl).await {
                            ::tracing::warn!(
                                room_id = %room_id,
                                cache_key = %key,
                                error = %e,
                                "Failed to refresh typing cache in batch lookup"
                            );
                        }
                    }
                }
            }
        }

        Ok(result)
    }

    pub async fn get_user_typing(&self, room_id: &str, user_id: &str) -> ApiResult<Option<u64>> {
        let now = current_timestamp_millis();
        let key = room_typing_key(room_id);

        let state: RoomTypingState = match self.cache.get::<RoomTypingState>(&key).await {
            Ok(Some(s)) => s,
            _ => return Ok(None),
        };

        for user in &state.users {
            if user.user_id == user_id {
                let expiry = user.started_ts + (user.timeout_ms as i64);
                if expiry > now {
                    return Ok(Some(user.timeout_ms));
                }
            }
        }

        Ok(None)
    }

    pub fn clear_expired_typing(&self) -> ApiResult<()> {
        // 在 Redis 模式下，过期条目由 TTL 自动清理。
        // 此方法保留用于兼容性，实际清理发生在 get_typing_users 读取时。
        Ok(())
    }
}

impl Default for TypingService {
    fn default() -> Self {
        // 无 Redis 的默认实例，仅用于测试
        Self { cache: Arc::new(CacheManager::new(&synapse_cache::CacheConfig::default())) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use synapse_cache::CacheConfig;

    fn create_test_service() -> TypingService {
        let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
        TypingService::new(cache)
    }

    #[tokio::test]
    async fn test_set_typing() {
        let service = create_test_service();

        service.set_typing("!room:example.com", "@user:example.com", 30000).await.unwrap();

        let timeout = service.get_user_typing("!room:example.com", "@user:example.com").await.unwrap();
        assert!(timeout.is_some());
        assert_eq!(timeout, Some(30000));
    }

    #[tokio::test]
    async fn test_clear_typing() {
        let service = create_test_service();

        service.set_typing("!room:example.com", "@user:example.com", 30000).await.unwrap();
        service.clear_typing("!room:example.com", "@user:example.com").await.unwrap();

        let timeout = service.get_user_typing("!room:example.com", "@user:example.com").await.unwrap();
        assert!(timeout.is_none());
    }

    #[tokio::test]
    async fn test_get_typing_users() {
        let service = create_test_service();

        service.set_typing("!room:example.com", "@user1:example.com", 30000).await.unwrap();
        service.set_typing("!room:example.com", "@user2:example.com", 30000).await.unwrap();

        let users = service.get_typing_users("!room:example.com").await.unwrap();

        assert_eq!(users.len(), 2);
        assert!(users.contains_key("@user1:example.com"));
        assert!(users.contains_key("@user2:example.com"));
    }

    #[tokio::test]
    async fn test_get_user_not_typing() {
        let service = create_test_service();

        let timeout = service.get_user_typing("!room:example.com", "@user:example.com").await.unwrap();
        assert!(timeout.is_none());
    }

    #[tokio::test]
    async fn test_typing_different_rooms() {
        let service = create_test_service();

        service.set_typing("!room1:example.com", "@user:example.com", 30000).await.unwrap();
        service.set_typing("!room2:example.com", "@user:example.com", 30000).await.unwrap();

        let users1 = service.get_typing_users("!room1:example.com").await.unwrap();
        let users2 = service.get_typing_users("!room2:example.com").await.unwrap();

        assert!(users1.contains_key("@user:example.com"));
        assert!(users2.contains_key("@user:example.com"));
    }

    #[tokio::test]
    async fn test_clear_expired_typing() {
        let service = create_test_service();

        // Set typing with 0 timeout (immediately expired)
        service.set_typing("!room:example.com", "@user:example.com", 0).await.unwrap();

        // Clear expired
        service.clear_expired_typing().unwrap();

        // Expired users are cleaned up on read in get_typing_users
        let timeout = service.get_user_typing("!room:example.com", "@user:example.com").await.unwrap();
        assert!(timeout.is_none());
    }

    #[tokio::test]
    async fn test_typing_timeout() {
        let service = create_test_service();

        service.set_typing("!room:example.com", "@user:example.com", 5000).await.unwrap();

        let timeout = service.get_user_typing("!room:example.com", "@user:example.com").await.unwrap();
        assert_eq!(timeout, Some(5000));
    }

    #[tokio::test]
    async fn test_overwrite_typing() {
        let service = create_test_service();

        service.set_typing("!room:example.com", "@user:example.com", 30000).await.unwrap();
        service.set_typing("!room:example.com", "@user:example.com", 60_000).await.unwrap();

        let timeout = service.get_user_typing("!room:example.com", "@user:example.com").await.unwrap();
        assert_eq!(timeout, Some(60_000));
    }
}
