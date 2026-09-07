# 01: B-1.1 admin 踢人并发化 + 修 `let _ =` 吞错 + 分页游标

**What to build:** `evict_user_from_joined_rooms` 在管理后台把单个用户从所有已加入房间踢出。当前实现：一次性无界加载所有 room_id → 串行逐房间 `remove_member` → `let _ =` 静默吞 `decrement_member_counts_batch` 错误。修复后：(a) DB 读改为分页游标（LIMIT 1000），(b) `remove_member` 走 `buffer_unordered(8)` 限流并发（参考 synapse `evict_max_concurrency = 8`），(c) `let _ =` 改为 `tracing::warn!` 记录 batch 失败并让 caller 知道汇总可能陈旧。

**Blocked by:** None (can start immediately)

**Status:** done — implemented by commits `c60f388f` + `7d1f56a1` (2026-09-01/04)

**审计条目：** B-1.1 — admin 踢人并发化 + 修 `let _ =` 吞错 + 分页游标

## 验收（2026-09-07）

**实现**：
- `c60f388f`: `feat(admin-evict): B-1.1 admin 踢人并发化 + 分页游标 + 修 let _ 吞错`
- `7d1f56a1`: `perf(services): batch admin evict member_count updates (B-1.1)`
- DB 读改分页游标（`LIMIT 1000` + after_room_id 游标）
- `remove_member` 走 `buffer_unordered(8)` 限流并发
- `let _ = decrement_member_counts_batch(...)` 改为 `tracing::warn!` + failures 收集
- batch member_count 更新不再逐 room_id 单独 UPDATE

**Spec reference:**
- synapse `AdminHandler.shutdown_room` → `TaskScheduler` 派发 `_redact_all_events`（**异步 + 进度回报**）——本仓因 work 量小，**采用轻量同步 + 限流并发**而非完整 TaskScheduler
- synapse `evict_max_concurrency = 8` 默认值

**现状（已调研）:**
- 入口：`POST /_synapse/admin/v1/users/<user_id>/joined_rooms` → `evict_user_from_joined_rooms` (`synapse-services/src/admin_user_service.rs:221-245`)
- DB 读：`membership_storage.get_joined_rooms(user_id)` (`synapse-storage/src/membership/mod.rs:360`) 走 `SELECT room_id FROM room_memberships WHERE user_id = $1 AND membership = 'join'`，**无 LIMIT**，`fetch_all`
- 循环：`admin_user_service.rs:234-239` 逐 room_id 调 `member_storage.remove_member(...)`
- 批量更新：`admin_user_service.rs:241` `let _ = self.room_storage.decrement_member_counts_batch(&removed).await;` —— **静默吞错**
- 错误返回：`AdminUserEvictionResult { joined_rooms, failures }` 已包含 failures 字段（不是新增）

**实现计划（acceptance criteria）:**
- [ ] `membership_storage::get_joined_rooms` 加可选 `limit: i64` 参数，`get_joined_rooms_page(user_id, limit, after_room_id)` 走 `LIMIT $2 AND room_id > $3`（after_room_id 是游标，避免 OFFSET 性能陷阱）
- [ ] `evict_user_from_joined_rooms` 改为循环分页：每页 1000，循环到 fetch < 1000 为止
- [ ] `remove_member` 串行循环改为 `futures::stream::iter(...).map(...).buffer_unordered(8)`，`Arc` 共享 `self.member_storage`（已经是 `Clone`）
- [ ] `let _ = decrement_member_counts_batch(...)` 改为 `match ... { Ok(()) => {}, Err(e) => { tracing::warn!(error = %e, removed_count = removed.len(), "decrement_member_counts_batch failed; room_summaries.updated_ts may be stale"); failures.push(...) } }`
- [ ] `evict_max_concurrency: usize` 字段加到 `ServerConfig::server::*`（默认 8），`AdminUserService` 构造时读
- [ ] 集成测试：`tests/integration/admin_user_eviction_tests.rs` 加 4 个 case：(a) 单用户 5 房间应全部踢出 (b) 单用户 1500 房间分 2 页游标正常 (c) `decrement_member_counts_batch` 注入 DB 错误，验证 warn 日志 + failures 字段非空 (d) 并发 8 不超限

**风险/边界:**
- 风险低：纯重构 + 错误处理改进，不改对外契约
- 边界：分页游标必须用 `>` 而非 `OFFSET`（已有 `B-TODO: empty array guard` 同类教训）
- 边界：`decrement_member_counts_batch` 的 failures 现在是**不阻塞**的 warn（成员已成功移除，汇总陈旧是次要问题）

**工作量:** 0.5d
