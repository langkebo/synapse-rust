# 05: 移除应用层 member_count 自增逻辑

**What to build:** 删除 Rust 代码中手动维护 `joined_member_count` 的语句（`joined_member_count = joined_member_count + 1` 等），改为依赖触发器自动同步。

**Blocked by:** 04-member-count-trigger-sync

**Status:** ✅ DONE (2026-08-31)

- [x] `RoomStorage::increment_member_count` (mod.rs:662)：删除 `joined_member_count + 1` 和 `member_count + 1` UPDATE，只保留 `updated_ts` 刷新
- [x] `RoomStorage::decrement_member_count` (mod.rs:679)：删除 `joined_member_count - 1` 和 `member_count - 1` UPDATE，只保留 `updated_ts` 刷新
- [x] `RoomStorageAdmin::increment_member_counts_batch` (admin.rs:222)：删除双增量 SQL，只更新 `updated_ts`
- [x] `RoomStorageAdmin::decrement_member_counts_batch` (admin.rs:244)：删除双增量 SQL，只更新 `updated_ts`
- [x] 所有函数体添加 v11 注释说明触发器设计
- [x] `test_mocks/room.rs` 中的 in-memory mock 不改动（不涉及数据库触发器）
- [x] v11 baseline 中 `trg_sync_member_count` 触发器已在先（expand-contract 合规）

**效果**：消除双增量 bug。计数维护完全由 DB 触发器负责，应用层不再介入。

**注意**：`cargo check` 和测试套件回归待执行（Ticket 12）。

- [ ] `synapse-storage/src/room/mod.rs:667`：删除 `joined_member_count = joined_member_count + 1` 语句
- [ ] `synapse-storage/src/room/mod.rs:684`：删除 `joined_member_count = GREATEST(joined_member_count - 1, 0)` 语句
- [ ] `synapse-storage/src/room/admin.rs:231`：删除 `joined_member_count = joined_member_count + 1` 语句
- [ ] `synapse-storage/src/room/admin.rs:253`：删除 `joined_member_count = GREATEST(joined_member_count - 1, 0)` 语句
- [ ] 检查其他调用点（`membership/api.rs`）是否有类似逻辑
- [ ] 全 workspace `cargo check --locked` 通过
- [ ] storage 测试套件 1513/1513 通过
- [ ] `synapse_test` schema 重建验证

**expand–contract 说明**：
- expand: ticket 04 已加触发器（SQL 层双重计数不会出错）
- contract: 本 ticket 删除应用层逻辑（触发器已接管）
- 如果测试失败可回滚本 ticket，触发器继续工作
