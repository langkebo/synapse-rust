# W-03: ScheduledTasks 4 循环无 shutdown hook

## 严重级
🟡 **P1** — 重要稳定性改进

## 状态
✅ **已修复** — commit `3de52959`

## 问题描述

`src/tasks/mod.rs` 中 `ScheduledTasks::start_all()` 启动的 4 个后台循环没有任何 shutdown signal：

| 方法 | 行号 | 间隔 |
|------|------|------|
| `start_health_check_task` | 116 | 10s |
| `start_performance_check_task` | 153 | 300s（含 60s 启动延迟） |
| `start_integrity_check_task` | 193 | 3600s（含 60s 启动延迟） |
| `start_maintenance_task` | 237 | 86400s（含 300s 启动延迟） |

## 影响

- SIGTERM 后循环继续运行，会持续输出错误日志：
  - `error!("Failed to perform database health check: {}", e)` (DB 已关闭)
  - `error!("Failed to collect performance metrics: {}", e)`
  - `error!("Failed to verify data integrity: {}", e)`
  - `error!("Database maintenance failed: {}", e)`
- 进程退出延迟最长 86400s（maintenance interval）—— 实际上 startup sleep 300s + interval 86400s 是最坏情况

## 修复方案

- `start_all(&self, shutdown: CancellationToken)` — 增加参数
- 每个 `start_*_task(&self, shutdown: CancellationToken)` — 接收 token
- 循环体改为 `tokio::select! { biased; _ = shutdown.cancelled() => break, _ = interval.tick() => { ... } }`
- Startup sleep 也包入 `select!` — SIGTERM 在 grace window 内也能立即退出
- Call site: `src/server/mod.rs:280` 传 `self.app_state.services.shutdown_token.clone()`

## 验证

```bash
cargo clippy --workspace --features "test-utils" -- -D warnings  # ✅
```

## 相关文件

- `src/tasks/mod.rs`
- `src/server/mod.rs`