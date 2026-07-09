# TODOS

> 按 skill/component 分组，按优先级排序（P0 最高 → P4 最低），已完成项移至底部 `## Completed` 节。

---

## Architecture Optimization (Round 2 延期项)

**Priority:** P2

- **C6 — 拆分 friend_room_service/mod.rs（1,827 行）**
  - 文件: `synapse-services/src/friend_room_service/mod.rs`
  - 按操作拆分为 friend_request.rs、friend_list.rs、direct_message.rs、sync.rs、tests.rs

- **C6 — 拆分 saml_service.rs → saml_service/ 目录（1,476 行）**
  - 文件: `synapse-services/src/saml_service.rs`
  - 转为目录模块，拆分 models.rs、service.rs、binding.rs、validation.rs、idp_manager.rs、tests.rs

- **C6 — 拆分 sync_service/tests.rs（1,859 行）**
  - 文件: `synapse-services/src/sync_service/tests.rs`
  - 按测试关注点拆分为 tests/ 子目录：filter_tests.rs、response_format_tests.rs、lazy_load_tests.rs 等

## Architecture Optimization (Round 3 未启动)

**Priority:** P2

- **C3 — 消除最后一个 too_many_arguments suppression**
  - 查找并移除 `#[allow(clippy::too_many_arguments)]`，用参数结构体重构

- **C2 — 提取 SyncService trait**
  - 为 SyncService 提取 `SyncServiceApi` trait，转换为 `Arc<dyn SyncServiceApi>`

- **C2 — 提取 RoomService trait**
  - 为 RoomService 提取 `RoomServiceApi` trait，转换为 `Arc<dyn RoomServiceApi>`

- **C4 — 拆分 room_service_tests_migrated.rs（4,867 行）**
  - 文件: `tests/integration/room_service_tests_migrated.rs`
  - 按测试主题拆分为子模块

- **C4 — 拆分其余 _migrated.rs 大文件**
  - 扫描 `tests/integration/` 下所有 `_migrated.rs` 文件，对超过 1,000 行的进行拆分

- **C6 — 为 /sync 端点添加快照测试**
  - 使用 insta 快照锁定 `/sync` API 响应格式

## Completed

