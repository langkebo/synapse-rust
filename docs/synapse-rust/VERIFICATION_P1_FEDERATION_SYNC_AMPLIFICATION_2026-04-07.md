# P1/P2 验证证据：联邦/同步放大治理、性能门禁与数据生命周期 — 2026-04-07

本文件记录本轮生产就绪任务中已落地部分的可回归验证证据。

## 1. 覆盖范围

- 联邦 server-keys 拉取（用于 key query 与联邦验签的远端 key 获取）
  - 全局并发上限：`federation.key_fetch_max_concurrency`
  - 请求超时：`federation.key_fetch_timeout_ms`
  - 失败 backoff：按 `origin+key_id` 短期缓存，避免验签/拉 key 风暴下无限重试
- 受控 429 契约（retry_after_ms）
  - `GET /_matrix/client/v3/sync`
  - `POST /_matrix/client/v3/sync`（Sliding Sync）
- 同步资源隔离（initial vs incremental）
  - 配置：`rate_limit.sync.enabled`、`rate_limit.sync.initial`、`rate_limit.sync.incremental`

## 2. 自动化测试证据

- server-keys 拉取限额与 backoff
  - `cargo test --locked --test integration api_federation_key_fetch_limits_tests -- --nocapture`
    - `test_federation_key_fetch_respects_timeout_config`
    - `test_federation_key_fetch_global_concurrency_limit_is_enforced`
    - `test_federation_key_fetch_backoff_skips_retries`
- 429 retry_after_ms 契约
  - `cargo test --locked --test integration api_rate_limit_contract_tests -- --nocapture`
    - `test_sync_rate_limited_returns_retry_after_ms`
    - `test_sliding_sync_rate_limited_returns_retry_after_ms`
- initial vs incremental 隔离限流
  - `cargo test --locked --test integration api_sync_isolation_rate_limit_tests -- --nocapture`
    - `test_sync_initial_vs_incremental_rate_limit_isolated`
- 生命周期与脚本校验
  - `cargo test --lib test_cutoff_ts_from_days`
  - `cargo test --lib test_cleanup_expired`
  - `bash -n scripts/run_ci_tests.sh`
  - `bash -n scripts/test/perf/run_tests.sh`
  - `python3 -m py_compile scripts/test/perf/guardrail.py`

## 3. 生产检查要点

- 根据部署规模设置：
  - `federation.key_fetch_max_concurrency`：限制全局远端 key 拉取并发，避免 key 风暴挤压 join/sync 等主链路
  - `federation.key_fetch_timeout_ms`：防止远端慢响应导致请求堆积
- 429 行为：
  - 过载时允许返回 `M_LIMIT_EXCEEDED` + `retry_after_ms`，上游应按 `retry_after_ms` 退避重试
- 同步隔离：
  - initial sync（无 `since`）与增量 sync（有 `since`）使用不同令牌桶，避免 initial 抢占增量资源
- 性能门禁：
  - `scripts/run_ci_tests.sh` 已默认支持 smoke gate，CI 中通过 `RUN_PERF_SMOKE=1` 启用
  - `scripts/test/perf/run_tests.sh soak` 与 `scripts/test/perf/guardrail.py --scenarios soak` 可用于 24h soak gate 基础门禁
- 数据生命周期：
  - `RetentionService::run_data_lifecycle_cycle` 统一执行事件、beacon、上传残留、审计事件、清理队列处理与裁剪
  - `/_synapse/admin/v1/retention/run` 与 `/_synapse/admin/v1/retention/status` 提供手工入口与最近一次摘要

## 4. 本轮补齐项

- P1：join 隔离舱、EDU（presence）处理的并发上限/隔离 + 配套 metrics 与压测场景
- P1：性能 smoke 门禁化到 CI
- P2：数据生命周期（位置/媒体/审计/队列）保留与持续清理任务 + 可观测 + 24h soak 门禁
