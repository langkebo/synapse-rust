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
//! ```text
//! use synapse_services::DirectoryService;
//!
//! #[tokio::main]
//! async fn main() {
//!     let service = DirectoryService::new();
//!
//!     // 设置房间别名
//!     service.set_room_alias("!room:example.com", "#myroom:example.com").await.unwrap();
//!
//!     // 通过别名获取房间 ID
//!     let room_id = service.get_room_id_by_alias("#myroom:example.com").await.unwrap();
//!     assert_eq!(room_id, Some("!room:example.com".to_string()));
//! }
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use synapse_common::error::ApiError;
use synapse_common::ApiResult;
use synapse_storage::RoomStoreApi;
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

pub struct DirectoryService {
    /// 别名到房间 ID 的映射（仅当 storage 为 None 时使用）
    aliases: Arc<RwLock<HashMap<String, String>>>,
    /// 房间 ID 到别名列表的映射（仅当 storage 为 None 时使用）
    room_aliases: Arc<RwLock<HashMap<String, Vec<String>>>>,
    /// 公共房间列表
    public_rooms: Arc<RwLock<HashMap<String, DirectoryRoom>>>,
    /// 持久化存储后端。Some 时所有别名操作委托到数据库；None 时降级为内存 HashMap
    room_storage: Option<Arc<dyn RoomStoreApi>>,
}

impl DirectoryService {
    /// 创建新的目录服务实例
    ///
    /// # 示例
    ///
    /// ```text
    /// let service = DirectoryService::new();
    /// ```
    pub fn new() -> Self {
        Self {
            aliases: Arc::new(RwLock::new(HashMap::new())),
            room_aliases: Arc::new(RwLock::new(HashMap::new())),
            public_rooms: Arc::new(RwLock::new(HashMap::new())),
            room_storage: None,
        }
    }

    /// 创建带持久化存储后端的目录服务实例。
    ///
    /// 当 `room_storage` 为 Some 时，所有别名操作（set/get/remove/get_aliases）
    /// 委托到数据库，服务器重启后别名不会丢失。
    pub fn with_storage(room_storage: Arc<dyn RoomStoreApi>) -> Self {
        Self {
            aliases: Arc::new(RwLock::new(HashMap::new())),
            room_aliases: Arc::new(RwLock::new(HashMap::new())),
            public_rooms: Arc::new(RwLock::new(HashMap::new())),
            room_storage: Some(room_storage),
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
        if let Some(storage) = &self.room_storage {
            match storage.get_room_aliases(room_id).await {
                Ok(aliases) => return aliases,
                Err(e) => {
                    tracing::warn!(room_id = %room_id, error = %e, "S24: Failed to get room aliases from storage, falling back to in-memory");
                }
            }
        }
        let room_aliases = self.room_aliases.read().await;
        room_aliases.get(room_id).cloned().unwrap_or_default()
    }

    pub async fn get_room_id_by_alias(&self, alias: &str) -> ApiResult<Option<String>> {
        if let Some(storage) = &self.room_storage {
            match storage.get_room_by_alias(alias).await {
                Ok(room_id) => return Ok(room_id),
                Err(e) => {
                    tracing::warn!(alias = %alias, error = %e, "S24: Failed to get room by alias from storage, falling back to in-memory");
                }
            }
        }
        Ok(self.aliases.read().await.get(alias).cloned())
    }

    pub async fn set_room_alias(&self, room_id: &str, alias: &str) -> ApiResult<()> {
        if let Some(storage) = &self.room_storage {
            storage
                .set_room_alias(room_id, alias, "")
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to set room alias in storage", &e))?;
            return Ok(());
        }
        let mut aliases = self.aliases.write().await;
        aliases.insert(alias.to_string(), room_id.to_string());

        let mut room_aliases = self.room_aliases.write().await;
        room_aliases.entry(room_id.to_string()).or_default().push(alias.to_string());

        Ok(())
    }

    pub async fn remove_room_alias(&self, alias: &str) -> ApiResult<()> {
        if let Some(storage) = &self.room_storage {
            storage
                .remove_room_alias_by_name(alias)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to remove room alias from storage", &e))?;
            return Ok(());
        }
        let mut aliases = self.aliases.write().await;
        if let Some(room_id) = aliases.remove(alias) {
            let mut room_aliases = self.room_aliases.write().await;
            if let Some(aliases_list) = room_aliases.get_mut(&room_id) {
                aliases_list.retain(|a| a != alias);
            }
        }
        Ok(())
    }

    pub async fn get_public_rooms(&self, limit: i32, _since: Option<&str>) -> ApiResult<Vec<DirectoryRoom>> {
        let rooms = self.public_rooms.read().await;
        let result: Vec<DirectoryRoom> = rooms.values().take(limit as usize).cloned().collect();
        Ok(result)
    }

    pub async fn search_public_rooms(&self, filter: Option<&str>, limit: i32) -> ApiResult<Vec<DirectoryRoom>> {
        let rooms = self.public_rooms.read().await;

        let mut result: Vec<DirectoryRoom> = Vec::new();

        for r in rooms.values() {
            let matches = if let Some(f) = filter {
                let f_lower = f.to_lowercase();
                let name_match = r.name.as_ref().is_some_and(|n| n.to_lowercase().contains(&f_lower));
                let topic_match = r.topic.as_ref().is_some_and(|t| t.to_lowercase().contains(&f_lower));
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

impl Default for DirectoryService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_set_and_get_room_alias() {
        let service = DirectoryService::new();

        service.set_room_alias("!room:example.com", "#test:example.com").await.unwrap();

        let room_id = service.get_room_id_by_alias("#test:example.com").await.unwrap();
        assert_eq!(room_id, Some("!room:example.com".to_string()));
    }

    #[tokio::test]
    async fn test_get_nonexistent_alias() {
        let service = DirectoryService::new();

        let room_id = service.get_room_id_by_alias("#nonexistent:example.com").await.unwrap();
        assert_eq!(room_id, None);
    }

    #[tokio::test]
    async fn test_remove_room_alias() {
        let service = DirectoryService::new();

        service.set_room_alias("!room:example.com", "#test:example.com").await.unwrap();
        service.remove_room_alias("#test:example.com").await.unwrap();

        let room_id = service.get_room_id_by_alias("#test:example.com").await.unwrap();
        assert_eq!(room_id, None);
    }

    #[tokio::test]
    async fn test_get_public_rooms() {
        let service = DirectoryService::new();

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
        let service = DirectoryService::new();

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
        let service = DirectoryService::new();

        service.set_room_alias("!room:example.com", "#alias1:example.com").await.unwrap();
        service.set_room_alias("!room:example.com", "#alias2:example.com").await.unwrap();

        let aliases = service.get_room_aliases("!room:example.com").await;
        assert_eq!(aliases.len(), 2);
        assert!(aliases.contains(&"#alias1:example.com".to_string()));
        assert!(aliases.contains(&"#alias2:example.com".to_string()));
    }

    // ── S24: 持久化存储委托测试 ──────────────────────────────────────────

    use synapse_storage::test_mocks::InMemoryRoomStore;

    /// 辅助：创建带房间的 InMemoryRoomStore
    async fn make_store_with_room() -> Arc<InMemoryRoomStore> {
        let store = Arc::new(InMemoryRoomStore::new());
        store
            .create_room("!room:example.com", "@alice:example.com", "public", "11", true)
            .await
            .unwrap();
        store
    }

    #[tokio::test]
    async fn s24_alias_persists_across_directory_service_instances() {
        // S24: 别名通过存储后端持久化，新 DirectoryService 实例（模拟重启）后别名不丢失
        let store = make_store_with_room().await;

        // 第一个 DirectoryService 实例设置别名
        let svc1 = DirectoryService::with_storage(store.clone());
        svc1.set_room_alias("!room:example.com", "#persistent:example.com")
            .await
            .unwrap();

        // 模拟重启：创建新的 DirectoryService，共享同一个存储后端
        let svc2 = DirectoryService::with_storage(store.clone());
        let room_id = svc2.get_room_id_by_alias("#persistent:example.com").await.unwrap();

        assert_eq!(room_id, Some("!room:example.com".to_string()),
            "S24: alias must survive DirectoryService re-instantiation when storage is used");
    }

    #[tokio::test]
    async fn s24_remove_alias_persists_to_storage() {
        // S24: 删除别名也持久化到存储
        let store = make_store_with_room().await;

        let svc1 = DirectoryService::with_storage(store.clone());
        svc1.set_room_alias("!room:example.com", "#removable:example.com")
            .await
            .unwrap();

        // 通过第二个实例删除
        let svc2 = DirectoryService::with_storage(store.clone());
        svc2.remove_room_alias("#removable:example.com").await.unwrap();

        // 第三个实例验证已删除
        let svc3 = DirectoryService::with_storage(store.clone());
        let result = svc3.get_room_id_by_alias("#removable:example.com").await.unwrap();
        assert_eq!(result, None, "S24: removed alias must not be found after deletion via storage");
    }

    #[tokio::test]
    async fn s24_get_room_aliases_from_storage() {
        // S24: get_room_aliases 从存储后端读取
        let store = make_store_with_room().await;

        let svc = DirectoryService::with_storage(store.clone());
        svc.set_room_alias("!room:example.com", "#alias_a:example.com").await.unwrap();
        svc.set_room_alias("!room:example.com", "#alias_b:example.com").await.unwrap();

        // 新实例验证
        let svc2 = DirectoryService::with_storage(store.clone());
        let aliases = svc2.get_room_aliases("!room:example.com").await;
        assert_eq!(aliases.len(), 2, "S24: get_room_aliases must return from storage");
        assert!(aliases.contains(&"#alias_a:example.com".to_string()));
        assert!(aliases.contains(&"#alias_b:example.com".to_string()));
    }

    #[tokio::test]
    async fn s24_in_memory_fallback_still_works() {
        // S24: 无存储时降级为内存模式（向后兼容）
        let svc = DirectoryService::new();
        svc.set_room_alias("!room:example.com", "#fallback:example.com").await.unwrap();

        let room_id = svc.get_room_id_by_alias("#fallback:example.com").await.unwrap();
        assert_eq!(room_id, Some("!room:example.com".to_string()));

        svc.remove_room_alias("#fallback:example.com").await.unwrap();
        let after = svc.get_room_id_by_alias("#fallback:example.com").await.unwrap();
        assert_eq!(after, None);
    }
}
