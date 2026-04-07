# Worker 控制面安全门禁（/_synapse/worker/v1）— 2026-04-07

本文件定义 synapse-rust 的 worker 控制面（`/_synapse/worker/v1/*`）在生产环境中的默认安全要求与验证证据。

## 1. 风险背景

- worker 控制面属于内部接口：包含 worker 注册、任务/命令分发、replication position 更新与事件拉取等能力
- 若这些端点仅依赖普通用户 JWT，将导致“任意泄露的用户 token”具备影响 worker/replication 的能力
- 若控制面被公网暴露，将成为高风险攻击面（DoS、越权、数据面间接损害）

## 2. 安全策略（P0 Gate）

### 2.1 Worker 调用类端点：shared-secret 门禁（不依赖用户 JWT）

当 `worker.replication.http.enabled=true` 时，以下端点必须携带正确的 shared-secret：

- `POST /_synapse/worker/v1/workers/{worker_id}/heartbeat`
- `POST /_synapse/worker/v1/workers/{worker_id}/connect`
- `POST /_synapse/worker/v1/workers/{worker_id}/disconnect`
- `GET /_synapse/worker/v1/workers/{worker_id}/commands`
- `POST /_synapse/worker/v1/commands/{command_id}/complete`
- `POST /_synapse/worker/v1/commands/{command_id}/fail`
- `POST /_synapse/worker/v1/tasks/claim/{worker_id}`
- `POST /_synapse/worker/v1/tasks/{task_id}/claim/{worker_id}`
- `POST /_synapse/worker/v1/tasks/{task_id}/complete`
- `POST /_synapse/worker/v1/tasks/{task_id}/fail`
- `GET /_synapse/worker/v1/replication/{worker_id}/position`
- `PUT /_synapse/worker/v1/replication/{worker_id}/{stream_name}`
- `GET /_synapse/worker/v1/events`

shared-secret header：

- `x-synapse-worker-secret: <secret>`

端点不要求 `Authorization`，避免 worker 侧持有用户/管理员 token；管理权限与 worker 调用权限分离。

### 2.2 管理类端点：管理员 JWT

以下端点仅允许管理员 token 调用（AdminUser）：

- worker 注册/查询/删除
- 任务分配与统计/选 worker
- command 下发（POST commands）

## 3. 配置要求

必须配置以下之一：

- `worker.replication.http.secret`
- `worker.replication.http.secret_path`

并显式开启：

- `worker.replication.http.enabled: true`

## 4. 验证证据（可回归）

- 集成测试：`api_worker_replication_auth_tests::test_worker_endpoints_require_replication_secret_when_enabled`

## 5. 相关文档

- 生产就绪检查清单：`PRODUCTION_READINESS_CHECKLIST_2026-04-07.md`
- 生产就绪优化方案：`PRODUCTION_READINESS_OPTIMIZATION_PLAN_2026-04-07.md`
