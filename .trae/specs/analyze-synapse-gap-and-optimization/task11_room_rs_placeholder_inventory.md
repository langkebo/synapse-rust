# Task 11 - room.rs 空壳接口清单与优先级（handlers/room.rs）

范围：仅覆盖 `src/web/routes/handlers/room.rs` 中“通过鉴权/校验后仍返回固定 JSON（空数组/空对象/常量字段/伪 token）且不是 M_UNRECOGNIZED”的接口。

## P0（协议语义风险 / 假成功风险）

1) **GET** `/_matrix/client/v3/rooms/{room_id}/keys/{event_id}` → `get_event_keys`
- 现状：对非法 `event_id` 直接返回 `"keys": []`；对合法事件也固定 `"keys": []`。
- 风险：客户端/SDK 可能把“未实现/失败”误判为“无密钥”，导致 E2EE 行为偏差。
- 建议：
  - 短期：改为 `M_UNRECOGNIZED`（与其它未支持端点一致），或至少对非法 `event_id` 返回 `M_INVALID_PARAM`。
  - 中期：复用 `backup_service` / E2EE 存储链路实现真实 keys 返回。
- 代码参考：[get_event_keys](file:///Users/ljf/Desktop/hu/synapse-rust/src/web/routes/handlers/room.rs#L64-L114)

2) **GET** `/_matrix/client/v3/rooms/{room_id}/thread/{event_id}` → `get_room_thread`
- 现状：完成成员鉴权与事件存在性校验，但 `replies/reply_count/participants` 永远为固定空值。
- 风险：线程能力被“宣称可用但永远无结果”，属于高优先级兼容性假象。
- 建议：直接复用 `ThreadService/ThreadStorage` 现有线程查询能力实现真实 replies；或短期改为 `M_UNRECOGNIZED` 以显式降级。
- 代码参考：[get_room_thread](file:///Users/ljf/Desktop/hu/synapse-rust/src/web/routes/handlers/room.rs#L116-L175)

3) **GET** `/_matrix/client/r0/rooms/{room_id}/initialSync` → `room_initial_sync`
- 现状：成员校验后，`messages.chunk/state/presence/account_data` 等大量字段固定为空；`start/end` 固定为 `"s"/"e"`。
- 风险：伪分页 token/同步语义错误，可能导致客户端缓存错误 token，后续增量同步异常。
- 建议：复用 `SyncService::room_sync` 或 `RoomService::get_room_messages` 产出真实 chunk 与 batch token；或短期返回 `M_UNRECOGNIZED`。
- 代码参考：[room_initial_sync](file:///Users/ljf/Desktop/hu/synapse-rust/src/web/routes/handlers/room.rs#L592-L651)

4) **POST** `/_matrix/client/r0/rooms/{room_id}/receipt/{receipt_type}/{event_id}` → `send_receipt`
- 现状：当 `event_id` 不以 `$` 开头时直接返回 `{}`（静默吞错）。
- 风险：客户端误判写入成功；回执/已读链路可能出现难以排查的一致性问题。
- 建议：对非法 `event_id` 返回 `M_INVALID_PARAM`（或至少复用 `validate_event_id` 强制失败），避免“假成功”。
- 代码参考：[send_receipt](file:///Users/ljf/Desktop/hu/synapse-rust/src/web/routes/handlers/room.rs#L1466-L1495)

5) **GET** `/_matrix/client/r0/rooms/{room_id}/receipts/{receipt_type}/{event_id}` → `get_receipts`
- 现状：当 `event_id` 不以 `$` 开头时直接返回 `{"chunk":[]}`。
- 风险：把参数错误伪装成“确实没有回执”，隐藏客户端 bug 或路由错误。
- 建议：同上，对非法 `event_id` 返回参数错误。
- 代码参考：[get_receipts](file:///Users/ljf/Desktop/hu/synapse-rust/src/web/routes/handlers/room.rs#L1497-L1529)

## P1（部分字段硬编码 / 伪分页）

1) **GET** `/_matrix/client/r0/rooms/{room_id}` → `get_room_info`
- 现状：`invited_members_count = 0`、`guest_can_join = false` 固定。
- 风险：计数与权限展示不准，影响客户端 UI 与行为。
- 建议：复用 `room_memberships` 统计 invite 数；guest/join 语义建议从状态事件推导（复用 `event_storage` state 查询）。
- 代码参考：[get_room_info](file:///Users/ljf/Desktop/hu/synapse-rust/src/web/routes/handlers/room.rs#L232-L285)

2) **GET** `/_matrix/client/r0/rooms/{room_id}/members/recent` → `get_room_members_recent`
- 现状：`chunk` 已来自 `room_service.get_room_members`，但 `start/end` 恒为 `"0"`。
- 风险：伪分页，客户端可能循环拉取或停止拉取。
- 建议：在 `room_service` 增加真实分页（from/limit + token），或改为返回与 Synapse 对齐的 token 语义。
- 代码参考：[get_room_members_recent](file:///Users/ljf/Desktop/hu/synapse-rust/src/web/routes/handlers/room.rs#L705-L724)

## P2（可接受的空响应 ACK：不应列为“空壳债务”）

说明：以下接口返回 `{}` 属于常见 ACK 语义，关键在于是否真的写入副作用；不建议作为“空壳接口”优先治理对象，但可纳入一致性回归测试。
- `POST /rooms/{room_id}/join`、`POST /rooms/{room_id}/leave`、`POST /rooms/{room_id}/forget`
- `POST /rooms/{room_id}/invite`、`POST /invite/{room_id}`
- `POST|PUT /rooms/{room_id}/read_markers`
- `PUT /rooms/{room_id}/account_data/{type}`
- `POST /rooms/{room_id}/kick|ban|unban`

## 建议 PR 切分（可直接开）

1) P0：receipt/get_receipts 的“非法 event_id 静默吞错”修正（仅错误语义，不引入新数据依赖）
2) P0：initialSync 选择“显式不支持”或复用 `SyncService::room_sync`（推荐复用）
3) P0：thread 接口复用 `ThreadService`（或短期 unrecognized）
4) P0：event_keys 改为 unrecognized（短期）+ 设计 keys 数据链路（中期）
5) P1：get_room_info 计数/guest 语义补齐；members/recent 真分页

