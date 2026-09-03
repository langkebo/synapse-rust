# 02: MSC4267 Forget on Leave 合规实现 (P0, 2-3d)

**What to build:** 实现 MSC4267——用户在离开房间时带 `forget: true` 标志，服务器在同一数据库事务内完成 leave + forget（清理 membership、device_keys、room_state_cache、push rules、tags），不留两步走的安全窗口。

**Blocked by:** None

**Status:** ✅ done — commit `fadf125e`

**验证记录:**
- `cargo build --locked` ✅ (2m58s)
- `cargo clippy --workspace --all-features --locked -- -D warnings` ✅ 0 警告
- `cargo test --lib -p synapse-services --features test-utils -- room::membership` → 80/80 PASS
- `cargo test --lib -p synapse-storage --features test-utils -- membership::` → 35/35 PASS
- capability `m.forget_forced_upon_leave: true` ✅ (capability_governance.rs)
- snapshot `capabilities_v3.snap` → `"enabled": true` ✅

**关键设计决定:**
- `MemberStoreApi::remove_member` / `forget_member` 增 `tx: Option<&mut sqlx::Transaction<...>>` 参数；所有 6 个 service-layer caller 传 `None`（保持原行为）；`leave_and_forget` 传 `Some(&mut tx)`
- `leave_and_forget` 中 pool==None 时（mock 环境）fallback 到非原子两调用路径，现有 in-memory 测试不受影响
- `is_remote_room` 拒绝组合（联邦 leave 是单独事务，不能与本地 forget 合并）—— 400 错误提示用户先 leave 远程房间再手动 /forget
- post-commit 工作（leave event、cache 失效、federation 广播、megolm 轮转）均在 tx.commit() 之后执行，确保 DB 权威状态先一致

**遗留:** federation race（远程 leave 事件在 forget 后到达）→ event idempotency check 留待后续 sprint（见 ticket 风险点）

**Spec reference:** https://github.com/matrix-org/matrix-spec-proposals/blob/main/proposals/4267-forget-on-leave.md

**现状（已调研）:**
- `/rooms/{roomId}/leave` 端点：`src/web/routes/room.rs`（→ handlers/room/members.rs）
- `/rooms/{roomId}/forget` 端点：独立存在（`src/web/routes/room.rs`）
- storage: `synapse_storage` 有 `forget_room(user_id, room_id)` 方法
- **缺口 1**: leave 端点完全**没有**解析 `forget: true` 字段
- **缺口 2**: leave + forget 是两个独立的 DB transaction，有 race 窗口
- **缺口 3**: `m.forget_forced_upon_leave = false` 在 capability 返回中明确声明不可用（需要翻成 true）

**实现计划（acceptance criteria）:**
- [ ] `src/web/routes/room.rs` / `handlers/room/members.rs`: leave handler 解析 `body.forget: bool`（默认 false）
- [ ] `synapse_services/src/room_service.rs` 或 `room_members_service.rs`: 新增 `leave_and_forget(user_id, room_id)` 方法，在同一事务内：
  - [ ] 插入 leave membership event
  - [ ] 调用 `forget_room` 清理 `device_keys`、`current_state_events`、`push_rules`、`tags`
  - [ ] 删除 `room_memberships` 行（或标记 forgotten）
- [ ] `capability_governance.rs`: `m.forget_forced_upon_leave: true`（从 false 翻 true）
- [ ] `migrations/`: 如需新表/列，新增 migration
- [ ] **联邦安全**: 确认 `federation/membership/leave.rs` 不走 forget 路径（联邦 leave 是通知远程 server，不需要/不应该 forget）
- [ ] 集成测试：
  - [ ] 普通 leave（forget=false）：用户仍可见 room 成员列表
  - [ ] leave with forget=true：同一事务内 forget 生效
  - [ ] forget 重复调用幂等（已 forget 的 room 再 forget 无害）
  - [ ] 大房间 forget 事务边界（batch 删除，避免单个大事务撑爆）
- [ ] `cargo build --locked`
- [ ] `cargo clippy --all-features --locked -- -D warnings`
- [ ] 提交 commit `feat(room): MSC4267 forget-on-leave (leave + forget in single transaction)`

**风险点:**
- ⚠️ **联邦事件循环 race**: 本地 forget 后，远程 server 的 leave 事件可能延迟到达并在本地重建 membership。需要 event saga 机制（`synapse-common` 有 task queue）或 event_idempotency 处理。
- ⚠️ **大房间 forget**: 大型房间 `current_state_events` 可能数万行，`DELETE FROM current_state_events WHERE room_id = $1` 需要事务 timeout 保护。
- ⚠️ **WebSocket 长连接**: forget 后是否需要向已订阅该 room 的 WebSocket push 连接发送事件（技术上不需要，但 Element Web 有 UX 期望）。
- ⚠️ **回归**: 当前 forget 端点行为必须保持不变（用户显式 forget 仍然是两步走），只改变 leave+forget 的语义。

**估算:** 2-3 人天（基础实现 2d，大房间事务边界 + 联邦 saga 3d）
