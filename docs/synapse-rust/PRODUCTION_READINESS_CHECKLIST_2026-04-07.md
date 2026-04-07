# synapse-rust 生产就绪检查清单（P0 已落地）— 2026-04-07

本清单用于把“生产就绪”验收变成可回归的证据集合；当前版本已完成 P0（控制面安全门禁）落地。

## 1. P0：控制面安全（已完成）

- Worker 控制面端点分层
  - 管理类端点继续使用 Admin JWT（AdminUser）
  - worker 调用类端点在 `worker.replication.http.enabled=true` 时额外要求 shared-secret
- shared-secret 传递方式
  - Header：`x-synapse-worker-secret: <secret>`
  - JWT 仍走 `Authorization: Bearer <access_token>`
- 回归证据
  - 集成测试：`api_worker_replication_auth_tests::test_worker_endpoints_require_replication_secret_when_enabled`
  - 文档：`WORKER_CONTROL_PLANE_SECURITY_2026-04-07.md`

## 2. P0：worker metrics 暴露面（已完成）

- 默认只监听 localhost
  - `SYNAPSE_WORKER_METRICS_HOST`（默认 `127.0.0.1`）
  - `SYNAPSE_WORKER_METRICS_PORT`（默认 `9091`）
- 可选鉴权
  - `SYNAPSE_WORKER_METRICS_TOKEN` 非空时，`GET /metrics` 需要 `Authorization: Bearer <token>`

## 3. 配置示例（homeserver.yaml）

```yaml
worker:
  replication:
    http:
      enabled: true
      secret: "CHANGE_ME"
```

## 4. P1/P2 跟进项（已完成本轮目标）

- P1：联邦 join/EDU/key 拉取放大治理
  - ✅ 已落地：server-keys 拉取并发上限、配置化 timeout、失败 backoff（回归见 `VERIFICATION_P1_FEDERATION_SYNC_AMPLIFICATION_2026-04-07.md`）
  - ✅ 已落地：join 隔离舱、EDU（presence）处理限额与隔离、配套 metrics 与压测场景
- P1：initial sync vs incremental sync 资源隔离门禁化
  - ✅ 已落地：sync initial vs incremental 分离限流（配置项：`rate_limit.sync.*`；回归见 `VERIFICATION_P1_FEDERATION_SYNC_AMPLIFICATION_2026-04-07.md`）
  - ✅ 已落地：性能 smoke 基线解析输出与 CI 门禁
- P2：数据生命周期（位置/媒体/审计/队列）保留与清理策略统一、24h soak 与容量模型门禁
  - ✅ 已落地：位置/媒体/审计/队列保留与持续清理、可观测、手工运维入口、24h soak gate 基础接入
