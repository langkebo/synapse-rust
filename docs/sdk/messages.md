# 消息与事件

发送、编辑、撤回、历史分页，以及 relations（回复/编辑/表情回应/线程）、置顶、搜索、上下文。

> 延迟发送（MSC4140）、Sticky（MSC4354）、阅后即焚、线程扩展等见
> [extensions.md](./extensions.md)；`/sync` 的时间线投递见 [sync.md](./sync.md)。

## 目录

- [发送消息](#发送消息)
- [事件内容形状](#事件内容形状)
- [延迟发送与 Sticky](#延迟发送与-sticky)
- [历史分页 `/messages`](#历史分页-messages)
- [单事件与上下文](#单事件与上下文)
- [编辑与撤回](#编辑与撤回)
- [回应与聚合 relations](#回应与聚合-relations)
- [线程](#线程)
- [置顶事件](#置顶事件)
- [回执与已读标记](#回执与已读标记)
- [打字状态](#打字状态)
- [搜索](#搜索)
- [常见错误](#常见错误)

---

## 发送消息

| 方法 | 端点 | 说明 |
|---|---|---|
| `PUT` | `/rooms/{room_id}/send/{event_type}/{txn_id}` | 标准发送（客户端主用） |
| `POST` | `/rooms/{room_id}/send/{event_type}/{txn_id}` | 同上（POST 形态） |
| `PUT` | `/rooms/{room_id}/send/m.reaction/{txn_id}` | 表情回应专用（见 [回应](#回应与聚合-relations)） |

**成功响应**：`{ "event_id": "$abc:matrix.test" }`（**不是** `{status, data}`）。

**`txn_id` 语义**：同一 `(user, device, txn_id, event_type)` 重放返回**同一个 `event_id`**，
不会重复发消息。客户端重试时必须复用原 `txn_id`（常用 `Date.now()` 或 UUID）。

**幂等边界**：`txn_id` 只在同一设备、同一会话内去重；换设备重发会真的发第二条。

---

## 事件内容形状

### 文本（含 HTML）

```json
{
  "msgtype": "m.text",
  "body": "第一行\n第二行",
  "format": "org.matrix.custom.html",
  "formatted_body": "第一行<br/>第二行"
}
```

`format` 必须是 `org.matrix.custom.html`，且 `formatted_body` 要与 `body` 语义一致
（客户端做降级渲染时用 `body`）。

### 图片 / 文件 / 音视频

```json
{
  "msgtype": "m.image",
  "body": "photo.jpg",
  "url": "mxc://matrix.test/abc",
  "info": { "mimetype": "image/jpeg", "w": 1024, "h": 768, "size": 123456, "thumbnail_url": "mxc://...", "thumbnail_info": { "w": 256, "h": 256 } }
}
```

| `msgtype` | 关键字段 |
|---|---|
| `m.image` | `url`、`info.w/h/mimetype/size`、`info.thumbnail_url` |
| `m.file` | `url`、`filename`、`info.mimetype/size` |
| `m.audio` | `url`、`info.duration`（毫秒）、`info.mimetype` |
| `m.video` | `url`、`info.duration/w/h`、`info.thumbnail_url` |
| `m.emote` | 同 `m.text`，渲染为 `/me` 动作 |

> `url` 已在新版规范中改为 `file: {url}`（MSC4278）形态；本仓**两种都接受写入**，
> 但读取历史事件时以 `content.file.url` 优先、回退 `content.url` 更稳。

### 自定义事件

任意 `event_type` 都可发；内容不做 schema 校验（除已知类型）。客户端自定义类型建议用
`io.hula.*` / `m.*` 前缀区分。

---

## 延迟发送与 Sticky

两条都作用在**写入路径的查询参数**上（不是 body）：

```http
PUT /_matrix/client/v3/rooms/{roomId}/send/m.room.message/{txn}
    ?org.matrix.msc4140.delay=30000
    &org.matrix.msc4354.sticky_duration_ms=3600000
```

| 参数 | 语义 | 约束 |
|---|---|---|
| `org.matrix.msc4140.delay` | 延迟多少毫秒后发送 | `0 < delay ≤ MSC4140_MAX_DELAY_MS`；非法 → 400 `M_INVALID_PARAM` |
| `org.matrix.msc4354.sticky_duration_ms` | 事件置顶（sticky）时长 | `0 < n ≤ 3600000`（1h）；短名 `sticky_duration_ms` 也认，命名空间优先 |

- 带 `delay` 时响应形状不同：是**延迟事件对象**（`delay_id` 等），不是 `event_id`。
- `delay` 与 `sticky` 可同时给：延迟事件在**真正投递时**才把 sticky TTL 写进 `unsigned`。
- 延迟事件的查询/取消/立即发送见 [extensions.md](./extensions.md#延迟事件msc4140)。

```typescript
// 30 秒后发送
const res = await call(base, `/_matrix/client/v3/rooms/${roomId}/send/m.room.message/${txn}` +
  '?org.matrix.msc4140.delay=30000',
  token, { method: 'PUT', body: JSON.stringify({ msgtype: 'm.text', body: '晚安' }) });
// res.delay_id 而不是 res.event_id
```

---

## 历史分页 `/messages`

`GET /_matrix/client/v3/rooms/{room_id}/messages`

| 参数 | 类型 | 默认 | 说明 |
|---|---|---|---|
| `from` | string | 无 | 游标（上次响应的 `start` / `end`，或 `/sync` 的 `prev_batch`） |
| `to` | string | 无 | 终止游标 |
| `dir` | `f` \| `b` | **`b`** | 方向。**其他取值 → 400 `M_INVALID_PARAM`**（不会静默回退） |
| `limit` | int | **`10`** | 夹在 `1..=1000` |
| `filter` | string | 无 | ⚠️ **不支持**：非空值 → 400 `M_INVALID_PARAM` |

**响应**：

```json
{
  "chunk": [ { "type": "m.room.message", "event_id": "$x", "sender": "@a:x",
               "origin_server_ts": 1760700000000, "content": { } } ],
  "start": "t42-10",
  "end": "t42-1",
  "state": []
}
```

**三个必须知道的点**

1. **`chunk` 里的消息事件不带 `unsigned`**（sticky TTL、`age`、`transaction_id` 都不在）。
   要读 `unsigned` 必须逐条走 `GET /rooms/{id}/event/{event_id}`。
2. **`dir` 缺省是 `b`**（向后），第一次加载最新消息用 `dir=b`；补更早的用 `dir=b` + `end`。
3. **`filter` 一旦传非空就被拒**。懒加载成员是 `/sync?filter=` 的能力，不是这里。

**自动回填**：`dir=b` 且本地返回不足 `limit` 条时，服务端会异步向联邦节点请求更早事件
（按房间做速率限制，避免回填风暴）。客户端不必自己判断，但**不要**因为"没拿满"就立即再请求。

---

## 单事件与上下文

| 方法 | 端点 | 说明 |
|---|---|---|
| `GET` | `/rooms/{room_id}/event/{event_id}` | 单个事件，**含完整 `unsigned`** |
| `GET` | `/rooms/{room_id}/context/{event_id}?limit=N` | 上下文：`event` + `events_before` + `events_after` + `start`/`end` + `state` |
| `GET` | `/client/v1/rooms/{room_id}/context/{event_id}` | v1 变体 |
| `GET` | `/_matrix/vendor/v1/rooms/{room_id}/timeline` | vendor 时间线视图 |

**`/context` 响应**：

```json
{
  "event": { "event_id": "$x", "type": "m.room.message", "content": {} },
  "events_before": [ ],
  "events_after": [ ],
  "start": "t1", "end": "t2",
  "state": [ ]
}
```

---

## 编辑与撤回

### 编辑消息

编辑**不是** PATCH，而是再发一条带 `m.replace` 关系的事件：

```json
PUT /_matrix/client/v3/rooms/{roomId}/send/m.room.message/{txn}
{
  "msgtype": "m.text",
  "body": "* 改好的文本",
  "m.new_content": { "msgtype": "m.text", "body": "改好的文本" },
  "m.relates_to": { "rel_type": "m.replace", "event_id": "$原事件" }
}
```

渲染规则：用 `m.new_content` 覆盖原文；`body` 里的星号加空格前缀是给不支持编辑的客户端的降级展示。

### 撤回

**端点需要 `txn_id` 且必须带**：

| 方法 | 端点 |
|---|---|
| `PUT` | `/rooms/{room_id}/redact/{event_id}/{txn_id}` |
| `POST` | `/rooms/{room_id}/redact/{event_id}/{txn_id}` |

```json
{ "reason": "发错了" }
```

响应 `{ "event_id": "$撤回事件" }`。

> ⚠️ **没有** `PUT /rooms/{id}/redact/{event_id}`（缺 `txn_id` 的形态）。历史文档写
> "此端点不需要事务 ID，服务器会自动生成" —— 那句是错的，按它构造的 URL 会 404。

撤回后的原事件 `content` 被清空，保留 `type` / `sender` / `redacts` 关系；
`unsigned.redacted_because` 给出撤回事件本身。

### 线程内撤回

`POST /_matrix/vendor/v1/rooms/{room_id}/replies/{event_id}/redact`（见 [extensions.md](./extensions.md)）

---

## 回应与聚合 relations

### 读关系

| 方法 | 端点 | 说明 |
|---|---|---|
| `GET` | `/rooms/{room_id}/relations/{event_id}` | 全部关系事件 |
| `GET` | `/rooms/{room_id}/relations/{event_id}/{rel_type}` | 按 `rel_type` 过滤（`m.annotation` / `m.replace` / `m.thread` / `m.reference`） |
| `GET` | `/rooms/{room_id}/relations/{event_id}/{rel_type}/{event_type}` | 再按事件类型过滤 |
| `GET` | `/client/v1/rooms/{room_id}/relations/{event_id}` | v1 变体 |
| `GET` | `/rooms/{room_id}/aggregations/{event_id}/{rel_type}` | **聚合计数**（如表情回应统计） |

参数：`from`、`to`、`limit`、`dir`、`recurse`（MSC3981，递归取子关系）。

**聚合响应**（注意：**没有 `next_batch`**，聚合是一次性返回的）：

```json
{
  "chunk": [
    { "type": "m.annotation", "key": "👍", "count": 3, "sender": null, "origin_server_ts": null },
    { "type": "m.annotation", "key": "🎉", "count": 1, "sender": null, "origin_server_ts": null }
  ]
}
```

`type` 固定为 `m.annotation`；`key` 是 annotation 的键（表情字符），`count` 是去重后的发送者数。
`sender` / `origin_server_ts` 仅在单条聚合视图（某些内部调用）下填充，聚合列表里为 `null`。

### 加表情回应

**专用端点**（推荐）：

```http
PUT /_matrix/client/v3/rooms/{roomId}/send/m.reaction/{txn}
{ "m.relates_to": { "rel_type": "m.annotation", "event_id": "$目标事件", "key": "👍" } }
```

**响应**：`{ "event_id": "$reaction" }`

规则（本仓实现）：

| 检查 | 失败结果 |
|---|---|
| 必须是房间成员 | 403 `M_FORBIDDEN`（`You must be a room member to react`） |
| 需满足 `m.reaction` 的 power level | 403 `M_FORBIDDEN` |
| `m.relates_to` 缺失 | 400 `M_BAD_JSON`（`Missing m.relates_to`） |
| `rel_type` 不是 `m.annotation` | 400 `M_BAD_JSON`（`rel_type must be m.annotation for reactions`） |

- 键名接受 `m.relates_to` 或 `relates_to`；`key` 缺失时回退读顶层 `body`，再缺省为 `👍`。
- 撤回表情 = 撤回那条 `m.reaction` 事件（走 `/redact`）。

### 回复

回复是普通消息 + `m.in_reply_to`：

```json
{
  "msgtype": "m.text",
  "body": "> <@bob:x> 原文\n\n我的回复",
  "format": "org.matrix.custom.html",
  "formatted_body": "<mx-reply><blockquote>...</blockquote></mx-reply>我的回复",
  "m.relates_to": { "m.in_reply_to": { "event_id": "$被回复事件" } }
}
```

注意 `m.in_reply_to` 在 `m.relates_to` **内部**（不是 `rel_type`/`event_id` 平铺）。

---

## 线程

| 方法 | 端点 | 说明 |
|---|---|---|
| `GET` | `/client/v1/rooms/{room_id}/threads?limit&from` | 线程列表 |
| `GET` | `/client/v1/rooms/{room_id}/threads/{thread_id}` | 线程详情 |
| `GET` | `/client/v1/rooms/{room_id}/threads/{thread_id}/replies` | 线程内回复 |
| `POST` | `/client/v1/rooms/{room_id}/threads/{thread_id}/replies` | 发线程回复 |
| `GET` | `/client/v1/rooms/{room_id}/threads/{thread_id}/replies`（同 GET） | — |

发线程消息的正确做法仍是普通 `/send`，带：

```json
{ "m.relates_to": { "rel_type": "m.thread", "event_id": "$线程根事件", "is_falling_back": true, "m.in_reply_to": { "event_id": "$上一条" } } }
```

线程的 vendor 扩展（freeze / mute / read / stats / search / 全局线程列表）见
[extensions.md](./extensions.md#线程)。

能力门控：`capabilities.m.thread.enabled`（当前 `true`）。

---

## 置顶事件

**vendor 路径**（本仓未在 `/_matrix/client/*` 下实现置顶）：

| 方法 | 端点 | 说明 |
|---|---|---|
| `GET` | `/_matrix/vendor/v1/rooms/{room_id}/pinned_events` | 列出置顶事件 |
| `POST` | `/_matrix/vendor/v1/rooms/{room_id}/pinned_events` | 置顶（体含 `event_id`） |
| `DELETE` | `/_matrix/vendor/v1/rooms/{room_id}/pinned_events/{event_id}` | 取消置顶 |

详细字段见 [extensions.md](./extensions.md#置顶事件)。

> 与 **Sticky 事件**不同：置顶是"房间级事件 ID 列表"（持久），Sticky 是"事件带 TTL 的
> `unsigned` 标记"（到时自动失效）。两者互不影响。

---

## 回执与已读标记

| 方法 | 端点 | 说明 |
|---|---|---|
| `POST` | `/rooms/{room_id}/receipt/{receipt_type}/{event_id}` | 发回执，`receipt_type` **只接受 `m.read` / `m.read.private`** |
| `GET` | `/_matrix/vendor/v1/rooms/{room_id}/receipts/{receipt_type}/{event_id}` | 读回执 |
| `POST`/`PUT` | `/rooms/{room_id}/read_markers` | 一次设置多个标记（`m.fully_read` 只能走这里） |

> ⚠️ `receipt_type` 白名单只有 `["m.read", "m.read.private"]`
> （`validators.rs::validate_receipt_type`）。传 `m.fully_read` → **400 `M_INVALID_PARAM`**。
> 已读书签必须用 `read_markers`。

`read_markers` 体：

```json
{ "m.read": "$event", "m.read.private": "$event", "m.fully_read": "$event" }
```

- `m.read`：公开已读（会同步给其他成员）。
- `m.read.private`：仅本设备/本账户可见的已读位置。
- `m.fully_read`：读位置书签（不产生公开回执）。

响应均为 `{}`；回执本身通过 `/sync` 的 `rooms.join.*.ephemeral.events` 里的 `m.receipt`
下发（不产生 timeline 事件）。

> 回执的 `event_id` 必须是**房间里真实存在的事件**；不存在的 ID 会 404 `M_NOT_FOUND`。

---

## 打字状态

| 方法 | 端点 | 体 |
|---|---|---|
| `PUT` | `/rooms/{room_id}/typing/{user_id}` | `{"typing": true, "timeout": 30000}` |
| `POST` | `/rooms/{room_id}/typing/{user_id}` | 同上 |
| `GET` | `/rooms/{room_id}/typing` | 当前正在输入的用户 |
| `POST` | `/rooms/typing` | 批量形式 |

- `user_id` 必须是**自己**；否则 403 `M_FORBIDDEN`。
- `timeout` 默认 30000ms，服务端会在超时后自动清除；停止输入请显式发 `{"typing": false}`。
- 打字状态**不落库到 timeline**，只在 `/sync` 的 `ephemeral` 出现。客户端不要把它当消息缓存。

---

## 搜索

| 方法 | 端点 | 说明 |
|---|---|---|
| `POST` | `/search` | 全局搜索（房间消息 + 房间成员） |
| `POST` | `/_matrix/vendor/v1/rooms/{room_id}/search` | 房间内搜索（vendor，支持中文分词配置） |

**`POST /search` 请求体**：

```json
{
  "search_categories": {
    "room_events": {
      "search_term": "排期",
      "keys": ["content.body"],
      "order_by": "recent",
      "event_context": { "before_limit": 5, "after_limit": 5, "include_profile": false },
      "filter": { "rooms": ["!x:matrix.test"], "limit": 20 }
    }
  }
}
```

响应：`{ "search_categories": { "room_events": { "results": [...], "count": 3, "highlights": {}, "groups": {} } } }`

---

## 常见错误

| 场景 | 状态 | errcode |
|---|---|---|
| 缺 `txn_id`-类路径段（如撤回少了 txn） | 404 | `M_UNRECOGNIZED`/`M_NOT_FOUND`（路由不匹配） |
| `dir` 取值非法 | 400 | `M_INVALID_PARAM` |
| `/messages` 传了非空 `filter` | 400 | `M_INVALID_PARAM` |
| 发送到不存在的房间 | 404 | `M_NOT_FOUND` |
| 无写权限（`events_default`/被禁言） | 403 | `M_FORBIDDEN` |
| 事件不存在（读/撤回） | 404 | `M_NOT_FOUND` |
| 体不是 JSON | 400 | `M_NOT_JSON` |
| 体超过上限 | 413 | `M_TOO_LARGE` |
| 发送过快 | 429 | `M_LIMIT_EXCEEDED` |
