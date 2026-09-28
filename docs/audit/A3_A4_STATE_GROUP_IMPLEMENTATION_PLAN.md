# A3+A4: Per-event State Groups 实现计划

**目标**：完成 MSC4297 v2.1 的 per-event state group 实现，并为存量房间做 backfill

**依赖关系**：A4-ii 依赖 A3 完成

## 背景

当前状态（2026-09-28）：
- ✅ `state_groups` / `state_group_state` / `event_to_state_groups` 表已存在
- ✅ `StateGroupStorage` 提供了完整 CRUD
- ✅ `state_record.rs` 实现了分叉处理和 copy-forward
- ❌ **仅在分叉时**创建 state group，普通消息事件不绑定
- ❌ **缺少幂等/重放门禁**
- ❌ **缺少热路径性能门禁**
- ❌ `get_state_event/get_state_events` 仍保留时间戳推导 fallback

**上游参考**：element-hq/synapse release-v1.161
- 单父节点时复用父 group（避免不必要的 insert）
- 每个 event 都绑定到 state group
- 删除了纯时间戳推导的路径

---

## A3: Per-event state group + idempotency/replay/performance gates

### 实施范围

#### 1. 修改 `copy_forward` 逻辑（`state_record.rs:153-189`）

当前行为：
```rust
async fn copy_forward(&self, room_id: &str, event_id: &str, event_type: &str, state_key: &str) -> ApiResult<()> {
    let Some(current) = self.state_groups.get_room_state_groups(room_id, 1).await?...;
    
    // 只复制 state 条目，不绑定当前事件到 group
    // 返回 OK()
}
```

需要改为：
```rust
async fn copy_forward(&self, room_id: &str, event_id: &str, event_type: &str, state_key: &str) -> ApiResult<()> {
    let Some(current) = self.state_groups.get_room_state_groups(room_id, 1).await?...;
    
    // 1. 检查事件是否已绑定（幂等性）
    if self.state_groups.get_state_group_for_event(event_id).await?.is_some() {
        return Ok(()); // 已绑定，跳过
    }
    
    // 2. 将事件绑定到当前 group（复用父 group）
    self.state_groups.bind_event_to_state_group(event_id, current.id).await?;
    
    // 3. 可选：创建新的 group（如果需要追踪状态演进）
    // 当前设计：单父时复用，不创建新 group
    
    Ok(())
}
```

**验收判据**：
- ✅ 重复事件不创建重复 binding（幂等性）
- ✅ 单父节点时复用父 group（不创建多余的 state group）
- ✅ 每条消息写入时都调用 `bind_event_to_state_group`

#### 2. 添加消息事件的 binding（`messaging/events.rs`）

当前：消息事件返回前不调用 `state_record.after_state_event`

需要：在非状态事件中绑定到父 group

位置：`synapse-services/src/room/messaging/events.rs` 的 `create_event_with_graph`

```rust
// 在 persist_event 成功后（非状态事件）
if !is_state_event {
    // 查找当前 state group（如果有）
    if let Some(group_id) = state_groups.get_current_state_group_for_room(&room_id).await? {
        // 绑定消息事件到当前 group
        state_groups.bind_event_to_state_group(&event_id, group_id).await?;
    }
    // 如果没有 state group 记录，说明房间还没发生过分叉，
    // 此时不需要绑定（保持向后兼容）
}
```

**验收判据**：
- ✅ 每个消息事件都有机会绑定到 state group
- ✅ 不影响消息写入延迟（性能门禁见下）

#### 3. 事务边界保护（R9）

当前：`create_event_with_graph` 是单事务

需要在同一事务内完成：
1. INSERT INTO events
2. INSERT INTO event_to_state_groups（如果是分叉解析后的新 group）

位置：`synapse-services/src/room/messaging/events.rs`

```rust
let mut tx = pool.begin().await?;

// 1. 写入事件
insert_event(&mut tx, event).await?;

// 2. 如果是分叉后的新 state group，在事务内写入
if let Some(new_group) = resolve_fork_if_needed(&mut tx, room_id).await? {
    insert_state_group(&mut tx, new_group).await?;
    insert_state_group_state(&mut tx, new_group.id, entries).await?;
    bind_event_to_group(&mut tx, event_id, new_group.id).await?;
}

// 3. 提交
tx.commit().await?;
```

**验收判据**：
- ✅ 所有 writes 在同一事务内
- ✅ 任一失败自动 rollback
- ✅ 不存在"事件已提交但 state group 未写入"的中间态

#### 4. 幂等/重放门禁（新增测试）

文件：`tests/integration/state_groups_idempotency_tests.rs`

测试用例：
1. **重复 binding 不创建新 group**
   ```rust
   #[tokio::test]
   async fn test_duplicate_bind_is_noop() {
       // 两次 bind_event_to_state_group 应该：
       // - 第一次：INSERT
       // - 第二次：ON CONFLICT DO UPDATE（实际上值相同）
       // - 结果：只有一条记录
   }
   ```

2. **重复事件写入**
   ```rust
   #[tokio::test]
   async fn test_duplicate_event_does_not_duplicate_binding() {
       // 模拟重复投递同一 event_id
       // 结果：event_to_state_groups 只有一条记录
   }
   ```

3. **分叉后恢复的单条消息绑定**
   ```rust
   #[tokio::test]
   async fn test_message_after_fork_is_bound_to_group() {
       // 1. 创建分叉 → 创建 state group
       // 2. 发送普通消息 → 应该绑定到当前 group
       // 3. 验证：event_to_state_groups 有记录
   }
   ```

#### 5. 性能门禁（新增 benchmark）

文件：`benches/state_group_write_latency.rs`

基准测试：
```rust
#[bench]
fn bench_message_without_state_group(b) {
    // 基准：无 state group 时的消息写入延迟
}

#[bench]
fn bench_message_with_state_group(b) {
    // 有 state group 时额外增加 1x bind_event_to_state_group
    // 允许的最大延迟增长：< 5%
}
```

CI 门禁（新增）：
```yaml
# .github/workflows/perf-gate.yml
state-group-write-latitude:
  runs-on: ubuntu-latest
  steps:
    - cargo bench --bench state_group_write_latency
    # 检查延迟增长是否 < 5%
    - python scripts/perf/check_latency_drift.py
```

**验收判据**：
- ✅ 消息写入延迟不因 A3 劣化超过 5%
- ✅ 性能测试纳入 CI（防止回归）

---

## A4: Backfill state groups for existing rooms

### 问题分析

当前代码（`synapse-storage/src/event/state.rs:77-100`）：
```rust
pub async fn get_state_event(...) -> Result<Option<StateEvent>, sqlx::Error> {
    if let Some(state_group_id) = self.current_state_group_id(room_id).await? {
        // 有 state group，使用 group
        return Ok(...state_events_of_group...);
    }
    
    // ❌ 回退到时间戳推导
    sqlx::query_as!(...)
        .order_by("origin_server_ts DESC")
        .fetch_optional()
        .await
}
```

删除时间戳推导的前提：所有房间都有 state group

### 实施方案

#### 选项 1：一次性迁移脚本

文件：`scripts/migration/backfill_state_groups_v12_rooms.py`

```python
#!/usr/bin/env python3
"""
Backfill state groups for unforked v12 rooms.

This script:
1. Finds all v12+ rooms with no state_group records
2. Computes current state using timestamp derivation (one-time)
3. Creates initial state group for each room
4. Binds all state events to the group

Usage:
    python3 scripts/migration/backfill_state_groups_v12_rooms.py --dry-run  # preview
    python3 scripts/migration/backfill_state_groups_v12_rooms.py          # execute
"""

import asyncio
import asyncpg
from typing import List, Dict


async def find_unbackfilled_rooms(db_pool) -> List[str]:
    """Find v12+ rooms without state_group records."""
    return await db_pool.fetchvals("""
        SELECT r.room_id
        FROM rooms r
        JOIN room_versions rv ON r.room_version = rv.id
        WHERE r.room_version >= 12
          AND NOT EXISTS (
              SELECT 1 FROM state_groups sg WHERE sg.room_id = r.room_id
          )
    """)


async def compute_initial_state(db_pool, room_id: str) -> List[Dict]:
    """Compute current state using timestamp derivation (one-time)."""
    return await db_pool.fetch("""
        SELECT DISTINCT ON (event_type, state_key)
               event_id, event_type, state_key
        FROM events
        WHERE room_id = $1 AND state_key IS NOT NULL
        ORDER BY event_type, state_key, origin_server_ts DESC
    """, room_id)


async def create_initial_state_group(db_pool, room_id: str, state_events: List[Dict]):
    """Create initial state group and bind state events."""
    async with db_pool.acquire() as conn:
        async with conn.transaction():
            # 1. Create state group
            group_id = await conn.fetchval("""
                INSERT INTO state_groups (room_id, event_id, state_hash, created_ts)
                VALUES ($1, $2, $3, $4)
                RETURNING id
            """, room_id, state_events[-1]['event_id'], hash(state_events), int(time.time()*1000))
            
            # 2. Insert state_group_state
            await conn.executemany("""
                INSERT INTO state_group_state (state_group_id, event_type, state_key, event_id)
                VALUES ($1, $2, $3, $4)
            """, [(group_id, e['event_type'], e['state_key'], e['event_id']) 
                  for e in state_events])
            
            # 3. Bind each state event
            await conn.executemany("""
                INSERT INTO event_to_state_groups (event_id, state_group_id)
                VALUES ($1, $2)
            """, [(e['event_id'], group_id) for e in state_events])


async def main(dry_run: bool):
    db_pool = await asyncpg.create_pool("postgresql://synapse:synapse@localhost/synapse")
    
    rooms = await find_unbackfilled_rooms(db_pool)
    print(f"Found {len(rooms)} v12+ rooms without state groups")
    
    if dry_run:
        print("DRY RUN - no changes made")
        for room_id in rooms[:10]:  # Preview first 10
            print(f"  Would backfill: {room_id}")
        return
    
    for room_id in rooms:
        state_events = await compute_initial_state(db_pool, room_id)
        await create_initial_state_group(db_pool, room_id, state_events)
        print(f"✓ Backfilled: {room_id}")
    
    print(f"Done! Backfilled {len(rooms)} rooms")
```

**验收判据**：
- ✅ `--dry-run` 模式不修改数据
- ✅ 每间房的 backfill 在独立事务中（失败不影响其他房间）
- ✅ 完成后所有 v12+ 房间都有 state group

#### 选项 2：惰性补建（首次读取时）

在 `get_state_event` 中：

```rust
pub async fn get_state_event(...) -> Result<Option<StateEvent>, sqlx::Error> {
    // 1. 尝试读 state group
    if let Some(state_group_id) = self.current_state_group_id(room_id).await? {
        return Ok(self.state_events_of_group(...).await?);
    }
    
    // 2. 没有 state group，计算当前状态
    let state_events = self.get_state_events_by_timestamp(room_id).await?;
    
    // 3. 异步创建初始 state group（fire-and-forget）
    tokio::spawn(async move {
        if let Err(e) = create_initial_state_group_for_room(&pool, &room_id, &state_events).await {
            tracing::warn!(?room_id, ?e, "Failed to backfill state group");
            // 降级：继续用时间戳推导，日志记录
        }
    });
    
    // 4. 返回时间戳推导的结果
    Ok(state_events.into_iter().next())
}
```

**优点**：
- 不需要预先迁移
- 渐进式补建

**缺点**：
- 首次读取有额外开销
- 可能存在竞态条件

**推荐**：选项 1（一次性迁移），因为：
1. 可预知执行时间和影响面
2. 可提前测试
3. 不会产生运行时不确定性

#### 5. 删除时间戳推导（A4-ii 最终步骤）

在完成 backfill 后，修改 `get_state_event`：

```rust
pub async fn get_state_event(...) -> Result<Option<StateEvent>, sqlx::Error> {
    let state_group_id = self.current_state_group_id(room_id).await?
        .ok_or_else(|| ApiError::internal(format!("Room {room_id} has no state group")))?;
    
    // 不再有 fallback
    Ok(self.state_events_of_group(state_group_id, Some(event_type), Some(state_key)).await??.into_iter().next())
}
```

**验收判据**：
- ✅ 编译通过
- ✅ 所有现有测试通过（证明 backfill 正确）
- ✅ 新增测试：断言每个房间都有 state group

---

## 执行顺序

```mermaid
graph TD
    A[A3-1: 修改 copy_forward 绑定消息事件] --> B[A3-2: 添加幂等测试]
    B --> C[A3-3: 添加性能门禁]
    C --> D[A4-1: 创建 backfill 迁移脚本]
    D --> E[A4-2: dry-run 验证]
    E --> F[A4-3: 执行 backfill]
    F --> G[A4-4: 验证所有 v12+ 房间有 state group]
    G --> H[A4-5: 删除时间戳推导]
    H --> I[CI 门禁完整]
```

**预计工作量**：
- A3: 2-3 hours（代码修改 + 测试）
- A4: 1-2 hours（脚本 + 验证）
- 总计：3-5 hours

---

## 风险与缓解

| 风险 | 概率 | 影响 | 缓解措施 |
|------|------|------|----------|
| backfill 耗时长 | 中 | 低 | 分批次执行，每批 100 间房，间隔 1s |
| 事务锁竞争 | 低 | 中 | 在低峰期执行，设置锁超时 |
| 删除 fallback 后发现漏背填 | 低 | 高 | 保留旧代码 1 个 sprint，随时回滚 |
| 性能回归 | 中 | 中 | 性能门禁 + A/B 测试 |

---

## 测试清单

### 单元测试
- [ ] `test_duplicate_bind_is_noop`
- [ ] `test_message_after_fork_is_bound_to_group`
- [ ] `test_state_group_upsert_on_conflict`

### 集成测试
- [ ] `test_full_message_flow_with_state_group`
- [ ] `test_fork_resolution_creates_state_group`
- [ ] `test_backfill_restores_state_for_old_rooms`

### 性能测试
- [ ] `bench_message_without_state_group`
- [ ] `bench_message_with_state_group`
- [ ] Performance drift < 5%

### 迁移验证
- [ ] `backfill --dry-run` 输出正确的房间列表
- [ ] 执行 backfill 后查询验证：`SELECT COUNT(*) FROM state_groups WHERE room_version >= 12`
- [ ] 随机抽样房间对比：backfill 前后 `get_state_events` 结果一致

---

## 相关文件

### 修改的文件
1. `synapse-services/src/room/state_record.rs` - copy_forward logic
2. `synapse-services/src/room/messaging/events.rs` - message event binding
3. `synapse-storage/src/event/state.rs` - remove timestamp derivation
4. `synapse-storage/src/state_groups.rs` - 可能需要的 helper 方法

### 新增的文件
1. `tests/integration/state_groups_idempotency_tests.rs` - 幂等测试
2. `benches/state_group_write_latency.rs` - 性能基准
3. `scripts/migration/backfill_state_groups_v12_rooms.py` - backfill 脚本
4. `.github/workflows/perf-gate.yml` - 性能门禁 CI

### 文档更新
1. `docs/audit/ROOM_V12_UPSTREAM_ALIGNMENT_OPTIMIZATION_2026-09-28.md` - 更新 A3/A4 状态
2. `.workbuddy/memory/2026-09-28.md` - 记录实施进展

---

## 验收标准（Definition of Done）

- [ ] 所有单元测试通过
- [ ] 所有集成测试通过
- [ ] 性能测试 < 5% 延迟增长
- [ ] backfill 脚本 dry-run 验证正确
- [ ] backfill 执行完成，无报错
- [ ] 验证所有 v12+ 房间都有 state group
- [ ] 时间戳推导代码已删除
- [ ] 性能门禁加入 CI
- [ ] 文档已更新

**签字**：___________ 日期：___________

---

**最后更新时间**：2026-09-28 17:08
