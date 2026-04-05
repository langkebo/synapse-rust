# Task 12 - 验证与回滚方案

## 1. 验证目标

- 路由路径与 HTTP 方法不变。
- 错误语义和响应结构不变。
- 测试入口和 CI 接线不失联。
- 任一批次失败时可按模块维度快速回退。

## 2. 每批次必做检查

1. 编译与诊断
   - 跑目标模块相关测试
     - 房间域主链：`cargo test --locked --test api_room_tests -- --test-threads=1`
     - 房间占位契约：`cargo test --locked --test api_placeholder_contract_p0_tests -- --test-threads=1`
     - Room sync：`cargo test --locked --test api_room_sync_tests -- --test-threads=1`
     - 房间摘要：`cargo test --locked --test api_room_summary_routes_tests -- --test-threads=1`
     - E2EE 主链：`cargo test --locked --test api_e2ee_tests -- --test-threads=1`
   - 最近编辑文件 diagnostics 为 0
2. 路由结构校验
   - 检查 `create_*_router()` 仍注册旧路径
   - 核对 `/_matrix/client/r0`、`/_matrix/client/v3` 兼容路径
3. 错误语义校验
   - 抽样验证 `M_NOT_FOUND`、`M_FORBIDDEN`、`M_UNRECOGNIZED`
4. 回归测试校验
   - 房间域：`api_room_tests`、`api_placeholder_contract_p0_tests`
   - E2EE 域：`api_e2ee_tests`
   - 中间件域：认证、联邦、限流、CORS/CSRF smoke test

## 3. 批次化回归清单

| 批次 | 必跑回归 | 可抽样 smoke |
| --- | --- | --- |
| Batch 1 `middleware` | 认证、联邦、登录、管理员、速率限制相关测试 | 预检请求、跨域 header、timeout |
| Batch 2 `e2ee` | `api_e2ee_tests`、key/distribution/verification 相关测试 | `/keys/query`, `/sendToDevice`, backup |
| Batch 3 `room handlers` | `api_room_tests`、placeholder contract、space/thread/summary | `/createRoom`, `/state`, `/messages`, `/members` |
| Batch 4 `room router` | 版本路由结构测试 + 房间域关键回归 | `r0/v3` 同路径对照 |

## 4. 回滚策略

- 每次只迁移一个模块簇，保持旧文件导出直到新模块稳定。
- 拆分初期允许旧文件 `pub use` 新模块，降低调用方变更面。
- 每个批次以“编译通过 + 关键回归通过”为最小提交单元。
- 失败回滚条件：
  - 路由路径丢失或方法变更
  - 高频错误语义漂移
  - 关键测试文件接线断开
  - 新增循环依赖或装配歧义

## 5. 发布准入

- 本批次对应回归集全绿。
- 无新增 diagnostics。
- `assembly.rs` / `server.rs` / 相关 router 文件审阅确认装配顺序不变。
- 回滚说明已随变更一起记录，能够在一个提交内撤回。

## 6. 最小 CI 对齐（命名口径）

- 本任务拆分属于“代码组织与路由稳定性”变更，CI 最小对齐口径为：
  - 关键 integration tests：`api_room_tests`、`api_room_sync_tests`、`api_e2ee_tests`、`api_placeholder_contract_p0_tests`
  - 若变更涉及 DB schema/迁移，则必须同时满足 `DB Migration Gate` 的阻断 job（如 `Schema Table Coverage`、`Schema Contract Coverage`、`Unified Schema Apply`、`sqlx Migrate Run`）
