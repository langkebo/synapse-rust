# 冗余清理变更日志

## 2026-05-28

- **清理工作启动**
  - **目标**: 启动全面冗余清理工作，优先处理 DM/Friend 领域，并记录所有变更。
  - **核验范围**: 本地及 origin 所有活跃分支。
  - **当前进展**: 
    - 创建冗余清理变更日志文件。
    - **收缩 DMService**: 将 `dm_service` 设为 `pub(crate)` 并限缩至 `#[cfg(any(test, feature = "test-utils"))]`，移出 `services::*` 公开 re-export。
    - **清理冗余 Cache 实现**: 彻底移除 `src/services/cache/` 目录及相关模块（`CacheService`, `RoomSummaryCache` 等），这些实现已被 `crate::cache::CacheManager` 取代且已无活跃引用。
    - **清理冗余 MessageQueue 实现**: 彻底移除 `src/services/message_queue/` 目录，已被 `RedisTaskQueue` 取代。
    - **限缩 TaskQueue/BackgroundTaskManager**: 将 `src/common/task_queue.rs` 中的内存版任务队列限缩至 `#[cfg(test)]`，生产环境统一使用 `RedisTaskQueue`。
    - **dm.rs 路由层收敛**: 进一步简化 `dm.rs` 中的编排逻辑：
      - `update_dm_room` 统一通过 `load_direct_room_snapshot` 委托给服务层。
      - `create_dm_room` 彻底收敛，将复杂的 fallback 编排逻辑移入 `create_dm_room_via_service` 内部。
      - `get_dm_rooms` 已收敛至服务层 `get_effective_direct_map`。
      - 所有路由层辅助函数（`load_effective_direct_map`, `upsert_direct_room_links`, `load_dm_partner_info`, `find_existing_direct_room_id` 等）均已添加 `#[cfg(not(feature = "friends"))]` 隔离。
    - **修复测试与警告**:
      - 修复了 `membership_storage_tests.rs` 中 68 处因 `add_member` 签名变更（增加事务参数）导致的编译错误。
      - 修复了 `dm_service_tests.rs` 中的可见性与类型推断错误，确保单元测试集通过 `cargo check`。
      - 抑制了 `dm_service.rs` 中的 `dead_code` 警告。
      - 确认并验证了 DM 核心测试（幂等性、账号数据持久化、好友 DM 复用）在清理后依然通过。
