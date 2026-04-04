# synapse-rust AppService 当前定位说明

> 日期：2026-04-03  
> 文档类型：能力补证 / 定位说明  
> 说明：本文档用于明确 AppService 当前可承诺范围与未成熟边界。

## 一、当前定位

AppService 当前应维持“部分实现”口径。已有路由、管理端点和基础测试，但完整行为边界、事务可靠性与生产承诺尚未收敛。

## 二、当前可承诺范围

| 范围 | 当前状态 | 代码证据 | 测试证据 | 文档来源 | 说明 |
|------|------|------|------|------|------|
| 主路由存在 | 部分实现 | `src/web/routes/assembly.rs` | `tests/unit/app_service_api_tests.rs` | `CAPABILITY_STATUS_BASELINE_2026-04-02.md` | 已有基础接线 |
| 基础管理接口 | 部分实现 | `src/web/routes/assembly.rs` | `tests/unit/app_service_api_tests.rs` | `APP_SERVICE_INTEGRATION.md` | 可作为当前最小能力说明 |
| 外部服务集成说明 | 部分实现 | `src/web/routes/assembly.rs` | `tests/unit/app_service_api_tests.rs` | `APP_SERVICE_INTEGRATION.md` | 文档存在不等于行为成熟 |

## 三、未成熟边界

- 不应把端点存在直接解释为桥接能力成熟。
- 事务处理、事件推送、命名空间行为仍缺少更严格验证。
- 当前不应作为主承诺能力对外宣传为成熟可用。

## 四、建议口径

对外应表述为：AppService 已有基础实现与文档接线，但当前仍处于能力边界收敛与验证补证阶段。
