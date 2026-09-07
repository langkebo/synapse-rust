# 03: B-1.4 friend fan-out link 读并发 + 接受 Vec<room_id> 单 SQL

**What to build:** `sync_dm_room_membership_change` 在好友 DM 房间变化时 fan-out 到所有持有该 DM 的 friend list。当前实现：外层 link 串行循环（每 link 一次 `get_friend_list_all_shards` DB 读）—— 28 shard fan-out 路径下，单 friend DM 变化可能要 N 次顺序 SQL 往返。修复后：(a) `get_friend_list_all_shards` 接受 `&[String]` 一次查多 room_id（单 SQL `WHERE room_id = ANY($1)`），(b) 外层 link 处理本身**保持串行**（保持 `&self` 借用不变性），但 DB 读通过一次 SQL 拿全量后**内部 shard 解析并发**。

**Blocked by:** None (can start immediately)

**Status:** done — implemented by commit `9344544a` (2026-09-04)

**审计条目：** B-1.4 — friend fan-out link 读并发 + 接受 Vec<room_id> 单 SQL

## 验收（2026-09-07）

**实现（commit `9344544a` 2026-09-04）**：
- `9344544a`: `feat(friend-fanout): B-1.4 friend DM link fan-out 单 SQL 批读`
- `synapse-services/src/friend_room_service/mod.rs:1279`：注释明确 "After: one `get_friend_list_all_shards_batch` SQL returns all"
- `synapse-services/src/friend_room_service/mod.rs:1297`：调用 `get_friend_list_all_shards_batch(&room_ids)` 一次拿全
- DB 层：单 SQL `SELECT ... WHERE room_id = ANY($1)` + 空数组守卫
- 早期 commit `22a689bd` 已用 `try_join_all` 让 state event 写并发

**Spec reference:**
- synapse `synapse/storage/databases/main/devices.py` — `account_data.MAX_USERS_PER_ACCOUNT_DATA_UPDATE = 100` 批大小限制
- synapse `account_data._get_account_data_for_user_in_room` — **单 SQL 一次拿所有目标**，不用逐 key 循环
- synapse 写路径用 `UNNEST($1::text[])` 把 array 平铺到 VALUES，单 INSERT 处理 N 条

**现状（已调研）:**
- 入口：`friend_room_service::sync_dm_room_membership_change` (`synapse-services/src/friend_room_service/mod.rs:1226`)
- 关键循环：`mod.rs:1257-1267` — `for link in links { let shards = self.friend_storage.get_friend_list_all_shards(&link.friend_room_id).await?; ... }`
- 注释明确："B-1.4: Phase 1 — sequentially fan out DB reads"，Phase 2 state event 写已用 `try_join_all` 并发
- DB 函数：`get_friend_list_all_shards(room_id: &str)` (in `synapse-storage/src/friend_room.rs` 待查具体行号) — 当前**单 room_id 入参**
- 借用意图：注释"holds only `&self` at any point (no concurrent mutable borrow of `self`)"—— 即**外层 link 循环不能直接改并发**，但 DB 读单 SQL 化是突破点

**实现计划（acceptance criteria）:**
- [ ] `friend_storage::get_friend_list_all_shards` 签名加 `get_friend_list_all_shards_batch(room_ids: &[String]) -> Result<HashMap<String, Vec<(String, Value)>>>`
  - DB 层：单 SQL `SELECT room_id, state_key, content FROM friend_list_shards WHERE room_id = ANY($1)`（参照已有 `B-TODO: room_id = ANY($1) empty array guard` 教训，加 `if room_ids.is_empty() { return Ok(HashMap::new()) }`）
  - 内存层：按 `room_id` 聚合 `(state_key, content)` 对
- [ ] 保留旧 `get_friend_list_all_shards(room_id: &str)` 作为单条便捷方法（**expand** 阶段，迁完所有 caller 再删）—— 这是 expand–contract 序列
- [ ] `sync_dm_room_membership_change` 改用 `_batch`：单 SQL 拿全量 → 内存中按 link 重组 shards → 后续循环不变
- [ ] 内部 shard 解析并发：每个 `(link, state_key, shard_content)` 处理走 `buffer_unordered(4)`（limit 4 防止 28 shard × N link 的内存爆涨）
- [ ] 集成测试：`tests/integration/friend_room_sync_tests.rs` 验证 5 link × 28 shard fan-out：(a) DB 读次数 = 1（不是 5），(b) 最终写入 shard 数等于 SQL 读到的全部 shard 数，(c) shard_content 解析正确
- [ ] 性能 benchmark（可选）：`.scratch/.../friend_fanout_bench.txt` 记录旧 vs 新 wall time 对比

**风险/边界:**
- 风险中：28 shard 是性能敏感路径，回归测试必跑 `friend_room_service` integration
- 边界：必须**保留**单条 `get_friend_list_all_shards`，不要破坏其他 caller
- 边界：DB 读 SQL 用 `ANY($1)` 而非 `IN`，避免参数展开问题
- 边界：空数组守卫（已有 `B-TODO: room_id = ANY($1) 空数组守卫` 教训）
- 边界：buffer_unordered 限流 4 而不是 8——28 shard × N link 的扇出更激进

**工作量:** 1d
