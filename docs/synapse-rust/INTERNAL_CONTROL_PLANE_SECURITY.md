# synapse-rust 内部控制面接口安全门禁

本文档聚焦 synapse-rust 的“内部控制面/worker 控制面”接口：哪些接口属于内部用途、默认如何避免公网暴露、以及生产环境的最小安全门禁。

## 1. 内部接口暴露清单

### 1.1 Worker 控制面（HTTP）

前缀：`/_synapse/worker/v1/*`

- 管理类端点（需要 Admin JWT）
  - `POST /_synapse/worker/v1/register`
  - `GET /_synapse/worker/v1/workers`
  - `GET /_synapse/worker/v1/workers/type/{worker_type}`
  - `GET /_synapse/worker/v1/workers/{worker_id}`
  - `DELETE /_synapse/worker/v1/workers/{worker_id}`
  - `POST /_synapse/worker/v1/workers/{worker_id}/commands`
  - `POST /_synapse/worker/v1/tasks`
  - `GET /_synapse/worker/v1/tasks`
  - `GET /_synapse/worker/v1/statistics`
  - `GET /_synapse/worker/v1/statistics/types`
  - `GET /_synapse/worker/v1/select/{task_type}`

- worker 调用类端点（需要 replication shared-secret）
  - `POST /_synapse/worker/v1/workers/{worker_id}/heartbeat`
  - `POST /_synapse/worker/v1/workers/{worker_id}/connect`
  - `POST /_synapse/worker/v1/workers/{worker_id}/disconnect`
  - `GET  /_synapse/worker/v1/workers/{worker_id}/commands`
  - `POST /_synapse/worker/v1/commands/{command_id}/complete`
  - `POST /_synapse/worker/v1/commands/{command_id}/fail`
  - `POST /_synapse/worker/v1/tasks/claim/{worker_id}`
  - `POST /_synapse/worker/v1/tasks/{task_id}/claim/{worker_id}`
  - `POST /_synapse/worker/v1/tasks/{task_id}/complete`
  - `POST /_synapse/worker/v1/tasks/{task_id}/fail`
  - `GET  /_synapse/worker/v1/replication/{worker_id}/position?stream_name=...`
  - `PUT  /_synapse/worker/v1/replication/{worker_id}/{stream_name}`
  - `GET  /_synapse/worker/v1/events`

### 1.2 Worker metrics（独立监听）

synapse_worker 二进制提供 `GET /metrics`（Prometheus 格式文本）。

## 2. 默认安全策略（避免公网误用）

- 默认配置下不暴露 worker 控制面
  - 仅当 `worker.enabled=true` 时才注册 `/_synapse/worker/v1/*` 路由
- worker 调用类端点必须使用独立的 shared-secret 头
  - 请求头：`x-synapse-worker-secret: <secret>`
  - 避免与用户请求的 `Authorization` 冲突
- synapse_worker metrics 默认仅监听 localhost
  - 默认 host：`127.0.0.1`
  - 默认 port：`9091`
  - 可选 token 鉴权：`SYNAPSE_WORKER_METRICS_TOKEN`
  - 若绑定非 localhost，必须配置 `SYNAPSE_WORKER_METRICS_TOKEN` 才会启动 metrics listener

## 3. 生产反向代理与网络隔离建议

- 不要把 `/_synapse/worker/v1/*` 暴露到公网
  - 推荐仅在内网/VPC 可达，或仅对 worker 子网放通
- 若必须通过反代访问
  - 确保 `/_synapse/worker/v1/*` 仅允许来自可信源 IP
  - 对管理类端点额外要求 Admin JWT；对 worker 调用类端点要求 `x-synapse-worker-secret`
- metrics
  - 推荐仅在 localhost/内网监听并由 Prometheus 内网抓取
  - 若走反代，务必启用 token 或反代层认证并限制来源

## 4. 配置与门禁（可回归）

### 4.1 Worker 控制面开关

- `worker.enabled`
  - `false`：不注册 `/_synapse/worker/v1/*` 路由（默认）
  - `true`：启用 worker 控制面路由

### 4.2 Replication shared-secret

- `worker.replication.http.enabled=true` 时：
  - worker 调用类端点缺失/错误 `x-synapse-worker-secret` 必须返回 401
  - `secret` 或 `secret_path` 至少配置其一

### 4.3 synapse_worker metrics

- `SYNAPSE_WORKER_METRICS_HOST`：默认 `127.0.0.1`
- `SYNAPSE_WORKER_METRICS_PORT`：默认 `9091`
- `SYNAPSE_WORKER_METRICS_TOKEN`：可选；设置后必须 `Authorization: Bearer <token>` 才能访问
