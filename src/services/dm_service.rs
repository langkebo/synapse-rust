//! DM Service - 直接消息服务
//!
//! 该模块提供 Matrix 直接消息（DM）房间管理功能。
//!
//! # 功能
//!
//! - DM 房间创建和管理
//! - 查找用户之间的现有 DM 房间
//! - 获取用户的所有 DM 房间
//! - 识别 DM 房间的对话伙伴
//!
//! # 示例
//!
//! ```text
//! use synapse_rust::services::{DMService, DMServiceImpl};
//!
//! #[tokio::main]
//! async fn main() {
//!     let service = DMServiceImpl::new();
//!     
//!     // 标记房间为 DM
//!     service.mark_room_as_dm(
//!         "!dm:example.com",
//!         "@alice:example.com",
//!         &["@bob:example.com".to_string()]
//!     ).await.unwrap();
//!     
//!     // 检查是否为 DM 房间
//!     let is_dm = service.is_dm_room("!dm:example.com", "@alice:example.com").await.unwrap();
//!     assert!(is_dm);
//! }
//! ```

use crate::common::ApiResult;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// DM 房间信息
///
/// 存储直接消息房间的元数据。
#[derive(Debug, Clone)]
pub struct DMRoom {
    /// 房间 ID（例如：!dm:example.com）
    pub room_id: String,
    /// 创建者用户 ID
    pub creator_id: String,
    /// 接收者用户 ID
    pub recipient_id: String,
    /// 创建时间戳（毫秒）
    pub created_ts: i64,
}

/// 直接消息服务 trait
///
/// 定义 DM 房间管理的核心接口。
/// 所有实现必须线程安全（`Send + Sync`）。
#[async_trait]
pub trait DMService: Send + Sync {
    /// 查找两个用户之间现有的 DM 房间
    ///
    /// # 参数
    ///
    /// * `user_id` - 第一个用户的 ID
    /// * `recipient_id` - 第二个用户的 ID
    ///
    /// # 返回
    ///
    /// 返回现有的 DM 房间 ID，如果不存在则返回 `None`。
    ///
    /// # 示例
    ///
    /// ```text
    /// let room_id = service.get_existing_dm("@alice:example.com", "@bob:example.com").await?;
    /// ```
    async fn get_existing_dm(&self, user_id: &str, recipient_id: &str)
        -> ApiResult<Option<String>>;

    /// 获取用户的所有 DM 房间
    ///
    /// # 参数
    ///
    /// * `user_id` - 用户 ID
    ///
    /// # 返回
    ///
    /// 返回用户参与的所有 DM 房间列表。
    async fn get_user_dms(&self, user_id: &str) -> ApiResult<Vec<DMRoom>>;

    /// 将房间标记为 DM 房间
    ///
    /// # 参数
    ///
    /// * `room_id` - 房间 ID
    /// * `creator_id` - 创建者用户 ID
    /// * `recipients` - 接收者用户 ID 列表
    ///
    /// # 错误
    ///
    /// 如果存储操作失败，返回错误。
    async fn mark_room_as_dm(
        &self,
        room_id: &str,
        creator_id: &str,
        recipients: &[String],
    ) -> ApiResult<()>;

    /// 检查房间是否为 DM 房间
    ///
    /// # 参数
    ///
    /// * `room_id` - 房间 ID
    /// * `user_id` - 用户 ID（用于权限检查）
    ///
    /// # 返回
    ///
    /// 如果房间是 DM 房间，返回 `true`。
    async fn is_dm_room(&self, room_id: &str, user_id: &str) -> ApiResult<bool>;

    /// 获取 DM 房间的对话伙伴
    ///
    /// # 参数
    ///
    /// * `room_id` - 房间 ID
    /// * `user_id` - 当前用户 ID
    ///
    /// # 返回
    ///
    /// 返回对话伙伴的用户 ID，如果用户不在该 DM 房间中则返回 `None`。
    async fn get_dm_partner(&self, room_id: &str, user_id: &str) -> ApiResult<Option<String>>;

    /// 更新 DM 房间的用户列表
    ///
    /// # 参数
    ///
    /// * `room_id` - 房间 ID
    /// * `user_id` - 执行更新的用户 ID
    /// * `users` - 新的用户 ID 列表
    async fn update_dm_users(
        &self,
        room_id: &str,
        user_id: &str,
        users: &[String],
    ) -> ApiResult<()>;
}

/// 直接消息服务实现
///
/// 使用内存存储的 DM 服务实现，适用于开发和测试环境。
/// 生产环境应使用数据库支持的实现。
pub struct DMServiceImpl {
    /// DM 房间映射（room_id -> DMRoom）
    dm_rooms: Arc<RwLock<HashMap<String, DMRoom>>>,
    /// 用户 DM 列表映射（user_id -> [room_ids]）
    user_dms: Arc<RwLock<HashMap<String, Vec<String>>>>,
}

impl DMServiceImpl {
    /// 创建新的 DM 服务实例
    ///
    /// # 示例
    ///
    /// ```text
    /// let service = DMServiceImpl::new();
    /// ```
    pub fn new() -> Self {
        Self {
            dm_rooms: Arc::new(RwLock::new(HashMap::new())),
            user_dms: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 创建 DM 查找键
    ///
    /// 生成用于查找两个用户之间 DM 房间的唯一键。
    /// 键的生成与用户 ID 的顺序无关。
    ///
    /// # 参数
    ///
    /// * `user_id` - 第一个用户的 ID
    /// * `recipient_id` - 第二个用户的 ID
    ///
    /// # 返回
    ///
    /// 返回排序后的用户 ID 组合键。
    ///
    /// # 示例
    ///
    /// ```text
    /// let key1 = DMServiceImpl::create_dm_key("@alice:example.com", "@bob:example.com");
    /// let key2 = DMServiceImpl::create_dm_key("@bob:example.com", "@alice:example.com");
    /// assert_eq!(key1, key2);
    /// ```
    pub fn create_dm_key(user_id: &str, recipient_id: &str) -> String {
        let mut ids = [user_id.to_string(), recipient_id.to_string()];
        ids.sort();
        format!("{}:{}", ids[0], ids[1])
    }

    /// 移除 DM 房间
    ///
    /// # 参数
    ///
    /// * `room_id` - 要移除的房间 ID
    pub async fn remove_dm_room(&self, room_id: &str) {
        let mut dms = self.dm_rooms.write().await;
        if let Some(dm) = dms.remove(room_id) {
            let mut user_dms = self.user_dms.write().await;

            if let Some(rooms) = user_dms.get_mut(&dm.creator_id) {
                rooms.retain(|r| r != room_id);
            }
            if let Some(rooms) = user_dms.get_mut(&dm.recipient_id) {
                rooms.retain(|r| r != room_id);
            }
        }
    }

    /// 获取 DM 房间数量
    ///
    /// # 返回
    ///
    /// 返回当前存储的 DM 房间总数。
    pub async fn get_dm_count(&self) -> usize {
        let dms = self.dm_rooms.read().await;
        dms.len()
    }
}

impl Default for DMServiceImpl {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DMService for DMServiceImpl {
    async fn get_existing_dm(
        &self,
        user_id: &str,
        recipient_id: &str,
    ) -> ApiResult<Option<String>> {
        let dms = self.dm_rooms.read().await;

        for (room_id, dm) in dms.iter() {
            if (dm.creator_id == user_id && dm.recipient_id == recipient_id)
                || (dm.creator_id == recipient_id && dm.recipient_id == user_id)
            {
                return Ok(Some(room_id.clone()));
            }
        }

        Ok(None)
    }

    async fn get_user_dms(&self, user_id: &str) -> ApiResult<Vec<DMRoom>> {
        let dms = self.dm_rooms.read().await;
        let user_dms = self.user_dms.read().await;

        let room_ids = user_dms.get(user_id).cloned().unwrap_or_default();

        let result: Vec<DMRoom> = room_ids
            .iter()
            .filter_map(|rid| dms.get(rid).cloned())
            .collect();

        Ok(result)
    }

    async fn mark_room_as_dm(
        &self,
        room_id: &str,
        creator_id: &str,
        recipients: &[String],
    ) -> ApiResult<()> {
        let recipient_id = recipients.first().map(|s| s.as_str()).unwrap_or("");

        let dm = DMRoom {
            room_id: room_id.to_string(),
            creator_id: creator_id.to_string(),
            recipient_id: recipient_id.to_string(),
            created_ts: chrono::Utc::now().timestamp_millis(),
        };

        let mut dms = self.dm_rooms.write().await;
        dms.insert(room_id.to_string(), dm);

        let mut user_dms = self.user_dms.write().await;
        user_dms
            .entry(creator_id.to_string())
            .or_default()
            .push(room_id.to_string());

        for recipient in recipients {
            user_dms
                .entry(recipient.clone())
                .or_default()
                .push(room_id.to_string());
        }

        Ok(())
    }

    async fn is_dm_room(&self, room_id: &str, user_id: &str) -> ApiResult<bool> {
        let dms = self.dm_rooms.read().await;
        let user_dms = self.user_dms.read().await;
        
        if !dms.contains_key(room_id) {
            return Ok(false);
        }

        let user_rooms = user_dms.get(user_id).cloned().unwrap_or_default();
        Ok(user_rooms.contains(&room_id.to_string()))
    }

    async fn get_dm_partner(&self, room_id: &str, user_id: &str) -> ApiResult<Option<String>> {
        let dms = self.dm_rooms.read().await;

        if let Some(dm) = dms.get(room_id) {
            let partner = if dm.creator_id == user_id {
                Some(dm.recipient_id.clone())
            } else if dm.recipient_id == user_id {
                Some(dm.creator_id.clone())
            } else {
                None
            };
            return Ok(partner);
        }

        Ok(None)
    }

    async fn update_dm_users(
        &self,
        room_id: &str,
        _user_id: &str,
        users: &[String],
    ) -> ApiResult<()> {
        let mut dms = self.dm_rooms.write().await;
        if let Some(dm) = dms.get_mut(room_id) {
            if let Some(primary_recipient) = users.first() {
                dm.recipient_id = primary_recipient.clone();
            }
        }

        let mut user_dms = self.user_dms.write().await;
        if let Some(dm) = dms.get(room_id) {
            let old_creator = dm.creator_id.clone();
            let old_recipient = dm.recipient_id.clone();

            if let Some(rooms) = user_dms.get_mut(&old_recipient) {
                rooms.retain(|r| r != room_id);
            }

            for user in users {
                user_dms
                    .entry(user.clone())
                    .or_default()
                    .push(room_id.to_string());
            }

            if !users.contains(&old_creator) {
                if let Some(rooms) = user_dms.get_mut(&old_creator) {
                    rooms.retain(|r| r != room_id);
                }
            }

            if !users.contains(&old_recipient) {
                if let Some(rooms) = user_dms.get_mut(&old_recipient) {
                    rooms.retain(|r| r != room_id);
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_dm_key() {
        let key1 = DMServiceImpl::create_dm_key("@alice:example.com", "@bob:example.com");
        let key2 = DMServiceImpl::create_dm_key("@bob:example.com", "@alice:example.com");

        assert_eq!(key1, key2);
        assert!(key1.starts_with("@alice:example.com"));
    }

    #[tokio::test]
    async fn test_mark_room_as_dm() {
        let service = DMServiceImpl::new();

        service
            .mark_room_as_dm(
                "!dm:example.com",
                "@alice:example.com",
                &["@bob:example.com".to_string()],
            )
            .await
            .unwrap();

        let is_dm = service
            .is_dm_room("!dm:example.com", "@alice:example.com")
            .await
            .unwrap();
        assert!(is_dm);
    }

    #[tokio::test]
    async fn test_is_not_dm_room() {
        let service = DMServiceImpl::new();

        let is_dm = service
            .is_dm_room("!room:example.com", "@alice:example.com")
            .await
            .unwrap();
        assert!(!is_dm);
    }

    #[tokio::test]
    async fn test_get_dm_partner() {
        let service = DMServiceImpl::new();

        service
            .mark_room_as_dm(
                "!dm:example.com",
                "@alice:example.com",
                &["@bob:example.com".to_string()],
            )
            .await
            .unwrap();

        let partner = service
            .get_dm_partner("!dm:example.com", "@alice:example.com")
            .await
            .unwrap();
        assert_eq!(partner, Some("@bob:example.com".to_string()));

        let partner = service
            .get_dm_partner("!dm:example.com", "@bob:example.com")
            .await
            .unwrap();
        assert_eq!(partner, Some("@alice:example.com".to_string()));
    }

    #[tokio::test]
    async fn test_get_user_dms() {
        let service = DMServiceImpl::new();

        service
            .mark_room_as_dm(
                "!dm1:example.com",
                "@alice:example.com",
                &["@bob:example.com".to_string()],
            )
            .await
            .unwrap();

        service
            .mark_room_as_dm(
                "!dm2:example.com",
                "@alice:example.com",
                &["@charlie:example.com".to_string()],
            )
            .await
            .unwrap();

        let dms = service.get_user_dms("@alice:example.com").await.unwrap();
        assert_eq!(dms.len(), 2);
    }

    #[tokio::test]
    async fn test_get_existing_dm() {
        let service = DMServiceImpl::new();

        let room_id = service
            .get_existing_dm("@alice:example.com", "@bob:example.com")
            .await
            .unwrap();
        assert_eq!(room_id, None);

        service
            .mark_room_as_dm(
                "!dm:example.com",
                "@alice:example.com",
                &["@bob:example.com".to_string()],
            )
            .await
            .unwrap();

        let room_id = service
            .get_existing_dm("@alice:example.com", "@bob:example.com")
            .await
            .unwrap();
        assert_eq!(room_id, Some("!dm:example.com".to_string()));

        let room_id = service
            .get_existing_dm("@bob:example.com", "@alice:example.com")
            .await
            .unwrap();
        assert_eq!(room_id, Some("!dm:example.com".to_string()));
    }

    #[tokio::test]
    async fn test_remove_dm_room() {
        let service = DMServiceImpl::new();

        service
            .mark_room_as_dm(
                "!dm:example.com",
                "@alice:example.com",
                &["@bob:example.com".to_string()],
            )
            .await
            .unwrap();

        assert_eq!(service.get_dm_count().await, 1);

        service.remove_dm_room("!dm:example.com").await;

        assert_eq!(service.get_dm_count().await, 0);

        let is_dm = service
            .is_dm_room("!dm:example.com", "@alice:example.com")
            .await
            .unwrap();
        assert!(!is_dm);
    }

    #[tokio::test]
    async fn test_update_dm_users() {
        let service = DMServiceImpl::new();

        service
            .mark_room_as_dm(
                "!dm:example.com",
                "@alice:example.com",
                &["@bob:example.com".to_string()],
            )
            .await
            .unwrap();

        service
            .update_dm_users(
                "!dm:example.com",
                "@alice:example.com",
                &[
                    "@bob:example.com".to_string(),
                    "@charlie:example.com".to_string(),
                ],
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_get_dm_partner_for_non_member() {
        let service = DMServiceImpl::new();

        service
            .mark_room_as_dm(
                "!dm:example.com",
                "@alice:example.com",
                &["@bob:example.com".to_string()],
            )
            .await
            .unwrap();

        let partner = service
            .get_dm_partner("!dm:example.com", "@charlie:example.com")
            .await
            .unwrap();
        assert_eq!(partner, None);
    }
}
