# W-01: Worker 二进制 graceful shutdown — PEL 消息丢失

## 严重级
🔴 **P0** — 阻塞性问题

## 状态
✅ **已修复** — commit `1f3a3c09`

## 问题描述

`synapse-common/src/task_queue.rs::consume_loop` 使用裸 `loop {}` 消费 Redis Stream，`synapse-worker` 二进制在 SIGTERM 时直接调用 `handle.abort()` 杀死当前消息处理任务。

**影响**：
- `XACK` 永远不会执行，当前消息永久留在 Redis PEL（Pending Entries List）
- 下次 `XREADGROUP` 会重新读取同一消息（因为 PEL 中标记 pending），导致**消息重复处理**（at-most-once 被违反 → at-least-once）
- PEL 持续增长

## 根因

1. `consume_loop` 没有 shutdown 参数，loop 内无法感知 SIGTERM
2. `handle.abort()` 是 violent kill——不等 handler 跑完就终止，XACK 代码永远无法执行
3. `monitor_loop` 同时被 abort，两个 cleanup 任务都丢失

## 修复方案

- `consume_loop(shutdown: CancellationToken)` — 在 XREADGROUP 前、handler 后两个关键点检查 `shutdown.is_cancelled()`
- `src/bin/synapse_worker.rs` — `handle.abort()` → `consume_shutdown.cancel()` + `tokio::time::timeout(30s, handle)` 优雅排水
- `synapse-common/Cargo.toml` — 添加 `tokio-util = { workspace = true }`

## 验证

```rust
// task_queue.rs:280-286
tokio::select! {
    biased;
    _ = shutdown.is_cancelled() => {
        tracing::debug!("consume_loop: shutdown received, exiting gracefully");
        break;
    }
    _ = interval.tick() => {
        // XREADGROUP ...
    }
}
```

## 相关文件

- `synapse-common/src/task_queue.rs`
- `src/bin/synapse_worker.rs`
- `synapse-common/Cargo.toml`
