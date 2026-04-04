# synapse-rust 最小互操作验证清单

> 日期：2026-04-03  
> 文档类型：能力补证 / 最小验证清单  
> 对应 backlog：`PROJECT_REVIEW_ACTION_BACKLOG_2026-04-03.md` / P1-5  
> 说明：本清单用于给核心能力域建立最小闭环验证点。当前正式状态仍以 `CAPABILITY_STATUS_BASELINE_2026-04-02.md` 为准；本文件只定义“至少要验证到什么程度”，不直接升级能力状态。

## 一、目标

为联邦、E2EE、Admin、AppService 四个核心能力域补齐最小闭环验证点，避免继续使用“已有路由/已有模块/已有测试文件”替代行为级证据。

## 二、验证范围总表

| 能力域 | 当前状态 | 最小闭环目标 | 现有证据 | 当前缺口 |
|------|------|------|------|------|
| Federation | 部分实现 | 至少证明错误路径、发送/接收主链、一次跨 homeserver 互操作样例 | `tests/integration/federation_error_tests.rs`、`tests/e2e/e2e_scenarios.rs`、`tests/friend_federation_test.rs` | 稳定跨 homeserver 闭环不足 |
| E2EE | 已实现待验证 | 至少证明设备密钥、查询/申领、备份/恢复、交叉签名中有明确闭环点 | `tests/unit/e2ee_api_tests.rs` | 跨设备恢复与客户端级闭环不足 |
| Admin | 已实现待验证 | 至少证明权限边界、关键管理动作、结果可观测 | `tests/integration/api_protocol_alignment_tests.rs` | 仍缺少一份明确的“最小验证点”收口说明 |
| AppService | 部分实现 | 至少证明注册/查询/事务接线三类行为各有验证点 | `tests/unit/app_service_api_tests.rs` | 当前测试偏结构断言，缺少行为闭环表达 |

## 三、按能力域的最小验证点

### 1. Federation

| 验证点 | 验证目标 | 现有证据 | 通过标准 |
|------|------|------|------|
| 错误路径 | 非法请求、签名异常、边界错误能返回稳定结果 | `tests/integration/federation_error_tests.rs` | 错误码、响应结构、失败路径稳定 |
| 发送链路 | 本地事件或请求能进入联邦发送主链 | `tests/friend_federation_test.rs` | 能观察到发送调用或发送结果 |
| 接收链路 | 远端请求进入接收处理逻辑并完成基本校验 | `tests/friend_federation_test.rs` | 接收结果可断言，不是仅“不报错” |
| 互操作样例 | 至少一组跨 homeserver 的最小互操作样例 | `tests/e2e/e2e_scenarios.rs` | 至少一次真实或准真实跨服务交互闭环 |

### 2. E2EE

| 验证点 | 验证目标 | 现有证据 | 通过标准 |
|------|------|------|------|
| 设备密钥 | 设备密钥上传、查询结果可断言 | `tests/unit/e2ee_api_tests.rs` | 上传后查询结果与预期一致 |
| 密钥查询/申领 | one-time key 或相关查询链路可闭环 | `tests/unit/e2ee_api_tests.rs` | 请求、返回、状态变化可验证 |
| 备份/恢复 | 备份创建与恢复链路至少有一条最小闭环 | `tests/unit/e2ee_api_tests.rs` | 不是仅接口可调，而是结果可对比 |
| 交叉签名 | 交叉签名材料上传/查询或验证链路成立 | `tests/unit/e2ee_api_tests.rs` | 关键字段与状态转换可断言 |
| 跨设备恢复 | 至少一组跨设备或恢复场景验证点 | 当前缺独立稳定证据 | 补齐后才能继续升级能力状态 |

### 3. Admin

| 验证点 | 验证目标 | 现有证据 | 通过标准 |
|------|------|------|------|
| 权限边界 | 非管理员访问被拒绝，管理员访问成功 | `tests/integration/api_protocol_alignment_tests.rs:437` | 同一接口对 admin / non-admin 行为可区分 |
| 关键查询能力 | 房间搜索、用户查询等关键管理接口可返回稳定结构 | `tests/integration/api_protocol_alignment_tests.rs:437` | 响应状态、字段结构、过滤行为可断言 |
| 管理动作落库 | 例如 server notice 等写操作可被后续查询观察到 | `tests/integration/api_protocol_alignment_tests.rs:768` | 写入与读取形成最小闭环 |

### 4. AppService

| 验证点 | 验证目标 | 现有证据 | 通过标准 |
|------|------|------|------|
| 注册配置 | AppService 注册对象、URL、token、namespace 基本约束存在 | `tests/unit/app_service_api_tests.rs` | 输入结构与基本约束通过 |
| 查询能力 | 用户 / 房间别名查询接线存在 | `tests/unit/app_service_api_tests.rs` | 查询参数与返回结构被验证 |
| 事务接线 | transaction / event push 基本路径存在 | `tests/unit/app_service_api_tests.rs` | 至少确认事务结构与事件载荷路径 |
| 后续补证方向 | 从结构断言升级到行为断言 | 当前缺更强证据 | 补一条真实 handler/service 级闭环后再升级状态 |

## 四、执行顺序建议

1. 先补 Federation 的互操作样例和发送/接收闭环。
2. 再补 E2EE 的跨设备恢复或备份恢复闭环。
3. Admin 维持当前集成测试优势，补一份更明确的验证映射即可。
4. AppService 先从“结构断言”升级到“handler/service 级行为断言”。

## 五、验收标准

- 联邦、E2EE、Admin、AppService 四个能力域都能各自对应到至少一个最小闭环验证点。
- 每个验证点都能定位到现有代码或测试证据；没有证据的地方必须显式标为缺口。
- 本清单不使用“完整实现”“生产就绪”“100%”这类总括结论。
- 后续若某能力要升级为“已实现并验证”，必须先补齐本清单中的缺口，而不是直接修改对外口径。
