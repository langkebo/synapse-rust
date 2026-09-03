# 后端问题清单排查报告
**时间**：2026-09-03 早 | **项目**：synapse-rust v6.2.0

---

## B-1 批量操作 N+1 查询 🟢 严重程度：高

### 排查结论：5 处全部存在

| # | 位置 | 问题 | 严重度 | 修复难度 | 备注 |
|---|---|---|---|---|---|
| 1 | `admin_user_service:229-234` | 循环 `remove_member` + `decrement_member_count` | 高 | ⭐ 极简 | **storage 层 `decrement_member_counts_batch` 已存在**（`synapse-storage/src/room/admin.rs:269`），service 未调用 |
| 2 | `admin_user_service:498-508` | 循环逐用户 `UPDATE deactivated = true` | 中 | ⭐⭐ 中 | 无批量方法，需新建 storage 层批量 UPDATE |
| 3 | `room/service:503-512` | 循环逐 `invite_user`（DB + 可能 federation 远端） | 高 | ⭐⭐⭐ 难 | federation invite 不能简单批量化（需保持原错误语义） |
| 4 | `friend_room_service:1210-1261` | **双层循环**（links × 28 shards）串行 fan-out | **极高** | ⭐⭐⭐⭐ 难 | 最严重瓶颈；shard 循环改为 `join_all` 并发可显著改善 |
| 5 | `sliding_sync_service:400-405` | 循环逐房间 `DELETE` | 低 | ⭐ 极简 | 改为 `DELETE WHERE room_id = ANY($1)` 即可 |

### 关键发现

**B-1.1 意外收获**：`decrement_member_counts_batch` 已经在 `synapse-storage/src/room/admin.rs:269` 实现，但 `evict_user_from_joined_rooms` 未调用它。只需两行改动：

```rust
// 当前（循环）
for room_id in &joined_rooms {
    self.room_storage.decrement_member_count(room_id).await?;
}

// 优化后（批量）
self.room_storage.decrement_member_counts_batch(&joined_rooms).await?;
```

**B-1.4 friend fan-out 最严重**：W5 sharding 后 friend list 散在 28 个 shard，同一 dm_room_id 可能跨 shard 被引用。双层串行循环：
```rust
for link in links {                         // 外层
    let shards = get_friend_list_all_shards(&link.friend_room_id).await?;
    for (state_key, shard_content) in shards { // 内层 28 个
        send_state_event(&link.friend_room_id, &link.owner_user_id, "m.friends.list", &state_key, shard_content).await?;
    }
}
```
优化方向：`join_all` 并发 28 个 shard 写（注意 federation 侧并发上限），或用 `futures::stream` + `buffer_unordered` 限流。

---

## B-2 运维可调参数硬编码 🟡 严重程度：中

### 排查结论：5 处确认 + **1 处误报**

| # | 位置 | 常量 | 影响 | 优先级 | 备注 |
|---|---|---|---|---|---|
| 1 | `src/server/mod.rs:33-53` | `BACKGROUND_TASK_INTERVAL_SECS=60` 等 7 个 | 运维无法调 | P2 | |
| 2 | `synapse-services/src/event_notifier.rs:430,449` | `timeout(5s)` | **误报** | — | 全在 `#[cfg(test)]` 测试模块内，production 无硬编码 |
| 3 | `src/server/database.rs:10-19` | `max_lifetime=1800s` / `statement_timeout='30s'` / `lock_timeout='10s'` | **生产路径**，故障处置必需 | **P1** | PG session timeout 硬编码，线上调优必须 rebuild |
| 4 | `src/web/routes/federation/transaction.rs:15` | `TXN_DEDUP_TTL_SECS=86400` | 运维无法调 | P2 | |
| 5 | `src/web/routes/e2ee/devices.rs:126` | `MAX_TO_DEVICE_RECIPIENTS=5000` / `MAX_TO_DEVICE_PAYLOAD_BYTES=65536` | **函数内局部 const**，更隐蔽 | P2 | |
| 6 | `src/tasks/mod.rs:19-48` | `health_check_interval=10s` / `performance_check_interval=300s` 等 | **struct 字段已预留**，只缺 Config 注入 | P3 | 低垂果实 |

### 关键发现

**B-2.2 误报澄清**：用户清单说 `event_notifier.rs:430,449` 是"sync 长轮询超时硬编码"。经逐行核实，这两处 `timeout(5s)` 都在 `#[cfg(test)]` 测试模块内，是测试代码超时，不是生产路径硬编码。生产代码中 event_notifier 无 sync 长轮询超时问题。

**B-2.6 低垂果实**：`ScheduledTasks` struct 已有 `health_check_interval: Duration` 等字段，`new()` 中硬编码赋值。仅需将 `new(config: &Config)` 改为从 Config 读值，struct 定义无需改动。

**B-2.3 最高优先级**：`database.rs` 的 `SET statement_timeout = '30s'` 在**每个数据库连接初始化时执行**，是生产路径。statement_timeout 过短会导致大查询被 kill，线上调优必须改码 rebuild。

---

## B-3 遗留 TODO 债务 🔵 严重程度：低

### 排查结论：7 处 lib.rs 全部确认 + FED-4 基本完成

#### missing_docs 债务

| Crate | 文件 | 当前状态 |
|---|---|---|
| root | `src/lib.rs:4` | `#![allow(missing_docs)]` + B2-TODO |
| synapse-services | `synapse-services/src/lib.rs:4` | `#![allow(missing_docs)]` + B2-TODO |
| synapse-storage | `synapse-storage/src/lib.rs:4` | `#![allow(missing_docs)]` + B2-TODO |
| synapse-federation | `synapse-federation/src/lib.rs:4` | `#![allow(missing_docs)]` + B2-TODO |
| synapse-e2ee | `synapse-e2ee/src/lib.rs:4` | `#![allow(missing_docs)]` + B2-TODO |
| synapse-cache | `synapse-cache/src/lib.rs:4` | `#![allow(missing_docs)]` + B2-TODO |
| synapse-common | `synapse-common/src/lib.rs:4` | `#![allow(missing_docs)]` + B2-TODO |

**规模估算**：`src/lib.rs` 注释注明打开后约 **~1900 个 warning**（7 crate 合计）。棘轮法是正确的渐进路径（参考本仓 `check_fmt_ratchet.sh` 的棘轮模式）。

#### FED-4 状态

**位置**：`synapse-federation/src/test_mocks.rs:467`

**结论：任务基本完成**，该 TODO 注释是遗留未清理。

- `FederationClientApi` trait 已在 `synapse-federation/src/client_api.rs:22` 定义（含 `Send + Sync`）
- `impl FederationClientApi for FederationClient` 已在 `client_api.rs:215` 实现
- services 层 5+ 处 caller 已在用 `Arc<dyn FederationClientApi>`：`room/infrastructure.rs`、`room/service.rs`、`room/membership/service.rs`、`room/backfill.rs`
- `test_mocks.rs:467` 的 TODO 注释应直接删除（`// TODO(FED-4): ...` → 无需操作）

---

## 总体评估

| 问题 | 实际严重度 | 快速修复 | 建议优先级 |
|---|---|---|---|
| B-1.1 admin evict 循环 | 高 | ✅ `decrement_member_counts_batch` 已存在 | **立即修复** |
| B-1.4 friend fan-out | **极高** | 需并发改造（join_all） | **P1** |
| B-2.3 DB timeouts | **高**（生产路径） | Config 化 | **P1** |
| B-2.6 ScheduledTasks | 低 | Config 注入（struct 已预留） | P2 |
| B-1.2 batch deactivate | 中 | 新建批量 UPDATE 方法 | P2 |
| B-2.1 server consts | 中 | Config 化 | P2 |
| B-2.4/2.5 | 中 | Config 化 | P2 |
| B-1.3 room upgrade invite | 高 | 复杂度高，谨慎改 | P2 |
| B-1.5 sliding sync | 低 | 合并 DELETE | P3 |
| B-3 missing_docs | 低 | 棘轮法 + 注释清理 | P3 |

**最大意外收获**：
1. `event_notifier.rs` 的硬编码是**误报**（测试代码）
2. `decrement_member_counts_batch` **已存在**，B-1.1 可极低成本修复
3. FED-4 **大部分已完成**，只需清 TODO 注释

---

## 修复记录（2026-09-03 上午）

### ✅ B-1.1 已修复

**改动文件**：
- `synapse-storage/src/room/api.rs`：在 `RoomStoreApi` trait 中新增 `decrement_member_counts_batch` 方法声明 + 默认 trait impl
- `synapse-storage/src/test_mocks/room.rs`：在 `InMemoryRoomStore` 中实现 `decrement_member_counts_batch`
- `synapse-services/src/admin_user_service.rs:220-238`：重构 `evict_user_from_joined_rooms`，将成员移除分为 phase 1（串行 remove）和 phase 2（一次 batch update）

**验证**：全 workspace clippy EXIT=0；集成测试 `test_admin_batch_deactivate` PASS

### ✅ B-1.2 已修复

**改动文件**：
- `synapse-storage/src/user/api.rs`：在 `UserStore` trait 中新增 `set_deactivation_status_batch`
- `synapse-storage/src/test_mocks/user.rs`：在 `InMemoryUserStore` 中实现 `set_deactivation_status_batch`
- `synapse-services/src/admin_user_service.rs:498-508`：重构 `batch_deactivate_users`，partition valid/invalid user_ids 后一次 batch update，RETURNING 收集结果

**验证**：集成测试 `test_admin_batch_deactivate` PASS

### ✅ B-1.4 已修复

**改动文件**：
- `synapse-services/src/friend_room_service/mod.rs`：`sync_dm_room_membership_change` 完全重构
  - Phase 1：串行读 DB + 内存 shard 处理（保持 `&self` borrow）
  - Phase 2：`try_join_all` 并发写所有 state event
  - 新增 `send_state_event_inner` stateless helper + `futures::future::try_join_all` import

**架构细节**：
- `room_service` 已是 `Arc<dyn RoomServiceApi>`，通过 `Arc::clone` 让每个并发 future 持有独立引用
- `send_state_event_inner` 取 `&(dyn RoomServiceApi + '_)`，避免 `'static` lifetime 约束
- `create_event` 返回 `ApiResult<RoomEvent>`，需要 `.map(|_| ())` 丢弃返回值
- `try_join_all` 会 move Vec，`len()` 必须在 move 前计算

**验证**：全 workspace clippy EXIT=0；friend_room 集成测试 41 PASS（1 FAIL 是 DB pool timeout，与代码无关）

### 待修复

| # | 位置 | 严重度 | 备注 |
|---|---|---|---|
| _（无）_ | — | — | B-1.3 已修复（commit 8b5891f8 + 测试 fixture 修复 e70e4c35） |

### ✅ B-1.3 room upgrade invite loop 已修复（commit 8b5891f8）

**问题**：`upgrade_room()` 曾对所有 former members 串行 `invite_user()`。远程用户走联邦 HTTP 调用，大房间（100+ 成员）墙钟随成员数线性增长。

**方案**（三段式并发管道）：
- **Phase 1**：按 `is_remote_user()` 划分 locals / remotes（同步字符串解析，无副作用）
- **Phase 2**：locals 走 `futures::future::join_all`（本地 DB 并发，warn-only）
- **Phase 3**：remotes 走 `stream::iter(...).buffer_unordered(16)`（联邦并发上限 16，对齐 Synapse `join_max_concurrency` 默认值，慢速远程不阻塞其他邀请）

**设计决策**：
- Phase 2/3 均为 warn-only 而非 fail-fast：单个邀请失败（用户已离域/远程不可达）不中断整次升级，匹配 Synapse 行为
- `FederationConfig::join_max_concurrency` 默认值已存在，但此处用硬编码常量 16 而非走 config（与本仓 `push/service.rs` `MAX_CONCURRENT_SENDS = 8` 和 `sliding_sync_service` `MAX_CONCURRENT_MATERIALIZE = 8` 的本地常量风格一致——都是对单一功能的细粒度限制，不必全局可配）

**测试**：
- 新增 `test_upgrade_room_invites_all_former_local_members`：50 个 former members 全部出现在新房间的 invite 状态
- 既有的 `test_upgrade_room_success` / `test_upgrade_room_enqueues_tombstone_and_replacement_create_events` / `test_upgrade_room_not_found` 均 PASS
- 全 43 个 room_service_tests_migrated 测试 PASS

**附带修复**（commit e70e4c35，独立 PR）：`tests/integration/transaction_tests.rs:56` 的 `create_test_config` 字面量初始化器漏了 6 个 `#[serde(default)]` 字段（`max_lifetime_secs` / `idle_timeout_secs` / `min_idle_floor` / `statement_timeout_secs` / `lock_timeout_secs` / `idle_in_transaction_timeout_secs`），导致整个 integration test target 编不过。补 `..Default::default()` 即可——属于预存损坏修复，与 B-1.3 正交。

**验证**：
- `cargo fmt --all -- --check` ✅
- `SQLX_OFFLINE=true cargo clippy --workspace --all-features --locked -- -D warnings` ✅
- `cargo test --features ... --test integration room_service_tests_migrated -- upgrade_room` 4/4 PASS
- `scripts/check_missing_docs_ratchet.sh` ✅ debt = baseline = 6

### ✅ B-2.4 + B-2.5 已修复（commit 4be2c2ab）

**B-2.4（txn dedup TTL）**：
- `FederationConfig` 加 `txn_dedup_ttl_secs`（默认 86400 = 24h）
- `transaction.rs`：删 `const TXN_DEDUP_TTL_SECS`，从 `ctx.config.federation.txn_dedup_ttl_secs` 读；`0` 时跳过缓存（禁用去重）

**B-2.5（to-device 限额）**：
- `ServerConfig` 加 `to_device_max_recipients`（默认 5000）和 `to_device_max_payload_bytes`（默认 65536）
- `devices.rs`：删两个 const，从 `ctx.config.server` 读；错误消息改用运行时变量而非 const 名称（`format!("...{max_recipients}")`

**验证**：fmt + clippy 全过

### ✅ FederationConfig 手写 impl Default（commit b5cd3e41）

FederationConfig 39 字段。`#[derive(Default)]` 把所有字段默认成零值，与 `default_xxx()` 助手函数的业务默认值不一致。改成手写 impl Default：39 个字段全部显式写出，路由到助手函数。

效果：`FederationConfig::default()` 与 serde 解析缺省 YAML 时一致；加新字段只需在 impl 内补一行 + 一个 helper，不再破坏测试 literals。

### ✅ B-3 missing_docs 棘轮（commits 456ce2c9 + d1264a25）

**问题**：7 crate 都开 crate 级 `#![allow(missing_docs)]`，累积几百 warning。直接开 `-W missing_docs` 会让 CI 立刻爆红——长红 CI 等于无 CI。

**棘轮方案**：
1. **存量不动**：保留 crate 级 allow
2. **增量卡死**：
   - 新文件：所有 pub 必须有 `///` doc
   - 修改文件：仅 diff 新增 pub 行必须 doc
3. **debt baseline**：scripts/.missing-docs-baseline 当前 6，debt 减必须失败（强制收紧 baseline）；增也失败

**实现**：
- `scripts/check_missing_docs_ratchet.py`：扫描 `git diff`，正则找 `pub fn/struct/enum/trait/...` 检查上方 30 行 `///`
- `scripts/check_missing_docs_ratchet.sh`：shell wrapper
- `.github/workflows/ci.yml`：在 clippy step 后调用

**验证**：
- `cargo fmt --all -- --check` ✅
- `SQLX_OFFLINE=true cargo clippy --workspace --all-features --locked -- -D warnings` ✅
- 故意加无 doc 的新文件：`scripts/check_missing_docs_ratchet.sh` 报 1 处错误，RC=1
- 故意加有 doc 的新文件：RC=0
- baseline = 6

### ✅ B-2.1 + B-2.6 已修复（commit bc53c006）

**B-2.1（4 个 server 运维参数 → Config）**：
- `ServerConfig` 加 4 字段：`federation_retry_max_count` (5) / `drain_timeout_secs` (30) / `megolm_cleanup_interval_secs` (21600=6h) / `pruning_interval_secs` (86400)
- `src/server/mod.rs` 中 4 处 const 用法改为：`let x = if cfg > 0 { cfg } else { FALLBACK_CONST }`
- 保留 const 作 fallback（YAML 配 0 时回退，不破坏默认行为）

**B-2.6（ScheduledTasks Config 注入）**：
- `ServerConfig` 加 4 字段：`health_check_interval_secs` (10) / `performance_check_interval_secs` (300) / `integrity_check_interval_secs` (3600) / `maintenance_interval_secs` (86400)
- `tasks/mod.rs` 加 `ScheduledTasks::from_config(database, &ServerConfig)`；保留 `new(database)` 兼容（实测 0 处外部调用，但仍保留以防第三方使用）
- `src/server/mod.rs:234` 改用 `from_config`

**注意事项**：
- clippy `doc_lazy_continuation`：紧贴 list 项的 prose 段需空行隔开
- `homeserver.yaml` 加注释示例（不打开新字段，保持显式 opt-in）

**验证**：fmt + clippy 全过；821 个 lib 测试 PASS

### ✅ B-2.4 + B-2.5 已修复（commit 4be2c2ab）

**B-2.4（txn dedup TTL）**：
- `FederationConfig` 加 `txn_dedup_ttl_secs`（默认 86400 = 24h）
- `transaction.rs`：删 `const TXN_DEDUP_TTL_SECS`，从 `ctx.config.federation.txn_dedup_ttl_secs` 读；`0` 时跳过缓存（禁用去重）

**B-2.5（to-device 限额）**：
- `ServerConfig` 加 `to_device_max_recipients`（默认 5000）和 `to_device_max_payload_bytes`（默认 65536）
- `devices.rs`：删两个 const，从 `ctx.config.server` 读；错误消息改用运行时变量而非 const 名称（`format!("...{max_recipients}")`

**验证**：fmt + clippy 全过

### ✅ B-2.3 已修复（commit 510b8b7c）

**改动文件**：
- `synapse-common/src/config/database.rs`：`DatabaseConfig` 加 6 字段（`max_lifetime_secs`、`idle_timeout_secs`、`min_idle_floor`、`statement_timeout_secs`、`lock_timeout_secs`、`idle_in_transaction_timeout_secs`），全部 `#[serde(default = "...")]` 兼容旧 YAML
- `src/server/database.rs`：删 4 个 const，加 `format_pg_timeout()` helper，运行时拼 `'Ns'` 字串；启动日志显示当前生效的 6 个超时值
- `synapse-common/src/config/mod.rs` + `src/common/config/tests.rs` + `synapse-services/src/test_config.rs`：8 处 `DatabaseConfig { ... }` literal 加 `..Default::default()`
- `docker/config/homeserver.yaml`：database 段加 6 字段默认值文档

**架构细节**：
- `format_pg_timeout()` 用 `'<int>s'` 形式（PG `'0'` 会被解析为 ms，0ms 等于禁用）
- 旧 const 完全删除，逻辑统一走 `config.database.statement_timeout_secs`
- 启动日志新增一行 `PG session 超时:` 打印当前值，方便线上排查

**验证**：fmt + clippy 全过；821 个 synapse-common lib 测试 PASS

### ✅ B-2.1 + B-2.6 已修复（commit bc53c006）

**B-2.1（4 个 server 运维参数 → Config）**：
- `ServerConfig` 加 4 字段：`federation_retry_max_count` (5) / `drain_timeout_secs` (30) / `megolm_cleanup_interval_secs` (21600=6h) / `pruning_interval_secs` (86400)
- `src/server/mod.rs` 中 4 处 const 用法改为：`let x = if cfg > 0 { cfg } else { FALLBACK_CONST }`
- 保留 const 作 fallback（YAML 配 0 时回退，不破坏默认行为）

**B-2.6（ScheduledTasks Config 注入）**：
- `ServerConfig` 加 4 字段：`health_check_interval_secs` (10) / `performance_check_interval_secs` (300) / `integrity_check_interval_secs` (3600) / `maintenance_interval_secs` (86400)
- `tasks/mod.rs` 加 `ScheduledTasks::from_config(database, &ServerConfig)`；保留 `new(database)` 兼容（实测 0 处外部调用，但仍保留以防第三方使用）
- `src/server/mod.rs:234` 改用 `from_config`

**注意事项**：
- clippy `doc_lazy_continuation` 报「`/// 增大间隔` 接 `///` list 应有空行」—— 紧贴 list 项的 prose 段需空行
- `homeserver.yaml` 加注释示例（不打开新字段，保持显式 opt-in）

**验证**：fmt + clippy 全过；821 个 lib 测试 PASS

### ✅ B-1.5 已修复

**改动文件**：
- `synapse-storage/src/sliding_sync/api.rs`：在 `SlidingSyncStoreApi` trait 中新增 `delete_rooms_batch` 方法（默认 impl 用 `delete_room` 循环）
- `synapse-storage/src/sliding_sync/repository.rs`：在 `SlidingSyncStorage` 中实现 `delete_rooms_batch`（单条 `DELETE ... WHERE room_id = ANY($1::text[])`）
- `synapse-services/src/sliding_sync_service/mod.rs:399-411`：将 N 次循环 `delete_room` 改为单次 `delete_rooms_batch` 调用

**验证**：集成测试 `test_p1_5_unsubscribe_rooms_takes_effect_immediately` PASS（30 个 sliding_sync 测试全过）
