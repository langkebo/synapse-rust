# Task 11 - P0 占位接口契约测试 Backlog

> 目的：将 Task 11 的 P0 “假成功/空壳”风险端点收敛为可执行的契约测试与明确的错误语义，避免 200 + 固定空返回掩盖真实缺口。

## 1. 已覆盖（P0 契约测试）

| 端点 | 风险点 | 期望语义 | 证据 |
| --- | --- | --- | --- |
| `GET /_matrix/client/{r0,v3}/pushrules/{scope}` | 非 global scope 可能被误当成成功 | `400` + `M_INVALID_PARAM` | [api_placeholder_contract_p0_tests.rs](file:///Users/ljf/Desktop/hu/synapse-rust/tests/integration/api_placeholder_contract_p0_tests.rs) |
| `GET /_matrix/client/{r0,v3}/directory/room/{room_alias}` | 缺失 alias 不能返回空成功 | `404` + `M_NOT_FOUND` | [api_placeholder_contract_p0_tests.rs](file:///Users/ljf/Desktop/hu/synapse-rust/tests/integration/api_placeholder_contract_p0_tests.rs) |
| `GET /_matrix/client/{r0,v3}/user/{user_id}/account_data/{type}` | 缺失 account_data 不能静默返回 `{}` | `404` + `M_NOT_FOUND` | [api_placeholder_contract_p0_tests.rs](file:///Users/ljf/Desktop/hu/synapse-rust/tests/integration/api_placeholder_contract_p0_tests.rs) |
| `GET /_matrix/client/{r0,v3}/rooms/{room_id}/keys/distribution` | “永远空/永远成功”掩盖缺失实现 | `404` + `M_NOT_FOUND`（无会话） | [api_placeholder_contract_p0_tests.rs](file:///Users/ljf/Desktop/hu/synapse-rust/tests/integration/api_placeholder_contract_p0_tests.rs) |
| `POST /_matrix/client/v3/rooms/{room_id}/report` | 空壳 `{}` 成功体 | `400` + `M_UNRECOGNIZED` | [api_placeholder_contract_p0_tests.rs](file:///Users/ljf/Desktop/hu/synapse-rust/tests/integration/api_placeholder_contract_p0_tests.rs)、[UNSUPPORTED_ENDPOINTS.md](file:///Users/ljf/Desktop/hu/synapse-rust/docs/synapse-rust/UNSUPPORTED_ENDPOINTS.md) |
| `GET /_matrix/client/v3/events?from=...` | 非法 token 静默降级为假成功 | `400` + `M_INVALID_PARAM` | [api_placeholder_contract_p0_tests.rs](file:///Users/ljf/Desktop/hu/synapse-rust/tests/integration/api_placeholder_contract_p0_tests.rs) |

## 2. 本轮新增覆盖（P0 契约测试）

| 端点 | 风险点 | 期望语义 | 证据 |
| --- | --- | --- | --- |
| `GET /_matrix/client/v3/rooms/{room_id}/keys/{event_id}` | 非法 `event_id` 返回 200 固定空 `keys` | `400` + `M_INVALID_PARAM` | [api_placeholder_contract_p0_tests.rs](file:///Users/ljf/Desktop/hu/synapse-rust/tests/integration/api_placeholder_contract_p0_tests.rs) |
| `GET /_matrix/client/v3/rooms/{room_id}/thread/{event_id}` | 非法 `event_id` 返回 200 固定空 | `400` + `M_INVALID_PARAM` | [api_placeholder_contract_p0_tests.rs](file:///Users/ljf/Desktop/hu/synapse-rust/tests/integration/api_placeholder_contract_p0_tests.rs) |
| `GET /_matrix/client/v3/rooms/{room_id}/thread/{event_id}` | 已创建 thread/reply 但返回恒空 | `200` 且 `reply_count >= 1`、`replies` 非空 | [api_placeholder_contract_p0_tests.rs](file:///Users/ljf/Desktop/hu/synapse-rust/tests/integration/api_placeholder_contract_p0_tests.rs) |
| `GET /_matrix/client/r0/rooms/{room_id}/initialSync` | 200 固定空 `messages` 且固定 `start/end` | `400` + `M_UNRECOGNIZED` | [api_placeholder_contract_p0_tests.rs](file:///Users/ljf/Desktop/hu/synapse-rust/tests/integration/api_placeholder_contract_p0_tests.rs)、[UNSUPPORTED_ENDPOINTS.md](file:///Users/ljf/Desktop/hu/synapse-rust/docs/synapse-rust/UNSUPPORTED_ENDPOINTS.md) |
| `POST /_matrix/client/r0/rooms/{room_id}/receipt/{receipt_type}/{event_id}` | 非法参数被吞掉或返回假成功 | `400` + `M_INVALID_PARAM` | [api_placeholder_contract_p0_tests.rs](file:///Users/ljf/Desktop/hu/synapse-rust/tests/integration/api_placeholder_contract_p0_tests.rs) |
| `GET /_matrix/client/r0/rooms/{room_id}/receipts/{receipt_type}/{event_id}` | 非法参数返回 200 空 `chunk` | `400` + `M_INVALID_PARAM` | [api_placeholder_contract_p0_tests.rs](file:///Users/ljf/Desktop/hu/synapse-rust/tests/integration/api_placeholder_contract_p0_tests.rs) |

## 3. 后续扩展建议（按新增发现补齐）

1. 以 `task11_room_rs_placeholder_inventory.md` 的 P1/P2 清单为输入，按“假成功风险 > 覆盖面 > 误导成本”排序逐步转化为契约测试。
2. 对“写接口返回 `{}`”类端点补充副作用断言（写入后可读/可查询），避免仅验证状态码。

## 4. P1/P2 候选端点（从 Task 11 inventory 提取）

> 说明：本节用于把 inventory 中的 P1/P2 候选端点落到“可执行契约测试”口径，目标是阻止回归为 **200 + 静态占位/空成功**（或伪分页/伪计数）。

### 4.1 P1（部分字段曾硬编码 / 伪分页风险）

| 端点 | 风险点 | 期望语义（最小契约） | 证据 |
| --- | --- | --- | --- |
| `GET /_matrix/client/r0/rooms/{room_id}` | `invited_members_count`、`guest_can_join` 不能长期固定值 | 1) 邀请用户后 `invited_members_count >= 1`；2) 若房间 state `m.room.guest_access=can_join`，则 `guest_can_join=true` | [api_placeholder_contract_p1p2_tests.rs](file:///Users/ljf/Desktop/hu/synapse-rust/tests/integration/api_placeholder_contract_p1p2_tests.rs) |
| `GET /_matrix/client/r0/rooms/{room_id}/members/recent?from=&limit=` | `start/end` 不能伪 token；分页语义不能恒定 | `start/end` 必须与 `from/limit` 切片一致（例如 from=0 limit=1 返回 start=0 end=1；下一页 from=1 返回 start=1 end=2） | [api_placeholder_contract_p1p2_tests.rs](file:///Users/ljf/Desktop/hu/synapse-rust/tests/integration/api_placeholder_contract_p1p2_tests.rs) |
| `GET /_matrix/client/v1/rooms/{room_id}/report/{event_id}/scanner_info` | 长期固定 `not_configured` 可能被误解为支持扫描 | 返回 200 时必须包含非空 `message`、明确 `scanner_enabled` 与 `status` 字段，禁止返回 `{}`/空结构假成功 | [api_placeholder_contract_p1p2_tests.rs](file:///Users/ljf/Desktop/hu/synapse-rust/tests/integration/api_placeholder_contract_p1p2_tests.rs) |

### 4.2 P2（ACK 语义：要求有真实副作用）

| 端点 | 风险点 | 期望语义（最小契约） | 证据 |
| --- | --- | --- | --- |
| `PUT /_matrix/client/v3/rooms/{room_id}/account_data/{type}` + `GET /_matrix/client/v3/rooms/{room_id}/account_data/{type}` | 写接口若回退成“只返回成功但未落库”会变成假 ACK | `PUT` 后 `GET` 必须能读回同 payload（写入副作用可见），禁止“200 + 空成功” | [api_placeholder_contract_p1p2_tests.rs](file:///Users/ljf/Desktop/hu/synapse-rust/tests/integration/api_placeholder_contract_p1p2_tests.rs) |

CI 说明：
- `ci.yml` 会随全量测试执行上述 P1/P2 契约测试。
- `db-migration-gate.yml` 的 `sqlx-migrate-run` 也会显式执行 `api_placeholder_contract_p1p2_tests`。
