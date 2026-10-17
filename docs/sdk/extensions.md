# 本仓扩展（`/_matrix/vendor/v1`）

`/_matrix/vendor/v1/*` 是本项目的**私有扩展命名空间**（185 条路由），不属于 Matrix 规范。
标准客户端会忽略它们；自有客户端（Tjg / HuLa）依赖它们。

**通用规则**

- 认证同样只认 `Authorization: Bearer`。
- 成功响应无信封（与规范端点一致）。
- 这些端点**不保证**跨版本兼容；升级服务端前请对照本页。
- 文本检索、分页等能力与 `/client/v3` 有差异时，本页逐条注明。

## 目录

- [房间摘要](#房间摘要)
- [线程](#线程)
- [置顶事件](#置顶事件)
- [延迟事件（MSC4140）](#延迟事件msc4140)
- [Sticky 事件（MSC4354）](#sticky-事件msc4354)
- [直聊 Direct](#直聊-direct)
- [Spaces](#spaces)
- [阅后即焚](#阅后即焚)
- [房间杂项](#房间杂项)
- [用户与会话杂项](#用户与会话杂项)
- [推送扩展](#推送扩展)
- [能力与开关对照](#能力与开关对照)

---

## 房间摘要

摘要是一份"房间展示所需的聚合视图"，避免客户端每次都去拼 `m.room.*` 状态。

| 方法 | 端点 | 说明 |
|---|---|---|
| `GET` | `/rooms/{room_id}/summary` | 单个摘要；不存在 → 404 |
| `POST`/`PUT`/`DELETE` | `/rooms/{room_id}/summary` | 创建/更新/删除（写侧，通常服务端自用） |
| `POST` | `/rooms/{room_id}/summary/sync` | 触发该房间摘要重算 |
| `POST` | `/rooms/{room_id}/summary/heroes/recalculate` | 重算 heroes |
| `POST` | `/rooms/{room_id}/summary/stats/recalculate` | 重算统计 |
| `POST` | `/rooms/{room_id}/summary/unread/clear` | 清未读计数 |
| `GET` | `/rooms/{room_id}/summary/members` | 摘要成员列表 |
| `GET` | `/rooms/{room_id}/summary/state` | 摘要状态 |
| `GET` | `/rooms/{room_id}/summary/state/{event_type}/{state_key}` | 摘要状态单条 |
| `PUT` | `/rooms/{room_id}/summary/state/{event_type}/{state_key}` | 写摘要状态 |
| `GET` | `/rooms/{room_id}/summary/stats` | 摘要统计 |

**也有标准面**：`GET /_matrix/client/v1/rooms/{room_id}/summary`（同数据）。

**`GET` 响应**（按 `RoomSummaryResponse` 序列化）：

```json
{
  "room_id": "!abc:matrix.test",
  "room_type": null,
  "name": "项目组",
  "topic": "排期与评审",
  "avatar_url": "mxc://matrix.test/xyz",
  "canonical_alias": "#project:matrix.test",
  "join_rule": "invite",
  "history_visibility": "shared",
  "guest_access": "forbidden",
  "is_direct": false,
  "is_space": false,
  "is_encrypted": true,
  "member_count": 12,
  "joined_member_count": 11,
  "invited_member_count": 1,
  "heroes": [ { "user_id": "@bob:matrix.test", "display_name": "Bob", "avatar_url": "mxc://..." } ],
  "last_event_ts": 1760700000000,
  "last_message_ts": 1760699990000,
  "allowed_room_ids": ["!space:matrix.test"]
}
```

| 字段 | 语义 |
|---|---|
| `heroes` | 无 `name` 时用于构造展示名的成员（已排除自己） |
| `allowed_room_ids` | **仅当** `join_rule` 为 `restricted` / `knock_restricted` 时出现（其余情况字段缺省，不是 `null`） |
| `last_event_ts` / `last_message_ts` | 后者只统计消息类事件（更贴近"最后一条消息"） |
| `is_encrypted` | 等价于存在 `m.room.encryption` 状态 |

> ⚠️ 摘要**不是实时的**：它由队列异步重算。需要实时性时用 `/sync` 或读状态事件。
> 另外 `capabilities.m.room.summary` 当前恒为 `false`（探针路径写错，见
> [discovery.md](./discovery.md#-已知能力声明缺陷探针路径与真实路由不一致)），
> **不要**据此判断摘要是否可用。

**批量**：`GET /_synapse/room_summary/v1/summaries`（当前用户全部房间）、
`POST /_synapse/room_summary/v1/summaries/batch`（指定房间批量）。
批量响应同时给 `summaries` / `rooms` / `chunk` 三个同内容的键 + `next_batch`（当前恒 `null`）。

---

## 线程

标准面（MSC3440）：

| 方法 | 端点 |
|---|---|
| `GET` | `/_matrix/client/v1/rooms/{room_id}/threads` |
| `GET` | `/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}` |
| `GET`/`POST` | `/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/replies` |

扩展面：

| 方法 | 端点 | 说明 |
|---|---|---|
| `GET` | `/threads` | 全局线程列表 |
| `POST` | `/threads` | 全局建线程 |
| `GET` | `/threads/subscribed` | 我参与/订阅的线程 |
| `GET` | `/threads/unread` | 有未读的线程 |
| `POST` | `/rooms/{room_id}/threads` | 房间内建线程 |
| `GET` | `/rooms/{room_id}/threads/unread` | 房间内未读线程 |
| `GET` | `/rooms/{room_id}/threads/search` | 线程全文搜索（keyset 游标，按相关性排序） |
| `DELETE` | `/rooms/{room_id}/threads/{thread_id}` | 删除线程 |
| `GET` | `/rooms/{room_id}/threads/{thread_id}/stats` | 线程统计 |
| `POST` | `/rooms/{room_id}/threads/{thread_id}/freeze` | 冻结（只读） |
| `POST` | `/rooms/{room_id}/threads/{thread_id}/unfreeze` | 解冻 |
| `POST` | `/rooms/{room_id}/threads/{thread_id}/mute` | 静音 |
| `POST` | `/rooms/{room_id}/threads/{thread_id}/read` | 标记已读 |
| `POST` | `/rooms/{room_id}/replies/{event_id}/redact` | 撤回线程内回复 |
| `GET` | `/user/{user_id}/rooms/{room_id}/threads` | 旧版（legacy）线程列表 |
| `GET` | `/rooms/{room_id}/thread/{event_id}` | 按根事件取线程 |

**列表响应**（`ThreadListResponse`）：

```json
{
  "threads": [
    {
      "id": 7, "room_id": "!abc:matrix.test", "thread_id": "$root:matrix.test",
      "root_event_id": "$root:matrix.test", "root_sender": "@alice:matrix.test",
      "root_content": { "msgtype": "m.text", "body": "讨论一下这个" },
      "root_origin_server_ts": 1760699000000,
      "latest_event_id": "$last:matrix.test", "latest_sender": "@bob:matrix.test",
      "latest_content": { "msgtype": "m.text", "body": "同意" },
      "latest_origin_server_ts": 1760700000000,
      "reply_count": 5,
      "participants": ["@alice:matrix.test", "@bob:matrix.test"],
      "is_frozen": false,
      "created_ts": 1760699000000, "updated_ts": 1760700000000
    }
  ],
  "next_batch": "eyJ0cyI6...",
  "total": 3
}
```

> 旧版 `/user/{user_id}/rooms/{room_id}/threads` 返回的是**精简形态**
> （`chunk` + 每个元素只有 `event_id` / `sender` / `content` / `origin_server_ts`）。
> 新代码请用 `/client/v1/rooms/{id}/threads`（结构化）或 `/rooms/{id}/threads/unread`。

---

## 置顶事件

| 方法 | 端点 | 请求体 | 响应 |
|---|---|---|---|
| `GET` | `/rooms/{room_id}/pinned_events` | — | `{"pinned_events": ["$a", "$b"]}` |
| `POST` | `/rooms/{room_id}/pinned_events` | `{"event_id": "$x"}` | `{"pinned_event": "$x"}` |
| `DELETE` | `/rooms/{room_id}/pinned_events/{event_id}` | — | `{"unpinned_event": "$x"}` |

- 需要房间成员身份；非成员 403。
- 置顶写入需要 `m.room.pinned_events` 的状态写权限（power level），不足 403。
- 底层就是 `m.room.pinned_events` 状态事件的 `content.pinned` 数组——
  客户端也可直接 `PUT /state/m.room.pinned_events/`（两者等价）。

---

## 延迟事件（MSC4140）

客户端可"先排期、后撤销"。所有路径在 `/_matrix/client/unstable/org.matrix.msc4140/`。

| 方法 | 端点 | 说明 |
|---|---|---|
| `PUT` | `/rooms/{room_id}/delayed_event/{event_type}/{txn_id}` | 排期（也支持在 `/send`、`/state` 上传 `?org.matrix.msc4140.delay=`） |
| `GET` | `/delayed_events` | 列出我的待发事件 |
| `GET` | `/delayed_events/{delay_id}` | 单个 |
| `POST` | `/delayed_events/{delay_id}` | 通用管理（体 `{"action": "..."}`） |
| `POST` | `/delayed_events/{delay_id}/cancel` | 取消 |
| `POST` | `/delayed_events/{delay_id}/restart` | 重新计时（心跳续期） |
| `POST` | `/delayed_events/{delay_id}/send` | 立即发送 |

**列表响应**：`{"delayed_events": [ <DelayedEvent> ]}`

**`DelayedEvent` 形状**（单条与列表同源，不会漂移）：

```json
{
  "delay_id": 17,
  "room_id": "!abc:matrix.test",
  "type": "m.room.message",
  "state_key": null,
  "delay_ms": 30000,
  "delayed_since_ts": 1760700000000,
  "content": { "msgtype": "m.text", "body": "晚安" }
}
```

- `state_key` 在消息事件下**省略**（不是 `null`）；状态事件才出现。
- `delay_id` 是**数字**（i64），与好友系统的 `request_id`（字符串）不同。
- 只有排期者本人能管理；否则 404（不泄露存在性）。
- `POST /delayed_events/{id}` 的 `action` 缺失/非字符串 → 400 `M_BAD_JSON` 并说明字段。

**与 Sticky 的组合**：延迟事件在**真正发送那一刻**才把 sticky TTL 写入 `unsigned`
（详见下一节与 `scheduled_dispatch`）。因此"延迟 + 置顶"是原子语义，不是降级。

---

## Sticky 事件（MSC4354）

| 方法 | 端点 | 说明 |
|---|---|---|
| `GET` | `/rooms/{room_id}/sticky_events` | 列出房间 sticky 事件 |
| `GET` | `/rooms/{room_id}/sticky_events/{event_type}` | 按类型 |
| `POST` | `/rooms/{room_id}/sticky_events` | 设置 sticky |
| `DELETE` | `/rooms/{room_id}/sticky_events/{event_type}` | 清除 |

**条目形状**：

```json
{ "room_id": "!abc:matrix.test", "user_id": "@alice:matrix.test",
  "event_id": "$x:matrix.test", "event_type": "m.room.message",
  "sticky_duration_ms": 3600000, "sticky_expiry_ts": 1760703600000 }
```

列表响应：`{"events": [ ... ]}`。

**另两条写入路径**（标准面，推荐优先使用）：

| 端点 | 参数 | 说明 |
|---|---|---|
| `PUT/POST /rooms/{id}/send/{event_type}/{txn}` | `?org.matrix.msc4354.sticky_duration_ms=N` | 发送时顺带置顶 |
| `PUT/POST /rooms/{id}/state/{event_type}[/{state_key}]` | 同上 | 写状态时顺带置顶 |

约束：

- 取值 `1..=3600000`（毫秒，上限 1 小时）；非整数/越界 → **400 `M_INVALID_PARAM`**。
- 短名 `sticky_duration_ms` 也接受，**命名空间名优先**。
- TTL 落在事件的 `unsigned.matrix.msc4354_sticky_duration_ttl_ms`。

> ⚠️ 读 `unsigned` 必须用 `GET /rooms/{id}/event/{event_id}`——
> `/messages` 的 `chunk` 与 `/sync` 的 `timeline.events` 都**不带** `unsigned`。
>
> 延迟事件面：`?org.matrix.msc4140.delay=` 与 sticky 可同时给；
> 投递时才写 `unsigned`（见 [messages.md](./messages.md#延迟发送与-sticky)）。
> 另外 **sticky 状态事件不允许**（`state_key` + sticky ⇒ 400 `M_INVALID_PARAM`，与上游一致）。

---

## 直聊 Direct

| 方法 | 端点 | 响应 |
|---|---|---|
| `GET` | `/direct` | `{"rooms": <m.direct 映射>}` |
| `PUT` | `/direct/{room_id}` | `{"room_id", "users", "direct_map", "updated_ts"}` |
| `POST` | `/create_dm` | `{"room_id"}` |
| `GET` | `/rooms/{room_id}/dm` | DM 时 `{"room_id", "m.direct": true}`；非 DM → **404** |
| `GET` | `/rooms/{room_id}/dm/partner` | `{"room_id", "user_id", "display_name", "avatar_url"}` |
| `POST` | `/rooms/create_private` | 建私有房间 |
| `GET` | `/user/mutual_rooms` | 共同房间 |
| `POST` | `/search_recipients` | 收件人搜索 |
| `POST` | `/search_rooms` | 房间搜索 |
| `GET` | `/my_rooms` | 我的房间（含状态） |

> `GET /rooms/{id}/dm` 用 **404** 表示"不是 DM"——客户端不要把它当"房间不存在"。

---

## Spaces

| 方法 | 端点 | 说明 |
|---|---|---|
| `POST` | `/spaces` | 建空间 |
| `GET` | `/spaces/{space_id}` | 空间详情 |
| `PUT`/`DELETE` | `/spaces/{space_id}` | 更新/删除 |
| `GET` | `/spaces/{space_id}/children` | 子项列表 |
| `POST` | `/spaces/{space_id}/children` | 添加子项 |
| `DELETE` | `/spaces/{space_id}/children/{room_id}` | 移除子项 |
| `GET` | `/spaces/{space_id}/hierarchy`、`/hierarchy/v1` | 层级树 |
| `GET` | `/spaces/{space_id}/tree_path` | 从根到该空间的路径 |
| `GET` | `/spaces/{space_id}/rooms` | 空间内房间 |
| `GET` | `/spaces/{space_id}/members` | 成员 |
| `GET` | `/spaces/{space_id}/state` | 状态 |
| `POST` | `/spaces/{space_id}/invite`、`/join`、`/leave` | 邀请/加入/离开 |
| `GET` | `/spaces/{space_id}/summary`、`/summary/with_children` | 摘要（含子树） |
| `GET` | `/spaces/{space_id}/stats` | 统计 |
| `GET` | `/spaces/public` | 公开空间 |
| `GET` | `/spaces/search` | 搜索空间 |
| `GET` | `/spaces/user` | 我的空间 |
| `GET` | `/spaces/statistics` | 全站空间统计 |
| `GET` | `/spaces/room/{room_id}` | 由房间反查所属空间 |
| `GET` | `/spaces/room/{room_id}/parents` | 房间的父空间列表 |

标准面另有 `GET /_matrix/client/v1|v3/spaces/{space_id}/hierarchy`。

---

## 阅后即焚

需要能力 `io.hula.burn_after_read`（认证后才出现在 `/capabilities`）。

| 方法 | 端点 | 说明 |
|---|---|---|
| `PUT` | `/rooms/{room_id}/burn` | 设置房间级 TTL |
| `GET` | `/rooms/{room_id}/burn` | 读配置 |
| `POST` | `/rooms/{room_id}/burn/{event_id}` | 标记某事件已读（启动倒计时） |
| `DELETE` | `/rooms/{room_id}/burn/{event_id}` | 取消 |
| `GET` | `/rooms/{room_id}/burn/pending` | 待焚毁列表 |
| `PUT` | `/user/burn/config` | 用户级配置 |
| `GET` | `/user/burn/stats` | 用户统计 |

---

## 房间杂项

| 方法 | 端点 | 说明 |
|---|---|---|
| `GET` | `/rooms/{room_id}/metadata` | 房间元数据聚合 |
| `GET` | `/rooms/{room_id}/unread_count` | 未读数 |
| `GET` | `/rooms/{room_id}/notifications` | 房间通知 |
| `GET` | `/rooms/{room_id}/permissions` | 我的权限 |
| `GET` | `/rooms/{room_id}/capabilities` | 房间级能力 |
| `GET` | `/rooms/{room_id}/version` | 房间版本 |
| `GET` | `/rooms/{room_id}/timeline` | 时间线视图 |
| `GET` | `/rooms/{room_id}/reduced_events` | 精简事件 |
| `GET` | `/rooms/{room_id}/event_perspective` | 事件视角（"从这里看"） |
| `GET` | `/rooms/{room_id}/event/{event_id}/url` | 事件的固定链接（可分享 URL） |
| `GET` | `/rooms/{room_id}/external_ids` | 外部 ID 映射 |
| `GET` | `/rooms/{room_id}/service_types` | 服务类型 |
| `GET` | `/rooms/{room_id}/invites` | 我收到的邀请 |
| `GET` | `/rooms/{room_id}/fragments/{user_id}` | 用户在本房间的片段 |
| `GET` | `/rooms/{room_id}/message_queue` | 消息队列状态 |
| `GET` | `/rooms/{room_id}/vault_data` / `PUT` | 房间级保险库数据 |
| `GET`/`PUT` | `/rooms/{room_id}/anti_screenshot` | 防截屏开关 |
| `GET` | `/rooms/{room_id}/turn_server` | 房间 TURN 服务器 |
| `GET`/`POST` | `/rooms/{room_id}/invite_allowlist`、`/invite_blocklist` | 邀请白/黑名单 |
| `POST` | `/rooms/{room_id}/convert/{event_id}` | 消息类型转换 |
| `POST` | `/rooms/{room_id}/translate/{event_id}`、`POST /translate` | 翻译 |
| `POST` | `/rooms/{room_id}/verify/{event_id}`、`PUT /rooms/{room_id}/sign/{event_id}` | 事件签名/校验 |
| `GET` | `/rooms/{room_id}/summary/stats` | 摘要统计 |
| `GET` | `/rooms/{room_id}/keys`、`/keys/{event_id}`、`/keys/count`、`/keys/version` | 房间密钥视图 |
| `POST` | `/rooms/{room_id}/keys/claim` | 认领房间密钥 |
| `GET`/`PUT` | `/rooms/{room_id}/room_keys/keys` | 房间密钥存取 |
| `GET` | `/rooms/{room_id}/keys/distribution` | 密钥分发状态（标准面另有 `/client/v1/...`） |
| `GET` | `/rooms/{room_id}/membership/{user_id}`、`/members/recent` | 成员视图 |
| `POST` | `/rooms/{room_id}/get_membership_events` | 成员变更历史 |
| `GET` | `/rooms/{room_id}/receipts/{receipt_type}/{event_id}` | 回执 |
| `GET` | `/rooms/{room_id}/report/{event_id}/scanner_info` | 内容扫描结果 |
| `PUT` | `/rooms/{room_id}/report/{event_id}/score` | 举报评分（内部） |
| `PUT` | `/rooms/{room_id}/relations/{event_id}/{rel_type}/{txn_id}` | relations 写入 |
| `GET` | `/rooms/{room_id}/rendered/` | 渲染视图（HTML 片段） |
| `GET` | `/rooms/{room_id}/resolve` | 房间 ID 解析 |

**Widgets**（`widgets` 在默认 feature 集内）：

| 方法 | 端点 |
|---|---|
| `POST` | `/widgets` |
| `GET`/`PUT`/`DELETE` | `/widgets/{widget_id}` |
| `GET` | `/widgets/{widget_id}/config` |
| `GET`/`POST` | `/widgets/{widget_id}/permissions` |
| `DELETE` | `/widgets/{widget_id}/permissions/{user_id}` |
| `GET`/`POST` | `/widgets/{widget_id}/sessions` |
| `GET`/`DELETE` | `/widgets/sessions/{session_id}` |
| `GET` | `/rooms/{room_id}/widgets` |
| `GET` | `/rooms/{room_id}/widgets/{widget_id}/capabilities` |
| `PUT` | `/rooms/{room_id}/widgets/{widget_id}/capabilities` |
| `GET` | `/rooms/{room_id}/widgets/jitsi/config` |

---

## 用户与会话杂项

| 方法 | 端点 | 说明 |
|---|---|---|
| `GET` | `/user/{user_id}/rooms` | 某用户的房间 |
| `GET` | `/user/{user_id}/rooms/{room_id}/threads` | 该用户在该房间的线程 |
| `GET` | `/push/devices` | 推送设备列表 |
| `POST` | `/push/devices` | 注册推送设备 |
| `DELETE` | `/push/devices/{device_id}` | 注销 |
| `POST` | `/push/send` | 主动推送（内部/测试） |
| `GET`/`POST` | `/external_services`、`/external_services/{id}` | 外部服务注册 |
| `GET` | `/external_services/health` | 健康检查 |

---

## 推送扩展

`/_matrix/vendor/v1/push/devices*` 与标准 `/_matrix/client/v3/pushers`（见
[devices.md](./devices.md)）是**两套**：前者按 `device_id` 管理本仓的推送设备记录，
后者是 Matrix 规范的 pusher。

---

## 能力与开关对照

| 扩展 | 能力位 | 当前 |
|---|---|---|
| 好友 | `capabilities.io.hula.friends`（**仅认证后**） | `true` |
| 阅后即焚 | `capabilities.io.hula.burn_after_read`（仅认证后） | `true` |
| 线程 | `capabilities.m.thread` | `true` |
| 房间摘要 | `capabilities.m.room.summary` | **`false`（探针缺陷，功能实际可用）** |
| 扩展 profile | `capabilities.m.profile_fields` | `true` |
| 语音 | `capabilities.m.voice` | `true` |
| 扩展语音 | `capabilities.io.hula.voice_extended`（仅认证后） | `false`（探针路径不存在） |
| 延迟事件 | `/versions` 的 `org.matrix.msc4140` | `true` |
| Sticky | `/versions` 的 `org.matrix.msc4354` | 由路由面推导；早期二进制不声明 |
| 脱水设备 | `/versions` 的 `org.matrix.msc3814` | `true` |
| Sliding Sync | `org.matrix.msc3886.sliding_sync` / `org.matrix.simplified_msc3575` | `true` |
