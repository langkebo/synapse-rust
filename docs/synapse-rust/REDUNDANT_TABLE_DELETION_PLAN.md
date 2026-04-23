# 冗余数据库表删除专项方案

> **日期**: 2026-04-22 | **最后审计**: 2026-04-22
> **范围**: 32 张存在活跃代码引用的候选冗余表 + 4 张零引用表
> **前提**: 基于 OPTIMIZATION_AND_DEDUPLICATION_PLAN_2026-04-21.md 第二节 Phase 3

---

## 一、分析方法

对每张表执行以下审查:
1. **SQL DML 引用定位**: 精确到文件:行号
2. **调用链追踪**: storage → service → route，确认是否真正活跃
3. **替代方案评估**: 是否可由现有核心表/运行时计算替代
4. **删除风险评级**: P0(不可删) / P1(需重构) / P2(可直接删)

---

## 二、分类结果总览

| 分类 | 表数 | 处置 | 代码状态 | 迁移状态 |
|------|------|------|----------|----------|
| **A — 不可删除（核心功能）** | 8 | 保留，不动 | — | — |
| **B — 可安全删除（死代码 / 过度设计）** | 4 | 直接删除 storage 方法 + schema | ✅ 已清理 | ✅ 已合并 |
| **C — 需重构后删除（低风险）** | 9 | 用现有表 + 运行时计算替代 | ✅ 已清理 | ✅ 已合并 |
| **D — 需重构后删除（中风险）** | 11 | 需要较大重构，分批执行 | ⚠️ 部分清理 | ⚠️ 部分合并 |
| **Z — 零引用（从未使用）** | 4 | 直接删除 | ✅ 无代码 | ✅ 已合并 |

> **审计发现**: 原方案存在多处表数统计错误和分类不一致，已在本次审计中修正。

---

## 三、A 类 — 不可删除（8 张）

这些表对应 Matrix 核心或广泛使用的功能，有完整的 storage→service→route 调用链。

| 表名 | 引用模块 | 不可删原因 |
|------|----------|-----------|
| `lazy_loaded_members` | `storage/device.rs` → `sync_service.rs` | Matrix spec 惰性加载成员，sync 核心功能 |
| `registration_captcha` | `storage/captcha.rs` → `captcha_service.rs` → 注册流程 | 注册验证码，安全必需 |
| `key_signatures` | `e2ee/device_keys/storage.rs` → `device_keys_service.rs` | E2EE 签名存储，加密核心 |
| `presence_subscriptions` | `storage/presence.rs` → `handlers/presence.rs` | 在线状态订阅，有活跃路由 |
| `thread_replies` | `storage/thread.rs` → `thread_service.rs` → 路由 | Thread 回复，Matrix MSC3440 核心 |
| `thread_read_receipts` | `storage/thread.rs` → `thread_service.rs` | Thread 未读计数，客户端依赖 |
| `thread_relations` | `storage/thread.rs` → `thread_service.rs` | Thread 关联，与 thread_replies 耦合 |
| `push_config` | `storage/push_notification.rs:554` → `push_service.rs:282` | 推送配置，FCM/APNS/WebPush 核心 |

**处置**: 不动。这些表是功能性必需的。

> **审计修正**: 原方案 A 类标题写"7 张"但实际列了 8 张（`push_config` 因 Markdown 格式问题被单独放在表格下方），已修正为 8 张。原方案第一批执行计划错误地将 `push_config` 列入删除范围，已修正。

---

## 四、B 类 — 可安全删除（4 张）✅ 已完成

这些表的 storage 方法虽然存在，但调用链断裂或赋值后未使用。

### B1. `password_policy` — 死代码，已改为纯配置驱动

| 维度 | 详情 |
|------|------|
| **原存储** | `services/auth/password_policy.rs` — 已改为纯配置结构体，无 DB 查询 |
| **原服务** | `PasswordPolicyService` — 已改为从配置文件读取策略 |
| **结论** | 密码策略已完全配置驱动，DB 表无任何代码引用 |

> **审计修正**: 原方案 B1 描述错误地引用 `key_rotation.rs:35,88,126`，这是 `key_rotation_log` 的引用而非 `password_policy`。已修正。

### B2. `key_rotation_history` — 冗余副本

| 维度 | 详情 |
|------|------|
| **原存储** | `key_rotation_history` 表 — 已无任何代码引用 |
| **替代** | `key_rotation_log` 表（`key_rotation.rs:35,92`, `e2ee/key_rotation/service.rs:196,309`） |
| **结论** | `key_rotation_log` 已是密钥轮换的主表，`key_rotation_history` 是冗余副本 |

> **审计修正**: 原方案缺少 B2 的详细描述，已补充。

### B3. `presence_routes` — 模块系统过度设计

| 维度 | 详情 |
|------|------|
| **存储** | `storage/module.rs:695` — 已改为返回空列表 |
| **路由** | `web/routes/module.rs:774,1016-1021` — admin 路由仍存在但返回空数据 |
| **结论** | Presence 路由在 Matrix 中是内建的，不需要 DB 驱动的动态路由表 |

> **审计修正**: 原方案编号为 B4，重新编号为 B3。

### B4. `password_auth_providers` — 模块系统过度设计

| 维度 | 详情 |
|------|------|
| **存储** | `storage/module.rs:680` — 已改为返回空列表 |
| **路由** | `web/routes/module.rs:729,1008-1013` — admin 路由仍存在但返回空数据 |
| **结论** | 密码认证提供者已由 OIDC/内建 auth 替代，DB 表冗余 |

> **审计修正**: 原方案编号为 B5，重新编号为 B4。

**执行计划 — B 类 ✅ 已完成**:

```
Phase B (低风险):
1. ✅ 删除 password_policy DB 查询，改为纯配置读取 (services/auth/password_policy.rs)
2. ✅ key_rotation_history 已无代码引用，key_rotation_log 是主表
3. ✅ module.rs 中 presence_routes DB 查询改为返回空结果
4. ✅ module.rs 中 password_auth_providers DB 查询改为返回空结果
5. ✅ 迁移已合并到: docker/deploy/migrations/20260421000001_consolidated_drop_redundant_tables.sql
   DROP TABLE IF EXISTS password_policy, key_rotation_history,
   presence_routes, password_auth_providers
6. ✅ 验证: cargo check + cargo test --lib (1628 passed)
```

> **审计修正**: 原方案 B 类总览写"4 张"但标题写"5 张"。实际删除 4 张表（不含 `push_config`），已统一为 4 张。

---

## 五、C 类 — 需重构后删除（9 张，低风险）✅ 代码已清理

这些表可由现有核心表 + 运行时计算替代，重构范围小。

### C1. `event_report_history` — 日志替代

| 维度 | 详情 |
|------|------|
| **存储** | `storage/event_report.rs:323` — `get_report_history` 已改为返回空列表 |
| **替代** | `tracing::info!` 日志记录 |
| **状态** | ✅ 代码已清理，迁移已合并 |

### C2. `event_report_stats` — 运行时聚合替代

| 维度 | 详情 |
|------|------|
| **存储** | 无活跃 SQL 查询（仅测试代码引用） |
| **替代** | `SELECT COUNT(*), date_trunc(...) FROM event_reports GROUP BY ...` |
| **状态** | ✅ 代码已清理，迁移已合并 |

### C3. `retention_stats` — 运行时聚合替代

| 维度 | 详情 |
|------|------|
| **存储** | `storage/retention.rs:353` — `get_stats` 已改为返回 None，`update_stats` 改为日志 |
| **替代** | 运行时 SQL 聚合 |
| **状态** | ✅ 代码已清理，迁移已合并 |

> **审计修正**: 原方案 C3 替代方案引用 `retention_cleanup_logs` 表，但该表本身也在删除列表中（D3），已修正替代方案为运行时 SQL 聚合。

### C4. `deleted_events_index` — 日志替代

| 维度 | 详情 |
|------|------|
| **存储** | `storage/retention.rs:336` — `get_deleted_events` 已改为返回空列表，`record_deleted_event` 改为日志 |
| **替代** | `tracing::info!` 日志 + `events` 表状态过滤 |
| **状态** | ✅ 代码已清理，迁移已合并 |

> **审计修正**: 原方案 C4 替代方案建议 `events.status = 'redacted'`，但 `events` 表无 `status` 列。实际替代方案为日志记录。

### C5. `worker_load_stats` — 日志替代

| 维度 | 详情 |
|------|------|
| **存储** | `worker/storage.rs` — 方法已完全删除 |
| **替代** | `tracing::debug!` 结构化日志 |
| **状态** | ✅ 代码已清理，迁移已合并 |

### C6. `worker_connections` — 日志替代

| 维度 | 详情 |
|------|------|
| **存储** | `worker/storage.rs` — 方法已完全删除 |
| **替代** | `tracing::info!` 结构化日志 |
| **状态** | ✅ 代码已清理，迁移已合并 |

> **审计修正**: 原方案 C5/C6 建议用 Prometheus gauge / DashMap 替代，但实际实现为日志替代，更简洁。

### C7. `spam_check_results` — 日志替代

| 维度 | 详情 |
|------|------|
| **存储** | `storage/module.rs:446` — 查询方法已改为返回空列表，写入方法保留为日志 |
| **路由** | `web/routes/module.rs:576,985` — admin 路由仍存在但返回空数据 |
| **状态** | ✅ 代码已清理，迁移已合并 |

### C8. `third_party_rule_results` — 日志替代

| 维度 | 详情 |
|------|------|
| **存储** | `storage/module.rs:481` — 查询方法已改为返回空列表，写入方法保留为日志 |
| **路由** | `web/routes/module.rs:598,989` — admin 路由仍存在但返回空数据 |
| **状态** | ✅ 代码已清理，迁移已合并 |

### C9. `rate_limit_callbacks` — 模块过度设计

| 维度 | 详情 |
|------|------|
| **存储** | `storage/module.rs:760` — 已改为返回空列表 |
| **路由** | `web/routes/module.rs:887,1036-1041` — admin 路由仍存在但返回空数据 |
| **替代** | 配置文件 + 代码内限流逻辑 |
| **状态** | ✅ 代码已清理，迁移已合并 |

> **审计修正**: 原方案将 `rate_limit_callbacks` 归入 D5（中风险），但实际代码已改为 stub，风险等级应为低。迁移文件已将其归入 C 类。已从 D 类移至 C 类，C 类表数从 8 修正为 9。

**执行计划 — C 类 ✅ 代码已清理，迁移已合并**:

```
Phase C (低风险):
1. ✅ worker_load_stats → storage 方法已删除，日志替代
2. ✅ worker_connections → storage 方法已删除，日志替代
3. ✅ event_report_history → get_report_history 返回空列表
4. ✅ event_report_stats → 无活跃 SQL，仅测试引用
5. ✅ retention_stats → get_stats 返回 None, update_stats 改为日志
6. ✅ deleted_events_index → get_deleted_events 返回空列表, record_deleted_event 改为日志
7. ✅ spam_check_results → 查询返回空列表, 写入改为日志
8. ✅ third_party_rule_results → 查询返回空列表, 写入改为日志
9. ✅ rate_limit_callbacks → get_rate_limit_callbacks 返回空列表
10. ✅ 迁移已合并到: docker/deploy/migrations/20260421000001_consolidated_drop_redundant_tables.sql
11. ✅ 验证: cargo check + cargo test --lib (1628 passed)
```

---

## 六、D 类 — 需重构后删除（11 张，中高风险）

这些表有密集的调用链，删除需要较大重构。

### D1. room_summary 子系统 (4 张) — ⚠️ 高风险，不建议删除

`room_summary_members`, `room_summary_state`, `room_summary_stats`, `room_summary_update_queue`

| 维度 | 详情 |
|------|------|
| **存储** | `storage/room_summary.rs` — **20+ 个方法**, ~800 行活跃 SQL |
| **服务** | `room_summary_service.rs` — 完整的 CRUD + 后台更新流程 |
| **调用** | `room_service.rs`, `sync_service.rs`, `sliding_sync_service.rs`, `admin/notification.rs:597`, admin 路由 |
| **替代** | 用 `room_memberships` + `room_state_events` + `room_summaries` 核心表运行时计算 |
| **风险** | **高** — 是 sync/sliding-sync 的核心性能优化层，删除可能导致性能退化 |
| **工作量** | ~500 行重构 |

**建议**: 保留全部 4 张表。这些表是 sync/sliding-sync 的核心性能优化层（预计算的摘要缓存），删除后每次同步请求都需要实时聚合查询，可能导致显著性能退化。如需优化，建议仅删除 `room_summary_update_queue`（可改为内存队列），保留其他 3 张缓存表。

### D2. space 子系统 (4 张) — ⚠️ 高风险，不建议删除

`space_events`, `space_statistics`, `space_summaries`, `space_members`

| 维度 | 详情 |
|------|------|
| **存储** | `storage/space.rs` — **15+ 个方法**, ~500 行活跃 SQL |
| **服务** | `space_service.rs` — 完整的 CRUD + 统计 + 成员管理 |
| **调用** | `web/routes/space/membership_state.rs`, `web/routes/space/lifecycle_query.rs`, `web/routes/admin/room.rs:839,883`, space 路由 |
| **替代** | Spaces 本质是特殊类型的 Room，可用 `rooms` + `room_memberships` + `events` 替代 |
| **风险** | **高** — Space hierarchy 和成员管理是 MSC1772 核心功能，有活跃 API 路由 |
| **工作量** | ~600 行重构 |

**建议**: 保留全部 4 张表。Space 是 MSC1772 核心功能，有完整的 storage→service→route 调用链。如需优化，可考虑将 `space_members` 合并到 `room_memberships`，但需要大量重构且收益有限。

### D3. retention 队列 (2 张) — ✅ 代码已清理，迁移已合并

`retention_cleanup_queue`, `retention_cleanup_logs`

| 维度 | 详情 |
|------|------|
| **存储** | `storage/retention.rs` — 所有方法已改为 stub + 日志替代 |
| **服务** | `retention_service.rs` — 仍调用 storage 方法，但得到 stub 响应 |
| **核心功能** | `run_cleanup` 仍正常工作（直接 DELETE FROM events），仅日志记录功能降级 |
| **风险** | **低** — 核心清理功能不受影响，仅丢失清理历史记录 |
| **状态** | ✅ 代码已清理，迁移已合并 |

> **审计发现**: 迁移文件已包含 `DROP TABLE retention_cleanup_queue/logs`，但 `retention_service.rs` 仍引用 `RetentionCleanupLog` 和 `RetentionCleanupQueueItem` 结构体。由于 storage 方法已改为 stub（不执行 SQL），代码不会崩溃，但存在以下问题：
> - `process_pending_cleanups` 调用 `get_pending_cleanups()` 返回空列表，清理队列处理已静默禁用
> - `run_cleanup` 调用 `create_cleanup_log()` 返回 stub（id=0），`complete_cleanup_log()` 忽略 id，功能降级为仅日志
> - `get_cleanup_logs` 返回空列表，管理员无法查看清理历史
>
> **建议**: 清理 `retention_service.rs` 中的 `RetentionCleanupLog`/`RetentionCleanupQueueItem` 引用，将 `process_pending_cleanups` 和 `prune_finished_cleanup_queue` 标记为 deprecated 或移除。

### D4. worker_task_assignments (1 张) — ⚠️ 中风险，未清理

| 维度 | 详情 |
|------|------|
| **存储** | `worker/storage.rs:522-635` — **7 个活跃 SQL 查询**（INSERT, SELECT, UPDATE） |
| **调用** | `worker/manager.rs:390-437` — `assign_task`, `assign_task_to_worker` |
| **替代** | Redis 队列 (`RedisTaskQueue` 已存在) |
| **风险** | **中** — Worker 任务分发影响分布式部署 |
| **工作量** | ~200 行重构 |
| **状态** | ❌ 代码未清理，迁移未包含 |

**建议**: 暂不删除。需要先确认 `RedisTaskQueue` 是否已完整实现并可作为替代。

---

## 七、零引用表（4 张）✅ 已完成

这些表在代码库中无任何引用，从未被使用。

| 表名 | 状态 |
|------|------|
| `private_messages` | ✅ 零引用，已删除 |
| `private_sessions` | ✅ 零引用，已删除 |
| `room_children` | ✅ 零引用，已删除 |
| `ip_reputation` | ✅ 零引用，已删除 |

**迁移**: 已合并到 `docker/deploy/migrations/20260421000001_consolidated_drop_redundant_tables.sql`

---

## 八、执行进度总览

| 批次 | 表数 | 代码清理 | 迁移合并 | 编译验证 | 状态 |
|------|------|----------|----------|----------|------|
| **Z — 零引用表** | 4 | ✅ 无代码 | ✅ 已合并 | ✅ 通过 | **完成** |
| **B — 死代码** | 4 | ✅ 已清理 | ✅ 已合并 | ✅ 通过 | **完成** |
| **C — 低风险重构** | 9 | ✅ 已清理 | ✅ 已合并 | ✅ 通过 | **完成** |
| **D3 — retention 队列** | 2 | ✅ 已清理 | ✅ 已合并 | ✅ 通过 | **完成** |
| **D1 — room_summary** | 4 | ❌ 未清理 | ❌ 未合并 | — | **建议保留** |
| **D2 — space** | 4 | ❌ 未清理 | ❌ 未合并 | — | **建议保留** |
| **D4 — worker_task** | 1 | ❌ 未清理 | ❌ 未合并 | — | **建议保留** |

**实际删除表数**: 19 张（Z:4 + B:4 + C:9 + D3:2）
**建议保留表数**: 8 张（A 类）+ 9 张（D1:4 + D2:4 + D4:1）= 17 张

> **D4 评估结论**: `worker_task_assignments` 与 `RedisTaskQueue` 功能互补（PG 持久化状态跟踪 vs Redis 实时分发），不可替代。建议保留。

---

## 九、待办事项

### ✅ 已完成

- [x] **清理 D3 service 层引用**: 已简化 `retention_service.rs`，移除 `process_cleanup_item`/`is_protected_event_type`，`process_pending_cleanups`/`prune_finished_cleanup_queue`/`schedule_room_cleanup` 改为 no-op
- [x] **清理 D3 storage 层 stub**: 已删除 `storage/retention.rs` 中 14 个无调用者的 stub 方法
- [x] **清理测试代码**: 已删除 `test_retention_cleanup_queue_item`/`test_deleted_event_index`/`test_retention_stats` 等已废弃表结构体测试（7 个）
- [x] **评估 D4 worker_task_assignments**: 确认 `RedisTaskQueue` 不可替代，建议保留

### 中优先级

- [ ] **清理残留路由**: B/C 类中仍有 admin 路由返回空数据（`presence_routes`, `password_auth_providers`, `spam_check_results`, `third_party_rule_results`, `rate_limit_callbacks`），当前为 gate 状态，可考虑标记 deprecated

### 低优先级（建议不做）

- [ ] ~~删除 D1 room_summary 子系统~~ — 高风险，建议保留
- [ ] ~~删除 D2 space 子系统~~ — 高风险，建议保留
- [ ] ~~删除 D4 worker_task_assignments~~ — RedisTaskQueue 不可替代，建议保留

---

## 十、验证机制

### 每张表删除前

1. `cargo check` 通过
2. `cargo clippy --lib` 零警告
3. `cargo test --lib` 通过

### 每批次完成后

4. `cargo test --lib` 全量通过
5. 确认迁移文件一致性（undo 脚本配套）
6. 性能回归检查（D 类表）

### 回滚策略

- 合并迁移文件: `docker/deploy/migrations/20260421000001_consolidated_drop_redundant_tables.sql`
- Undo 脚本: `docker/deploy/migrations/20260421000001_consolidated_drop_redundant_tables.undo.sql`
- 遵循"先 gate 后删"原则 — 先将 storage 方法改为 stub，确认无问题后再合并 DROP TABLE 迁移

---

## 十一、预期收益

| 指标 | 当前 | 删除后 (已完成批次) | 变化 |
|------|------|---------------------|------|
| 已删除表数 | — | 19 张 | **-19 张** |
| 冗余 storage 代码 | ~2,500 行 | ~0 (B/C/D3 已清理) | **-2,500 行** |
| DB 无效查询 | 基线 | 消除 (stub 替代) | **改善** |
| 维护认知负担 | 32 张候选冗余表 | 9 张待评估 (D1/D2/D4) | **-72%** |
