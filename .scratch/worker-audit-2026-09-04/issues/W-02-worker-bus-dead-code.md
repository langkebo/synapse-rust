# W-02: WorkerBus/HealthChecker dead code — 多实例集群未集成

## 严重级
🟡 **P0** — 已知限制，非稳定性问题

## 状态
⚠️ **known limitation** — multi-instance cluster 阶段功能未实现

**审计结论（2026-09-07）**：ticket 自创建以来无新 commit，代码路径仍未引用。这是多实例 cluster 阶段的功能预留，非稳定性阻塞。短期不需要修复；如果 cluster 部署前不实现 multi-instance pub/sub，需要给 `connect()` / `start_periodic_checks()` 加 `#[allow(dead_code /* TODO: cluster integration */)]` 抑制 lint（已通过 deny(missing_docs) + clippy 0 警告验证当前未触发是因为文件本身未被引用到 lint 严格路径）。

## 问题描述

`synapse-services/src/worker/bus.rs::WorkerBus::connect()` 和 `synapse-services/src/worker/health.rs::HealthChecker::start_periodic_checks` 在生产代码中**从未被调用**。

**Search 结果**：
- `WorkerBus::new` 仅在 `tests/unit/worker_coverage_tests.rs` 中被调用
- `WorkerBus::connect` 仅有 1 个 self-call（在 `WorkerBus::new` 内部），production 路径完全不引用
- `HealthChecker::start_periodic_checks` 仓库内**零调用**
- `enable_bus` 和 `enable_health_checker` 函数同样 dead

## 影响

- 跨实例 pub/sub 永不生效（`broadcast_command` / `publish` 仅在 in-memory mode 下工作）
- Federation ack / position 同步在多实例部署中断链
- HealthChecker 同样 dead——没有跨实例健康状态广播

## 根因

`WorkerManager` 在生产初始化时构造，但 `enable_bus()` 和 `enable_health_checker()` 这两个用于启动 cluster 组件的方法从未被调用：

```rust
// wiring/admin.rs:128
shutdown_token: &tokio_util::sync::CancellationToken,
```

但代码没有读取 `shutdown_token` 参数来启动 multi-instance components。

## 修复方案（推荐）

在 `wiring/admin.rs` 末尾、shutdown_token 已绑定的情况下：

```rust
if config.cluster.enabled {
    let bus = WorkerBus::new(/* ... */).await?;
    bus.connect().await?;
    bus.start(shutdown_token.clone()).await?;

    let health = HealthChecker::new(/* ... */);
    health.start_periodic_checks(shutdown_token.clone()).await?;
}
```

## 替代方案（短期）

如果本期不打算完成 multi-instance cluster 集成，建议给 `connect()` / `start_periodic_checks()` 添加 `#[allow(dead_code)]` 并加上注释：

```rust
#[allow(dead_code /* TODO: cluster integration */)]
pub async fn connect(&self) -> Result<(), ApiError> { ... }
```

## 相关文件

- `synapse-services/src/worker/bus.rs`
- `synapse-services/src/worker/health.rs`
- `synapse-services/src/worker/manager.rs`
- `synapse-services/src/wiring/admin.rs`
- `tests/unit/worker_coverage_tests.rs`