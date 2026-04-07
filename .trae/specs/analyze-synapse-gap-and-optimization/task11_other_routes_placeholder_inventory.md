# Task 11 - 其它 routes 空壳接口清单与优先级（非 room.rs）

范围：覆盖 `src/web/routes/` 中除 `src/web/routes/handlers/room.rs` 以外，满足“已鉴权/已校验但返回静态占位结果”的端点；本清单仅包含 **仍返回 200/空结构/空字符串** 且 **不是 `M_UNRECOGNIZED`** 的点位。已改为显式 `M_UNRECOGNIZED` 的端点，不再保留在本清单，统一归档到 `docs/synapse-rust/UNSUPPORTED_ENDPOINTS.md`。

## P0（语义误导/假成功）

当前无仍在返回 200 假成功体的 P0 端点。

- `GET /_matrix/client/{r0|v3}/directory/room/{room_alias}`、`GET /_matrix/client/{r0|v3}/user/{user_id}/account_data/{type}`、`GET /_matrix/client/{r0|v3}/pushrules/{scope}`、`GET /_matrix/client/{r0|v3}/rooms/{room_id}/keys/distribution` 已改为“真实数据或明确错误”，并由 `tests/integration/api_placeholder_contract_p0_tests.rs` 覆盖。
- `POST /_matrix/client/v3/rooms/{room_id}/report` 已改为显式 `M_UNRECOGNIZED`，不再属于空壳库存，统一归档到 `docs/synapse-rust/UNSUPPORTED_ENDPOINTS.md`。
- `GET /_matrix/client/{r0|v3}/events` 当前通过 [get_events](file:///Users/ljf/Desktop/hu/synapse-rust/src/web/routes/handlers/sync.rs#L62-L82) 直接透传 `sync_service.get_events` 错误；`tests/integration/api_placeholder_contract_p0_tests.rs` 已补“服务报错必须返回 `500 + M_UNKNOWN`，且不得返回 `chunk` 成功体”的契约测试。

## P1（固定状态返回：可接受占位但应 gated）

1) **GET** `/_matrix/client/v1/rooms/{room_id}/report/{event_id}/scanner_info` → `get_scanner_info`
- 现状：固定 `scanner_enabled:false/status:not_configured`。
- 风险：低；但建议按 feature flag/配置 gated，避免“看似支持但永远不可用”的长期状态。
- 证据：`tests/integration/api_placeholder_contract_p1p2_tests.rs` 已补契约测试，要求返回体关键字段非空，禁止回归为 200 + 空结构。
- 代码参考：[get_scanner_info](file:///Users/ljf/Desktop/hu/synapse-rust/src/web/routes/directory_reporting.rs#L200-L214)

## P2（ACK 语义：不按空壳债务处理）

说明：返回 `{}` 的写接口（例如 set/delete alias、update_report_score 等）通常符合 Matrix ACK 习惯；不建议作为空壳债务优先项，除非发现“写入失败仍返回成功”的路径。

## 本轮复扫结论（2026-04-05）

1) **`src/web/routes/room_summary.rs` 当前未新增“鉴权后静态占位”端点**
- `clear_unread` 虽然返回固定的 `unread_notifications: 0 / unread_highlight: 0`，但在成功响应前已调用 `room_summary_service.clear_unread`，并最终落到存储层执行 `UPDATE room_summaries SET unread_notifications = 0, unread_highlight = 0`，应归类为真实 ACK，而不是空壳成功。
- 代码参考：[clear_unread](file:///Users/ljf/Desktop/hu/synapse-rust/src/web/routes/room_summary.rs#L434-L450)、[room_summary_service.rs](file:///Users/ljf/Desktop/hu/synapse-rust/src/services/room_summary_service.rs#L630-L637)、[room_summary storage](file:///Users/ljf/Desktop/hu/synapse-rust/src/storage/room_summary.rs#L688-L698)

2) **`e2ee_routes.rs` 中 `get_verification_status` 暂不按空壳处理**
- `status: "not_found"` 来自 `device_trust_service.get_verification_status` 的真实查询结果/防枚举分支，不是“未实现时硬编码返回 200 成功体”。
- 代码参考：[get_verification_status](file:///Users/ljf/Desktop/hu/synapse-rust/src/web/routes/e2ee_routes.rs#L758-L781)、[device_trust service](file:///Users/ljf/Desktop/hu/synapse-rust/src/e2ee/device_trust/service.rs#L376-L399)
