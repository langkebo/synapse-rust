# synapse-rust E2EE 最小闭环清单

> 日期：2026-04-03  
> 文档类型：能力补证 / 最小验证清单  
> 说明：本清单用于替代“E2EE 完整实现”类笼统说法。当前正式状态仍以 `CAPABILITY_STATUS_BASELINE_2026-04-02.md` 为准。

## 一、当前定位

E2EE 当前更准确的口径是“已实现待验证”。模块覆盖度较高，但跨设备、恢复、交叉签名与客户端级闭环证据仍不足。

## 二、最小子能力清单

| 子能力 | 当前状态 | 代码证据 | 测试证据 | 文档来源 | 剩余风险 |
|------|------|------|------|------|------|
| 设备密钥 | 已实现待验证 | `src/services/container.rs` | `tests/unit/e2ee_api_tests.rs` | `CAPABILITY_STATUS_BASELINE_2026-04-02.md` | 设备级行为仍缺少更细颗粒验证 |
| 密钥查询 / 申领 | 已实现待验证 | `src/services/container.rs` | `tests/unit/e2ee_api_tests.rs` | `CAPABILITY_STATUS_BASELINE_2026-04-02.md` | 规范级交互证据不足 |
| 密钥备份 / 恢复 | 部分实现 | `src/services/container.rs` | `tests/unit/e2ee_api_tests.rs` | `SYSTEM_GAP_ANALYSIS_AND_OPTIMIZATION_PLAN_2026-04-02.md` | 恢复闭环与客户端级验证不足 |
| 交叉签名 | 部分实现 | `src/services/container.rs` | `tests/unit/e2ee_api_tests.rs` | `SYSTEM_GAP_ANALYSIS_AND_OPTIMIZATION_PLAN_2026-04-02.md` | 仍需补齐验证升级路径 |
| 跨设备恢复 | 部分实现 | `src/services/container.rs` | 当前无独立闭环证据 | `PROJECT_REVIEW_WEEKLY_PLAN_2026-04-03.md` | 仍不能宣称完整成熟 |

## 三、最小验证要求

- 设备、密钥、备份、交叉签名、恢复五类子能力必须分别给出证据。
- 不再使用“100% 完整实现”“生产就绪”等总括措辞。
- 只有补齐跨设备与恢复闭环后，才适合继续升级状态。
