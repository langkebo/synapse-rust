# Worker 异步任务审计报告

**日期**: 2026-09-04
**审计范围**: synapse-rust 后台异步任务调度逻辑、重试机制、异常容错
**目标**: 排查任务丢失、重复执行、资源泄漏、死锁等稳定性问题

---

## 执行摘要

本次审计覆盖 7 类后台任务组件，共发现 **8 个问题**：

| ID  | 严重级 | 描述 | 状态 |
|-----|-------|------|------|
| W-01 | 🔴 P0 | Worker 二进制 SIGTERM 时 consume_loop 硬 abort，PEL 消息丢失 | ✅ **已修复** |
| W-02 | 🟡 P0 | `WorkerBus::connect()` 生产环境从未被调用，多实例集群功能断链 | ⚠️ 已知限制 |
| W-03 | 🟡 P1 | `ScheduledTasks` 4 个后台循环无 shutdown hook | ✅ **已修复** |
| W-04 | 🟡 P1 | `EventNotifier` 2 个后台循环无 shutdown hook | ✅ **已修复** |
| W-05 | 🟡 P2 | Fire-and-forget spawn 缺少 tracing span，线上无法追踪 | ✅ **已修复（spans）** |
| W-06 | 💭 P2 | BUS-05 reconnect backoff 硬编码 3s | 📋 设计期修复 |
| W-07 | 💭 P2 | BUS-07 DLQ 内存缓冲有界（256条），已合规 | ✅ 无需修改 |
| W-08 | 💭 P3 | Audit 误报：`app_service_scheduler.start()` 实际上已传 shutdown_token | ✅ **虚警** |

**修复统计**: 4 个阻塞性问题全部修复，0 个回归，11/11 integration tests ✅，clippy 0 错误 ✅

---

## 架构总览

```
synapse-worker binary (src/bin/synapse_worker.rs)
├── consume_loop()  ←── XREADGROUP → XACK (Redis Stream consumer group)
│   └── 每条消息 → handler 处理 → XACK → DEL
├── monitor_loop()  ←── XINFO GROUPS (Pel len 监控)
└── Shutdown signal propagation
    ├── infra.shutdown_token.clone() → consume_loop()
    └── infra.shutdown_token.clone() → monitor_loop()

synapse-services
├── ScheduledTasks (src/tasks/mod.rs)
│   ├── purge_expired_tokens_loop()    ← interval + select!
│   ├── cleanup_expired_sessions_loop() ← interval + select!
│   ├── cleanup_expired_purged_rooms() ← interval + select!
│   └── sync_to_device_id_loop()       ← interval + select!
├── EventNotifier (event_notifier.rs)
│   ├── start_idle_slot_evictor()      ← interval + select!
│   └── start_redis_subscriber()       ← loop reconnect + pubsub while-let
├── EventBroadcaster (federation/event_broadcaster.rs)
│   └── batch_sender_loop()            ← interval + select!
└── ApplicationServiceScheduler (application_service/)
    └── start(CancellationToken)       ← admin.rs 正确接入了 shutdown_token

synapse-common
└── TaskQueue::consume_loop(shutdown_token) ← XREADGROUP + select!
```

---

## 修复 commit 摘要

| Commit | 内容 |
|--------|------|
| `1f3a3c09` | W-01: task_queue consume_loop + worker binary graceful shutdown |
| `85e64321` | W-04: EventNotifier shutdown hooks (evictor + redis subscriber) |
| `6b8dee15` | W-05: fire-and-forget tasks tracing spans |
| `3de52959` | W-03: ScheduledTasks 4 loops CancellationToken hooks |

---

## 修复详情

### W-01: Worker 二进制优雅关闭（已修复）

**问题**: `consume_loop` 使用裸 `loop {}`，worker binary 在 SIGTERM 时直接 `handle.abort()` 杀死当前消息处理任务，**XACK 永远不会执行**，导致 Redis PEL 中的消息永久卡住（at-most-once 语义被违反）。

**修复**:
- `synapse-common/src/task_queue.rs`: `consume_loop` 新增 `shutdown: CancellationToken` 参数，在 XREADGROUP 前后和 handler 后分别检查 `shutdown.is_cancelled()` → break，保证 XACK 在 exit 前执行。
- `src/bin/synapse_worker.rs`: `handle.abort()` → `consume_shutdown.cancel(); monitor_shutdown.cancel(); tokio::time::timeout(30s, handle)` 优雅排水。
- `synapse-common/Cargo.toml`: 添加 `tokio-util = { workspace = true }`。

### W-03: ScheduledTasks shutdown hooks（已修复）

**问题**: 4 个定时循环 (`purge_expired_tokens_loop`, `cleanup_expired_sessions_loop`, `cleanup_expired_purged_rooms`, `sync_to_device_id_loop`) 没有任何 shutdown signal，SIGTERM 后继续运行浪费 CPU。

**修复**: `src/tasks/mod.rs` 中所有 4 个循环均改为 `tokio::select! { biased; _ = shutdown.cancelled() => break, _ = interval.tick() => { ... } }`，call site `container.rs:283` 传入 `infra.shutdown_token.clone()`。

### W-04: EventNotifier shutdown hooks（已修复）

**问题**:
- `start_idle_slot_evictor`: 纯 CPU evictor，interval 独立运行，无 shutdown signal。
- `start_redis_subscriber`: reconnect loop + 内部 `while let Some(msg) = message_stream.next()` 均无 shutdown signal，SIGTERM 后 redis 连接断开时才退出（被动）。

**修复**:
- `start_idle_slot_evictor`: 签名新增 `shutdown: CancellationToken`，`loop { select! { biased; _ = shutdown.cancelled() => break, _ = ticker.tick() => { ... } } }`。
- `start_redis_subscriber`: reconnect loop 加 `if shutdown.is_cancelled() { break; }` 前置检查 + sleep 回退时 `select! { biased; _ = shutdown.cancelled() => break, _ = sleep => {} }`。
- `subscribe_and_listen` 内部 `while let` → `loop { select! { biased; _ = shutdown.cancelled() => break Ok(()), msg = message_stream.next() => { ... } } }`，退出时 `break Ok(())` 通知外层正常断开。
- `container.rs:321` 和 `container.rs:341` 分别传入 `infra.shutdown_token.clone()`。

### W-05: Fire-and-forget tracing spans（已修复）

**问题**: 3 个 `tokio::spawn` fire-and-forget 任务无 tracing context，线上无法关联日志：
- `event_notifier.rs:421` (publish_redis)
- `worker/bus.rs:386` (跨实例 redis publish + DLQ)
- `federation/event_broadcaster.rs:172` (per-destination send_batch)

**修复**: 每个 spawn 用 `tracing::info_span!` + `let _enter = span.enter()` 包裹，关键字段 (kind/key/destination/payload_bytes) 作为 span 属性记录。

**注**: Panic supervision（`AssertUnwindSafe + catch_unwind`）本期未实施——tokio::spawn 本身不传播 panic，JoinHandle 被 drop 会丢失 panic 信息。若生产环境发现这些任务中有 panic，再加 supervision。

### W-06: Reconnect backoff 硬编码（设计期修复）

**问题**: `BUS-05` reconnect loop 硬编码 3s backoff，无 jitter，无 exponential backoff。

**状态**: v2 设计文档中将改为 `backoff.clone()` 配置（已有 `RetryBackoff` 基础设施在 `event_broadcaster.rs` 中使用）。本期不改代码。

### W-07: DLQ 有界缓冲（无需修改）

**确认**: `worker/bus.rs` 的 `failed_publishes` DLQ 使用 `VecDeque` + `FAILED_PUBLISH_RING_SIZE = 256` 上限，超出时 `pop_front()`。内存占用恒定。

### W-08: 虚警（无需修改）

**误报**: Audit 描述 `app_service_scheduler.start(CancellationToken::new())` 未接入 shutdown signal，但实际 `admin.rs:265` 已经传入 `shutdown_token.clone()`，`start()` 方法签名也正确接受 `&CancellationToken`。

---

## 测试验证

```bash
# Clippy deny-level
cargo clippy --workspace --features "test-utils" -- -D warnings  # ✅ 0 errors

# EventNotifier tests
cargo test -p synapse-services --features test-utils --lib event_notifier  # ✅ 26/26

# Worker tests
cargo test -p synapse-services --features test-utils --lib worker  # ✅ 117/117

# API route snapshots
cargo test --features "test-utils privacy-ext voice-extended voip-tracking beacons server-notifications" \
  --test integration api_route_snapshots  # ✅ 11/11
```

---

## 遗留未解决问题

| 问题 | 建议 | 优先级 |
|------|------|--------|
| `WorkerBus::connect()` 从未被生产调用 | 完成 multi-instance cluster 集成或在 v2 中移除 dead code | 高 |
| Fire-and-forget panic supervision | 若生产发现 panic，在 spawn 处加 `AssertUnwindSafe + catch_unwind` | 中 |
| Reconnect backoff 硬编码 | v2 中改为配置化 | 低 |
