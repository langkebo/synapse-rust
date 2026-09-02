# 03: events 表 HASH(room_id) 64 分区

**What to build:** 将 `events` 表改为按 `HASH(room_id)` 64 分区。解决高频写入表的 DDL 锁风险、单表无限增长、ANALYZE 统计陈旧问题。

**Blocked by:** 01-v11-baseline-scaffold

**Status:** ⏸️ DEFERRED（不进入 v11 baseline）

## DEFER 理由（2026-08-31 评估）

经过 Rust 端代码深度调研（47 个文件、`stream_ordering` 在 8 个核心模块中的语义分析），**P0 阻塞 + P1 风险叠加**：

### 1. P0 阻塞：4 张子表的 FK 引用

PostgreSQL 规则：被分区表的外键引用必须包含分区键。当前 FK 关系：

| 子表 | FK 引用 | 影响 |
|------|---------|------|
| `event_edges` L1837 | `events(event_id)` | 缺 `room_id` |
| `event_forward_extremities` L1844 | `events(event_id)` | 缺 `room_id` |
| `state_groups` L2422 | `events(event_id)` | 缺 `room_id` |
| `event_to_state_groups` L2437 | `events(event_id)` | 缺 `room_id` |

PG 会直接拒绝：`error: insufficient columns in UNIQUE constraint for referenced table`。

**绕过代价**：必须把这些 FK 全部 `NOT VALID` + 应用层兜底——这破坏 v10 设计的 "DB-04-b: Rust 层负责级联清理" 约束（`fk_events_room` 已注释）。FK 失效风险转移回应用层。

### 2. P1 风险：stream_ordering watermark 退化

- `synapse-services/src/sliding_sync_service/mod.rs:378` 调用 `get_max_stream_ordering()` 作为连接级 watermark
- HASH 分区后变成 64 分区 `UNION ALL` + `MAX()`，QPS 退化
- `next_batch` 数值跳跃——客户端可见行为变更

### 3. P1 风险：6 处全表聚合查询

- `batch.rs:299` MAX
- `admin.rs:314/355` COUNT
- `basic.rs:148`、`dag.rs:132`
- 测试 `db_tests.rs`

需要重写为分区裁剪或应用层聚合。

### 4. 决策

`events` 是核心写入表，**v11 还在 baseline 阶段**，应优先保证稳定。建议在以下条件满足后重提：

- [ ] events 单表 > 5000 万行 或 QPS > 1k/s
- [ ] 已迁移 FK 策略（所有引用 events 的 FK 改 NOT VALID）
- [ ] `pg_partman` 已集成
- [ ] 完整 E2E 测试 `event_edges` / state_groups 路径通过

**替代方案**（如果分区确实必要）：
- 考虑 LIST (room_id_prefix) 而非 HASH（更易运维）
- 或按月 RANGE 分区（`PARTITION BY RANGE (origin_server_ts)`）——查询模式更友好
- 或直接用 `pg_partman` 管理时间分区

---

## 原 Ticket 计划（保留为参考）

- [ ] 在 v11 baseline 中：`CREATE TABLE events (... PRIMARY KEY (event_id, room_id)) PARTITION BY HASH (room_id)`
- [ ] 创建 `auto_create_events_partitions(modulus INT)` 函数：批量建 N 个 PARTITION OF
- [ ] 默认创建 64 个分区（`events_p0` 到 `events_p63`）
- [ ] 验证 `EXPLAIN (ANALYZE)` 单房间查询走单一分区（partition pruning 生效）
- [ ] 验证跨房间查询走 64 分区扫描（Plan 显示分区裁剪）
- [ ] 重建 `synapse_test` schema，确认分区表正常应用
- [ ] 验证 storage 测试套件不因分区变更失败

**关键决策**：
- 分区数 64（每房间事件独立分布，适合 federation 负载均衡）
- 主键必须包含分区键：`PRIMARY KEY (event_id, room_id)`
- 不动外键（DB-04-b 已删 events CASCADE FK）
