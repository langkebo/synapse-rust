# Task 11 - 空壳接口自动扫描与 CI 阻断策略（v1）

目标：把“已鉴权/已校验但返回静态占位结果”的接口变成显式工程债务，并阻断新增同类问题；对短期必须保留的占位提供豁免机制与清理时限。

## 1. 术语与判定

### 1.1 空壳接口（Placeholder Endpoint）
满足以下条件之一即可判定为空壳：
- 经过鉴权/存在性/成员校验后，成功分支仍返回固定 JSON（例如 `{}`、`[]`、固定字符串/数字、伪 token）。
- 对明显非法参数（如 event_id 非 `$`）直接返回 200 空结果（静默吞错），而不是参数错误。
- 返回体字段语义与系统真实能力不一致，且长期不会被真实数据填充（例如 `session_id:""`）。

### 1.2 非空壳（可接受的 ACK）
以下情况不按空壳处理：
- Matrix/Synapse 常见的“写操作 ack”返回 `{}`，前提是副作用确实落库/入队。
- “明确不支持”的端点返回 `M_UNRECOGNIZED`（或明确错误码），并且不返回 200 假成功体。
- 已改为 `M_UNRECOGNIZED` 的端点应从空壳盘点中移除，统一归档到 `docs/synapse-rust/UNSUPPORTED_ENDPOINTS.md`。

补充说明：
- 扫描脚本命中空成功体后，不应直接等同于“placeholder”。
- 必须先判断该端点是否存在真实副作用，例如：已落库、已入队、已完成事务投递、已更新状态、已完成标签/元数据变更。
- 对这类“真实 ACK”端点，可登记到 `scripts/shell_routes_allowlist.txt`，但需要在注释中写明原因，避免 allowlist 退化成无差别豁免。

## 2. 扫描范围

### 2.1 需要强约束的目录（默认阻断）
- `src/web/routes/handlers/**`
- `src/web/routes/**`（除明确的 `thirdparty`/`assembly` 等“不支持即 unrecognized”的模块外）

### 2.2 需要弱约束的目录（只告警）
- `docs/**`、`tests/**`（仅在这里出现占位字符串不应阻断）

## 3. 扫描规则（静态规则）

### 3.1 强信号（必须阻断）
- 在 handlers 中出现 `let _ = auth_user;`（已落地为单元测试门禁）。
- 成功分支返回体包含明显“占位值”：
  - 空 token：`next_batch: "0"`、`start: "s"`/`end: "e"`、空 `room_id`、空 `session_id/session_key`。
  - 静态数组/对象：`"chunk": []`、`"events": []`、`{}`，且前置做过 `room_exists/is_member/validate_token`。
- 非法参数静默成功：明显非法 id（如 event_id 不以 `$` 开头）时直接返回 `{}` 或 `{"chunk":[]}`。

### 3.2 弱信号（建议阻断，但需要白名单）
- 返回体有固定字段但值可能合法（例如 `"start":"0","end":"0"`）：需要结合是否有真实分页 token 来判断。

## 4. 豁免机制（必须有清理时限）

### 4.1 豁免条件
- 接口短期必须存在（为了路由兼容/客户端探测），但实现尚未完成。
- 必须满足：
  - 返回 `M_UNRECOGNIZED`（或明确错误码），不允许 200 假成功。
  - 同步登记到 `docs/synapse-rust/UNSUPPORTED_ENDPOINTS.md`，作为“明确不支持清单”。
  - 在豁免清单里声明：端点、原因、owner、截止日期（到期后 CI 阻断）。

### 4.2 豁免清单建议格式
- 文件：`.trae/specs/analyze-synapse-gap-and-optimization/task11_placeholder_exemptions.md`
- 每条豁免包含：`endpoint`、`reason`、`owner_role`、`expires_at`、`replacement_plan`。
- 当前状态：模板文件已创建；默认应保持“无已批准豁免项”，只有在端点已改为明确错误后才允许登记。

## 5. CI 阻断策略（最小落地）

阶段化落地（避免一次性引入大量误报）：
1) **已落地**：handlers 目录禁止 `let _ = auth_user;`（单元测试门禁）。
2) **下一步**：把 P0 清单中的端点加入“契约测试”：
   - 成员访问：要么返回真实数据，要么返回 `M_UNRECOGNIZED`，禁止 200 空占位。
   - 非法参数：必须返回参数错误（禁止静默 200）。
3) **再下一步**：把 “固定占位值模式” 扩展到静态扫描（白名单 + 到期机制）。

### 5.2 扫描器 v2 复核结论（2026-04-05）
- `scripts/detect_shell_routes.sh` 已扩展为识别多行 `Ok(Json(...json!({})...))` 与 `serde_json::json!({})` 模式。
- 新规则首次复扫额外暴露 13 个旧扫描器未统计的空成功体。
- 复核后，这 13 个端点当前判定为“真实副作用 ACK”，而不是 placeholder：
  - `app_service.rs` 中的 transaction / user query / room alias query
  - `e2ee_routes.rs` 中的 send-to-device / upload device signing / cancel room key request
  - `sticky_event.rs` 中的 sticky event set / clear
  - `tags.rs` 中的 tag set / delete
  - `voip.rs` 中的 call candidates / hangup
- 这些端点已登记到 `scripts/shell_routes_allowlist.txt`，后续若语义变化，应重新复核是否仍属于可接受 ACK。

### 5.1 当前契约测试覆盖（2026-04-05）
- 已落地测试文件：`tests/integration/api_placeholder_contract_p0_tests.rs`
- 已落地测试文件（P1/P2）：`tests/integration/api_placeholder_contract_p1p2_tests.rs`
- CI 接线：
  - `ci.yml` 的 `bash scripts/run_ci_tests.sh` 会跑到 `cargo test/nextest` 全量（包含上述 integration target）。
  - `db-migration-gate.yml` 的 `sqlx-migrate-run` job 也会显式执行 `api_placeholder_contract_p1p2_tests`（防迁移链路下回归）。
- 已覆盖：
  - `GET /_matrix/client/{r0|v3}/directory/room/{room_alias}`
  - `GET /_matrix/client/{r0|v3}/user/{user_id}/account_data/{type}`
  - `GET /_matrix/client/{r0|v3}/pushrules/{scope}`
  - `GET /_matrix/client/{r0|v3}/rooms/{room_id}/keys/distribution`
  - `POST /_matrix/client/v3/rooms/{room_id}/report`
  - `GET /_matrix/client/{r0|v3}/events`
- 已覆盖（P1/P2）：
  - `GET /_matrix/client/r0/rooms/{room_id}`（invite 计数与 guest_access 映射）
  - `GET /_matrix/client/r0/rooms/{room_id}/members/recent`（伪分页 token 防回归）
  - `GET /_matrix/client/v1/rooms/{room_id}/report/{event_id}/scanner_info`（禁止 200 空成功体）
  - `PUT/GET /_matrix/client/v3/rooms/{room_id}/account_data/{type}`（ACK 必须可读回）
- 详细 backlog 见：`.trae/specs/analyze-synapse-gap-and-optimization/task11_p0_contract_test_backlog.md`
- `report` 端点已收口为显式 `M_UNRECOGNIZED`，并同步归档到 `docs/synapse-rust/UNSUPPORTED_ENDPOINTS.md`。
- `events` 端点已补“服务报错必须透传 500 + M_UNKNOWN，且不得返回 chunk 成功体”的契约测试。
- 当前 P0 契约测试最小集已补齐，可作为 `Task 11` 第三项完成的证据之一。

## 6. 清理时限建议（默认）
- P0：合入后 7 天内必须替换为真实实现或改为 `M_UNRECOGNIZED`。
- P1：合入后 30 天内完成实现或明确不支持。
- P2：不设硬时限，但纳入季度性治理清单。
