# 阶段 5 性能基准对比报告（优化前 vs 优化后）

- **优化前基线**: `docs/audit/05_performance_baseline.json`（分支 `feat/architecture-optimization-round2`，2026-07-10）
- **优化后复测**: `docs/audit/11_performance_after.json`（分支 `optimization/audit-2026-07`，2026-07-12）
- **测量方法**: 与基线**完全一致** —— 用带真实热点索引 DDL 的镜像表，按相同生产级数据量播种，预热后对同样 7 个 DB 层查询形状跑 `EXPLAIN (ANALYZE, BUFFERS, TIMING ON)`。同一份 `bench_seed.sql` + `bench_explain.sql`。

> **诚实声明（关键）**: 所有形状在预热镜像表上都是**亚毫秒级**。在这个量级下，单次运行间的抖动（0.01–0.15ms）**超过了任何真实信号**。因此下面每个场景的 exec_ms 差值**不作为“提速”主张** —— 真正有意义的结论是**执行计划**（是否命中索引、是否有 seq scan），而不是微秒级差值。

---

## 1. DB 层查询延迟（p50/p95/p99 无法在本环境采集，见第 4 节）

| 场景 | 计划 | 基线 exec_ms | 优化后 exec_ms | 判定 | 标注 |
|---|---|---|---|---|---|
| 1. /sync 全量成员（1000 人房间） | Bitmap Index Scan `idx_room_memberships_room_membership` | 0.114 | 0.082 | INDEX HIT | ⚠️→✅ 持平（亚毫秒噪声；计划变体，同为索引服务） |
| 2. /sync 增量（stream_ordering, LIMIT 100） | Index Scan `idx_events_room_stream_ordering` + Limit | 0.041 | 0.027 | INDEX HIT | 持平（噪声）。规划器仍选窄索引而非覆盖索引，与基线观察一致 |
| 3. /sliding_sync 时间线（ts DESC, LIMIT 50） | Index Scan `idx_events_room_time` | 0.046 | 0.015 | INDEX HIT | 持平（噪声） |
| 4. 发消息 → 最新 stream_ordering | Index Only Scan（Heap Fetches: 1） | 0.024 | 0.013 | INDEX-ONLY HIT | 持平（噪声） |
| 6. 设备密钥查询（100 用户 user_id=ANY） | Bitmap Heap Scan `idx_device_keys_user_device` | 0.447 | 0.600 | INDEX HIT | ⚠️ **关注**（仍走索引，亚毫秒）。本次规划器选了 bitmap 变体，`ANY(ARRAY(子查询))` 形状会在运行间翻转计划，非代码回归 |
| 6b. OTK 原子领取（LIMIT 1） | Index Scan `idx_one_time_keys_user_device` | 0.019 | 0.014 | INDEX HIT | 持平（噪声） |
| aux. account_data 按用户列出 | Bitmap Index Scan `idx_account_data_user_type` | 0.023 | 0.018 | INDEX HIT | 持平（噪声） |

**索引命中率**: 7/7 形状走索引，**0 次 seq scan** —— 与基线相同。经过 24 个优化任务，热点索引策略**端到端保持完好**。

**✅ / ⚠️ / ❌ 汇总**:
- ✅ 无计划层回归；全部命中预期索引，无全表扫描。
- ⚠️ 场景 6（device_keys ANY）本次为 bitmap 计划、约 0.6ms —— 规划器抖动，不是本分支任何代码改动引起（无 device_keys 读路径变更）。需在真实运行服务器 + `pg_stat_statements` 下复核。
- ❌ 无需回滚的回归项：**0**。

---

## 2. QPS 吞吐变化

**未采集（NOT CAPTURED）** —— 与基线一致。QPS 需要一台构建并运行中的 homeserver（:8008）+ 播种场景数据 + `BENCH_ADMIN_TOKEN`，本环境不具备。基线同样未采集，故此维度是“覆盖度不变”，而非“测得持平”。

## 3. 缓存命中率变化

**未采集（NOT CAPTURED）** —— 与基线一致。无实时流量即无命中率可测。审计-04 已记录 P1 未缓存热读点（`get_filter`、`list_account_data`、`get_max_device_list_stream_id`、`get_state_events`），这是后续最高杠杆优化项，但需运行中的服务器才能量化。

## 4. DB 查询次数 / N+1 消除验证

**✅ N+1 已消除（OPT-012 / 审计-03：device `delete_devices_batch`）**

- **位置**: `synapse-storage/src/device/mod.rs`
- **验证方法**: 结构化查询计数证明（读取批处理辅助函数源码；无实时流量故不用 `pg_stat_statements`）。
- **优化前**: 按设备循环，每个设备各调用一次 `delete_lazy_loaded_members_for_device` + `record_device_list_change` → **2×N** 条查询。
- **优化后**: 先按 `user_id` 用 HashMap 分组，每个用户组各只调用一次：
  - `delete_lazy_loaded_members_for_devices_batch` —— 单条 `DELETE ... WHERE user_id=$1 AND device_id = ANY($2)`（mod.rs:347）
  - `record_device_list_changes_batch` —— 单条 CTE `INSERT ... UNNEST($3::TEXT[])`（mod.rs:202）
- **查询次数**: 每个不同 user_id **恒定 2 条**，与设备数无关（原为 2×N）。
- **覆盖测试**: `test_delete_devices_batch_uses_batch_side_effects`（建 3 设备、批量删除、断言批处理路径写入 3 条 `device_lists_changes`）。

结论：常见的“单用户多设备删除”场景，查询数从 **2×N 降到 2**。

## 5. 内存占用变化

**未采集（NOT CAPTURED）** —— 与基线一致。需运行中的 release 服务器负载下采样，本环境不具备。

---

## 总体结论

| 维度 | 状态 |
|---|---|
| DB 层查询计划 | ✅ 无回归，7/7 命中索引，0 seq scan |
| N+1（OPT-012） | ✅ 结构性消除（2×N → 2） |
| 需回滚的回归 | ✅ 0 项 |
| 关注项 | ⚠️ 场景 6 bitmap 计划抖动（亚毫秒，非代码回归） |
| p50/p95/p99 延迟 | ⚪ 未采集（无运行服务器，与基线相同） |
| QPS 吞吐 | ⚪ 未采集（与基线相同） |
| 缓存命中率 | ⚪ 未采集（与基线相同） |
| 内存占用 | ⚪ 未采集（与基线相同） |

**要拿到真实的 p50/p95/p99 + QPS + 内存 + 缓存命中率**，需完成基线 `next_steps` 里已列的前置条件：构建并运行 release 服务器对接 docker DB、编写场景播种脚本（1000 人房间 / 100 用户设备密钥 / 10MB 媒体，目前**无播种脚本**）、注册 admin 导出 token、跑 `cargo bench` 拿真实数字，并接入 `pg_stat_statements` 做每端点查询数断言以捕获 N+1 回归。这些超出当前无服务器环境的能力范围。
