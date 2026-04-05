# Task 11 - P0 契约测试待办清单

> 目的：把 `Task 11` 第三项剩余的测试缺口收敛为可执行 backlog。  
> 范围：仅覆盖当前 `task11_other_routes_placeholder_inventory.md` 中的 P0 端点。  
> 原则：测试断言必须验证“真实数据或明确错误”，禁止继续接受 200 空占位成功体。

## 1. 测试目标

- 将当前 P0 空壳端点纳入集成测试覆盖。
- 为每个端点定义最小前置数据、目标断言和通过条件。
- 为 `Task 11` 第三项的“全部测试验证方法”提供明确落地路径。

## 2. 建议落点

- 推荐新建测试文件：`tests/integration/api_placeholder_contract_p0_tests.rs`
- 若需要复用房间测试初始化逻辑，可参考：
  - `tests/integration/api_room_placeholder_contract_tests.rs`
  - `tests/integration/api_enhanced_features_tests.rs`

## 3. backlog 清单

| 状态 | 优先级 | 端点 | 当前问题 | 最小前置数据 | 目标断言 | 建议测试名称 |
| --- | --- | --- | --- | --- | --- | --- |
| 已完成 | P0 | `POST /_matrix/client/v3/rooms/{room_id}/report` | 旧实现曾在鉴权和房间校验后返回假成功 | 已注册用户 + 已创建房间 | 现在必须返回显式 `M_UNRECOGNIZED`，禁止伪 `submitted/report_id` 成功体 | `test_report_room_contract_returns_unrecognized` |
| 已完成 | P0 | `GET /_matrix/client/{r0\|v3}/directory/room/{room_alias}` | 未命中时返回空 `room_id` | 已注册用户；构造一个不存在的 alias | 未命中必须是明确错误，或至少不能返回空 `room_id` 成功体 | `test_directory_room_alias_contract` |
| 已完成 | P0 | `GET /_matrix/client/{r0\|v3}/user/{user_id}/account_data/{type}` | 未命中时返回空对象占位 | 已注册用户；查询不存在的 account data 类型 | 未命中不能返回 `{}` 假成功；应返回真实未命中语义 | `test_account_data_contract` |
| 已完成 | P0 | `GET /_matrix/client/{r0\|v3}/pushrules/{scope}` | 非 `global` scope 返回空对象占位 | 已注册用户；请求非 `global` scope | 非法/未支持 scope 应返回明确错误，而非 `{}` | `test_push_rules_scope_contract` |
| 已完成 | P0 | `GET /_matrix/client/{r0\|v3}/rooms/{room_id}/keys/distribution` | 未命中时返回空 `session_id/session_key` | 已注册用户 + 已创建房间，但无 outbound session | 未命中时不得返回空 session 成功体 | `test_room_key_distribution_contract` |
| 已完成 | P0 | `GET /_matrix/client/{r0\|v3}/events` | 旧实现曾吞掉服务错误并回退为空 `chunk` 成功体 | 已注册用户；补齐 join membership 后破坏 `events` 表以稳定制造服务失败 | 出错时必须透传 `500 + M_UNKNOWN`，且响应体不得包含 `chunk` | `test_sync_events_contract_surfaces_service_errors` |

## 4. 断言要求

### 4.1 成功态

- 成功返回必须包含真实业务数据，或至少能证明副作用已发生。
- 不允许使用空字符串、固定空对象、固定空数组伪装“正常成功”。

### 4.2 失败态

- 参数错误必须返回参数类错误，不允许静默成功。
- 未支持能力应返回 `M_UNRECOGNIZED` 或明确约定错误码。
- 未命中语义应返回 `M_NOT_FOUND` 或等价的明确错误，而不是占位成功体。

## 5. 执行顺序建议

1. `pushrules/{scope}` 与 `directory/room/{room_alias}`
   - 前置数据简单，适合先建立测试骨架。
2. `account_data/{type}` 与 `rooms/{room_id}/keys/distribution`
   - 能快速验证“不得返回空占位成功体”的断言。
3. `rooms/{room_id}/report`
   - 已收口为显式 `M_UNRECOGNIZED`，并同步归档到 `UNSUPPORTED_ENDPOINTS.md`。
4. `events`
   - 已补“服务报错必须透传 500”的契约测试，避免回退为空 `chunk` 成功体。

## 5.1 已落地测试

- 测试文件：[api_placeholder_contract_p0_tests.rs](file:///Users/ljf/Desktop/hu/synapse-rust/tests/integration/api_placeholder_contract_p0_tests.rs)
- 已覆盖：
  - `test_push_rules_scope_contract_rejects_non_global_scope`
  - `test_directory_room_alias_contract_returns_not_found_for_missing_alias`
  - `test_account_data_contract_returns_not_found_for_missing_custom_type`
  - `test_room_key_distribution_contract_returns_not_found_without_session`
  - `test_report_room_contract_returns_unrecognized`
  - `test_sync_events_contract_surfaces_service_errors`

## 6. 完成判定

- 上表 6 个端点全部具备集成测试设计并已落地。
- `tests/integration/api_placeholder_contract_p0_tests.rs` 已覆盖全部 6 个 P0 端点。
- `task11_scan_and_ci_gate.md` 与本 backlog 的端点列表保持一致。
- `tasks.md` 中 `Task 11` 第三项可据此勾选完成。
