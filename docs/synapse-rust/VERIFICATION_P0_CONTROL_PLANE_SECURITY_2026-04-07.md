# P0 验证证据：控制面与内部接口安全 — 2026-04-07

本文件记录 P0 “控制面与内部接口安全”相关变更的可回归验证证据与操作要点。

## 1. 覆盖范围

- `/_synapse/worker/v1/*`
  - worker 调用类端点：replication shared-secret（`x-synapse-worker-secret`）
  - 管理类端点：Admin JWT
  - 默认不注册：仅当 `worker.enabled=true` 才注册路由
- `synapse_worker` 的 `/metrics`
  - 默认绑定 localhost
  - 可选 token 鉴权
  - 若绑定非 localhost，必须配置 token 才启动 metrics listener

## 2. 自动化测试证据

- integration
  - `cargo test --locked --test integration api_worker_replication_auth_tests -- --nocapture`
    - `test_worker_endpoints_require_replication_secret_when_enabled`
    - `test_worker_endpoints_do_not_require_replication_secret_when_disabled`
    - `test_admin_worker_endpoints_still_require_admin_jwt`
- unit
  - `cargo test --locked --test unit worker_api_tests -- --nocapture`

## 3. 人工检查要点（生产部署前）

- 确认 `worker.enabled=false` 时外部无法访问 `/_synapse/worker/v1/*`
- 若启用 worker 控制面：
  - worker 调用类端点仅允许内网访问，并启用 `worker.replication.http.enabled=true` + 配置 secret/secret_path
  - 管理类端点通过反代层或网络策略限制来源，并要求 Admin JWT
- metrics：
  - 推荐仅在 localhost/内网监听并由 Prometheus 抓取
  - 若必须绑定非 localhost，配置 `SYNAPSE_WORKER_METRICS_TOKEN`，并限制来源 IP

