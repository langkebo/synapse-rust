# synapse-rust 联邦最小闭环清单

> 日期：2026-04-03  
> 文档类型：能力补证 / 最小验证清单  
> 说明：本清单用于把联邦从笼统描述拆成最小子能力。当前正式状态仍以 `CAPABILITY_STATUS_BASELINE_2026-04-02.md` 为准。

## 一、当前定位

联邦当前应维持“部分实现”口径。已有路由与局部测试接线，但跨 homeserver 互操作闭环仍不足，不能直接宣称已完成或成熟可用。

## 二、最小子能力清单

| 子能力 | 当前状态 | 代码证据 | 测试证据 | 文档来源 | 剩余风险 |
|------|------|------|------|------|------|
| 路由装配 | 已实现待验证 | `src/web/routes/assembly.rs` | `tests/integration/federation_error_tests.rs` | `CAPABILITY_STATUS_BASELINE_2026-04-02.md` | 路由存在不等于行为闭环 |
| 签名相关处理 | 部分实现 | `src/web/routes/assembly.rs` | `tests/integration/federation_error_tests.rs` | `SYSTEM_GAP_ANALYSIS_AND_OPTIMIZATION_PLAN_2026-04-02.md` | 仍缺少完整签名链路验证 |
| 发送链路 | 部分实现 | `src/web/routes/assembly.rs` | `tests/integration/federation_error_tests.rs` | `SYSTEM_GAP_ANALYSIS_AND_OPTIMIZATION_PLAN_2026-04-02.md` | 跨服务发送行为缺少互操作证据 |
| 接收链路 | 部分实现 | `src/web/routes/assembly.rs` | `tests/integration/federation_error_tests.rs` | `SYSTEM_GAP_ANALYSIS_AND_OPTIMIZATION_PLAN_2026-04-02.md` | 接收后的状态迁移与认证仍待补证 |
| 互操作闭环 | 未实现 | `src/web/routes/assembly.rs` | 当前无稳定跨 homeserver 闭环证据 | `PROJECT_REVIEW_WEEKLY_PLAN_2026-04-03.md` | 这是当前最大缺口 |

## 三、最小验证要求

- 至少确认联邦错误路径、签名路径、事件发送/接收路径各自有独立证据。
- 至少补一组跨 homeserver 互操作验证，才能把“部分实现”升级为更高状态。
- 对外口径必须明确：当前联邦属于核心能力域，但尚未形成规范级闭环。
