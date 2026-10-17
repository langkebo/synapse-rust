# 好友系统

好友是本仓的**私有扩展**，路由全部在 `/_matrix/vendor/v1/friends/*`（不在 `/client/*` 下）。
底层实现是"每个用户一个好友列表房间 + `m.friends.list` 状态事件"，但对客户端只暴露
REST 语义——**不要**直接读写那个房间的状态事件。

**能力发现**：`GET /capabilities` 带 token 时才有 `io.hula.friends.enabled`
（未认证时该键不出现，不要据此判定"没有好友功能"）。

## 目录

- [端点总览](#端点总览)
- [好友列表](#好友列表)
- [好友请求](#好友请求)
- [删除好友与好友属性](#删除好友与好友属性)
- [私信房间](#私信房间)
- [搜索与建议](#搜索与建议)
- [分组](#分组)
- [状态码与错误](#状态码与错误)
- [迁移提示（旧文档 → 实际）](#迁移提示旧文档--实际)

---

## 端点总览

| 方法 | 端点 | 用途 |
|---|---|---|
| `GET` | `/friends` | 好友列表（Keyset 分页） |
| `POST` | `/friends` | 发送好友请求（等价 `/friends/request`） |
| `DELETE` | `/friends/{user_id}` | 删除好友 |
| `POST` | `/friends/request` | 发送好友请求 |
| `POST` | `/friends/request/{user_id}/accept` | 接受 |
| `POST` | `/friends/request/{user_id}/reject` | 拒绝 |
| `POST` | `/friends/request/{user_id}/cancel` | 撤回自己发出的请求 |
| `GET` | `/friends/requests/incoming` | 收到待处理 |
| `GET` | `/friends/requests/outgoing` | 发出待处理 |
| `GET` | `/friends/check/{user_id}` | 是否好友 |
| `GET` | `/friends/{user_id}/info` | 好友详情 |
| `GET` | `/friends/{user_id}/status` | 好友关系状态 |
| `PUT` | `/friends/{user_id}/status` | 设置关系状态 |
| `PUT` | `/friends/{user_id}/note` | 好友备注 |
| `PUT` | `/friends/{user_id}/displayname` | 好友备注名 |
| `GET` | `/friends/{user_id}/groups` | 该好友所在分组 |
| `GET` | `/friends/dm/{user_id}` | 查私信房间 |
| `POST` | `/friends/dm/{user_id}` | 建私信房间（幂等） |
| `GET`/`POST` | `/friends/search` | 搜索可添加用户 |
| `GET` | `/friends/suggestions` | 推荐好友 |
| `GET`/`POST` | `/friends/groups` | 分组列表 / 建组 |
| `DELETE` | `/friends/groups/{group_id}` | 删组 |
| `PUT` | `/friends/groups/{group_id}/name` | 改组名 |
| `POST` | `/friends/groups/{group_id}/add/{user_id}` | 加入分组 |
| `DELETE` | `/friends/groups/{group_id}/remove/{user_id}` | 移出分组 |
| `GET` | `/friends/groups/{group_id}/friends` | 组内好友 |

---

## 好友列表

`GET /_matrix/vendor/v1/friends`

**查询参数**

| 参数 | 默认 | 说明 |
|---|---|---|
| `limit` | `50` | 夹在 `1..=200` |
| `from` | 无 | **Keyset 游标**，取上次响应的 `next_batch` |
| `sort_by` | `alphabet` | 排序字段 |
| `offset` | 无 | ⚠️ **已废弃**：`offset > 0` 且没给 `from` → **400 `M_BAD_JSON`**（`Legacy offset pagination is no longer supported; use from cursor`） |

**响应**：

```json
{
  "friends": [
    {
      "user_id": "@bob:matrix.test",
      "username": "bob",
      "displayname": "Bob",
      "avatar_url": "mxc://matrix.test/bob",
      "note": "同事",
      "status": "normal",
      "online": true,
      "presence": "online",
      "last_active_ts": 1760700000000,
      "last_seen_ts": 1760700000000,
      "added_ts": 1759000000000,
      "sort_letter": "B",
      "dm_room_id": "!dm:matrix.test",
      "dm_room_active": true,
      "dm_room_state": "join"
    }
  ],
  "total": 37,
  "limit": 50,
  "offset": null,
  "next_offset": 50,
  "next_batch": "eyJvIjo1MH0",
  "room_id": "!friends-of-alice:matrix.test",
  "version": 12,
  "cached": false,
  "generated_ts": 1760700000123
}
```

**要点**

- 外层键是 **`friends`**，不是 `data`。历史文档的 `{"status":"ok","data":[...]}` 从未存在。
- 分页必须用 `next_batch`；`next_batch` 为 `null` 表示到底。
- `room_id` / `version` / `cached` 暴露内部实现（好友列表房间与缓存版本），客户端**不要依赖**。
- `status` 是关系状态（`favorite` / `normal` / `blocked` / `hidden`），**不是**在线状态；
  在线看 `presence` / `online`。

---

## 好友请求

### 发送

`POST /_matrix/vendor/v1/friends/request`（或 `POST /friends`）

```json
{ "user_id": "@bob:matrix.test", "message": "一起做项目？" }
```

**响应**：`{ "request_id": "42", "status": "pending" }`

> `request_id` 是**字符串**（服务端把内部 i64 转了 string）。按数字处理会类型不符。

| 错误 | 状态 | errcode |
|---|---|---|
| 给自己发 | 400 | `M_BAD_JSON`（`Cannot send friend request to yourself`） |
| `user_id` 格式非法 | 400 | `M_INVALID_PARAM` |
| 已经是好友 / 已有待处理请求 | 400 | `M_BAD_JSON`（由底层好友房间状态机拒绝） |

### 待处理列表

| 端点 | 响应 |
|---|---|
| `GET /friends/requests/incoming` | `{"requests": [<FriendRequest>...]}` |
| `GET /friends/requests/outgoing` | `{"requests": [<FriendRequest>...]}` |

```json
{
  "user_id": "@bob:matrix.test",
  "displayname": "Bob",
  "avatar_url": "mxc://...",
  "message": "一起做项目？",
  "timestamp": 1760700000000,
  "status": "pending"
}
```

`status` ∈ `pending` / `accepted` / `rejected` / `cancelled`（小写）。

### 接受 / 拒绝 / 撤回

| 操作 | 端点 | 路径参数 | 响应 |
|---|---|---|---|
| 接受 | `POST /friends/request/{user_id}/accept` | **请求方 user_id** | `{"room_id": "!dm:...", "status": "accepted"}` |
| 拒绝 | `POST /friends/request/{user_id}/reject` | 请求方 user_id | `{"status": "rejected"}` |
| 撤回 | `POST /friends/request/{user_id}/cancel` | 被请求方 user_id | `{"status": "cancelled"}` |

> ⚠️ 路径参数是 **`{user_id}`**，不是 `{request_id}`；动作是 **`reject`**，不是 `decline`。
> 旧文档写的 `/friends/request/{request_id}/accept|decline` 与 `/friends/requests`
> 在本服务中**都不存在**（404）。

`accept` 会**顺手建好私信房间**并返回 `room_id`——客户端拿到后直接跳转，不必再调
`POST /friends/dm/{user_id}`。

---

## 删除好友与好友属性

### 删除

`DELETE /_matrix/vendor/v1/friends/{user_id}`

**响应**：`{ "removed": true, "user_id": "@bob:matrix.test", "removed_ts": 1760700000000 }`

> 路径参数是 user_id；`DELETE /friends`（不带 ID）不存在。`user_id` 必须放在 body 里的旧写法也失效。

### 备注

`PUT /friends/{user_id}/note` → `{"note": "同事"}`

响应：`{"user_id": "...", "note": "...", "updated_ts": 1760700000000}`；`note` 上限 **1000 字符**，超长 400。

### 关系状态

`PUT /friends/{user_id}/status` → `{"status": "favorite"}`

白名单：`favorite` / `normal` / `blocked` / `hidden`，其它值 400（错误文案会列出合法值）。

`GET /friends/{user_id}/status` → 好友信息对象（非好友时返回
`{"user_id": "...", "status": "none", "is_friend": false}`）。

### 备注名

`PUT /friends/{user_id}/displayname` → `{"displayname": "老鲍"}`

长度 `1..=256`，否则 400。响应 `{"user_id", "displayname", "updated_ts"}`。

### 详情

`GET /friends/{user_id}/info` → 好友信息对象；非好友 → **404 `M_NOT_FOUND`**
（`Friend {id} not found`）。

### 是否好友

`GET /friends/check/{user_id}` →

```json
{ "user_id": "@bob:matrix.test", "is_friend": true, "are_friends": true }
```

> 两个键都给（`is_friend` 与 `are_friends` 同值），兼容不同客户端命名。用哪个都行。

---

## 私信房间

| 方法 | 端点 | 响应 |
|---|---|---|
| `GET` | `/friends/dm/{user_id}` | `{"room_id": "!dm:..." \| null, "user_id": "@bob:..."}` |
| `POST` | `/friends/dm/{user_id}` | `{"room_id": "!dm:...", "user_id": "@bob:...", "created": true\|false}` |

- `GET` 在**没有**私信房间时返回 `room_id: null`（**不是 404**）。
- `POST` 是**幂等**的：已有房间时返回 `created: false` 与同一个 `room_id`。
- 新建房间的参数固定为：`visibility=private`、`preset=private_chat`、`is_direct=true`、
  邀请对方——客户端无法覆盖（避免建出不合规的 DM）。

> 判断"这是不是 DM 房间"应看房间的 `m.room.member` 事件里 `is_direct: true`，
> 或用 vendor 的 `GET /rooms/{room_id}/dm`（见 [extensions.md](./extensions.md)）。

---

## 搜索与建议

### 搜索

`GET /friends/search?q=<term>&mode=exact&limit=20` 或 `POST /friends/search`（体 `{"q"|"query", "mode", "limit"}`）

- `q` 与 `query` 都接受；**空白/缺失** → 400（`Search term cannot be empty`）。
- `mode=exact` 走精确匹配，否则模糊匹配。
- 服务端对搜索有独立限流（令牌桶），超限 429。

**响应**：

```json
{
  "results": [
    {
      "user_id": "@bob:matrix.test",
      "username": "bob",
      "displayname": "Bob",
      "avatar_url": "mxc://...",
      "presence": "online",
      "online": true,
      "last_active_ts": 1760700000000,
      "last_seen_ts": 1760700000000,
      "created_ts": 1759000000000,
      "match_score": 0.87,
      "match_type": "fuzzy"
    }
  ]
}
```

### 建议

`GET /friends/suggestions?limit=10` → `{"suggestions": [ ... ]}`

> 搜索结果**只包含可见用户**（`filter_users_with_shared_rooms`）——不会把全站用户
> 暴露给任意调用者。客户端不要假设"搜不到 = 用户不存在"。

---

## 分组

| 方法 | 端点 | 请求体 | 响应 |
|---|---|---|---|
| `GET` | `/friends/groups` | — | `{"groups": [ ... ]}` |
| `POST` | `/friends/groups` | `{"name": "同事"}` | 分组对象 |
| `DELETE` | `/friends/groups/{group_id}` | — | `{"deleted": true, "group_id", "deleted_ts"}` |
| `PUT` | `/friends/groups/{group_id}/name` | `{"name": "新名"}` | `{"group_id", "name", "updated_ts"}` |
| `POST` | `/friends/groups/{group_id}/add/{user_id}` | — | `{"group_id", "user_id", "added_ts"}` |
| `DELETE` | `/friends/groups/{group_id}/remove/{user_id}` | — | `{"group_id", "user_id", "removed_ts"}` |
| `GET` | `/friends/groups/{group_id}/friends` | — | `{"friends": [ ... ]}` |
| `GET` | `/friends/{user_id}/groups` | — | `{"groups": [ ... ]}` |

组名长度 `1..=50`，超出 400。`group_id` 是字符串。

---

## 状态码与错误

| 场景 | 状态 | errcode |
|---|---|---|
| 缺 token / 坏 token / token 过期 | 401 | `M_UNAUTHORIZED`（`Missing or invalid authorization header` / `Invalid token` / `Token expired`） |
| `user_id` 格式非法 | 400 | `M_INVALID_PARAM` |
| 请求体字段未知（`deny_unknown_fields`） | 400 | `M_BAD_JSON` |
| 旧版 `offset` 分页 | 400 | `M_BAD_JSON` |
| 好友不存在（`/info`） | 404 | `M_NOT_FOUND` |
| 搜索过于频繁 | 429 | `M_LIMIT_EXCEEDED` |

> 本服务所有"参数/语义不对"的 400 都走 `ApiError::bad_request` → **`M_BAD_JSON`**（不是
> `M_BAD_REQUEST`，该码在本服务不存在）；只有校验器显式用 `invalid_param` 的地方才是
> `M_INVALID_PARAM`（如 `user_id` 格式、`receipt_type` 白名单）。客户端分支建议按
> **HTTP 400 + `error` 文案**处理，不要只认 errcode。

> **所有请求体都开了 `deny_unknown_fields`**：多传一个字段（例如旧文档里的 `is_private`）
> 直接 400 `M_BAD_JSON`，不是"忽略未知字段"。这一点在跨版本兼容时最容易踩。

---

## 迁移提示（旧文档 → 实际）

| 旧文档写法 | 实际情况 |
|---|---|
| `GET /friends/room` | ❌ 不存在 |
| `GET /friends/requests` | ❌ 不存在 → `/requests/incoming` / `/requests/outgoing` |
| `POST /friends/request/{request_id}/accept` | ❌ 路径是 `{user_id}` |
| `POST .../{request_id}/decline` | ❌ 是 `/reject`，且路径是 `{user_id}` |
| `DELETE /friends`（body 带 `user_id`） | ❌ 是 `DELETE /friends/{user_id}` |
| `GET /friends` 返回 `{status, data}` | ❌ 返回 `{friends, total, next_batch, ...}` |
| `POST /friends/dm/{user_id}` body `{is_private}` | ❌ 该端点**不读 body**：`is_private` 被静默忽略（建出的房间固定 `is_direct=true`、`private_chat`） |
| `check` 返回 `are_friends` | ✅ 仍在，但同时给 `is_friend` |
