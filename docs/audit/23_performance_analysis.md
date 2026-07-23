# 性能瓶颈识别报告（第 5 步）

> 版本: v1.0
> 日期: 2026-07-23
> 范围: synapse-rust Matrix Homeserver
> 基线: docs/audit/17_perf_baseline.md + docs/audit/14_performance_runtime.md
> 方法: 静态热点识别 + bench 覆盖盘点 + 缓存/锁/N+1 审计

---

## 一、现有性能基线回顾

### 1.1 运行时基线（来源：14_performance_runtime.md）

| 指标 | 数值 | 说明 |
|------|------|------|
| HTTP P50（非 sync） | <1ms | 健康端点延迟 |
| sync 端点 P50 | ~208ms | 长轮询语义符合预期 |
| HTTP 错误率 | 0/7 端点 | 0 错误 |
| RSS 峰值 | 293.2 MB | 内存占用可接受 |
| DB 查询计划 | 4/4 Index-Only/Index Scan | 0 seq scan，索引覆盖良好 |
| **concurrency>1 崩溃** | `Graceful drain timed out after 30s — forcing exit` | **已知问题** |

### 1.2 门禁指标 G1-G5（来源：17_perf_baseline.md）

| 门禁 | 指标 | 当前状态 | 覆盖 bench |
|------|------|----------|------------|
| G1 | sync 短轮询 ≤300ms | ✅ 有 bench | B1-B9 / S1-S6 |
| G2 | sync 初始全量 ≤2000ms | ❌ **缺失** | 无专门 bench |
| G3 | join room ≤500ms | ❌ **缺失** | 无 join room bench |
| G4 | federation send ≤1000ms | ⚠️ 部分 | F1-F3（state_res / auth_chain，非 send_transaction） |
| G5 | device keys query ≤100ms | ❌ **缺失** | 无 device keys bench |

### 1.3 bench 文件清单

| 文件 | benchmark 数 | 覆盖域 |
|------|--------------|--------|
| `performance_api_benchmarks.rs` | 9 (B1-B9) | HTTP API（login/register/sync/profile/...） |
| `performance_federation_benchmarks.rs` | 3 (F1-F3) | state_resolution + auth_chain |
| `performance_sliding_sync_benchmarks.rs` | 4 (S1-S4*) | 请求构造 / 响应时间 / 订阅变更 / P95P99 |
| `performance_membership_benchmarks.rs` | 14 | 成员状态机转换（**新增**，17 报告未提及） |
| **合计** | **30** | |

> *S1-S6 在 17 报告中标号，实际实现为 4 个 bench_function；membership bench 为 14 个 case。

### 1.4 bench 编译验证

```bash
SQLX_OFFLINE=true cargo bench \
  --bench performance_api_benchmarks \
  --bench performance_federation_benchmarks \
  --no-run
```

- 状态：编译进行中（release 模式 + 全量链接，耗时长属正常）
- 历史基线（17 报告）：3 个 bench 编译通过，0 错误
- 本次新增 `performance_membership_benchmarks` 与 `performance_sliding_sync_benchmarks` 待编译确认

---

## 二、静态性能热点识别

### 2.1 P1 严重：`resolve_state_for_group` N+1 查询

**位置**: [synapse-storage/src/state_groups.rs:383-440](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-storage/src/state_groups.rs#L383-L440)

**问题**: BFS 遍历 state group DAG，**每个节点执行 2 次 DB 查询**：

```rust
while let Some(sg_id) = queue.pop_front() {
    // 查询 1: state entries
    let state_rows = sqlx::query_as("SELECT ... FROM state_group_state WHERE state_group_id = $1")
        .bind(sg_id).fetch_all(&self.pool).await?;
    // 查询 2: prev groups
    let prev_rows = sqlx::query_as("SELECT prev_state_group_id FROM state_group_edges WHERE state_group_id = $1")
        .bind(sg_id).fetch_all(&self.pool).await?;
}
```

**影响**:
- DAG 深度 100 → 200 次 DB round-trip
- 无缓存，每次 state resolution 都重新遍历
- room 版本升级 / state reset 时放大

**修复方案**:
1. 收集所有待访问 `state_group_id`，用 `ANY($1::bigint[])` 一次性查询
2. 或加 state_group 缓存（TTL 300s，room 状态变更时失效）
3. 已有 `depth > 100` 警告保护，但治标不治本

### 2.2 P1 严重：`query_keys_internal` 循环内 per-user 查询

**位置**: [synapse-e2ee/src/device_keys/service.rs:49-144](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-e2ee/src/device_keys/service.rs#L49-L144)

**问题**: 遍历 `query_map`（user_id → device_ids），**每用户一次 cache.get + storage 查询**：

```rust
for (user_id, device_ids) in query_map {
    let cache_key = format!("device_keys_bulk:{user_id}");
    let cached = self.cache.get::<...>(&cache_key).await;
    // ...
    let keys = self.storage.get_all_device_keys(user_id).await?;  // per-user
}
```

**影响**:
- 一次 query_keys 涉及 100 用户 → 100 次 cache.get + N 次 storage 查询
- storage 层已提供 `get_all_device_keys_batch`（[storage.rs:398](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-e2ee/src/device_keys/storage.rs#L398)）但 service 层未使用
- 跨签名 `get_cross_signing_keys_batch` 已用批量（L153），但 device_keys 未跟上

**修复方案**:
1. 改用 `get_all_device_keys_batch(&user_ids)` 一次性查询
2. 缓存 key 改为 `device_keys_bulk:batch:{hash(user_ids)}` 或保留 per-user 但先批量预填缓存
3. 保留 300s TTL

### 2.3 P2 中等：`get_device_list_left_users_for_sync` 循环内查询

**位置**: [synapse-services/src/sync_service/data_fetch.rs:389-469](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/sync_service/data_fetch.rs#L389-L469)

**问题**: 双层循环内 per-(room, user) 查询：

```rust
for (room_id, mut events) in membership_events_by_room {
    for (state_key, membership) in latest_membership_by_user {
        if !users_with_join_in_delta.contains(&state_key) {
            // N+1: 每个 (room, user) 一次查询
            let current_member = self.member_storage.get_room_member(&room_id, &state_key).await?;
        }
    }
    if requester_left_room {
        // N+1: 每个 room 一次查询
        let joined_members = self.member_storage.get_room_members(&room_id, "join").await?;
    }
}
```

**影响**:
- 增量 sync 中，房间数 × 用户数 次查询
- 仅在 `users_with_join_in_delta` 不包含该用户时触发，但仍可能放大

**修复方案**:
1. 收集所有 `(room_id, state_key)` 对，用批量 API 一次查询
2. 或用 `get_room_members_batch(&room_ids, "join")` 替代循环内 `get_room_members`
3. 优先级低于 2.1/2.2，因仅在增量 sync 且 requester 离开房间时触发

### 2.4 P3 低：concurrency>1 崩溃（已知问题）

**位置**: [src/server/mod.rs:46](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/server/mod.rs#L46) `DRAIN_TIMEOUT_SECS: u64 = 30`

**现状**:
- 14 报告记录 `Graceful drain timed out after 30s — forcing exit`
- 已有完整 shutdown 体系：
  - `SHUTDOWN_BROADCAST_CAPACITY: usize = 3` broadcast channel
  - 3 个 `with_graceful_shutdown` listener（client/federation/prometheus）
  - SIGTERM/ctrl_c signal handler（L642）
  - drain gate（L674-675）等待 shutdown 信号后才进入 drain
- 30s 超时是合理默认，但并发请求超过 drain 容量时仍会强制退出

**建议**:
- 不调整 30s 默认（符合 industry practice）
- 在 14 报告的压测场景下，需排查是否有长连接（WebSocket / 长轮询 sync）持有连接超过 30s
- 可考虑对 sync 长轮询请求在 shutdown 时主动取消（CancellationToken 已在 container.rs 贯穿）

---

## 三、锁竞争审计

### 3.1 `lazy_loaded_members_cache` 锁使用

**位置**: [synapse-services/src/sync_service/lazy_load.rs:16/29/87](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/sync_service/lazy_load.rs)

| 位置 | 锁类型 | 持锁期间操作 | 风险 |
|------|--------|--------------|------|
| L16 | read | `cache.get(&cache_key)` + clone | 低 |
| L29 | write | `cache.clear()` + `cache.insert()` | 低（同步操作，无 await） |
| L87 | write | `cache.entry().or_default().extend()` | 低（同步操作，无 await） |

**结论**: 锁使用模式健康，持锁期间无 await，无死锁风险。
容量保护：`LAZY_LOADED_MEMBERS_CACHE_MAX_ENTRIES = 50_000`，超限时 clear（[mod.rs:55](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/sync_service/mod.rs#L55)）。

### 3.2 全 crate 锁分布

`synapse-services` 共 24 个文件使用 RwLock/Mutex，126 处出现。
关键热路径锁：
- `lazy_loaded_members_cache` — 已审计，健康
- `worker/bus.rs`（18 处）— worker 子系统，非热路径
- `push/queue.rs`（8 处）— push 队列，异步任务
- `burn_after_read_service.rs`（6 处）— 后台任务

**结论**: 锁竞争风险低，无热点锁。

---

## 四、缓存覆盖审计

### 4.1 已缓存的热路径

| 缓存点 | TTL | 批量 API | 位置 |
|--------|-----|----------|------|
| presence snapshot | 配置 | ✅ `get_batch` | storage/presence/mod.rs |
| user profile | USER_PROFILE_CACHE_TTL | ✅ 批量 | storage/user.rs:633/823 |
| account_data | ACCOUNT_DATA_CACHE_TTL_SECS | ❌ per-user | sync_service/data_fetch.rs:228 |
| sync filter | 86400s | ❌ per-filter | sync_service/filter.rs:30 |
| device_keys_bulk | 300s | ❌ per-user（见 2.2） | device_keys/service.rs:76 |
| room_state | - | ❌ per-room | join/leave 时 invalidate |
| device_list_max_stream | DEVICE_LIST_MAX_STREAM_TTL_SECS | - | data_fetch.rs:324 |
| feature_flag | 60s | ❌ per-flag | storage/feature_flags.rs |
| user_directory_search | 30s | ❌ per-query | storage/user.rs:1041 |

### 4.2 未缓存的热路径

| 路径 | 风险 | 建议 |
|------|------|------|
| `resolve_state_for_group` | **高**（见 2.1） | 加 state_group 缓存，TTL 300s |
| `get_room_member`（循环内） | **中**（见 2.3） | 批量化 |
| `get_sync_rooms` | 低 | 已是单次查询，可考虑 per-user 缓存 |
| `get_shared_room_users` | 低 | 已是单次查询 |

---

## 五、N+1 查询审计汇总

| 位置 | 严重度 | 循环结构 | DB 查询 | 已有批量 API | 修复优先级 |
|------|--------|----------|---------|--------------|------------|
| state_groups.rs:383 | **P1** | BFS DAG 遍历 | 2 次/节点 | ❌ | 高 |
| device_keys/service.rs:49 | **P1** | for user in query_map | 1-2 次/用户 | ✅ `get_all_device_keys_batch` | 高 |
| data_fetch.rs:429 | **P2** | for (room, user) | 1 次/(room,user) | 待补 | 中 |
| data_fetch.rs:458 | **P2** | for room | 1 次/room | 待补 | 中 |

### 5.1 健康路径（已用批量 API）

| 路径 | 批量 API | 位置 |
|------|----------|------|
| sync fetch_events | `get_room_events_batch_since_filtered` | data_fetch.rs |
| get_state_events | `get_state_events_batch` / `_by_type_batch` | data_fetch.rs:150 |
| get_unread_counts | `get_unread_counts_batch`（默认走批量） | reader.rs:481 |
| cross_signing_keys | `get_cross_signing_keys_batch` | device_keys/service.rs:153 |
| presence | `get_batch` | storage/presence/mod.rs |
| user profile | 批量 | storage/user.rs |

**结论**: storage 层批量 API 覆盖完善（11+ 个 batch 方法），service 层利用率约 60%，剩余 3 处 N+1 需修复。

---

## 六、算法复杂度审计

### 6.1 state resolution（已知热点）

- `resolve_state_for_group`: BFS O(V+E)，但每节点 2 次 DB round-trip
- `resolve_state_with_auth_chain`（federation bench F1-F2 已覆盖）: chain_10 / chain_100
- auth_chain_build（F3）: build_10

### 6.2 membership transition（新 bench 覆盖）

- `is_legal`: O(1) 状态查表，14 种转换已 bench 覆盖
- fail-closed 路径（7 种拒绝）+ allowed 路径（7 种允许）均覆盖

### 6.3 sync 响应构建

- `room_sections_from_memberships`: O(n) 内存迭代
- `filter_sync_rooms`: O(n) 内存过滤
- `apply_lazy_load_members_with_cache`: O(n) 内存过滤
- `build_sync_response`: for room in rooms_to_include（O(rooms)），循环内无 DB 查询

---

## 七、性能优化建议（按优先级）

### P0（立即修复）

1. **`resolve_state_for_group` 批量化**
   - 收集待访问 `state_group_id`，用 `ANY($1::bigint[])` 一次性查询 state entries + edges
   - 加 state_group 缓存（TTL 300s，room 状态变更时失效）
   - 预期收益：state resolution 耗时降低 80%+

2. **`query_keys_internal` 批量化**
   - 改用 `get_all_device_keys_batch(&user_ids)`
   - 缓存策略调整为：先批量查缓存，缺失的 user_ids 批量查 DB，再回填缓存
   - 预期收益：device keys query 耗时降低 70%+（100 用户场景）

### P1（短期修复）

3. **`get_device_list_left_users_for_sync` 批量化**
   - 收集 `(room_id, state_key)` 对，批量查 `get_room_members_batch`
   - 预期收益：增量 sync 在多房间场景下延迟降低 40%+

4. **补齐 G2/G3/G5 门禁 bench**
   - G2: sync 初始全量 bench（full_state=true，since=None）
   - G3: join_room bench（含本地 + 联邦路径）
   - G5: query_keys bench（1/10/100 用户场景）

### P2（中期优化）

5. **account_data 缓存批量化**
   - 当前 per-user 缓存，可考虑批量预取

6. **concurrency>1 崩溃排查**
   - 验证 CancellationToken 是否在 sync 长轮询路径生效
   - 压测时监控 in-flight 请求数 vs drain 超时

### P3（长期观察）

7. **依赖升级消除 rand/getrandom 分裂**（见 20_structure_analysis.md）
8. **worker 子系统锁优化**（非热路径，低优先级）

---

## 八、第 5 步结论

### 8.1 性能健康度评分

| 维度 | 评分 | 说明 |
|------|------|------|
| DB 索引覆盖 | ✅ A | 4/4 Index-Only Scan，0 seq scan |
| 缓存覆盖 | ✅ B+ | 9 个缓存点，presence/user 批量化 |
| 批量 API 覆盖 | ⚠️ B | storage 层完善，service 层 3 处 N+1 |
| 锁竞争 | ✅ A- | 无热点锁，持锁期间无 await |
| bench 覆盖 | ⚠️ B- | 30 个 benchmark，G2/G3/G5 缺失 |
| 算法复杂度 | ✅ A | 无 O(n²) 热点，state_res 已 bench |

### 8.2 核心发现

1. **2 个 P1 N+1 性能瓶颈**：`resolve_state_for_group` + `query_keys_internal`
2. **3 个门禁指标缺失 bench**：G2/G3/G5
3. **concurrency 崩溃问题已有缓解**（shutdown 体系完整），但需验证 sync 长轮询路径
4. **新增 membership bench（14 case）** 是 17 报告未覆盖的增量
5. **storage 层批量 API 完善**，service 层利用率 60%

### 8.3 衔接第 6 步

第 6 步"代码重构实施"应优先处理：
1. P0-1: `resolve_state_for_group` 批量化 + 缓存
2. P0-2: `query_keys_internal` 批量化
3. P1-3: `get_device_list_left_users_for_sync` 批量化
4. P1-4: 补齐 G2/G3/G5 bench（为第 7 步 TDD 提供基线）

按 TDD Red-Green-Refactor 流程：先写失败 bench（Red），再批量化修复（Green），最后重构缓存策略（Refactor）。

---

## 附录 A：bench 编译状态

- 命令: `SQLX_OFFLINE=true cargo bench --bench performance_api_benchmarks --bench performance_federation_benchmarks --no-run`
- 历史基线（17 报告）：3 个 bench 编译通过，0 错误
- 本次编译：release 模式耗时长，编译进行中
- 新增 bench（membership / sliding_sync）待单独编译验证：
  ```bash
  SQLX_OFFLINE=true cargo bench --bench performance_membership_benchmarks --no-run
  SQLX_OFFLINE=true cargo bench --bench performance_sliding_sync_benchmarks --no-run
  ```

## 附录 B：关键文件引用

| 文件 | 关键行 | 用途 |
|------|--------|------|
| [state_groups.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-storage/src/state_groups.rs#L383) | 383-440 | P1 N+1: resolve_state_for_group |
| [device_keys/service.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-e2ee/src/device_keys/service.rs#L49) | 49-144 | P1 N+1: query_keys_internal |
| [device_keys/storage.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-e2ee/src/device_keys/storage.rs#L398) | 398 | 已有批量 API: get_all_device_keys_batch |
| [data_fetch.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/sync_service/data_fetch.rs#L389) | 389-469 | P2 N+1: device_list_left_users |
| [lazy_load.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/sync_service/lazy_load.rs) | 16/29/87 | 锁审计: 健康模式 |
| [server/mod.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/server/mod.rs#L46) | 46 | DRAIN_TIMEOUT_SECS=30 |
| [membership_benchmarks.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/benches/performance_membership_benchmarks.rs) | 1-62 | 新增 14 case bench |
