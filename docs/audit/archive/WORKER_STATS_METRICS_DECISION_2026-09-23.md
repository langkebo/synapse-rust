# 决策记录：worker CPU/内存监控走 Prometheus `/metrics`，不进 DB

日期：2026-09-23　范围：worker 负载指标（`load_stats` / `worker_statistics`）

## 背景

S1–S3 把**已存在**的心跳载荷 `load_stats` 接到了 `worker_statistics` 表：

- 写入：`synapse-storage/src/worker/repository.rs:465` 的 `upsert_statistics`，
  `INSERT … ON CONFLICT (worker_id) DO UPDATE`，六个指标列全部 `COALESCE(EXCLUDED.<col>,
  worker_statistics.<col>)`（`repository.rs:478-482`）——后到的部分心跳不会清空先前值。
- 读取：`synapse-storage/src/worker/repository.rs:757` 的 `get_statistics`，LEFT JOIN
  `worker_statistics` 后按 `AS "col?"` 投影出六列（`repository.rs:776-781`）。
- S4 补齐 worker 侧发送方与采集器：`src/bin/synapse_worker.rs:356` 的
  `heartbeat_load_stats` + `src/worker/heartbeat.rs:48` 的 `collect_load_stats`。

`collect_load_stats` 只填 `queue_depth`，其余五项恒为 `None`
（`src/worker/heartbeat.rs:48-59`）。真实来源只有 Redis：`queue.get_metrics("synapse_workers").queue_length`
（`src/bin/synapse_worker.rs:357`），底层是 `XLEN`。
`cpu_usage` / `memory_usage` / `active_connections` / `requests_per_second` /
`average_latency_ms`（`synapse-storage/src/worker/models.rs:515-528` 的
`WorkerLoadStatsUpdate`）**在本仓没有任何生产者**，因此保持 `None`。

## 决策

1. **不删这 5 个列。** 它们是活的契约：由 `upsert_statistics` 写入
   （`repository.rs:473-482`、`repository.rs:488-492`），由 `get_statistics` 读出
   （`repository.rs:776-781`、`repository.rs:812-816`），外部 worker 也可以上报它们。
   删除会打断一条**当前可用**的上报/查询路径。
2. **不为 CPU/内存新增 DB 列。** 现有列已足够承载"若有人上报"的语义，重复建模属反冗余铁律 1/2。
3. **若确需 CPU/内存监控，从 worker 既有的 `/metrics` 端点暴露**（S5，独立一步），
   DB/契约保持不变。

## 证据

- worker `/metrics` 处理器：`src/bin/synapse_worker.rs:442` 的 `get_metrics`（注册于
  `src/bin/synapse_worker.rs:178`）当前只输出 Redis 队列指标
  `synapse_worker_queue_length` / `_consumer_lag` / `_consumer_pending`
  （`src/bin/synapse_worker.rs:458-464`），无进程级指标。
- 全仓搜索进程级指标名 `process_resident|process_cpu|resident_memory|memory_bytes|rss`：
  `src/`（worker 二进制所在层）**0 命中**；全仓仅有的匹配全部无关——`synapse-test-utils/src/lib.rs:306`
  的 `detect_physical_memory_bytes`（宿主物理内存探测）、`scripts/bench_harness.rs:128` 的
  `measure_server_rss_mb`（bench 里 `ps -o rss=` 采样）、SQL 别名 `rss` 与
  `synapse-services/src/external_service_integration.rs:70` 的 `include_rss` 配置字段。
  没有任何 Prometheus 进程采集器。
- `get_statistics` 的文档注释明确写入契约：
  "Identity/lifecycle fields live in `workers`; the counters and load metrics
  live in `worker_statistics`, which is now written by [`upsert_statistics`]"
  （`synapse-storage/src/worker/repository.rs:746-748`）。

## 明确否决的方案

- **新建 `worker_stats` 表**：与既有 `worker_statistics` 职责重复（铁律 2）。
- **引入 Prometheus / InfluxDB / Timescale**：为一个未发布的进程级指标更换存储栈，收益与风险不匹配。
- **`pending_commands` / `active_tasks` 列**：**从不曾存在**。全仓 `pending_commands` 只命中
  命令队列 API `get_pending_commands`（`synapse-storage/src/worker/repository.rs:241`、
  `synapse-web/src/routes/worker.rs:467`）；`active_tasks` 只命中 room-service 的任务登记字段
  `Arc<RwLock<HashMap<String, JoinHandle<()>>>>`（`synapse-services/src/room/service.rs:154`），
  二者与 worker 统计无关。

## 若日后需要 S5

最小形态：一个进程采集器喂给 `/metrics`，输出 `process_cpu_seconds_total` /
`process_resident_memory_bytes`。它需要引入依赖（如 `sysinfo`）——按铁律 3，
**必须评估其对生产依赖图的影响**（若仅测试用则进 `[dev-dependencies]`）。
**不改 schema**，`worker_statistics` 与 `/metrics` 各自保持现状。
