//! Directory Service - 目录服务
//!
//! 该模块提供 Matrix 房间目录和别名管理功能。
//!
//! # 功能
//!
//! - 房间别名管理（设置、获取、删除）
//! - 规范别名（canonical alias）管理
//! - 公共房间列表查询
//! - 公共房间搜索
//!
//! # 示例
//!
//! ```rust,ignore
//! use synapse_rust::services::{DirectoryService, DirectoryServiceImpl};
//!
//! #[tokio::main]
//! async fn main() {
//!     let service = DirectoryServiceImpl::new();
//!     
//!     // 设置房间别名
//!     service.set_room_alias("!room:example.com", "#myroom:example.com").await.unwrap();
//!     
//!     // 通过别名获取房间 ID
//!     let room_id = service.get_room_id_by_alias("#myroom:example.com").await.unwrap();
//!     assert_eq!(room_id, Some("!room:example.com".to_string()));
//! }
//! ```

use crate::common::ApiResult;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 公共目录中的房间信息
///
/// 包含房间的基本元数据，用于公共房间列表展示。
#[derive(Debug, Clone)]
pub struct DirectoryRoom {
    /// 房间 ID（例如：!room:example.com）
    pub room_id: String,
    /// 房间名称（可选）
    pub name: Option<String>,
    /// 房间主题（可选）
    pub topic: Option<String>,
    /// 房间头像 URL（可选）
    pub avatar_url: Option<String>,
    /// 房间成员数量
    pub member_count: i64,
    /// 是否允许任何人读取房间内容
    pub world_readable: bool,
    /// 是否允许访客加入
    pub guest_can_join: bool,
}

/// 目录服务 trait
///
/// 定义房间目录和别名管理的核心接口。
/// 所有实现必须线程安全（`Send + Sync`）。
#[async_trait]
pub trait DirectoryService: Send + Sync {
    /// 通过别名获取房间 ID
    ///
    /// # 参数
    ///
    /// * `alias` - 房间别名（例如：#myroom:example.com）
    ///
    /// # 返回
    ///
    /// 返回房间 ID，如果别名不存在则返回 `None`。
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// let room_id = service.get_room_id_by_alias("#myroom:example.com").await?;
    /// ```
    async fn get_room_id_by_alias(&self, alias: &str) -> ApiResult<Option<String>>;

    /// 设置房间别名
    ///
    /// 将别名映射到指定的房间 ID。如果别名已存在，将被覆盖。
    ///
    /// # 参数
    ///
    /// * `room_id` - 房间 ID
    /// * `alias` - 要设置的别名
    ///
    /// # 错误
    ///
    /// 如果存储操作失败，返回错误。
    async fn set_room_alias(&self, room_id: &str, alias: &str) -> ApiResult<()>;

    /// 删除房间别名
    ///
    /// 从目录中移除指定的别名。如果别名不存在，操作静默成功。
    ///
    /// # 参数
    ///
    /// * `alias` - 要删除的别名
    async fn remove_room_alias(&self, alias: &str) -> ApiResult<()>;

    /// 获取房间的规范别名
    ///
    /// 规范别名是房间的主要别名，用于标识房间。
    ///
    /// # 参数
    ///
    /// * `room_id` - 房间 ID
    ///
    /// # 返回
    ///
    /// 返回规范别名，如果未设置则返回 `None`。
    async fn get_canonical_alias(&self, room_id: &str) -> ApiResult<Option<String>>;

    /// 设置房间的规范别名
    ///
    /// # 参数
    ///
    /// * `room_id` - 房间 ID
    /// * `alias` - 规范别名，传入 `None` 清除规范别名
    async fn set_canonical_alias(&self, room_id: &str, alias: Option<&str>) -> ApiResult<()>;

    /// 获取公共房间列表
    ///
    /// 分页获取公共房间列表。
    ///
    /// # 参数
    ///
    /// * `limit` - 返回的最大房间数量
    /// * `since` - 分页令牌（用于获取下一页）
    ///
    /// # 返回
    ///
    /// 返回公共房间列表。
    async fn get_public_rooms(
        &self,
        limit: i32,
        since: Option<&str>,
    ) -> ApiResult<Vec<DirectoryRoom>>;

    /// 搜索公共房间
    ///
    /// 根据过滤条件搜索公共房间。搜索范围包括房间名称和主题。
    ///
    /// # 参数
    ///
    /// * `filter` - 搜索过滤词（可选）
    /// * `limit` - 返回的最大房间数量
    ///
    /// # 返回
    ///
    /// 返回匹配的公共房间列表。
    async fn search_public_rooms(
        &self,
        filter: Option<&str>,
        limit: i32,
    ) -> ApiResult<Vec<DirectoryRoom>>;
}

/// 目录服务实现
///
/// 使用内存存储的目录服务实现，适用于开发和测试环境。
/// 生产环境应使用数据库支持的实现。
pub struct DirectoryServiceImpl {
    /// 别名到房间 ID 的映射
    aliases: Arc<RwLock<HashMap<String, String>>>,
    /// 房间 ID 到别名列表的映射
    room_aliases: Arc<RwLock<HashMap<String, Vec<String>>>>,
    /// 房间 ID 到规范别名的映射
    canonical_aliases: Arc<RwLock<HashMap<String, String>>>,
    /// 公共房间列表
    public_rooms: Arc<RwLock<HashMap<String, DirectoryRoom>>>,
}

impl DirectoryServiceImpl {
    /// 创建新的目录服务实例
    ///
    /// # 示例
    ///
    /// ```rust,ignore
    /// let service = DirectoryServiceImpl::new();
    /// ```
    pub fn new() -> Self {
        Self {
            aliases: Arc::new(RwLock::new(HashMap::new())),
            room_aliases: Arc::new(RwLock::new(HashMap::new())),
            canonical_aliases: Arc::new(RwLock::new(HashMap::new())),
            public_rooms: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 添加公共房间到目录
    ///
    /// # 参数
    ///
    /// * `room` - 要添加的房间信息
    pub async fn add_public_room(&self, room: DirectoryRoom) {
        let mut rooms = self.public_rooms.write().await;
        rooms.insert(room.room_id.clone(), room);
    }

    /// 从目录移除公共房间
    ///
    /// # 参数
    ///
    /// * `room_id` - 要移除的房间 ID
    pub async fn remove_public_room(&self, room_id: &str) {
        let mut rooms = self.public_rooms.write().await;
        rooms.remove(room_id);
    }

    /// 获取房间的所有别名
    ///
    /// # 参数
    ///
    /// * `room_id` - 房间 ID
    ///
    /// # 返回
    ///
    /// 返回房间的所有别名列表。
    pub async fn get_room_aliases(&self, room_id: &str) -> Vec<String> {
        let room_aliases = self.room_aliases.read().await;
        room_aliases.get(room_id).cloned().unwrap_or_default()
    }
}

impl Default for DirectoryServiceImpl {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DirectoryService for DirectoryServiceImpl {
    async fn get_room_id_by_alias(&self, alias: &str) -> ApiResult<Option<String>> {
        Ok(self.aliases.read().await.get(alias).cloned())
    }

    async fn set_room_alias(&self, room_id: &str, alias: &str) -> ApiResult<()> {
        let mut aliases = self.aliases.write().await;
        aliases.insert(alias.to_string(), room_id.to_string());

        let mut room_aliases = self.room_aliases.write().await;
        room_aliases
            .entry(room_id.to_string())
            .or_default()
            .push(alias.to_string());

        Ok(())
    }

    async fn remove_room_alias(&self, alias: &str) -> ApiResult<()> {
        let mut aliases = self.aliases.write().await;
        if let Some(room_id) = aliases.remove(alias) {
            let mut room_aliases = self.room_aliases.write().await;
            if let Some(aliases_list) = room_aliases.get_mut(&room_id) {
                aliases_list.retain(|a| a != alias);
            }
        }
        Ok(())
    }

    async fn get_canonical_alias(&self, room_id: &str) -> ApiResult<Option<String>> {
        Ok(self.canonical_aliases.read().await.get(room_id).cloned())
    }

    async fn set_canonical_alias(&self, room_id: &str, alias: Option<&str>) -> ApiResult<()> {
        let mut canonical = self.canonical_aliases.write().await;
        if let Some(a) = alias {
            canonical.insert(room_id.to_string(), a.to_string());
        } else {
            canonical.remove(room_id);
        }
        Ok(())
    }

    async fn get_public_rooms(
        &self,
        limit: i32,
        _since: Option<&str>,
    ) -> ApiResult<Vec<DirectoryRoom>> {
        let rooms = self.public_rooms.read().await;
        let result: Vec<DirectoryRoom> = rooms.values().take(limit as usize).cloned().collect();
        Ok(result)
    }

    async fn search_public_rooms(
        &self,
        filter: Option<&str>,
        limit: i32,
    ) -> ApiResult<Vec<DirectoryRoom>> {
        let rooms = self.public_rooms.read().await;

        let mut result: Vec<DirectoryRoom> = Vec::new();

        for r in rooms.values() {
            let matches = if let Some(f) = filter {
                let f_lower = f.to_lowercase();
                let name_match = r
                    .name
                    .as_ref()
                    .is_some_and(|n| n.to_lowercase().contains(&f_lower));
                let topic_match = r
                    .topic
                    .as_ref()
                    .is_some_and(|t| t.to_lowercase().contains(&f_lower));
                name_match || topic_match
            } else {
                true
            };

            if matches {
                result.push(r.clone());
            }

            if result.len() >= limit as usize {
                break;
            }
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_set_and_get_room_alias() {
        let service = DirectoryServiceImpl::new();

        service
            .set_room_alias("!room:example.com", "#test:example.com")
            .await
            .unwrap();

        let room_id = service
            .get_room_id_by_alias("#test:example.com")
            .await
            .unwrap();
        assert_eq!(room_id, Some("!room:example.com".to_string()));
    }

    #[tokio::test]
    async fn test_get_nonexistent_alias() {
        let service = DirectoryServiceImpl::new();

        let room_id = service
            .get_room_id_by_alias("#nonexistent:example.com")
            .await
            .unwrap();
        assert_eq!(room_id, None);
    }

    #[tokio::test]
    async fn test_remove_room_alias() {
        let service = DirectoryServiceImpl::new();

        service
            .set_room_alias("!room:example.com", "#test:example.com")
            .await
            .unwrap();
        service
            .remove_room_alias("#test:example.com")
            .await
            .unwrap();

        let room_id = service
            .get_room_id_by_alias("#test:example.com")
            .await
            .unwrap();
        assert_eq!(room_id, None);
    }

    #[tokio::test]
    async fn test_set_canonical_alias() {
        let service = DirectoryServiceImpl::new();

        service
            .set_canonical_alias("!room:example.com", Some("#main:example.com"))
            .await
            .unwrap();

        let alias = service
            .get_canonical_alias("!room:example.com")
            .await
            .unwrap();
        assert_eq!(alias, Some("#main:example.com".to_string()));
    }

    #[tokio::test]
    async fn test_clear_canonical_alias() {
        let service = DirectoryServiceImpl::new();

        service
            .set_canonical_alias("!room:example.com", Some("#main:example.com"))
            .await
            .unwrap();
        service
            .set_canonical_alias("!room:example.com", None)
            .await
            .unwrap();

        let alias = service
            .get_canonical_alias("!room:example.com")
            .await
            .unwrap();
        assert_eq!(alias, None);
    }

    #[tokio::test]
    async fn test_get_public_rooms() {
        let service = DirectoryServiceImpl::new();

        service
            .add_public_room(DirectoryRoom {
                room_id: "!room1:example.com".to_string(),
                name: Some("Room 1".to_string()),
                topic: None,
                avatar_url: None,
                member_count: 10,
                world_readable: true,
                guest_can_join: true,
            })
            .await;

        let rooms = service.get_public_rooms(10, None).await.unwrap();
        assert_eq!(rooms.len(), 1);
    }

    #[tokio::test]
    async fn test_search_public_rooms() {
        let service = DirectoryServiceImpl::new();

        service
            .add_public_room(DirectoryRoom {
                room_id: "!room1:example.com".to_string(),
                name: Some("Test Room".to_string()),
                topic: Some("A test topic".to_string()),
                avatar_url: None,
                member_count: 10,
                world_readable: true,
                guest_can_join: true,
            })
            .await;

        service
            .add_public_room(DirectoryRoom {
                room_id: "!room2:example.com".to_string(),
                name: Some("Another Room".to_string()),
                topic: None,
                avatar_url: None,
                member_count: 5,
                world_readable: true,
                guest_can_join: false,
            })
            .await;

        let rooms = service.search_public_rooms(Some("test"), 10).await.unwrap();
        assert_eq!(rooms.len(), 1);
        assert_eq!(rooms[0].room_id, "!room1:example.com");
    }

    #[tokio::test]
    async fn test_get_room_aliases() {
        let service = DirectoryServiceImpl::new();

        service
            .set_room_alias("!room:example.com", "#alias1:example.com")
            .await
            .unwrap();
        service
            .set_room_alias("!room:example.com", "#alias2:example.com")
            .await
            .unwrap();

        let aliases = service.get_room_aliases("!room:example.com").await;
        assert_eq!(aliases.len(), 2);
        assert!(aliases.contains(&"#alias1:example.com".to_string()));
        assert!(aliases.contains(&"#alias2:example.com".to_string()));
    }
}
