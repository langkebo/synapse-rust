# AppService D2 决策模板

> 用途: `appservice` `D2` 基线 / 调参后复测的结论记录模板
> 使用方式: 复制到 `artifacts/appservice/<date>/decision.md` 后填写

## 基本信息

- 日期：YYYY-MM-DD
- 环境：生产 / 预发布 / 压测
- 负责人：待补
- 观察窗口：待补
- 样本目录：`artifacts/appservice/<date>/baseline/`
- 变更后目录：`artifacts/appservice/<date>/after-change/`

## 当前默认值

- `MAX_SERVICES_PER_TICK`：待补
- `HIGH_PENDING_TRANSACTION_THRESHOLD`：待补
- `HIGH_PENDING_EVENT_THRESHOLD`：待补

## 本轮变更

- 是否改参数：否 / 是
- 若改参数，本轮只改了哪个参数：待补
- 改动前值：待补
- 改动后值：待补
- 改动理由：待补

## 样本完整性检查

- `baseline` 已生成 `daily-report.md`：是 / 否
- `baseline` 已生成 3 个 `D2` 场景原始样本：是 / 否
- `after-change` 是否已生成：是 / 否
- 三出口一致性是否可信：是 / 否
- 资源摘要是否已补齐：是 / 否

## 

- `continuous-ingress`：待补
- `mixed-backoff`：待补
- `recovery`：待补
- CPU / RSS：待补
- DB 连接池：待补
- 慢查询 / 外部依赖：待补

## 结论

- 最终结论：保持默认值 / 继续观察 / 进入参数评审
- 结论理由：待补
- 是否允许继续下一轮参数调整：是 / 否

## 回退条件

- 日报门禁从 `通过` 退化为 `预警` 或 `失败`
- 资源占用较基线恶化超过 `20%`
- 健康 AS 成功推进下降
- 恢复窗口结束后仍无法回到 `idle`

## 下一步

- 下一步计划：待补
- 预计再次复测时间：待补
- 需要谁参与评审：待补

## 快速结论示例

### 示例 A：保持默认值

- 最终结论：保持默认值
- 结论理由：`D2` 全部场景通过，未观察到稳定容量瓶颈，恢复窗口内积压可回落
- 下一步计划：继续累计 1 轮 `D2` 样本，再评估是否进入 `D3`

### 示例 B：继续观察

- 最终结论：继续观察
- 结论理由：`mixed-backoff` 出现轻微 `预警`，但健康 AS 仍有推进，尚不足以直接改参数
- 下一步计划：保持默认值，补 1 到 2 轮 `D2` 样本后复评

### 示例 C：进入参数评审

- 最终结论：进入参数评审
- 结论理由：连续样本中 `capacity_limited` 持续存在，且恢复场景结束后服务未稳定回到 `idle`
- 下一步计划：只评审 1 个参数，优先考虑 `MAX_SERVICES_PER_TICK`
