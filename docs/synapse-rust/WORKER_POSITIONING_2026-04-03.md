# synapse-rust Worker 当前定位说明

> 日期：2026-04-03  
> 文档类型：能力补证 / 定位说明  
> 说明：本文档用于明确 Worker 当前可承诺范围与未成熟边界。

## 一、当前定位

Worker 当前应维持“部分实现”口径，并明确限定为：单进程主服务可运行，多 Worker / 复制 / 队列形态未成熟。

## 二、当前可承诺范围

| 范围 | 当前状态 | 代码证据 | 测试证据 | 文档来源 | 说明 |
|------|------|------|------|------|------|
| 单进程主服务可运行 | 已实现待验证 | `src/services/container.rs` | `tests/unit/worker_api_tests.rs` | `CAPABILITY_STATUS_BASELINE_2026-04-02.md` | 当前正式口径应停留在此 |
| Worker 相关模块存在 | 部分实现 | `src/services/container.rs` | `tests/unit/worker_api_tests.rs` | `SYSTEM_GAP_ANALYSIS_AND_OPTIMIZATION_PLAN_2026-04-02.md` | 模块存在不等于部署成熟 |
| 多进程 / 复制 / 队列 | 部分实现 | `src/services/container.rs` | 当前无成熟闭环证据 | `SYSTEM_GAP_ANALYSIS_AND_OPTIMIZATION_PLAN_2026-04-02.md` | 不应对外承诺为可用形态 |

## 三、未成熟边界

- 不应把 Worker API 或相关模块存在视为多 Worker 已成熟。
- 当前缺少复制协议、任务队列与部署形态验证闭环。
- 对外口径应避免使用“支持多 Worker”类表述。

## 四、建议口径

对外应表述为：当前主服务以单进程形态为正式可解释范围，Worker 相关能力仍处于预留或未成熟阶段。
