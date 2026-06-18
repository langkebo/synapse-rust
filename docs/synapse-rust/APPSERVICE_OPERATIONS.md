# synapse-rust AppService 运维手册

> 版本: v1.1
> 日期: 2026-06-18
> 基线: 对标上游 `element-hq/synapse` `docs/application_services.md` 与 `docs/workers.md`
> 维护: 应用服务负责人 + 运维负责人

---

## 一、架构概述

### 1.1 组件关系

```
app_service_config_files (YAML)
    │
    ▼
ApplicationServiceManager
    ├── 启动期 YAML 加载与 namespace 校验
    ├── 运行时事件匹配 (namespace regex)
    ├── pending queue 管理
    └── Scheduler 调度
         ├── TransactionController
         │    ├── 每轮活跃 AS 限流 (MAX_SERVICES_PER_TICK=8)
         │    ├── 优先级: pending transaction > pending events
         │    └── Round-robin 调度
         ├── Recoverer
         │    ├── 基础 backoff 退避
         │    ├── HTTP 失败分类
         │    └── 连续 fatal 失败自动禁用
         └── 聚合状态写回
              ├── scheduler_transaction_state
              ├── scheduler_last_result / scheduler_last_tick_ts
              └── success/failure/backoff/capacity 计数器
```

### 1.2 配置示例

```yaml
app_service_config_files:
  - /path/to/bridge.yaml
```

`bridge.yaml` 示例：

```yaml
id: "my_bridge"
url: "http://localhost:9000"
as_token: "bridge_secret_token"
hs_token: "hs_secret_token"
sender_localpart: "my_bridge_bot"
namespaces:
  users:
    - regex: "@bridge_.*"
      exclusive: true
  rooms:
    - regex: "!bridge_.*"
  aliases:
    - regex: "#bridge_.*"
```

> 说明：当前实例只有在启动时加载了非空的 `app_service_config_files` 时才会自动启动 scheduler；仅通过 admin API 动态注册 appservice，不会在当前进程里补启动 scheduler。

---

## 二、运行状态监控

### 2.1 聚合统计 API

```bash
# AppService 统计（实时聚合）
curl -H "Authorization: Bearer $ADMIN_TOKEN" \
  http://localhost:8008/_synapse/admin/v1/appservices/statistics
```

返回示例：

```json
[
  {
    "as_id": "my_bridge",
    "is_enabled": true,
    "pending_event_count": 42,
    "pending_transaction_count": 1,
    "scheduler": {
      "available": true,
      "backlog_state": "high",
      "last_result": "dispatched",
      "last_tick_ts": 1781765986747,
      "last_dispatched_events": 10,
      "last_elapsed_ms": 145,
      "pending_event_count": 42,
      "pending_transaction_count": 1,
      "transaction_state": "pending_transaction",
      "total_success_count": 1500,
      "total_failure_count": 3,
      "total_backoff_count": 5,
      "total_capacity_limited_count": 2
    }
  }
]
```

### 2.2 Telemetry Metrics

```bash
curl -H "Authorization: Bearer $ADMIN_TOKEN" \
  http://localhost:8008/_synapse/admin/v1/telemetry/metrics | jq '.appservice_scheduler'
```

### 2.3 Prometheus Metrics

```bash
curl http://localhost:9090/metrics | grep '^synapse_appservice_scheduler_'
```

关键 Prometheus 指标：

| 指标 | 说明 |
|------|------|
| `synapse_appservice_scheduler_total_services` | appservice 总数 |
| `synapse_appservice_scheduler_available_services` | scheduler 当前可见服务数 |
| `synapse_appservice_scheduler_backoff_services` | 当前快照处于 backoff 的服务数 |
| `synapse_appservice_scheduler_capacity_limited_services` | 当前快照处于容量限流的服务数 |
| `synapse_appservice_scheduler_services_with_pending_transactions` | 当前存在 pending transaction 的服务数 |
| `synapse_appservice_scheduler_pending_events` | 待投递事件总数 |
| `synapse_appservice_scheduler_pending_transactions` | 待处理 transaction 总数 |
| `synapse_appservice_scheduler_success_count` | 聚合成功投递次数 |
| `synapse_appservice_scheduler_failure_count` | 聚合失败次数 |
| `synapse_appservice_scheduler_backoff_count` | 聚合退避次数 |
| `synapse_appservice_scheduler_capacity_limited_count` | 聚合容量限流次数 |
| `synapse_appservice_scheduler_in_flight_count` | 聚合 in-flight 数量 |

### 2.4 Worker 拓扑验证

```bash
# 验证 appservice worker 状态
curl http://localhost:8008/_synapse/worker/v1/topology/validate
```

---

## 三、调度参数调优

### 3.1 当前默认值

| 参数 | 默认值 | 说明 |
|------|--------|------|
| `MAX_SERVICES_PER_TICK` | 8 | 每轮调度最大活跃 AS 数 |
| `HIGH_PENDING_EVENT_THRESHOLD` | 50 | 事件积压高水位阈值 |
| `HIGH_PENDING_TRANSACTION_THRESHOLD` | 2 | Transaction 积压高水位阈值 |
| `MAX_FATAL_FAILURES` | 3 | 连续 fatal 失败后自动禁用 |

### 3.2 调度策略

1. **优先级排序**：有 pending transaction 的 AS 优先于仅有 pending event 的 AS
2. **Round-robin**：同优先级内按 AS 注册顺序轮转
3. **容量限流**：每轮最多处理 `MAX_SERVICES_PER_TICK` 个活跃 AS
4. **退避重试**：失败 AS 按 `retry_count` 执行指数退避

### 3.3 阈值调优指南

| 场景 | 建议 |
|------|------|
| 少量 AS + 高频事件 | 适当提高 `MAX_SERVICES_PER_TICK` |
| 大量 AS + 低频事件 | 维持默认值，监控 capacity_limited 计数 |
| event backlog 持续增长 | 降低 `HIGH_PENDING_EVENT_THRESHOLD` 触发更早告警 |
| transaction 频繁超时 | 检查 bridge 服务响应时间，而非调整阈值 |
| 多个 AS 同时 recovery | 确认 `MAX_SERVICES_PER_TICK` 足够容纳 recovery + 正常 service |

---

## 四、故障定位

### 4.1 Bridge 事件未投递

| 症状 | 排查步骤 |
|------|----------|
| 事件未到达 bridge | 1. 检查 namespace regex 是否匹配 |
|  | 2. `curl .../appservices/statistics` 查看 pending_events |
|  | 3. 检查 bridge 服务是否可达 |
|  | 4. 查看 worker 日志 `[APPSERVICE]` 前缀 |
| AS 被自动禁用 | 1. 检查连续 fatal 失败次数 |
|  | 2. 修复 bridge 后手动重新启用（见 5.2） |
| 事件积压不降 | 1. 检查 bridge 处理能力 |
|  | 2. 检查 `MAX_SERVICES_PER_TICK` 是否需要调整 |
|  | 3. 检查是否有 AS 处于 retry_backoff 独占调度窗口 |

### 4.2 Transaction 无法完成

| 症状 | 排查 |
|------|------|
| Transaction 一直 pending | 检查 bridge 的 `PUT /transactions/{txn_id}` 端点 |
| HTTP 返回 5xx | 检查 bridge 服务日志，排查内部错误 |
| HTTP 超时 | 检查网络连通性，增加 bridge 超时配置 |
| 同一 AS 有多个并发 transaction | 此为异常状态，scheduler 应保证单 AS 单 transaction |

### 4.3 Scheduler 状态异常

```bash
# 查看 scheduler 聚合状态
curl -H "Authorization: Bearer $ADMIN_TOKEN" \
  http://localhost:8008/_synapse/admin/v1/appservices/statistics | jq '.[] | {as_id, pending_event_count, pending_transaction_count, scheduler: {last_result: .scheduler.last_result, transaction_state: .scheduler.transaction_state, backlog_state: .scheduler.backlog_state}}'
```

关键状态字段说明：

| 字段 | 取值 | 含义 |
|------|------|------|
| `scheduler.transaction_state` | `idle` | 当前无待处理 transaction |
| `scheduler.transaction_state` | `pending_transaction` | 有待重试或待继续处理的 transaction |
| `scheduler.transaction_state` | `retry_backoff` | 当前正在退避窗口内 |
| `scheduler.transaction_state` | `capacity_limited` | 当前 tick 被容量边界跳过 |
| `scheduler.last_result` | `dispatched` | 最近一个 tick 已发起 dispatch 或完成健康推进 |
| `scheduler.last_result` | `success` | 最近一个观测窗口确认成功 |
| `scheduler.last_result` | `backoff` | 最近一个 tick 明确进入 backoff |
| `scheduler.last_result` | `capacity_limited` | 最近一个 tick 被容量限流 |

> 注意：live 场景中 `last_result="dispatched"` 与 `transaction_state="pending_transaction"` 可以同时出现，这表示该服务当前仍有 pending transaction，但最近一次 tick 并未再次进入 retry 窗口；是否发生过退避应优先看 `total_backoff_count`。

---

## 五、运维操作

### 5.1 查看 AS 配置

```bash
curl -H "Authorization: Bearer $ADMIN_TOKEN" \
  http://localhost:8008/_synapse/admin/v1/appservices
```

### 5.2 手动推送事件

```bash
curl -X POST \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  http://localhost:8008/_synapse/admin/v1/appservices/my_bridge/events \
  -d '{
    "room_id": "!room:example.com",
    "event_type": "m.room.message",
    "sender": "@user:example.com",
    "content": {
      "msgtype": "m.text",
      "body": "manual appservice push"
    }
  }'
```

> 注意：管理面 `push_event` 会校验 namespace 所有权，非 AS 管辖范围内的事件会被拒绝。

### 5.3 手动重新启用被禁用的 AS

被自动禁用的 AS 需在修复 bridge 后重新启用。当前无需重启服务，可直接通过现有管理 API 更新 `is_enabled=true`：

```bash
curl -X PUT \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  http://localhost:8008/_synapse/admin/v1/appservices/my_bridge \
  -d '{
    "is_enabled": true
  }'
```

如需同时恢复说明信息或 URL，也可在同一 `PUT` 请求中一并更新对应字段。

### 5.4 清除积压事件

```bash
# 查看积压量
curl -H "Authorization: Bearer $ADMIN_TOKEN" \
  http://localhost:8008/_synapse/admin/v1/appservices/statistics | jq '.[].pending_event_count'

# 若 bridge 持续不可用，可考虑重启服务清空 in-memory queue
# 或等待 scheduler 自然消耗（不会丢失持久化事件）
```

---

## 六、生产压测

### 6.1 压测场景

| 场景 | 描述 | 验收指标 |
|------|------|----------|
| event-only | 纯事件积压，无 transaction | 入队到首次 dispatch p95 <= 200ms |
| transaction-only | 纯 transaction 积压 | transaction 重试间隔符合退避策略 |
| mixed | 事件 + transaction 混合 | 无长期饥饿，transaction 优先 |
| mixed+backoff | 混合 + 部分 AS 退避 | 失败 AS 不阻塞健康 AS |
| recovery | 多个 AS 同时恢复 | 恢复窗口内所有 AS 获得 dispatch |
| continuous-ingress | 持续事件流入 | 积压不无限增长 |
| super-event-heavy | 超大事件量单一 AS | 其他 AS 不被饿死 |

### 6.2 压测执行

```bash
# 1. 准备压测环境
# 2. 启动监控
bash scripts/collect_redis_observability.sh &
bash scripts/collect_pg_observability.sh &

# 3. 注入负载（通过 API 或专用压测脚本）
# 4. 观察三出口一致性
curl -H "Authorization: Bearer $ADMIN_TOKEN" \
  http://localhost:8008/_synapse/admin/v1/appservices/statistics | jq .
curl -H "Authorization: Bearer $ADMIN_TOKEN" \
  http://localhost:8008/_synapse/admin/v1/telemetry/metrics | jq '.appservice_scheduler'
curl http://localhost:9090/metrics | grep '^synapse_appservice_scheduler_'

# 5. 对比日志
docker compose logs synapse-rust | grep "\[APPSERVICE\]"
```

如需重复执行较长时间窗的单场景 soak，可直接使用：

```bash
python3 scripts/appservice_extended_soak.py continuous-ingress --duration 90 \
  --output /tmp/appservice-soak/continuous-ingress.json
python3 scripts/appservice_extended_soak.py mixed-backoff --duration 45 \
  --output /tmp/appservice-soak/mixed-backoff.json
python3 scripts/appservice_extended_soak.py recovery --recovery-wait 20 \
  --output /tmp/appservice-soak/recovery.json
```

如需把单场景 soak 直接作为门禁，可追加：

```bash
python3 scripts/appservice_extended_soak.py mixed-backoff \
  --duration 45 \
  --fail-on warning \
  --output /tmp/appservice-soak/mixed-backoff.json
```

`--fail-on` 支持：

- `never`：只输出样本，不根据结果返回失败
- `failure`：仅在判定为 `失败` 时返回非零退出码
- `warning`：在 `预警` 或 `失败` 时都返回非零退出码

`appservice_extended_soak.py` 当前会同时抓取：

- `/_synapse/admin/v1/appservices/statistics`
- `/_synapse/admin/v1/telemetry/metrics`
- Prometheus `synapse_appservice_scheduler_*` 指标

输出结果包含：

- `scenario_metrics`
  - 场景本身的注入量、重点 AS 状态或 recovery 服务样本
- `preflight` / `final`
  - 场景开始前和结束后的三出口聚合快照
- `consistency`
  - 三出口关键聚合值是否一致、差异字段列表和逐字段对比明细

如 Prometheus 不在默认地址，可追加：

```bash
python3 scripts/appservice_extended_soak.py mixed-backoff \
  --duration 45 \
  --prometheus-url http://localhost:9090/metrics \
  --output /tmp/appservice-soak/mixed-backoff.json
```

如需按审计报告 `P0-02` 的关键聚合口径直接归档三出口结果，可使用：

```bash
ADMIN_TOKEN=xxxx bash scripts/appservice_stress_test.sh mixed-backoff 60
```

脚本会在 `/tmp/appservice_stress_results_<pid>/` 下输出：

- `outlet-consistency-preflight.json` / `outlet-consistency-post-run.json`
  - 统一对比 `statistics`、`telemetry`、Prometheus 三出口的以下关键聚合值：
    `total_services`、`scheduler_available_services`、`services_in_backoff`、
    `services_capacity_limited`、`services_with_pending_transactions`、
    `total_pending_events`、`total_pending_transactions`、`total_success_count`、
    `total_failure_count`、`total_backoff_count`、`total_capacity_limited_count`、
    `total_in_flight_count`
- `<scenario>.json`
  - 单场景标准化结果，包含场景指标、三出口聚合快照和一致性结论，可直接作为压测记录模板的原始样本

如需把长时间窗场景直接汇总成日报，可使用：

```bash
python3 scripts/appservice_daily_report.py \
  --day D2 \
  --output-dir /tmp/appservice-daily-report \
  --resource-summary "CPU 0.9 core, RSS +13%, pool 68%, no slow-query burst, no external spill"
```

如需把整天的 `D1/D2/D3` 日报直接接入 CI 或值班门禁，可追加：

```bash
python3 scripts/appservice_daily_report.py \
  --day D2 \
  --fail-on warning \
  --output-dir /tmp/appservice-daily-report \
  --resource-summary "CPU 0.9 core, RSS +13%, pool 68%, no slow-query burst, no external spill"
```

当 `--fail-on warning` 生效时：

- `保持默认值`：退出码 `0`
- `继续观察`：退出码 `1`
- `进入参数评审`：退出码 `1`

## 5. 部署 / CI 门禁接入

### 5.1 部署后 smoke gate

可在现有部署烟雾测试中直接开启 `appservice` 日报门禁：

```bash
ADMIN_AUTH_HEADER="Authorization: Bearer $ADMIN_TOKEN" \
RUN_APPSERVICE_GATE=1 \
APPSERVICE_GATE_DAY=D2 \
APPSERVICE_GATE_FAIL_ON=warning \
APPSERVICE_GATE_RESOURCE_SUMMARY="deploy gate; 待补 CPU/RSS/连接池摘要" \
bash scripts/deployment_smoke_test.sh
```

可用环境变量：

- `RUN_APPSERVICE_GATE=1`：启用 `appservice` gate
- `APPSERVICE_GATE_DAY`：选择 `D1` / `D2` / `D3`
- `APPSERVICE_GATE_FAIL_ON`：选择 `never` / `failure` / `warning`
- `APPSERVICE_GATE_OUTPUT_DIR`：指定日报和原始样本输出目录

### 5.2 CI 可选 gate

如 CI 环境已准备好 `appservice` 配置和 `ADMIN_TOKEN`，可在 `docker` 验证阶段追加：

```bash
RUN_APPSERVICE_GATE=1 \
APPSERVICE_GATE_ADMIN_TOKEN="$ADMIN_TOKEN" \
APPSERVICE_GATE_DAY=D2 \
APPSERVICE_GATE_FAIL_ON=warning \
APPSERVICE_GATE_RESOURCE_SUMMARY="ci gate; 待补 CPU/RSS/连接池摘要" \
bash scripts/ci_backend_validation.sh docker
```

注意：

- `ci_backend_validation.sh` 默认不会启用该 gate
- 只有当 CI 环境实际挂载了 `appservice` 配置、bridge 和指标抓取后，才建议打开

## 6. 阈值调优执行清单

正式调优流程、样本要求、单变量调参顺序、回退条件见：

- [APPSERVICE_THRESHOLD_TUNING_CHECKLIST.md](file:///Users/ljf/Desktop/hu_ts/synapse-rust/docs/synapse-rust/APPSERVICE_THRESHOLD_TUNING_CHECKLIST.md)

## 7. P0 D2 一键执行

如需在目标环境直接跑一轮 `P0` 的 `D2` 样本并自动归档，可使用：

```bash
ADMIN_TOKEN="$ADMIN_TOKEN" \
APPSERVICE_D2_RESOURCE_SUMMARY="baseline run; 待补 CPU/RSS/连接池/慢查询摘要" \
bash scripts/run_appservice_p0_d2.sh baseline
```

参数调整后复跑：

```bash
ADMIN_TOKEN="$ADMIN_TOKEN" \
APPSERVICE_D2_RESOURCE_SUMMARY="after change; 待补 CPU/RSS/连接池/慢查询摘要" \
bash scripts/run_appservice_p0_d2.sh after-change
```

默认会生成：

```text
artifacts/appservice/<date>/
  baseline/
  after-change/
  decision.md
```

其中：

- `baseline/`、`after-change/` 下会保存 `daily-report.json`、`daily-report.md`、场景原始 `json`、`resource-summary.txt`、`run-metadata.json`
- `decision.md` 会在首次执行时自动生成，作为本轮调优结论和回退记录模板

可选环境变量：

- `APPSERVICE_D2_DATE`：指定归档日期
- `APPSERVICE_D2_FAIL_ON`：默认 `warning`
- `APPSERVICE_D2_OUTPUT_ROOT`：指定归档根目录
- `APPSERVICE_D2_RESOURCE_FILE`：从文件读取资源摘要
- `APPSERVICE_D2_NEXT_PLAN`：写入 `decision.md` 的下一步计划

该脚本按日报阶段自动选择默认场景：

| 阶段 | 默认场景 |
|------|----------|
| `D1` | `event-only`、`transaction-only`、`mixed` |
| `D2` | `mixed-backoff`、`recovery`、`continuous-ingress` |
| `D3` | `super-event-heavy` |

并在输出目录生成：

- `<scenario>.json`
  - 每个已执行场景的原始样本；`D1/D3` 会直接输出对应 stress 场景结果，`D2` 会输出 long-window soak 样本
- `daily-report.json`
  - 结构化日报摘要，可供后续程序消费
- `daily-report.md`
  - 直接贴近审计报告 `P0-02 压测日报模板` 的文本版日报

如需覆盖默认场景顺序，可显式传入：

```bash
python3 scripts/appservice_daily_report.py \
  --day D1 \
  --scenarios event-only transaction-only mixed \
  --output-dir /tmp/appservice-d1-report
```

`appservice_stress_test.sh` 现支持通过环境变量覆盖结果目录，便于被日报脚本或 CI 包装复用：

```bash
RESULTS_DIR=/tmp/appservice-stress/d1 \
ADMIN_TOKEN=xxxx \
bash scripts/appservice_stress_test.sh event-only 60
```

### 6.3 2026-06-18 Live 验证摘要

| 场景 | 结论 | 关键观测 |
|------|------|----------|
| `event-only` | 通过 | 本地 probe 可拉起 scheduler 观测，三出口一致可读 |
| `transaction-only` | 通过 | `stress_as_1` 持续失败时 `pending_transaction=1`，`total_backoff_count=12`，统计面可见 `retry_backoff` |
| `mixed` | 通过 | 20 秒 steady-state 后 `pending_event_count=3`、`pending_transaction_count=0`、`total_success_count=115` |
| `mixed-backoff` | 通过 | 健康 `stress_as_1` 成功推进；失败 `stress_as_2` 保持 `pending_transaction=1`，`total_backoff_count=24` |
| `recovery` | 通过 | 5 个服务都恢复到 `scheduler_available_services=5`、`pending=0`；累计 `failure_count=5`、`backoff_count=40` |
| `continuous-ingress` | 通过 | 20 秒内注入 `270` 条事件，结束时剩余积压 `3`，未出现无限增长 |
| `super-event-heavy` | 通过 | 重 AS 成功 `5`、轻 AS 合计成功 `5`，未出现轻服务饿死 |

### 6.4 Extended Soak 追加观察

| 场景 | 结果 | 观察 |
|------|------|------|
| `continuous-ingress` `90s` | 通过 | 注入 `1206` 条事件后，最终仅剩 `pending=3`，`capacity_limited=0`，未见 backlog 发散 |
| `mixed-backoff` `45s` | 关注 | 健康与失败服务都抬升到约 `190` 级别 `pending_event_count`，且双方都出现 `pending_transaction=1`；`capacity_limited=0`，但 backlog 在较长窗口内明显堆积 |
| `recovery` `20s wait` | 通过 | 5 个服务最终都回到 `transaction_state=idle` 且 `pending=0`，telemetry 汇总为 `scheduler_available_services=5`、`services_with_pending_transactions=0` |

> 解释：`mixed-backoff` 的 extended soak 结果说明，当前默认阈值虽然还没有直接打出 `capacity_limited`，但在失败服务持续存在的较长窗口里，健康服务的 backlog 也会被明显抬高。这个现象更像“长时间公平性/排空速度”风险，而不是短窗口功能失败。

### 6.5 压测记录模板

参见审计报告 `P0-02 压测记录模板` 与 `P0-02 压测日报模板`。

---

## 七、告警配置

### 7.1 Prometheus 告警规则

```yaml
groups:
  - name: synapse_appservice
    rules:
      - alert: AppServiceHighPendingEvents
        expr: synapse_appservice_scheduler_pending_events > 5000
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "AppService pending events high"

      - alert: AppServiceHighBackoff
        expr: rate(synapse_appservice_scheduler_backoff_count[5m]) > 0.5
        for: 10m
        labels:
          severity: warning
        annotations:
          summary: "AppService backoff rate is high"

      - alert: AppServiceHighCapacityLimited
        expr: rate(synapse_appservice_scheduler_capacity_limited_count[5m]) > 0.5
        for: 10m
        labels:
          severity: warning
        annotations:
          summary: "AppService capacity limit frequently hit"
```

### 7.2 日志告警

关注以下日志模式：

```
[APPSERVICE] Fatal delivery failure for AS {id}, disabling service
[APPSERVICE] Capacity limited: {n} services skipped this tick
[APPSERVICE] Recovery: AS {id} back to active after {n} retries
```

---

## 八、相关文档

- 综合审计报告：[COMPREHENSIVE_AUDIT_REPORT_2026-06-03.md](COMPREHENSIVE_AUDIT_REPORT_2026-06-03.md)（P0-02 章节）
- 运维手册：[OPERATIONS.md](OPERATIONS.md)
- Worker 拓扑基线：[WORKER_TOPOLOGY_BASELINE_2026-06-14.md](WORKER_TOPOLOGY_BASELINE_2026-06-14.md)
