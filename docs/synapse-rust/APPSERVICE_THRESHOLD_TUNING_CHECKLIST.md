# AppService Threshold Tuning Checklist

> 版本: v1.0
> 日期: 2026-06-18
> 适用范围: `appservice` 生产压测、阈值调优、运维门禁

## 1. 目标

- 为 `MAX_SERVICES_PER_TICK`、`HIGH_PENDING_TRANSACTION_THRESHOLD`、`HIGH_PENDING_EVENT_THRESHOLD` 建立可重复、可回退、可审计的调优流程。
- 统一 `D1 / D2 / D3` 样本采集、日报门禁、人工评审、参数变更、回退决策。
- 输出可直接用于值班和发布评审的执行清单，避免“只采样、不落参”。

## 2. 调优前置条件

在开始任何阈值调整前，必须满足以下前置条件：

- 已完成一轮部署后基础检查：`bash scripts/deployment_smoke_test.sh`
- 已能通过管理接口读取 `/_synapse/admin/v1/appservices/statistics`
- 已具备 Prometheus 指标抓取能力，至少能访问 `appservice_scheduler_*` 指标
- 已准备好 `ADMIN_TOKEN`
- 已确认压测目标环境中存在有效的 `appservice` 配置，且 bridge 行为与生产一致
- 已确认当前默认参数及计划候选参数，并记录在本次调优单中

建议先执行一次门禁日报，确认基线样本可生成：

```bash
ADMIN_TOKEN=... \
BASE_URL=http://127.0.0.1:8008 \
PROMETHEUS_URL=http://127.0.0.1:9090/metrics \
python3 scripts/appservice_daily_report.py \
  --day D2 \
  --fail-on warning \
  --output-dir /tmp/appservice-d2-baseline \
  --resource-summary "baseline run; 待补 CPU/RSS/连接池摘要"
```

## 3. 样本计划

每轮阈值调优至少保留以下三层样本：

| 样本层 | 场景 | 目标 | 最低要求 |
|---|---|---|---|
| `D1` | `event-only` / `transaction-only` / `mixed` | 识别基础限流与 retry/backoff 行为 | 每个场景至少 1 份成功样本 |
| `D2` | `continuous-ingress` / `mixed-backoff` / `recovery` | 验证长时间窗口积压、恢复和三出口一致性 | 每个场景至少 1 份日报样本 |
| `D3` | 多实例或长时间 soak | 验证生产级漂移、恢复、容量边界 | 至少 1 次完整运行并留档 |

如果 `D2` 尚未稳定通过，不进入 `D3` 调参。

## 4. 观测面检查项

每轮样本都必须记录以下观测面：

- 管理接口：`statistics`、`summary`、单服务明细
- Prometheus：`appservice_scheduler_*` 指标
- 资源面：CPU、RSS、数据库连接池占用、慢查询、bridge 外部依赖错误
- 三出口一致性：管理接口、Prometheus、日报聚合结论是否一致

若出现以下任一情况，样本判定为“不可信”，本轮不允许调参：

- 三出口关键聚合不一致
- Prometheus 抓取缺失或延迟严重
- 样本期间 bridge 自身故障导致结论失真
- 样本期间发生与 `appservice` 无关的大规模资源争用

## 5. 调参顺序

推荐按以下顺序单变量调整，禁止同轮同时修改多个核心阈值：

1. `MAX_SERVICES_PER_TICK`
2. `HIGH_PENDING_TRANSACTION_THRESHOLD`
3. `HIGH_PENDING_EVENT_THRESHOLD`

执行原则：

- 每轮只调整 1 个参数
- 每次调整幅度不超过当前值的 `10%` 到 `20%`
- 每次调整后必须重新跑完整 `D2`
- 未完成新一轮 `D2` 之前，不允许继续叠加下一次参数变更

## 6. 判定规则

### 6.1 保持默认值

满足以下条件时保持当前参数：

- `D2` 报告结论为“保持默认值”
- 未观察到稳定的容量瓶颈
- `capacity_limited` 与积压在恢复窗口内可回落
- 资源占用无明显异常

### 6.2 进入继续观察

满足以下任一条件时进入“继续观察”，不立即改参数：

- `D2` 报告出现 `预警`
- 长时间窗口内存在轻微积压，但可恢复
- `retry_backoff` 行为存在波动，但未影响健康 AS 推进
- 资源占用偏高但未触发系统级告警

### 6.3 进入参数评审

满足以下任一条件时进入参数评审：

- `D2` 报告结论为“进入参数评审”
- 连续两轮 `D2` 报告都出现相同 `预警`
- 出现稳定的 `capacity_limited` 且影响健康 AS
- `pending_events` 或 `pending_transactions` 长时间不回落
- 恢复场景结束后，服务仍无法回到 `idle`

## 7. 回退条件

参数变更后，出现以下任一情况必须回退到上一个稳定值：

- 日报门禁从 `通过` 退化为 `预警` 或 `失败`
- 资源占用显著恶化，超过变更前基线 `20%`
- 健康 AS 的成功推进变差
- 多实例场景出现新的饿死、重复 backoff、恢复超时
- 值班侧新增人工干预频率明显上升

回退后必须补一份说明，至少包含：

- 回退参数
- 回退时间
- 触发回退的指标或现象
- 对应样本路径
- 下一轮计划

## 8. 执行步骤

推荐优先使用一键脚本统一归档目录、元数据和决策模板：

```bash
ADMIN_TOKEN="$ADMIN_TOKEN" \
APPSERVICE_D2_RESOURCE_SUMMARY="baseline run; 待补 CPU/RSS/连接池/慢查询摘要" \
bash scripts/run_appservice_p0_d2.sh baseline
```

参数调整后建议使用：

```bash
ADMIN_TOKEN="$ADMIN_TOKEN" \
APPSERVICE_D2_RESOURCE_SUMMARY="after change; 待补 CPU/RSS/连接池/慢查询摘要" \
bash scripts/run_appservice_p0_d2.sh after-change
```

默认归档结构：

```text
artifacts/appservice/<date>/
  baseline/
  after-change/
  decision.md
  decision.autofill.md
```

### 8.1 部署后门禁

```bash
ADMIN_AUTH_HEADER="Authorization: Bearer $ADMIN_TOKEN" \
RUN_APPSERVICE_GATE=1 \
APPSERVICE_GATE_DAY=D2 \
APPSERVICE_GATE_FAIL_ON=warning \
APPSERVICE_GATE_RESOURCE_SUMMARY="deploy gate; 待补 CPU/RSS/连接池摘要" \
bash scripts/deployment_smoke_test.sh
```

### 8.2 CI 可选门禁

```bash
RUN_APPSERVICE_GATE=1 \
APPSERVICE_GATE_ADMIN_TOKEN="$ADMIN_TOKEN" \
APPSERVICE_GATE_DAY=D2 \
APPSERVICE_GATE_FAIL_ON=warning \
APPSERVICE_GATE_RESOURCE_SUMMARY="ci gate; 待补 CPU/RSS/连接池摘要" \
bash scripts/ci_backend_validation.sh docker
```

### 8.3 参数变更后复测

```bash
ADMIN_TOKEN=... \
BASE_URL=http://127.0.0.1:8008 \
PROMETHEUS_URL=http://127.0.0.1:9090/metrics \
python3 scripts/appservice_daily_report.py \
  --day D2 \
  --fail-on warning \
  --output-dir /tmp/appservice-d2-after-change \
  --resource-summary "after tuning; 待补 CPU/RSS/连接池摘要"
```

若需要把资源摘要与下一步计划一起固化到归档目录，可额外传入：

```bash
ADMIN_TOKEN="$ADMIN_TOKEN" \
APPSERVICE_D2_RESOURCE_FILE=/path/to/resource-summary.txt \
APPSERVICE_D2_NEXT_PLAN="先保持默认值，继续累计 D2 样本后再决定是否调整 MAX_SERVICES_PER_TICK" \
bash scripts/run_appservice_p0_d2.sh baseline
```

说明：

- `decision.md` 用于人工维护最终结论
- `decision.autofill.md` 由脚本自动刷新，可作为填写 `decision.md` 的初稿
- 当 `baseline` 与 `after-change` 都已生成时，`decision.autofill.md` 会自动补出结论与场景逐项对比

## 9. 每轮交付物

每轮调优结束后，至少提交以下交付物：

- `daily-report.json`
- `daily-report.md`
- 关键场景的原始 `soak` JSON
- 资源摘要
- 参数变更记录
- 是否保持默认值 / 继续观察 / 进入参数评审 的结论

建议按以下目录结构归档：

```text
artifacts/appservice/
  2026-06-18/
    baseline/
    after-change/
    decision.md
```

## 10. 评审单模板

每次正式参数调整前，建议补一份评审单，至少回答以下问题：

- 当前默认值是什么？
- 本轮只改哪个参数？改动幅度是多少？
- 为什么不是先改别的参数？
- `D2` 样本是否完整？
- 三出口一致性是否可信？
- 回退阈值是什么？
- 负责人是谁？预计观察窗口多久？

可直接复用的填写模板见：

- [APPSERVICE_D2_DECISION_TEMPLATE.md](file:///Users/ljf/Desktop/hu_ts/synapse-rust/docs/synapse-rust/APPSERVICE_D2_DECISION_TEMPLATE.md)

## 11. 完成定义

`appservice` 阈值调优任务满足以下条件时，可视为本阶段完成：

- 部署门禁已启用并可稳定产出日报
- 至少 1 轮 `D2` 样本可重复通过
- 已有正式参数评审与回退策略
- 值班手册已包含参数调整和恢复动作
- 样本、结论、变更记录可追溯
