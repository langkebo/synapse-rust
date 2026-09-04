# W-04: EventNotifier 2 循环无 shutdown hook

## 严重级
🟡 **P1** — 重要稳定性改进

## 状态
✅ **已修复** — commit `85e64321`

## 问题描述

`synapse-services/src/event_notifier.rs` 中 `EventNotifier::start_idle_slot_evictor` 和 `EventNotifier::start_redis_subscriber` 都有长循环但无 shutdown signal：

| 方法 | 行号 | 性质 |
|------|------|------|
| `start_idle_slot_evictor` | 153 (spawn) | CPU-only 内存清理 |
| `start_redis_subscriber` | 257 (spawn) | 网络 I/O 阻塞 await |

## 影响

- `start_redis_subscriber`: 内部 reconnect loop 和 pubsub `while let` 都依赖 redis 连接断开才会退出——SIGTERM 后 redis 已关，循环空转重连 + 1s sleep 浪费时间
- `start_idle_slot_evictor`: SIGTERM 后继续运行，graceful shutdown 期间不必要 CPU 开销
- 容器 SIGTERM 后日志中持续出现 `EventNotifier subscription error: ... reconnecting in 1s...` —— 噪音 + 可能掩盖真正的错误

## 修复方案

`start_idle_slot_evictor(interval, shutdown: CancellationToken)`:

```rust
loop {
    tokio::select! {
        biased;
        _ = shutdown.cancelled() => break,
        _ = ticker.tick() => { /* retain */ }
    }
}
```

`start_redis_subscriber(shutdown: CancellationToken)`:
- 外层 reconnect loop: 顶部 `if shutdown.is_cancelled() { break; }` + sleep 包入 `select!`
- 内层 `subscribe_and_listen` 接收 `shutdown`，把 `while let Some(msg) = next().await` 改成 `loop { select! { biased; _ = shutdown.cancelled() => break Ok(()), msg = next() => ... } }`，退出时 `break Ok(())`

Call sites 在 `container.rs:321` (redis subscriber) 和 `container.rs:341` (idle evictor)，分别传入 `infra.shutdown_token.clone()`。

## 验证

```bash
cargo test -p synapse-services --features test-utils --lib event_notifier  # ✅ 26/26
```

## 相关文件

- `synapse-services/src/event_notifier.rs`
- `synapse-services/src/container.rs`