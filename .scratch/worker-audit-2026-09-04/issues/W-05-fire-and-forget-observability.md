# W-05: Fire-and-forget spawn 缺 tracing span

## 严重级
🟡 **P2** — 可观测性问题

## 状态
✅ **已修复（tracing spans 部分）** — commit `6b8dee15`
📋 **Panic supervision 待定**

## 问题描述

3 个 `tokio::spawn` fire-and-forget 任务没有 tracing context，线上无法关联日志：

| 文件 | 行号 | 性质 |
|------|------|------|
| `synapse-services/src/event_notifier.rs` | 421 | `publish_redis` 跨实例 notify |
| `synapse-services/src/worker/bus.rs` | 386 | redis publish + DLQ 回退 |
| `synapse-federation/src/event_broadcaster.rs` | 172 | per-destination federation send_batch |

## 影响

- Federation `send_batch` 失败时，原始请求日志显示成功，但 batch 实际被丢弃——无法追踪
- DLQ 中堆积的消息无法追溯到 originating fire-and-forget 调用
- 生产环境排查问题需要 grep 整库日志，没有结构化字段可过滤

## 修复方案（已实施）

每个 spawn 用 `tracing::info_span!` + `let _enter = span.enter()` 包裹：

```rust
let span = tracing::info_span!(
    "EventBroadcaster.send_batch_fire_and_forget",
    destination = %dest,
);
tokio::spawn(async move {
    let _enter = span.enter();
    // ... body
});
```

## 待优化（Panic supervision）

`tokio::spawn` 不会传播 panic——task 直接结束，JoinHandle 被 drop 后 panic 信息完全丢失。

**未实施的方案**：在 spawn 内用 `std::panic::AssertUnwindSafe + futures::FutureExt::catch_unwind` 包裹 body：

```rust
use futures::FutureExt;
use std::panic::AssertUnwindSafe;

tokio::spawn(async move {
    AssertUnwindSafe(async move { /* body */ })
        .catch_unwind()
        .await
        .unwrap_or_else(|e| {
            tracing::error!(panic = ?e, "fire-and-forget task panicked");
        });
});
```

**为什么本期没做**：workspace `clippy::panic = "allow"` 但 `expect_used` 和 `unwrap_used` 是 deny，catch_unwind 需要 AssertUnwindSafe 包裹且对 Send/Sync 约束较复杂。当前未在生产观察到这些任务的 panic 报告（tracing 没 record 到 + 没有 metrics），属预防性优化。

## 验证

```bash
cargo test -p synapse-services --features test-utils --lib worker  # ✅ 117/117
```

## 相关文件

- `synapse-services/src/event_notifier.rs`
- `synapse-services/src/worker/bus.rs`
- `synapse-federation/src/event_broadcaster.rs`