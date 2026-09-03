# 06: ToDeviceService 切换到批量 user_exists 消除循环查询（P1-3）

**What to build:** 修改 `synapse-e2ee/src/to_device/service.rs` 中的多用户处理逻辑（行 47-51 附近），将 `for user in target_users { user_storage.user_exists(user).await }` 改为：先收集所有用户 ID，单次调用 `filter_existing_users()`，然后在内存中过滤。

> **注**：经探索发现 `UserStore` trait 已有 `filter_existing_users(&[String]) -> Vec<String>` 方法（`synapse-storage/src/user.rs:172`），使用 `WHERE user_id = ANY($1)` 批量查询，无需新增 trait 方法。

**Status:** ✅ done（cargo build --locked ✅，to_device 4/4 integration tests ✅）

- [x] 收集 target_users 的所有 user_id 到 Vec
- [x] 单次调用 `filter_existing_users(&all_user_ids).await`（使用已有方法）
- [x] 用返回 HashSet in-memory 过滤 target_users
- [x] 保留 warn 日志语义（用户不存在时记录 debug）
- [x] cargo build --locked 通过
- [x] 验证 to_device 集成测试通过（4/4）

## 验证

**cargo build（SQLX_OFFLINE=true）**：✅ synapse-e2ee 全编译通过

**to_device_sync_tests_migrated**：✅ 4/4 passed

**api_e2ee_advanced_tests**：✅ 3/3 passed（包含 to_device 端到端）

## 修改文件

| 文件 | 改动 |
|------|------|
| `synapse-e2ee/src/to_device/service.rs` | `send_messages` 改用 `filter_existing_users` 批量查询，移除 per-user `user_exists` 循环 |